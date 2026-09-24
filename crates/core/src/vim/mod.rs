//! Modo VIM opcional (`config.editor.modo_vim`), sin nada de UI:
//! - `gramatica`: qué comando forman las teclas escritas (operador +
//!   conteo + movimiento/objeto de texto).
//! - `movimientos`: a dónde lleva cada movimiento y qué rango cubre cada
//!   objeto de texto.
//! - `ejecutor`: aplica los comandos sobre un `Editor` (Normal y Visual).
//! - `linea_comando`: el prompt `:` y el parseo de lo que se escribe ahí.
//!
//! El estado que persiste entre teclas vive en `crate::EstadoVim`; `app`
//! solo rutea teclas hacia acá y ejecuta los comandos `:` que tocan
//! paneles/pestañas/disco.

pub mod ejecutor;
pub mod gramatica;
pub mod linea_comando;
pub mod movimientos;

pub use ejecutor::{cancelar, ejecutar_tecla, refrescar_visual, salir_de_insertar, OpcionesVim};
pub use linea_comando::{ediciones_de_sustitucion, parsear as parsear_linea_comando, ComandoLinea, EstadoLineaComando, Sustitucion};
