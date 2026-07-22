# Codewhale

**One runtime. Supported hosted and local models. Your machine.**

Codewhale is a coding agent for your terminal. It works with supported hosted
and local models; open models first. Give it a provider, a model, and a task: it reads your
code, edits files, runs commands, checks its work, and stops when the job
is done or it needs you. Switch models mid-task with `/model`. Use the TUI
for interactive work, `codewhale exec` for scripts and CI. Rust, MIT,
runs on your machine.

**Why Codewhale:**
- **No lock-in.** DeepSeek, Claude, GPT, Kimi, GLM, 30+ providers, and your
  own vLLM, SGLang, or Ollama — no key required — run through one runtime
  and one toolset. Context budgets and prices come from the real route. An
  unknown price shows as unknown, never as $0.
- **Safe by construction.** Plan mode is read-only. Approvals gate every
  risky call. Codewhale reports an OS command sandbox only when it actually
  wraps the command: Seatbelt on macOS when available, and opt-in bubblewrap
  on Linux when installed. Windows currently reports none. A repo's
  `constitution.json` compiles into write holds that even Full Access cannot
  skip.
- **Work that survives.** Fleets record every step in an append-only
  ledger; `fleet resume` picks up where you stopped. Every turn leaves a
  receipt you can inspect.

Born as `deepseek-tui`. Its community needed more providers, so we built
one where the model is a component, not the product.

[简体中文](README.zh-CN.md) · [日本語](README.ja-JP.md) · [Tiếng Việt](README.vi.md) · [한국어](README.ko-KR.md) · [Español](README.es-419.md) · [Português](README.pt-BR.md) · [codewhale.net](https://codewhale.net/) · [Docs](docs) · [Changelog](CHANGELOG.md)

[![CI](https://github.com/Hmbown/CodeWhale/actions/workflows/ci.yml/badge.svg)](https://github.com/Hmbown/CodeWhale/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/codewhale-cli?label=crates.io)](https://crates.io/crates/codewhale-cli)
[![npm](https://img.shields.io/npm/v/codewhale?label=npm)](https://www.npmjs.com/package/codewhale)

![Codewhale running in a terminal](assets/screenshot.png)

## Install

```bash
npm install -g codewhale
```

Cargo, Docker, Nix, Scoop, prebuilt archives, Android/Termux, and a CNB mirror
for users who cannot reach GitHub are covered in
[docs/INSTALL.md](docs/INSTALL.md). Coming from `deepseek-tui`? Your config and
sessions carry over — see [docs/REBRAND.md](docs/REBRAND.md).

## Use

```bash
codewhale auth set --provider deepseek   # or export ANTHROPIC_API_KEY, etc.
codewhale                                # open the TUI
codewhale exec "fix the failing test"    # headless
codewhale web                            # local browser client on 127.0.0.1
```

In the TUI: `/model` switches provider and model together, `/fleet` runs a
team of workers, and `/restore` undoes a turn. When the composer is idle, `Tab`
cycles Plan / Act / Operate and `Shift+Tab` cycles the Ask / Auto-Review / Full
Access permission posture. `!` runs a shell command through the normal approval
path.

## Learn more

- [docs/PROVIDERS.md](docs/PROVIDERS.md) — every provider route: hosted,
  gateway, and local
- [docs/FLEET.md](docs/FLEET.md) — fleets, the ledger, and resume
- [docs/CONFIGURATION.md](docs/CONFIGURATION.md) — `config.toml`, hooks, and
  the constitution
- [docs/WEB.md](docs/WEB.md) — the loopback-only embedded browser client and
  its one-time authentication boundary

Everything else — modes, keybindings, sandbox details, MCP, the runtime API,
architecture — is in [docs](docs) and on
[codewhale.net](https://codewhale.net/).

## Contributing

All feedback is a gift. Issues, PRs, repro steps, logs, feature requests, and
first contributions are all real project work here. When a PR can't merge
as-is, maintainers harvest what works and the author stays credited — in the
commit, the changelog, and [docs/CONTRIBUTORS.md](docs/CONTRIBUTORS.md). If a
model or provider you use is missing, or something breaks on your machine,
telling us is the most useful thing you can do.

- [Open issues](https://github.com/Hmbown/CodeWhale/issues) — good first
  contributions live here
- [CONTRIBUTING.md](CONTRIBUTING.md) — dev setup and PR flow
- [docs/CONTRIBUTORS.md](docs/CONTRIBUTORS.md) — everyone who has shaped this
- [Buy me a coffee](https://www.buymeacoffee.com/hmbown)

Thanks to [DeepSeek](https://github.com/deepseek-ai) for the models and support
that started the project, [DataWhale](https://github.com/datawhalechina) 🐋 for
welcoming us into the Whale Brother family, and
[OpenWarp](https://github.com/zerx-lab/warp) and
[Open Design](https://github.com/nexu-io/open-design) for collaborating on the
terminal-agent experience.

## License

[MIT](LICENSE). Independent community project; not affiliated with any model
provider.

[![Star History Chart](https://api.star-history.com/chart?repos=Hmbown/CodeWhale&type=date&legend=top-left)](https://www.star-history.com/?repos=Hmbown%2FCodeWhale&type=date)
