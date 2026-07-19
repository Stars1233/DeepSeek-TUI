<!-- source: README.md sha256:f76ebd048ff1 -->
# Codewhale

**Un runtime. Todos los modelos. Tu máquina.**

Codewhale es un agente de código para tu terminal. Funciona con cualquier
modelo; los modelos abiertos primero. Le das un proveedor, un modelo y una
tarea: lee tu código, edita archivos, ejecuta comandos, verifica su trabajo y
se detiene cuando la tarea queda lista o te necesita. Cambia de modelo a
mitad de tarea con `/model`. Usa la TUI para el trabajo interactivo y
`codewhale exec` para scripts y CI. Rust, MIT, corre en tu máquina.

**Por qué Codewhale:**
- **Sin lock-in.** DeepSeek, Claude, GPT, Kimi, GLM, más de 30 proveedores, y
  tu propio vLLM, SGLang u Ollama — sin key — corren por un solo runtime y un
  solo conjunto de herramientas. Los presupuestos de contexto y los precios
  vienen de la ruta real. Un precio desconocido se muestra como desconocido,
  nunca como $0.
- **Seguro por construcción.** El modo Plan es de solo lectura. Las
  aprobaciones controlan cada llamada riesgosa. El sandbox del sistema
  operativo resiste — Seatbelt, Landlock, seccomp, bwrap. El
  `constitution.json` de un repo se compila en bloqueos de escritura que ni
  siquiera Full Access puede saltarse.
- **Trabajo que sobrevive.** Los fleets registran cada paso en un libro mayor
  de solo agregado; `fleet resume` retoma donde te detuviste. Cada turno deja
  un recibo que puedes inspeccionar.

Nació como `deepseek-tui`. Su comunidad necesitaba más proveedores, así que
construimos un runtime donde el modelo es un componente, no el producto.

[English](README.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja-JP.md) · [Tiếng Việt](README.vi.md) · [한국어](README.ko-KR.md) · [Português](README.pt-BR.md) · [codewhale.net](https://codewhale.net/) · [Docs](docs) · [Changelog](CHANGELOG.md)

[![CI](https://github.com/Hmbown/CodeWhale/actions/workflows/ci.yml/badge.svg)](https://github.com/Hmbown/CodeWhale/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/codewhale-cli?label=crates.io)](https://crates.io/crates/codewhale-cli)
[![npm](https://img.shields.io/npm/v/codewhale?label=npm)](https://www.npmjs.com/package/codewhale)

![Codewhale ejecutándose en una terminal](assets/screenshot.png)

## Instalación

```bash
npm install -g codewhale
```

Cargo, Docker, Nix, Scoop, archivos precompilados, Android/Termux y un espejo
en CNB para quienes no pueden acceder a GitHub están cubiertos en
[docs/INSTALL.md](docs/INSTALL.md). ¿Vienes de `deepseek-tui`? Tu configuración
y tus sesiones se conservan — mira [docs/REBRAND.md](docs/REBRAND.md).

## Uso

```bash
codewhale auth set --provider deepseek   # or export ANTHROPIC_API_KEY, etc.
codewhale                                # open the TUI
codewhale exec "fix the failing test"    # headless
codewhale web                            # local browser client on 127.0.0.1
```

En la TUI: `/model` cambia proveedor y modelo juntos, `/fleet` ejecuta un
equipo de workers, `/restore` deshace un turno, `Tab` cicla entre
Plan / Act / Operate, `Shift+Tab` cicla la postura de aprobación
Ask / Auto-Review / Full Access, y `!` ejecuta un comando de shell por la ruta
normal de aprobación.

## Para saber más

- [docs/PROVIDERS.md](docs/PROVIDERS.md) — cada ruta de proveedor: alojada,
  gateway y local
- [docs/FLEET.md](docs/FLEET.md) — fleets, el libro mayor y resume
- [docs/CONFIGURATION.md](docs/CONFIGURATION.md) — `config.toml`, hooks y la
  constitution
- [docs/WEB.md](docs/WEB.md) — cliente de navegador integrado solo en loopback
  y su límite de autenticación de un solo uso

Todo lo demás — modos, atajos de teclado, detalles del sandbox, MCP, la API
del runtime, arquitectura — está en [docs](docs) y en
[codewhale.net](https://codewhale.net/).

## Contribuir

Todo feedback es un regalo. Issues, PRs, pasos de reproducción, logs,
solicitudes de features y primeras contribuciones: todo eso es trabajo real del
proyecto aquí. Cuando un PR no se puede fusionar tal cual, los mantenedores
rescatan lo que funciona y el autor conserva su crédito — en el commit, en el
changelog y en [docs/CONTRIBUTORS.md](docs/CONTRIBUTORS.md). Si falta un modelo
o proveedor que usas, o algo se rompe en tu máquina, decírnoslo es lo más útil
que puedes hacer.

- [Issues abiertos](https://github.com/Hmbown/CodeWhale/issues) — las buenas
  primeras contribuciones viven aquí
- [CONTRIBUTING.md](CONTRIBUTING.md) — setup de desarrollo y flujo de PRs
- [docs/CONTRIBUTORS.md](docs/CONTRIBUTORS.md) — todas las personas que le han
  dado forma a esto
- [Invítame un café](https://www.buymeacoffee.com/hmbown)

Gracias a [DeepSeek](https://github.com/deepseek-ai) por los modelos y el apoyo
que dieron inicio al proyecto, a [DataWhale](https://github.com/datawhalechina)
🐋 por recibirnos en la familia Whale Brother, y a
[OpenWarp](https://github.com/zerx-lab/warp) y
[Open Design](https://github.com/nexu-io/open-design) por colaborar en la
experiencia de agente en terminal.

## Licencia

[MIT](LICENSE). Proyecto comunitario independiente; sin afiliación con ningún
proveedor de modelos.

[![Star History Chart](https://api.star-history.com/chart?repos=Hmbown/CodeWhale&type=date&legend=top-left)](https://www.star-history.com/?repos=Hmbown%2FCodeWhale&type=date)
