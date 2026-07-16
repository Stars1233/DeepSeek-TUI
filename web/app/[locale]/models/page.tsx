import Link from "next/link";
import { getFacts } from "@/lib/facts";
import { buildPageMetadata } from "@/lib/page-meta";

export async function generateMetadata({ params }: { params: Promise<{ locale: string }> }) {
  const { locale } = await params;
  const isZh = locale === "zh";
  return buildPageMetadata({
    path: "/models",
    locale,
    title: isZh ? "模型与提供商 · Codewhale" : "Models & providers · Codewhale",
    description: isZh
      ? "Codewhale 托管与本地提供商路由的配置方式和完整注册表。"
      : "Configuration guidance and the full registry for Codewhale's hosted and local provider routes.",
  });
}

export default async function ModelsPage({ params }: { params: Promise<{ locale: string }> }) {
  const { locale } = await params;
  const isZh = locale === "zh";
  const p = (path: string) => (isZh ? `/zh${path}` : `/en${path}`);
  const facts = await getFacts();
  const providerDocs = "https://github.com/Hmbown/CodeWhale/blob/main/docs/PROVIDERS.md";

  const setupPatterns = isZh
    ? [
        {
          title: "DeepSeek",
          detail: `新配置默认使用 ${facts.defaultModel ?? "deepseek-v4-pro"}。可以通过 --provider、/provider 或 CODEWHALE_PROVIDER 明确选择其他路由。`,
          reference: "DEEPSEEK_API_KEY",
        },
        {
          title: "本地运行时",
          detail: "vLLM、SGLang 和 Ollama 可以直连 localhost。按需配置端点和模型；本地部署通常不需要 API 密钥。",
          reference: "vllm · sglang · ollama",
        },
        {
          title: "OpenRouter",
          detail: "OpenRouter 用一个托管端点访问多个模型。提供商和模型仍由你明确选择，不会根据模型名称或提示词自动切换。",
          reference: "OPENROUTER_API_KEY",
        },
      ]
    : [
        {
          title: "DeepSeek",
          detail: `New configurations default to ${facts.defaultModel ?? "deepseek-v4-pro"}. Select another route explicitly with --provider, /provider, or CODEWHALE_PROVIDER.`,
          reference: "DEEPSEEK_API_KEY",
        },
        {
          title: "Local runtimes",
          detail: "vLLM, SGLang, and Ollama can connect directly to localhost. Set an endpoint and model as needed; local deployments usually require no API key.",
          reference: "vllm · sglang · ollama",
        },
        {
          title: "OpenRouter",
          detail: "OpenRouter provides one hosted endpoint for many models. You still select the provider and model explicitly; model names and prompts do not switch routes.",
          reference: "OPENROUTER_API_KEY",
        },
      ];

  return (
    <div className="models-page">
      <section className="community-welcome">
        <div className="portal-current" aria-hidden="true" />
        <div className="portal-container community-welcome-inner">
          <div className="eyebrow">{isZh ? "模型与提供商" : "Models and providers"}</div>
          <h1>{isZh ? "选择模型和提供商。" : "Choose a model and provider."}</h1>
          <p>
            {isZh
              ? `Codewhale 包含 ${facts.providers.length} 条提供商路由。提供商、模型和端点都是明确的配置；每条路由使用同一个本地运行时、工具和审批边界。托管提供商使用你配置的凭据，本地 vLLM、SGLang 和 Ollama 端点通常不需要密钥。`
              : `Codewhale includes ${facts.providers.length} provider routes. The provider, model, and endpoint are explicit configuration, and every route uses the same local runtime, tools, and approval boundaries. Hosted providers use credentials you configure; local vLLM, SGLang, and Ollama endpoints usually require no key.`}
          </p>
          <div className="portal-actions">
            <Link href={providerDocs} className="portal-button portal-button-primary">
              {isZh ? "阅读提供商文档" : "Read the provider docs"}
            </Link>
            <Link href={p("/install")} className="portal-button portal-button-secondary">
              {isZh ? "安装 Codewhale" : "Install Codewhale"}
            </Link>
          </div>
        </div>
      </section>

      <section className="portal-section">
        <div className="portal-container portal-section-grid">
          <div className="portal-section-copy">
            <span>{isZh ? "配置方式" : "Configuration paths"}</span>
            <h2>{isZh ? "常用提供商路由" : "Common provider routes"}</h2>
            <p>
              {isZh
                ? "托管提供商的密钥可以通过 codewhale auth set 保存，也可以使用文档中列出的配置项或环境变量。提供商和模型分别选择；模型名称不会隐式改变提供商。"
                : "Hosted-provider credentials can be saved with codewhale auth set or supplied through documented configuration and environment variables. Provider and model selection remain separate; a model name never changes the provider implicitly."}
            </p>
          </div>
          <div className="portal-topic-list">
            {setupPatterns.map((pattern) => (
              <Link key={pattern.title} href={p("/docs#providers")}>
                <strong>{pattern.title}</strong>
                <span>{pattern.detail}</span>
                <span className="font-mono break-all">{pattern.reference}</span>
              </Link>
            ))}
          </div>
        </div>
      </section>

      <section className="portal-section portal-section-muted">
        <div className="portal-container">
          <div className="portal-docs-heading">
            <div>
              <span>{isZh ? "仓库数据" : "Repository data"}</span>
              <h2>{isZh ? "内置提供商注册表" : "Built-in provider registry"}</h2>
            </div>
            <Link href={providerDocs}>{isZh ? "打开源文档 ↗" : "Open the source document ↗"}</Link>
          </div>
          <p className={`mb-6 max-w-3xl text-sm text-ink-soft ${isZh ? "leading-[1.9] tracking-wide" : "leading-relaxed"}`}>
            {isZh
              ? "下面的列表由仓库中的提供商注册表生成，并随发布更新。这里列出路由 ID 和常用认证环境变量；传输协议、默认端点、模型解析和完整认证优先级以 docs/PROVIDERS.md 为准。"
              : "This list is generated from the provider registry in the repository and updated with releases. It shows route IDs and common authentication environment variables; docs/PROVIDERS.md is the source for wire protocols, default endpoints, model resolution, and full authentication precedence."}
          </p>
          <ul className="grid gap-x-10 sm:grid-cols-2 hairline-t">
            {facts.providers.map((provider) => (
              <li key={provider.id} className="py-4 hairline-b min-w-0">
                <div className="font-display text-base">{provider.label}</div>
                <div className="mt-1 font-mono text-[0.68rem] text-indigo break-all">{provider.id}</div>
                <div className="mt-1 font-mono text-[0.64rem] text-ink-mute break-all leading-relaxed">{provider.env}</div>
              </li>
            ))}
          </ul>
          <p className={`mt-6 max-w-3xl text-sm text-ink-soft ${isZh ? "leading-[1.9] tracking-wide" : "leading-relaxed"}`}>
            {isZh ? (
              <>
                如果需要的提供商尚未列出，请先{" "}
                <Link href="https://github.com/Hmbown/CodeWhale/issues/new/choose" className="body-link">提交 issue</Link>
                ，说明端点、认证方式和模型能力；也欢迎发送包含注册表、文档和测试的 pull request。
              </>
            ) : (
              <>
                If a provider is missing, please{" "}
                <Link href="https://github.com/Hmbown/CodeWhale/issues/new/choose" className="body-link">file an issue</Link>
                {" "}with its endpoint, authentication method, and model capabilities. Pull requests that update the registry, documentation, and tests are welcome too.
              </>
            )}
          </p>
        </div>
      </section>
    </div>
  );
}
