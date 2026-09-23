//! Explorador de archivos de `tcode` (PLAN.md §3): árbol de directorios
//! navegable con carga perezosa (`Ctrl+B`), y buscador difuso sobre la
//! lista completa de archivos del proyecto (`Ctrl+P`). No sabe nada de
//! terminal/`ratatui` — el crate `ui` los dibuja.
//!
//! Los watchers de archivos, también descritos como responsabilidad de
//! este crate en PLAN.md §3, llegan más adelante (recarga en caliente de
//! archivos modificados fuera del editor).

mod buscador;
mod estado_prompt;
mod explorador;
mod nodo;

pub use buscador::{listar_archivos_recursivo, BuscadorArchivos, ResultadoBusqueda};
pub use estado_prompt::{EstadoConfirmarBorrado, EstadoPromptExplorador, ModoPromptExplorador};
pub use explorador::{raiz_por_defecto, Explorador};
pub use nodo::Nodo;
