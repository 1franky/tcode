# tcode

Editor TUI portable con atajos estilo VSCode, potencia de Neovim (LSP nativo,
tree-sitter, buffers múltiples, splits) y comandos en español para
memorización rápida.

100% operable por teclado, corre en terminal (Linux, Windows portable,
macOS). Ver el diseño completo en [PLAN.md](./PLAN.md).

## Estado

En diseño / fase M0 (fundamentos). Aún no hay código publicado — ver
[PLAN.md](./PLAN.md) para la visión, arquitectura y roadmap.

## Flujo de ramas

- `main`: rama protegida, siempre desplegable. Solo recibe cambios vía Pull
  Request desde `develop`.
- `develop`: rama de integración donde se desarrolla el día a día.

## Licencia

Distribuido bajo licencia dual [MIT](./LICENSE-MIT) o
[Apache-2.0](./LICENSE-APACHE), a elección de quien lo use — el estándar del
ecosistema Rust.
