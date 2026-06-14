#![allow(dead_code)]

//! Per-provider adapter contracts (#3084).
//!
//! This is a *foundation* module: it defines a typed contract describing how
//! each provider differs along the axes CodeWhale cares about — capability
//! envelope, authentication model, and request dialect — plus a conformance
//! check that catches malformed capability descriptors. Consumers (the engine,
//! routing, diagnostics) are wired in later changes; for now this module only
//! introduces the substrate and a pair of worked examples.
//!
//! The capability *data* already lives in [`crate::config::provider_capability`].
//! This module does not duplicate or re-derive those numbers; it wraps them in
//! a trait so that per-provider behaviour can be reasoned about uniformly and
//! validated in CI. DeepSeek remains a first-class example here, consistent
//! with CodeWhale's DeepSeek-forward stance.

use crate::config::{ApiProvider, ProviderCapability, RequestPayloadMode, provider_capability};

/// How a provider expects callers to authenticate.
///
/// This is deliberately coarse: it captures the *shape* of the credential a
/// provider needs, not the concrete secret. The `&'static str` on
/// [`AuthModel::EnvVar`] names the canonical environment variable a provider
/// reads its key from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthModel {
    /// The provider authenticates with an API key read from a named environment
    /// variable (e.g. `DEEPSEEK_API_KEY`).
    EnvVar(&'static str),
    /// The provider authenticates via an OAuth flow rather than a static key.
    OAuth,
    /// The provider ships with a built-in/managed key and needs no caller
    /// credential (e.g. a bundled or proxied endpoint).
    BuiltInKey,
}

/// Which on-the-wire request dialect a provider speaks.
///
/// This is a higher-level marker than [`RequestPayloadMode`]: it distinguishes
/// providers that are byte-for-byte OpenAI-compatible from those (like
/// DeepSeek) that speak an OpenAI-shaped protocol with native extensions, and
/// from Anthropic's distinct Messages API. It is derived from — and kept
/// consistent with — the payload mode reported by
/// [`crate::config::provider_capability`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestDialect {
    /// Standard OpenAI-compatible chat/completions or responses payloads.
    OpenAiCompatible,
    /// DeepSeek's OpenAI-shaped dialect with native extensions
    /// (reasoning content, prompt-cache telemetry).
    DeepSeekNative,
    /// Native Anthropic Messages API.
    Anthropic,
}

/// A compact, typed view over a provider's capability envelope.
///
/// Every field here is sourced from [`crate::config::provider_capability`]
/// rather than hard-coded, so adapters stay in lockstep with the canonical
/// capability matrix. `supports_tools` and `supports_streaming` are not tracked
/// as separate columns in [`ProviderCapability`] today — every chat provider
/// CodeWhale targets supports both — so they are represented as
/// adapter-declared constants and default to `true`. They exist as explicit
/// fields so future providers that diverge can override them without a
/// signature change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityDescriptor {
    /// Maximum input the model can accept, in tokens.
    pub context_window: u32,
    /// Official maximum output tokens for this provider+model combo.
    pub max_output: u32,
    /// Whether the provider+model supports a thinking/reasoning mode.
    pub supports_thinking: bool,
    /// Whether tool/function calling is supported. `true` for all current
    /// CodeWhale providers.
    pub supports_tools: bool,
    /// Whether streaming responses are supported. `true` for all current
    /// CodeWhale providers.
    pub supports_streaming: bool,
    /// Whether the provider returns prompt-cache telemetry fields.
    pub supports_cache: bool,
}

impl CapabilityDescriptor {
    /// Build a descriptor from a canonical [`ProviderCapability`].
    ///
    /// Tool and streaming support are assumed `true`, matching every provider
    /// CodeWhale currently targets; the `supports_*` mapping pulls thinking and
    /// cache support straight from the capability matrix.
    #[must_use]
    pub fn from_capability(cap: &ProviderCapability) -> Self {
        Self {
            context_window: cap.context_window,
            max_output: cap.max_output,
            supports_thinking: cap.thinking_supported,
            supports_tools: true,
            supports_streaming: true,
            supports_cache: cap.cache_telemetry_supported,
        }
    }
}

/// Map a canonical [`RequestPayloadMode`] to a higher-level [`RequestDialect`].
///
/// Anthropic's Messages API maps to [`RequestDialect::Anthropic`]; everything
/// else is OpenAI-shaped. The DeepSeek-native distinction is applied by the
/// adapter itself (see [`DeepSeekAdapter::request_dialect`]) because it depends
/// on the provider identity, not just the payload mode.
#[must_use]
pub fn dialect_for_payload_mode(mode: RequestPayloadMode) -> RequestDialect {
    match mode {
        RequestPayloadMode::AnthropicMessages => RequestDialect::Anthropic,
        RequestPayloadMode::ChatCompletions | RequestPayloadMode::Responses => {
            RequestDialect::OpenAiCompatible
        }
    }
}

/// The per-provider contract requested by #3084.
///
/// An adapter ties a provider identity to its capability envelope, auth model,
/// and request dialect. Capability numbers must come from
/// [`crate::config::provider_capability`] — adapters describe *behaviour*, they
/// do not own the capability matrix.
pub trait ProviderAdapter {
    /// The provider this adapter represents.
    fn provider(&self) -> ApiProvider;

    /// The resolved model string this adapter describes.
    fn resolved_model(&self) -> &str;

    /// The capability envelope for this provider + resolved model.
    ///
    /// The default implementation derives the descriptor from the canonical
    /// capability matrix; implementors normally do not override it.
    fn capability(&self) -> CapabilityDescriptor {
        let cap = provider_capability(self.provider(), self.resolved_model());
        CapabilityDescriptor::from_capability(&cap)
    }

    /// How callers authenticate with this provider.
    fn auth_model(&self) -> AuthModel;

    /// The on-the-wire request dialect this provider speaks.
    fn request_dialect(&self) -> RequestDialect;
}

/// Outcome of an adapter conformance check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConformanceError {
    /// The context window was zero.
    ZeroContextWindow,
    /// The max output token count was zero.
    ZeroMaxOutput,
    /// `max_output` exceeded `context_window`, which is never valid.
    MaxOutputExceedsContextWindow {
        max_output: u32,
        context_window: u32,
    },
}

impl std::fmt::Display for ConformanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroContextWindow => write!(f, "context_window must be > 0"),
            Self::ZeroMaxOutput => write!(f, "max_output must be > 0"),
            Self::MaxOutputExceedsContextWindow {
                max_output,
                context_window,
            } => write!(
                f,
                "max_output ({max_output}) must be <= context_window ({context_window})"
            ),
        }
    }
}

impl std::error::Error for ConformanceError {}

/// Check an adapter's capability invariants without panicking.
///
/// Invariants:
/// - `context_window > 0`
/// - `max_output > 0`
/// - `max_output <= context_window`
///
/// Returns the first violated invariant, or `Ok(())` if the adapter conforms.
pub fn check_adapter_conformance<A: ProviderAdapter>(adapter: &A) -> Result<(), ConformanceError> {
    let cap = adapter.capability();
    if cap.context_window == 0 {
        return Err(ConformanceError::ZeroContextWindow);
    }
    if cap.max_output == 0 {
        return Err(ConformanceError::ZeroMaxOutput);
    }
    if cap.max_output > cap.context_window {
        return Err(ConformanceError::MaxOutputExceedsContextWindow {
            max_output: cap.max_output,
            context_window: cap.context_window,
        });
    }
    Ok(())
}

/// Assert that an adapter conforms, panicking with the violated invariant.
///
/// Convenience wrapper over [`check_adapter_conformance`] intended for tests and
/// CI policy checks where a violation should abort.
///
/// # Panics
///
/// Panics if any capability invariant is violated.
pub fn assert_adapter_conformance<A: ProviderAdapter>(adapter: &A) {
    if let Err(err) = check_adapter_conformance(adapter) {
        panic!(
            "provider adapter for {:?} failed conformance: {err}",
            adapter.provider()
        );
    }
}

// ============================================================================
// Worked examples
// ============================================================================

/// Adapter for the DeepSeek provider — CodeWhale's first-class provider.
///
/// Speaks the OpenAI-shaped DeepSeek-native dialect (reasoning content +
/// prompt-cache telemetry) and authenticates with the `DEEPSEEK_API_KEY`
/// environment variable.
#[derive(Debug, Clone)]
pub struct DeepSeekAdapter {
    resolved_model: String,
}

impl DeepSeekAdapter {
    /// Canonical environment variable DeepSeek reads its API key from.
    pub const API_KEY_ENV: &'static str = "DEEPSEEK_API_KEY";

    /// Build a DeepSeek adapter for a resolved model string.
    #[must_use]
    pub fn new(resolved_model: impl Into<String>) -> Self {
        Self {
            resolved_model: resolved_model.into(),
        }
    }
}

impl ProviderAdapter for DeepSeekAdapter {
    fn provider(&self) -> ApiProvider {
        ApiProvider::Deepseek
    }

    fn resolved_model(&self) -> &str {
        &self.resolved_model
    }

    fn auth_model(&self) -> AuthModel {
        AuthModel::EnvVar(Self::API_KEY_ENV)
    }

    fn request_dialect(&self) -> RequestDialect {
        RequestDialect::DeepSeekNative
    }
}

/// Adapter for the generic OpenAI provider — a worked OpenAI-compatible example.
///
/// Speaks the standard OpenAI dialect and authenticates with the
/// `OPENAI_API_KEY` environment variable. Its dialect is derived from the
/// canonical payload mode rather than asserted, demonstrating the
/// payload-mode → dialect mapping.
#[derive(Debug, Clone)]
pub struct OpenAiCompatibleAdapter {
    resolved_model: String,
}

impl OpenAiCompatibleAdapter {
    /// Canonical environment variable OpenAI reads its API key from.
    pub const API_KEY_ENV: &'static str = "OPENAI_API_KEY";

    /// Build an OpenAI-compatible adapter for a resolved model string.
    #[must_use]
    pub fn new(resolved_model: impl Into<String>) -> Self {
        Self {
            resolved_model: resolved_model.into(),
        }
    }
}

impl ProviderAdapter for OpenAiCompatibleAdapter {
    fn provider(&self) -> ApiProvider {
        ApiProvider::Openai
    }

    fn resolved_model(&self) -> &str {
        &self.resolved_model
    }

    fn auth_model(&self) -> AuthModel {
        AuthModel::EnvVar(Self::API_KEY_ENV)
    }

    fn request_dialect(&self) -> RequestDialect {
        // Derived from the canonical payload mode, demonstrating the mapping
        // helper rather than hard-coding the dialect.
        let cap = provider_capability(self.provider(), self.resolved_model());
        dialect_for_payload_mode(cap.request_payload_mode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deepseek_adapter_conforms() {
        let adapter = DeepSeekAdapter::new("deepseek-v4-flash");
        // Should not panic.
        assert_adapter_conformance(&adapter);
        assert!(check_adapter_conformance(&adapter).is_ok());
    }

    #[test]
    fn openai_adapter_conforms() {
        let adapter = OpenAiCompatibleAdapter::new("gpt-4o");
        assert_adapter_conformance(&adapter);
        assert!(check_adapter_conformance(&adapter).is_ok());
    }

    #[test]
    fn deepseek_markers_are_as_expected() {
        let adapter = DeepSeekAdapter::new("deepseek-v4-flash");
        assert_eq!(adapter.provider(), ApiProvider::Deepseek);
        assert_eq!(adapter.auth_model(), AuthModel::EnvVar("DEEPSEEK_API_KEY"));
        assert_eq!(adapter.request_dialect(), RequestDialect::DeepSeekNative);
    }

    #[test]
    fn openai_markers_are_as_expected() {
        let adapter = OpenAiCompatibleAdapter::new("gpt-4o");
        assert_eq!(adapter.provider(), ApiProvider::Openai);
        assert_eq!(adapter.auth_model(), AuthModel::EnvVar("OPENAI_API_KEY"));
        // Openai uses the chat-completions payload mode, which maps to the
        // OpenAI-compatible dialect.
        assert_eq!(adapter.request_dialect(), RequestDialect::OpenAiCompatible);
    }

    #[test]
    fn capability_descriptor_matches_canonical_matrix() {
        let adapter = DeepSeekAdapter::new("deepseek-v4-flash");
        let canonical = provider_capability(ApiProvider::Deepseek, "deepseek-v4-flash");
        let descriptor = adapter.capability();

        assert_eq!(descriptor.context_window, canonical.context_window);
        assert_eq!(descriptor.max_output, canonical.max_output);
        assert_eq!(descriptor.supports_thinking, canonical.thinking_supported);
        assert_eq!(
            descriptor.supports_cache,
            canonical.cache_telemetry_supported
        );
        // Tools and streaming are adapter-declared and true for all current
        // providers.
        assert!(descriptor.supports_tools);
        assert!(descriptor.supports_streaming);
    }

    #[test]
    fn deepseek_v4_flash_has_expected_envelope() {
        // V4-flash resolves to the 1M context window and 384K max output.
        let adapter = DeepSeekAdapter::new("deepseek-v4-flash");
        let cap = adapter.capability();
        assert_eq!(cap.context_window, 1_000_000);
        assert_eq!(cap.max_output, 384_000);
        assert!(cap.supports_thinking);
        assert!(cap.supports_cache);
    }

    #[test]
    fn payload_mode_dialect_mapping() {
        assert_eq!(
            dialect_for_payload_mode(RequestPayloadMode::ChatCompletions),
            RequestDialect::OpenAiCompatible
        );
        assert_eq!(
            dialect_for_payload_mode(RequestPayloadMode::Responses),
            RequestDialect::OpenAiCompatible
        );
        assert_eq!(
            dialect_for_payload_mode(RequestPayloadMode::AnthropicMessages),
            RequestDialect::Anthropic
        );
    }

    #[test]
    fn conformance_rejects_zero_context_window() {
        // A hand-rolled adapter with a deliberately broken envelope exercises
        // the conformance checker directly (the real adapters always conform).
        struct BrokenAdapter;
        impl ProviderAdapter for BrokenAdapter {
            fn provider(&self) -> ApiProvider {
                ApiProvider::Openai
            }
            fn resolved_model(&self) -> &str {
                "broken"
            }
            fn capability(&self) -> CapabilityDescriptor {
                CapabilityDescriptor {
                    context_window: 0,
                    max_output: 10,
                    supports_thinking: false,
                    supports_tools: true,
                    supports_streaming: true,
                    supports_cache: false,
                }
            }
            fn auth_model(&self) -> AuthModel {
                AuthModel::OAuth
            }
            fn request_dialect(&self) -> RequestDialect {
                RequestDialect::OpenAiCompatible
            }
        }
        assert_eq!(
            check_adapter_conformance(&BrokenAdapter),
            Err(ConformanceError::ZeroContextWindow)
        );
    }

    #[test]
    fn conformance_rejects_max_output_exceeding_context_window() {
        struct BrokenAdapter;
        impl ProviderAdapter for BrokenAdapter {
            fn provider(&self) -> ApiProvider {
                ApiProvider::Openai
            }
            fn resolved_model(&self) -> &str {
                "broken"
            }
            fn capability(&self) -> CapabilityDescriptor {
                CapabilityDescriptor {
                    context_window: 100,
                    max_output: 200,
                    supports_thinking: false,
                    supports_tools: true,
                    supports_streaming: true,
                    supports_cache: false,
                }
            }
            fn auth_model(&self) -> AuthModel {
                AuthModel::BuiltInKey
            }
            fn request_dialect(&self) -> RequestDialect {
                RequestDialect::OpenAiCompatible
            }
        }
        assert_eq!(
            check_adapter_conformance(&BrokenAdapter),
            Err(ConformanceError::MaxOutputExceedsContextWindow {
                max_output: 200,
                context_window: 100,
            })
        );
    }

    #[test]
    #[should_panic(expected = "failed conformance")]
    fn assert_conformance_panics_on_violation() {
        struct BrokenAdapter;
        impl ProviderAdapter for BrokenAdapter {
            fn provider(&self) -> ApiProvider {
                ApiProvider::Openai
            }
            fn resolved_model(&self) -> &str {
                "broken"
            }
            fn capability(&self) -> CapabilityDescriptor {
                CapabilityDescriptor {
                    context_window: 10,
                    max_output: 0,
                    supports_thinking: false,
                    supports_tools: true,
                    supports_streaming: true,
                    supports_cache: false,
                }
            }
            fn auth_model(&self) -> AuthModel {
                AuthModel::OAuth
            }
            fn request_dialect(&self) -> RequestDialect {
                RequestDialect::OpenAiCompatible
            }
        }
        assert_adapter_conformance(&BrokenAdapter);
    }
}
