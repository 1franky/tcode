//! Configuración de `tcode`: ajustes generales (`config.toml`) y temas
//! visuales (`runtime/themes/*.toml`, PLAN.md §7). No sabe nada de
//! terminal/`ratatui` — la conversión de hex a un tipo de color de UI vive
//! en el crate `ui`.

mod color;
mod config;
mod panel_admin;
mod selector;
mod tema;

pub use color::analizar_color_hex;
pub use config::{
    cargar, directorio_config, directorio_temas_usuario, guardar, recargar, ruta_config, Config,
    ConfigEditor, ConfigInterfaz, ConfigLenguajes,
};
pub use panel_admin::{
    indice_de, CampoEditor, CampoTemas, EstadoPanelAdmin, FocoPanelAdmin, OpcionExterna,
    ResultadoBusquedaAdmin, Seccion,
};
pub use selector::{EstadoSelectorTema, FiltroTipoTema};
pub use tema::{
    cargar_tema, duplicar_tema_para_editar, tema_por_defecto, EstiloToken, InfoTema,
    ResultadoDuplicarTema, Tema, TemaBusqueda, TemaDiagnosticos, TemaGit, TemaSintaxis,
    TemaStatusbar, TemaUi, TEMAS_EMBEBIDOS, TEMA_POR_DEFECTO,
};
