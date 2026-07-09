//! Local-only release runtime QA through real pseudo-terminals.
//!
//! These scenarios cover the live TUI checks that unit tests cannot prove:
//! six-worker fanout liveness/cancellation, multi-terminal route isolation,
//! and queued steering via Ctrl+S. Every provider is a loopback wiremock
//! server and every process receives a sealed HOME.

#![cfg(unix)]

#[path = "support/qa_harness/mod.rs"]
mod qa_harness;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use qa_harness::harness::{Harness, SealedWorkspace, make_sealed_workspace};
use qa_harness::keys;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const BOOT_TIMEOUT: Duration = Duration::from_secs(20);
const INTERACTION_TIMEOUT: Duration = Duration::from_secs(15);
const COMPOSER_READY_TEXT: &str = "Write a task";
const MUSE_MODEL: &str = "muse-spark-1.1";
const GPT_MODEL: &str = "gpt-5.6-terra";
const DEEPSEEK_TEST_MODEL: &str = "deepseek-v4-pro";
static RELEASE_RUNTIME_QA_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn sse_chunk(value: Value) -> String {
    format!(
        "data: {}\n\n",
        serde_json::to_string(&value).expect("SSE JSON")
    )
}

fn text_sse(model: &str, text: &str) -> String {
    [
        sse_chunk(json!({
            "id": "chatcmpl-local-qa",
            "object": "chat.completion.chunk",
            "model": model,
            "choices": [{
                "index": 0,
                "delta": { "content": text },
                "finish_reason": null
            }]
        })),
        sse_chunk(json!({
            "id": "chatcmpl-local-qa",
            "object": "chat.completion.chunk",
            "model": model,
            "choices": [{
                "index": 0,
                "delta": {},
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 12,
                "completion_tokens": 4,
                "total_tokens": 16
            }
        })),
        "data: [DONE]\n\n".to_string(),
    ]
    .join("")
}

fn fanout_tool_call_sse() -> String {
    let tool_calls = (1..=6)
        .map(|worker| {
            json!({
                "index": worker - 1,
                "id": format!("call_agent_{worker}"),
                "type": "function",
                "function": {
                    "name": "agent",
                    "arguments": serde_json::to_string(&json!({
                        "message": format!("stay busy worker {worker} until the parent QA turn is cancelled"),
                        "agent_type": "explorer",
                        "session_name": format!("qa-worker-{worker}")
                    }))
                    .expect("agent arguments")
                }
            })
        })
        .collect::<Vec<_>>();

    [
        sse_chunk(json!({
            "id": "chatcmpl-fanout",
            "object": "chat.completion.chunk",
            "model": DEEPSEEK_TEST_MODEL,
            "choices": [{
                "index": 0,
                "delta": { "tool_calls": tool_calls },
                "finish_reason": null
            }]
        })),
        sse_chunk(json!({
            "id": "chatcmpl-fanout",
            "object": "chat.completion.chunk",
            "model": DEEPSEEK_TEST_MODEL,
            "choices": [{
                "index": 0,
                "delta": {},
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 20,
                "completion_tokens": 12,
                "total_tokens": 32
            }
        })),
        "data: [DONE]\n\n".to_string(),
    ]
    .join("")
}

fn sse_response(body: String) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .insert_header("cache-control", "no-cache")
        .set_body_string(body)
}

fn json_response(value: Value) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "application/json")
        .set_body_json(value)
}

async fn mount_models(server: &MockServer, models: &[&str]) {
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(json_response(json!({
            "object": "list",
            "data": models
                .iter()
                .map(|model| json!({ "id": model, "object": "model" }))
                .collect::<Vec<_>>()
        })))
        .mount(server)
        .await;
}

async fn mount_text_model(server: &MockServer, model: &str, answer: &str) {
    mount_models(server, &[model]).await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(sse_response(text_sse(model, answer)))
        .mount(server)
        .await;
}

fn common_tui_builder(ws: &SealedWorkspace) -> qa_harness::harness::HarnessBuilder {
    Harness::builder(Harness::cargo_bin("codewhale-tui"))
        .cwd(ws.workspace())
        .clear_env()
        .seal_home(ws.home())
        .env("RUST_LOG", "warn")
        .args([
            "--workspace",
            ws.workspace().to_str().expect("utf-8 workspace path"),
            "--no-project-config",
            "--skip-onboarding",
        ])
        .size(42, 150)
}

fn wait_for_counter(
    harness: &mut Harness,
    counter: &AtomicUsize,
    expected: usize,
    timeout: Duration,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        harness.pump();
        if counter.load(Ordering::SeqCst) >= expected {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(anyhow!(
                "counter did not reach {expected} within {timeout:?}; observed {}\n{}",
                counter.load(Ordering::SeqCst),
                harness.debug_dump()
            ));
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}

fn type_and_submit(harness: &mut Harness, text: &str) -> Result<()> {
    harness.send(keys::key::text(text))?;
    harness.wait_for_idle(Duration::from_millis(150), Duration::from_secs(2))?;
    harness.send(keys::key::enter())?;
    Ok(())
}

fn type_and_tab(harness: &mut Harness, text: &str) -> Result<()> {
    harness.send(keys::key::text(text))?;
    harness.wait_for_text(text, Duration::from_secs(3))?;
    harness.send(b"\t")?;
    Ok(())
}

fn chat_requests(requests: &[Request]) -> Vec<Value> {
    requests
        .iter()
        .filter(|request| request.url.path().ends_with("/chat/completions"))
        .map(|request| request.body_json().expect("chat body JSON"))
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn release_multi_terminal_muse_and_gpt_routes_stay_isolated() -> Result<()> {
    let _guard = RELEASE_RUNTIME_QA_LOCK.lock().await;
    let meta_server = MockServer::start().await;
    let openai_server = MockServer::start().await;
    mount_text_model(&meta_server, MUSE_MODEL, "meta-route-ok").await;
    mount_models(&openai_server, &["gpt-5.6-luna", GPT_MODEL]).await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(sse_response(text_sse(GPT_MODEL, "openai-route-ok")))
        .mount(&openai_server)
        .await;

    let ws = make_sealed_workspace()?;
    let openai_base_url = openai_server.uri();
    let meta_base_url = meta_server.uri();
    let shared_openai_env = [
        ("OPENAI_API_KEY", "openai-local-test-key"),
        ("OPENAI_BASE_URL", openai_base_url.as_str()),
        ("OPENAI_MODEL", "gpt-5.6-luna"),
    ];
    let shared_meta_env = [
        ("META_MODEL_API_KEY", "meta-local-test-key"),
        ("MODEL_API_KEY", "meta-local-test-key"),
        ("META_MODEL_API_BASE_URL", meta_base_url.as_str()),
        ("META_MODEL_API_MODEL", MUSE_MODEL),
    ];

    let mut meta_builder = common_tui_builder(&ws).env("CODEWHALE_PROVIDER", "meta");
    let mut openai_builder = common_tui_builder(&ws).env("CODEWHALE_PROVIDER", "openai");
    for (key, value) in shared_openai_env.into_iter().chain(shared_meta_env) {
        meta_builder = meta_builder.env(key, value);
        openai_builder = openai_builder.env(key, value);
    }

    let mut meta_tui = meta_builder.spawn()?;
    let mut openai_tui = openai_builder.spawn()?;
    meta_tui.wait_for_text(COMPOSER_READY_TEXT, BOOT_TIMEOUT)?;
    openai_tui.wait_for_text(COMPOSER_READY_TEXT, BOOT_TIMEOUT)?;

    // Change terminal B's model through the live command path while terminal A
    // remains open on Meta. Both processes share one sealed settings file.
    type_and_submit(&mut openai_tui, "/model gpt-5.6-terra")?;
    openai_tui.wait_for(
        |frame| frame.row(0).contains(GPT_MODEL),
        INTERACTION_TIMEOUT,
    )?;
    assert!(
        meta_tui.frame().contains(MUSE_MODEL),
        "terminal A route changed when terminal B selected a model:\n{}",
        meta_tui.debug_dump()
    );

    type_and_submit(&mut meta_tui, "route probe from meta terminal")?;
    type_and_submit(&mut openai_tui, "route probe from openai terminal")?;
    meta_tui.wait_for_text("meta-route-ok", INTERACTION_TIMEOUT)?;
    openai_tui.wait_for_text("openai-route-ok", INTERACTION_TIMEOUT)?;

    let meta_requests = meta_server.received_requests().await.unwrap_or_default();
    let openai_requests = openai_server.received_requests().await.unwrap_or_default();
    let meta_chat = chat_requests(&meta_requests);
    let openai_chat = chat_requests(&openai_requests);
    assert_eq!(
        meta_chat.len(),
        1,
        "unexpected Meta chat requests: {meta_chat:#?}"
    );
    assert_eq!(
        openai_chat.len(),
        1,
        "unexpected OpenAI chat requests: {openai_chat:#?}"
    );
    assert_eq!(meta_chat[0]["model"], MUSE_MODEL);
    assert_eq!(openai_chat[0]["model"], GPT_MODEL);
    assert!(
        meta_chat[0]
            .to_string()
            .contains("route probe from meta terminal")
    );
    assert!(!meta_chat[0].to_string().contains("openai terminal"));
    assert!(
        openai_chat[0]
            .to_string()
            .contains("route probe from openai terminal")
    );
    assert!(!openai_chat[0].to_string().contains("meta terminal"));

    let _ = meta_tui.shutdown();
    let _ = openai_tui.shutdown();
    Ok(())
}

#[derive(Clone)]
struct FanoutResponder {
    child_requests: Arc<AtomicUsize>,
}

impl Respond for FanoutResponder {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body = request.body_json::<Value>().unwrap_or(Value::Null);
        let raw = body.to_string();

        if raw.contains("stay busy worker") && !raw.contains("launch six QA workers") {
            self.child_requests.fetch_add(1, Ordering::SeqCst);
            return sse_response(text_sse(DEEPSEEK_TEST_MODEL, "child-finished-too-soon"))
                .set_delay(Duration::from_secs(20));
        }

        if raw.contains("launch six QA workers") {
            return sse_response(fanout_tool_call_sse());
        }

        sse_response(text_sse(DEEPSEEK_TEST_MODEL, "unexpected-request"))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn release_six_worker_fanout_keeps_typing_render_and_esc_cancel_live() -> Result<()> {
    let _guard = RELEASE_RUNTIME_QA_LOCK.lock().await;
    let server = MockServer::start().await;
    mount_models(&server, &[DEEPSEEK_TEST_MODEL]).await;
    let child_requests = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(FanoutResponder {
            child_requests: Arc::clone(&child_requests),
        })
        .mount(&server)
        .await;

    let ws = make_sealed_workspace()?;
    std::fs::write(
        ws.home().join(".codewhale").join("config.toml"),
        "[subagents]\nmax_concurrent = 6\nlaunch_concurrency = 6\nmax_admitted = 6\n",
    )?;
    let mut tui = common_tui_builder(&ws)
        .env("CODEWHALE_PROVIDER", "deepseek")
        .env("DEEPSEEK_API_KEY", "deepseek-local-test-key")
        .env("DEEPSEEK_BASE_URL", server.uri())
        .env("DEEPSEEK_MODEL", DEEPSEEK_TEST_MODEL)
        .args(["--yolo", "--max-subagents", "6"])
        .spawn()?;
    tui.wait_for_text(COMPOSER_READY_TEXT, BOOT_TIMEOUT)?;

    type_and_submit(
        &mut tui,
        "launch six QA workers and keep the parent turn open",
    )?;
    wait_for_counter(&mut tui, &child_requests, 6, INTERACTION_TIMEOUT)?;
    tui.wait_for_text("6 running / 6", Duration::from_secs(5))?;

    let fanout_frame = tui.debug_dump();
    assert!(
        fanout_frame.contains("6 running / 6"),
        "all six workers were not visible in the sidebar:\n{fanout_frame}"
    );

    // The provider is deliberately holding every child open. Prove keyboard
    // input and rendering remain live during the storm, then interrupt the
    // still-live orchestration turn directly with Esc.
    tui.send(keys::key::text("fanout-live-marker"))?;
    tui.wait_for_text("fanout-live-marker", Duration::from_secs(3))?;
    let before_cancel = tui.debug_dump();
    assert!(
        before_cancel.contains("Agent") || before_cancel.contains("agent"),
        "fanout UI did not expose agent activity:\n{before_cancel}"
    );

    let cancel_started = Instant::now();
    tui.send(b"\x1b")?;
    tui.wait_for(
        |frame| {
            let text = frame.text().to_ascii_lowercase();
            text.contains("cancelled") || text.contains("interrupted")
        },
        Duration::from_secs(5),
    )?;
    assert!(
        cancel_started.elapsed() < Duration::from_secs(5),
        "Esc cancellation exceeded the five-second liveness budget"
    );

    tui.send(keys::key::text("post-cancel-live"))?;
    tui.wait_for_text("post-cancel-live", Duration::from_secs(3))?;
    assert_eq!(child_requests.load(Ordering::SeqCst), 6);

    let _ = tui.shutdown();
    Ok(())
}

#[derive(Clone)]
struct SteeringResponder {
    initial_requests: Arc<AtomicUsize>,
    steer_requests: Arc<AtomicUsize>,
}

impl Respond for SteeringResponder {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body = request.body_json::<Value>().unwrap_or(Value::Null);
        let raw = body.to_string();
        if raw.contains("queued steering from ctrl-s") {
            self.steer_requests.fetch_add(1, Ordering::SeqCst);
            return sse_response(text_sse(DEEPSEEK_TEST_MODEL, "steering-applied"));
        }
        if raw.contains("initial slow turn") {
            self.initial_requests.fetch_add(1, Ordering::SeqCst);
            return sse_response(text_sse(DEEPSEEK_TEST_MODEL, "initial-turn-output"))
                .set_delay(Duration::from_secs(3));
        }
        sse_response(text_sse(DEEPSEEK_TEST_MODEL, "unexpected-request"))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn release_queued_steering_ctrl_s_sends_now_with_clear_status() -> Result<()> {
    let _guard = RELEASE_RUNTIME_QA_LOCK.lock().await;
    let server = MockServer::start().await;
    mount_models(&server, &[DEEPSEEK_TEST_MODEL]).await;
    let initial_requests = Arc::new(AtomicUsize::new(0));
    let steer_requests = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(SteeringResponder {
            initial_requests: Arc::clone(&initial_requests),
            steer_requests: Arc::clone(&steer_requests),
        })
        .mount(&server)
        .await;

    let ws = make_sealed_workspace()?;
    let mut tui = common_tui_builder(&ws)
        .env("CODEWHALE_PROVIDER", "deepseek")
        .env("DEEPSEEK_API_KEY", "deepseek-local-test-key")
        .env("DEEPSEEK_BASE_URL", server.uri())
        .env("DEEPSEEK_MODEL", DEEPSEEK_TEST_MODEL)
        .spawn()?;
    tui.wait_for_text(COMPOSER_READY_TEXT, BOOT_TIMEOUT)?;

    type_and_submit(&mut tui, "initial slow turn")?;
    wait_for_counter(&mut tui, &initial_requests, 1, Duration::from_secs(3))?;

    type_and_tab(&mut tui, "queued steering from ctrl-s")?;
    tui.wait_for_text("Ctrl+S send now", Duration::from_secs(5))?;
    assert!(
        tui.frame().contains("queued steering from ctrl-s"),
        "queued steering preview was not readable:\n{}",
        tui.debug_dump()
    );

    let steer_started = Instant::now();
    tui.send(b"\x13")?;
    wait_for_counter(&mut tui, &steer_requests, 1, INTERACTION_TIMEOUT)?;
    tui.wait_for_text("steering-applied", INTERACTION_TIMEOUT)?;
    assert!(
        steer_started.elapsed() < Duration::from_secs(10),
        "Ctrl+S steering was not incorporated promptly"
    );

    let _ = tui.shutdown();
    Ok(())
}
