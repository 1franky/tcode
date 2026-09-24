//! Explorador de archivos de `tcode` (PLAN.md §3): árbol de directorios
//! navegable con carga perezosa (`Ctrl+B`), y buscador difuso sobre la
//! lista completa de archivos del proyecto (`Ctrl+P`). No sabe nada de
//! terminal/`ratatui` — el crate `ui` los dibuja.
//!
//! También los indicadores de git del gutter ([`DiffGit`], BACKLOG.md
//! P2 #6): leer la versión de `HEAD` de un archivo y calcular qué líneas
//! cambiaron respecto de ella.
//!
//! Los watchers de archivos, también descritos como responsabilidad de
//! este crate en PLAN.md §3, llegan más adelante (recarga en caliente de
//! archivos modificados fuera del editor).

mod buscador;
mod estado_prompt;
mod explorador;
mod git;
mod nodo;

pub use buscador::{listar_archivos_recursivo, BuscadorArchivos, ResultadoBusqueda};
pub use estado_prompt::{EstadoConfirmarBorrado, EstadoPromptExplorador, ModoPromptExplorador};
pub use explorador::{partes_ruta_en_proyecto, raiz_por_defecto, Explorador};
pub use git::{calcular_marcas, leer_base_head, DiffGit, MarcaGit};
pub use nodo::Nodo;
