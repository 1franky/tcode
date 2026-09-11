//! Configuración de `tcode`: ajustes generales (`config.toml`) y temas
//! visuales (`runtime/themes/*.toml`, PLAN.md §7). No sabe nada de
//! terminal/`ratatui` — la conversión de hex a un tipo de color de UI vive
//! en el crate `ui`.

mod color;
mod config;
mod tema;

pub use color::analizar_color_hex;
pub use config::{
    cargar, directorio_config, directorio_temas_usuario, guardar, recargar, ruta_config, Config,
    ConfigEditor, ConfigInterfaz,
};
pub use tema::{
    cargar_tema, tema_por_defecto, EstiloToken, Tema, TemaDiagnosticos, TemaGit, TemaSintaxis,
    TemaStatusbar, TemaUi, TEMA_POR_DEFECTO,
};
