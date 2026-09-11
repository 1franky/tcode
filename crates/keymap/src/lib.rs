//! Sistema de atajos de `tcode` (PLAN.md §4): parseo de combinaciones y
//! secuencias encadenadas (chords) desde TOML, resolución en tiempo real
//! tecla-a-tecla, y detección de conflictos entre atajos.
//!
//! No ejecuta comandos — solo produce el *nombre* del comando en español
//! (`"archivo.guardar"`) que le corresponde a una secuencia de teclas. La
//! ejecución real (el dispatcher comando -> acción) vive en el crate `app`
//! por ahora; se moverá a un crate `commands` dedicado cuando llegue la
//! paleta de comandos en M2.

mod combinacion;
mod keymap;
mod resolvedor;

pub use combinacion::{desde_evento, parsear_atajo, parsear_combinacion, Combinacion, Direccion, Tecla};
pub use keymap::{cargar, detectar_conflictos, keymap_por_defecto, ruta_keymap_usuario, Conflicto, Keymap};
pub use resolvedor::{Resolucion, Resolvedor};
