//! Paleta de comandos de `tcode` (`Ctrl+Shift+P`, PLAN.md §4): un registro
//! fijo de comandos invocables por nombre y el estado de la paleta que los
//! filtra con `tcode-fuzzy` mientras se escribe. No sabe nada de
//! terminal/`ratatui` — el crate `ui` la dibuja, `app` decide qué hacer
//! con el id de comando que devuelve.

mod paleta;
mod registro;

pub use paleta::{EstadoPaleta, ResultadoPaleta};
pub use registro::{comandos_disponibles, Comando};
