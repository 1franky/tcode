//! Resaltado de sintaxis de `tcode` vía tree-sitter (PLAN.md §6, §11):
//! parsing con las gramáticas oficiales de cada lenguaje y mapeo a los
//! nombres de token que usa `tcode_config::TemaSintaxis` para colorearlos.
//! No sabe nada de terminal/`ratatui` — el crate `ui` convierte los
//! [`Token`] resultantes en `Span`s coloreados.

mod lenguaje;
mod plegado;
mod resaltador;
mod simbolos;

pub use lenguaje::Lenguaje;
pub use plegado::{rangos_por_indentacion, RangoPlegable};
pub use resaltador::{Resaltador, Token, NOMBRES_RESALTADO};
pub use simbolos::{Simbolo, SimboloEsquema};
