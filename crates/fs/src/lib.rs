//! Explorador de archivos de `tcode` (PLAN.md §3, §4 `Ctrl+B`): árbol de
//! directorios navegable, con carga perezosa de carpetas. No sabe nada de
//! terminal/`ratatui` — el crate `ui` lo dibuja.
//!
//! El buscador difuso (`Ctrl+P`) y los watchers de archivos, también
//! descritos como responsabilidad de este crate en PLAN.md §3, llegan en
//! M2.

mod explorador;
mod nodo;

pub use explorador::{raiz_por_defecto, Explorador};
pub use nodo::Nodo;
