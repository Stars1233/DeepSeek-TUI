<!-- source: README.md sha256:a14f7d3aa7d1 -->
# Codewhale

**Um runtime. Modelos hospedados e locais compatíveis. Sua máquina.**

O Codewhale é um agente de código para o seu terminal. Funciona com modelos
hospedados e locais compatíveis; modelos abertos em primeiro lugar. Você informa um provedor, um
modelo e uma tarefa: ele lê seu código, edita arquivos, executa comandos,
verifica o próprio trabalho e para quando a tarefa termina ou quando precisa
de você. Troque de modelo no meio da tarefa com `/model`. Use a TUI para
trabalho interativo e `codewhale exec` para scripts e CI. Rust, MIT, roda na
sua máquina.

**Por que Codewhale:**
- **Sem lock-in.** DeepSeek, Claude, GPT, Kimi, GLM, mais de 30 provedores, e
  seu próprio vLLM, SGLang ou Ollama — sem key — rodam por um único runtime e
  um único conjunto de ferramentas. Orçamentos de contexto e preços vêm da
  rota real. Um preço desconhecido aparece como desconhecido, nunca como $0.
- **Seguro por construção.** O modo Plan é somente leitura. Aprovações
  controlam cada chamada arriscada. O Codewhale só informa um sandbox de
  comandos do sistema operacional quando ele realmente envolve o comando:
  Seatbelt no macOS quando disponível e bubblewrap opcional no Linux quando
  instalado. O Windows atualmente informa que não há sandbox. O
  `constitution.json` de um repositório é compilado em bloqueios de escrita
  que nem o Full Access consegue pular.
- **Trabalho que sobrevive.** Fleets registram cada passo em um livro-razão
  de apenas inclusão; `fleet resume` retoma de onde você parou. Cada turno
  deixa um recibo que você pode inspecionar.

Nasceu como `deepseek-tui`. Sua comunidade precisava de mais provedores,
então construímos um runtime em que o modelo é um componente, não o produto.

[English](README.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja-JP.md) · [Tiếng Việt](README.vi.md) · [한국어](README.ko-KR.md) · [Español](README.es-419.md) · [codewhale.net](https://codewhale.net/) · [Docs](docs) · [Changelog](CHANGELOG.md)

[![CI](https://github.com/Hmbown/CodeWhale/actions/workflows/ci.yml/badge.svg)](https://github.com/Hmbown/CodeWhale/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/codewhale-cli?label=crates.io)](https://crates.io/crates/codewhale-cli)
[![npm](https://img.shields.io/npm/v/codewhale?label=npm)](https://www.npmjs.com/package/codewhale)

![Codewhale rodando em um terminal](assets/screenshot.png)

## Instalação

```bash
npm install -g codewhale
```

Cargo, Docker, Nix, Scoop, arquivos pré-compilados, Android/Termux e um
espelho CNB para quem não consegue acessar o GitHub estão cobertos em
[docs/INSTALL.md](docs/INSTALL.md). Vindo do `deepseek-tui`? Sua configuração
e suas sessões são preservadas — veja [docs/REBRAND.md](docs/REBRAND.md).

## Uso

```bash
codewhale auth set --provider deepseek   # or export ANTHROPIC_API_KEY, etc.
codewhale                                # open the TUI
codewhale exec "fix the failing test"    # headless
codewhale web                            # local browser client on 127.0.0.1
```

Na TUI: `/model` troca provedor e modelo juntos, `/fleet` executa uma equipe
de workers e `/restore` desfaz um turno. Quando o compositor está ocioso, `Tab`
cicla entre Plan / Act / Operate e `Shift+Tab` cicla a postura de permissão Ask
/ Auto-Review / Full Access. `!` executa um comando de shell pelo caminho normal
de aprovação.

## Saiba mais

- [docs/PROVIDERS.md](docs/PROVIDERS.md) — cada rota de provedor: hospedada,
  gateway e local
- [docs/FLEET.md](docs/FLEET.md) — fleets, o livro-razão e resume
- [docs/CONFIGURATION.md](docs/CONFIGURATION.md) — `config.toml`, hooks e a
  constitution
- [docs/WEB.md](docs/WEB.md) — cliente de navegador incorporado apenas em
  loopback e sua fronteira de autenticação de uso único

Todo o resto — modos, atalhos de teclado, detalhes do sandbox, MCP, a API do
runtime, arquitetura — está em [docs](docs) e em
[codewhale.net](https://codewhale.net/).

## Contribuindo

Todo feedback é um presente. Issues, PRs, passos de reprodução, logs, pedidos
de funcionalidade e primeiras contribuições — tudo isso é trabalho real do
projeto aqui. Quando um PR não pode ser mesclado como está, os mantenedores
aproveitam o que funciona e o autor continua creditado — no commit, no
changelog e em [docs/CONTRIBUTORS.md](docs/CONTRIBUTORS.md). Se um modelo ou
provedor que você usa está faltando, ou se algo quebra na sua máquina, nos
contar é a coisa mais útil que você pode fazer.

- [Issues abertas](https://github.com/Hmbown/CodeWhale/issues) — boas
  primeiras contribuições moram aqui
- [CONTRIBUTING.md](CONTRIBUTING.md) — setup de desenvolvimento e fluxo de PR
- [docs/CONTRIBUTORS.md](docs/CONTRIBUTORS.md) — todo mundo que ajudou a
  moldar o projeto
- [Me pague um café](https://www.buymeacoffee.com/hmbown)

Obrigado à [DeepSeek](https://github.com/deepseek-ai) pelos modelos e pelo
apoio que deram início ao projeto, à
[DataWhale](https://github.com/datawhalechina) 🐋 por nos receber na família
Whale Brother, e a [OpenWarp](https://github.com/zerx-lab/warp) e
[Open Design](https://github.com/nexu-io/open-design) pela colaboração na
experiência de agente no terminal.

## Licença

[MIT](LICENSE). Projeto comunitário independente; sem afiliação com nenhum
provedor de modelos.

[![Gráfico de Star History](https://api.star-history.com/chart?repos=Hmbown/CodeWhale&type=date&legend=top-left)](https://www.star-history.com/?repos=Hmbown%2FCodeWhale&type=date)
