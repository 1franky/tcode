# tcode

Editor TUI portable con atajos estilo VSCode, potencia de Neovim (LSP nativo,
tree-sitter, buffers múltiples, splits) y comandos en español para
memorización rápida.

100% operable por teclado, corre en terminal (Linux, Windows portable,
macOS). Ver [MANUAL.md](./MANUAL.md) para la guía de uso, o
[PLAN.md](./PLAN.md) para el diseño completo.

## Estado

M0 a M4 completos (última release: v0.4.2): fundamentos, temas/atajos/
sintaxis/explorador, paleta de comandos/buscador difuso/splits/cliente
LSP, búsqueda-reemplazo/vista Markdown/vista CSV/multi-cursor, y panel de
administración completo — 10+ temas con selector y editor visual,
editor de atajos con detección de conflictos, LSP configurable desde la
UI, y los **13 lenguajes objetivo** con resaltado de sintaxis (Rust,
Python, JavaScript/TypeScript, Go, Java, Kotlin, C/C++, C#, Ruby, PHP,
HTML/CSS, Markdown, SQL). Ver [PLAN.md](./PLAN.md) §11 para el roadmap
completo, y [PRUEBAS.md](./PRUEBAS.md) para el checklist de pruebas
manuales antes de cada release.

## Instalación

### Linux y macOS

```bash
curl -fsSL https://raw.githubusercontent.com/1franky/tcode/main/install/linux.sh | bash
```

Instala el binario en `~/.local/bin/tcode` sin `sudo`, y lo añade al `PATH`
si hace falta.

### Windows

```powershell
irm https://raw.githubusercontent.com/1franky/tcode/main/install/windows.ps1 | iex
```

Instala en modo portable en `%LOCALAPPDATA%\tcode`, sin necesidad de
administrador.

Ambos scripts descargan el binario de la
[última release](https://github.com/1franky/tcode/releases) — publicada
automáticamente al taggear una versión en `main`
(`.github/workflows/release.yml`). Los binarios de Linux son estáticos
(musl): no dependen de la versión de glibc del sistema. `tcode --version`
(o `-v`) confirma qué versión quedó instalada.

### Compilar desde el código fuente

```bash
git clone https://github.com/1franky/tcode.git
cd tcode
cargo build --release
# binario en target/release/tcode
```

## Flujo de ramas

- `main`: rama protegida, siempre desplegable. Solo recibe cambios vía Pull
  Request desde `develop`. Cada tag `v*.*.*` en `main` dispara la
  compilación y publicación automática de binarios (Linux, macOS, Windows).
- `develop`: rama de integración donde se desarrolla el día a día, recibida
  vía Pull Request desde ramas `feature/*`.

## Licencia

Distribuido bajo licencia dual [MIT](./LICENSE-MIT) o
[Apache-2.0](./LICENSE-APACHE), a elección de quien lo use — el estándar del
ecosistema Rust.
