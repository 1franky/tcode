//! Paleta de comandos de `tcode` (`Ctrl+Shift+P`, PLAN.md §4): un registro
//! fijo de comandos invocables por nombre y el estado de la paleta que los
//! filtra con `tcode-fuzzy` mientras se escribe. No sabe nada de
//! terminal/`ratatui` — el crate `ui` la dibuja, `app` decide qué hacer
//! con el id de comando que devuelve. También el selector de símbolos
//! del archivo (`Ctrl+K .`), que es la misma forma de lista filtrable.

mod paleta;
mod registro;
mod selector_simbolos;

pub use paleta::{EstadoPaleta, ResultadoPaleta};
pub use registro::{comandos_disponibles, Comando};
pub use selector_simbolos::{EntradaSimbolo, EstadoSelectorSimbolos, ResultadoSimbolo};
