import Image from "next/image";
import Link from "next/link";
import { InstallCodeBlock } from "@/components/install-code-block";
import { Whale } from "@/components/whale";
import { getFacts } from "@/lib/facts";

const REPO = "https://github.com/Hmbown/CodeWhale";

// Revalidate against source-proven runtime facts without giving up static edge
// caching. `getFacts()` rejects legacy or older KV snapshots.
export const revalidate = 300;

const WORKFLOW = [
  {
    en: ["Inspect", "Read the repository, its instructions, and the task."],
    zh: ["检查", "读取仓库、项目说明与任务。"],
  },
  {
    en: ["Act", "Edit files through explicit approval boundaries."],
    zh: ["执行", "在明确的审批边界内修改文件。"],
  },
  {
    en: ["Verify", "Run checks and inspect the result."],
    zh: ["验证", "运行检查并核对结果。"],
  },
  {
    en: ["Report", "Leave a concise, durable receipt."],
    zh: ["报告", "留下简洁、可追溯的工作收据。"],
  },
] as const;

const SURFACES = [
  {
    en: ["TUI", "Interactive terminal work"],
    zh: ["TUI", "交互式终端工作"],
  },
  {
    en: ["codewhale exec", "Scripts and CI"],
    zh: ["codewhale exec", "脚本与 CI"],
  },
  {
    en: ["Web client", "Loopback-only browser client"],
    zh: ["Web 客户端", "仅限本机回环的浏览器客户端"],
  },
  {
    en: ["Runtime API + MCP", "Local integrations"],
    zh: ["运行时 API + MCP", "本地集成"],
  },
  {
    en: ["Fleet", "Durable multi-agent work"],
    zh: ["Fleet", "持久化多智能体工作"],
  },
] as const;

export default async function HomePage({ params }: { params: Promise<{ locale: string }> }) {
  const { locale } = await params;
  const isZh = locale === "zh";
  const facts = await getFacts();
  const sourceVersion = facts.version ?? "unknown";
  const publishedRelease = facts.latestPublishedRelease;
  const sourceIsPublished = publishedRelease?.version === sourceVersion;
  const providerCount = facts.providers.length;

  return (
    <div className="product-home">
      <section className="product-hero">
        <div className="product-current" aria-hidden="true" />
        <div className="product-life" aria-hidden="true">
          <span className="product-life-fish product-life-fish-a" />
          <span className="product-life-fish product-life-fish-b" />
          <span className="product-life-fish product-life-fish-c" />
          <span className="product-life-jelly product-life-jelly-a" />
          <span className="product-life-jelly product-life-jelly-b" />
        </div>
        <div className="product-container product-hero-grid">
          <div className="product-hero-copy">
            <p className="product-kicker">
              {isZh ? "数据与代码如海" : "An ocean of data and code"}
            </p>
            <h1>
              {isZh ? (
                <>
                  潜入深海，
                  <br />
                  <span>你不必亲自下潜。</span>
                </>
              ) : (
                <>
                  Dive into the deep
                  <br />
                  <span>so you don&apos;t have to.</span>
                </>
              )}
            </h1>
            <p>
              {isZh
                ? "Codewhale 把大模型的杠杆交给普通人：在你的终端里读取仓库、修改文件、运行检查、留下收据。不必已经是程序员，也能把东西做出来——运行在你自己的机器上。"
                : "Codewhale gives ordinary people the leverage of LLMs to build things. In your terminal it reads the repo, edits files, runs checks, and leaves a receipt — without assuming you already speak code. It runs on your machine."}
            </p>
            <div className="product-actions">
              <Link href={`/${locale}/install`} className="product-button product-button-primary">
                {isZh ? "安装" : "Install"}
              </Link>
              <Link href={`/${locale}/docs`} className="product-button">
                {isZh ? "文档" : "Docs"}
              </Link>
              <a href={REPO} className="product-button">
                GitHub
              </a>
            </div>
            <div className="product-install">
              <InstallCodeBlock
                cmd="npm install -g codewhale"
                copyLabel={isZh ? "复制" : "Copy"}
                copiedLabel={isZh ? "已复制 ✓" : "Copied ✓"}
              />
            </div>
            <p className="product-facts">
              {publishedRelease
                ? isZh
                  ? `最新发布 ${publishedRelease.tag}`
                  : `Latest release ${publishedRelease.tag}`
                : isZh
                  ? "发布状态暂不可用"
                  : "Release status unavailable"}{" "}
              <span>·</span>{" "}
              {isZh
                ? `${sourceIsPublished ? "当前源码" : "源码候选版"} v${sourceVersion}：`
                : `${sourceIsPublished ? "Current source" : "Source candidate"} v${sourceVersion}: `}
              {providerCount} {isZh ? "个提供商路由" : "provider routes"}{" "}
              <span>·</span> {facts.license ?? "MIT"}
            </p>
          </div>

          <figure className="product-shot">
            <Image
              src="/codewhale-tui.png"
              alt={
                isZh
                  ? `Codewhale v${sourceVersion} 的全新终端会话，使用本地 Ollama 路由且没有空的 Work 栏`
                  : `Fresh Codewhale v${sourceVersion} terminal session using a local Ollama route, with no empty Work bar`
              }
              width={1280}
              height={720}
              sizes="(max-width: 900px) calc(100vw - 2rem), 58vw"
              priority
            />
            <figcaption>
              {isZh
                ? `v${sourceVersion} ${sourceIsPublished ? "已发布版" : "源码候选"} · 本地 Ollama 路由 · Plan / Act / Operate`
                : `v${sourceVersion} ${sourceIsPublished ? "published release" : "source candidate"} · local Ollama route · Plan / Act / Operate`}
            </figcaption>
          </figure>
        </div>
      </section>

      <section className="product-proof">
        <div className="product-container product-proof-grid">
          <h2>
            {isZh ? (
              <>终端原生的水下壳。模型与提供商中立。本地优先。</>
            ) : (
              <>An underwater terminal shell. Model-neutral. Local-first.</>
            )}
          </h2>
          <p>
            {isZh
              ? "连接你已有的托管、网关或本地模型。Codewhale 在你的机器上运行；模型是可选择的组件，不是产品本身。Plan / Act / Operate 与明确的审批边界，让深潜也保持可控。"
              : "Bring the hosted, gateway, or local model you already use. Codewhale runs on your machine and treats the model as a selectable component—not the product. Plan / Act / Operate and explicit permission postures keep the deep dive under your control."}
          </p>
        </div>
      </section>

      <section className="product-workflow">
        <div className="product-container">
          <h2>
            {isZh ? "从任务到经过验证的改动。" : "From task to verified change."}
          </h2>
          <ol className="product-workflow-steps">
            {WORKFLOW.map((step, index) => {
              const [title, description] = isZh ? step.zh : step.en;
              return (
                <li key={title}>
                  <span>{String(index + 1).padStart(2, "0")}</span>
                  <h3>{title}</h3>
                  <p>{description}</p>
                </li>
              );
            })}
          </ol>
          <div className="product-receipt" aria-label={isZh ? "工作流程示例" : "Example work receipt"}>
            <span>$ codewhale exec &quot;fix the failing test&quot;</span>
            <span>inspect&nbsp;&nbsp; repository and instructions</span>
            <span>act&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp; edit through the selected permission posture</span>
            <span>verify&nbsp;&nbsp;&nbsp; cargo test --locked</span>
            <strong>report&nbsp;&nbsp;&nbsp; checks passed · receipt saved</strong>
          </div>
        </div>
      </section>

      <section className="product-boundaries">
        <div className="product-container product-boundaries-grid">
          <div>
            <h2>
              {isZh ? (
                <>
                  你的模型。
                  <br />
                  <span>你的边界。</span>
                </>
              ) : (
                <>
                  Your model.
                  <br />
                  <span>Your boundaries.</span>
                </>
              )}
            </h2>
            <p>
              {isZh
                ? "显式选择模型、工作模式与权限姿态。Codewhale 不会把未知成本显示成零，也不会把预览功能说成已发布产品。"
                : "Choose the model, working mode, and permission posture explicitly. Unknown cost stays unknown, and preview surfaces stay labeled as such."}
            </p>
          </div>
          <dl className="product-boundary-list">
            <div>
              <dt>{providerCount} {isZh ? "个提供商路由" : "provider routes"}</dt>
              <dd>{isZh ? "托管、网关与本地模型" : "Hosted, gateway, and local models"}</dd>
            </div>
            <div>
              <dt>Plan · Act · Operate</dt>
              <dd>{isZh ? "从只读规划到自主执行" : "Read-only planning through autonomous operation"}</dd>
            </div>
            <div>
              <dt>Ask · Auto-Review · Full Access</dt>
              <dd>{isZh ? "为任务选择权限姿态" : "Choose the permission posture for the work"}</dd>
            </div>
            <div>
              <dt>TUI · exec · web · API</dt>
              <dd>{isZh ? "交互式与无头运行时界面" : "Interactive and headless runtime surfaces"}</dd>
            </div>
          </dl>
        </div>
      </section>

      <section className="product-surfaces">
        <div className="product-container">
          <h2>
            {isZh ? "在工作发生的地方使用运行时。" : "Use the runtime where the work happens."}
          </h2>
          <div className="product-surface-list">
            {SURFACES.map((surface) => {
              const [name, description] = isZh ? surface.zh : surface.en;
              return (
                <div key={name}>
                  <strong>{name}</strong>
                  <span>{description}</span>
                </div>
              );
            })}
          </div>
          <Link href={`/${locale}/runtime`}>
            {isZh ? "查看运行时界面与稳定性说明 →" : "See runtime surfaces and stability notes →"}
          </Link>
        </div>
      </section>

      <section className="product-install-band">
        <div className="product-container product-install-grid">
          <h2>{isZh ? "从一条命令开始。" : "Start with one command."}</h2>
          <div>
            <InstallCodeBlock
              cmd="npm install -g codewhale"
              copyLabel={isZh ? "复制" : "Copy"}
              copiedLabel={isZh ? "已复制 ✓" : "Copied ✓"}
            />
            <p>
              Cargo · {isZh ? "预编译包" : "Binaries"} · Docker · Nix · Windows · Android / Termux ·{" "}
              {isZh ? "中国镜像" : "China mirrors"}
            </p>
            <Link href={`/${locale}/install`}>
              {isZh ? "阅读安装指南 →" : "Read the install guide →"}
            </Link>
          </div>
        </div>
      </section>

      <section className="product-community">
        <div className="product-container product-community-grid">
          <div className="product-community-illustration" aria-hidden="true">
            <span />
            <Whale size={180} />
          </div>
          <div>
            <h2>{isZh ? "公开构建" : "Built in public"}</h2>
            <p>
              {isZh
                ? "Codewhale 采用 MIT 许可证，由来自不同时区、语言和技术背景的贡献者共同塑造。"
                : "MIT-licensed and shaped by contributors across runtimes, providers, platforms, documentation, and tests."}
            </p>
          </div>
          <nav aria-label={isZh ? "社区链接" : "Community links"}>
            <a href={REPO}>GitHub</a>
            <a href={`${REPO}/issues`}>Issues</a>
            <Link href={`/${locale}/contribute`}>{isZh ? "参与贡献" : "Contribute"}</Link>
            {publishedRelease ? (
              <a href={publishedRelease.url}>{publishedRelease.tag}</a>
            ) : (
              <a href={`${REPO}/releases`}>Releases</a>
            )}
          </nav>
        </div>
      </section>
    </div>
  );
}
