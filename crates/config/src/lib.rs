//! Configuración de `tcode`: ajustes generales (`config.toml`) y temas
//! visuales (`runtime/themes/*.toml`, PLAN.md §7). No sabe nada de
//! terminal/`ratatui` — la conversión de hex a un tipo de color de UI vive
//! en el crate `ui`. La config por proyecto (`.tcode/config.toml`, que
//! se mezcla encima de la global) vive en `proyecto`; la conversión de
//! temas en formato Helix (BACKLOG.md P3 #13), en `helix`.

mod color;
mod confianza;
mod config;
mod editor_tema;
mod helix;
mod panel_admin;
mod pliegues_guardados;
mod proyecto;
mod selector;
mod tema;

pub use color::{analizar_color_hex, formatear_color_hex, hsl_a_rgb, rgb_a_hsl};
pub use confianza::{sha256_hex, ConfigConfianza, ProyectoConfiable};
pub use config::{
    cargar, directorio_config, directorio_temas_usuario, guardar, guardar_en, recargar, ruta_config, ComandoLsp,
    Config, ConfigEditor, ConfigInterfaz, ConfigLenguajes, GuardadoAutomatico,
    SEGUNDOS_GUARDADO_AUTOMATICO_POR_DEFECTO,
};
pub use editor_tema::{
    campos_color, CampoColor, ComponenteHsl, EstadoEditorTema, ModoEdicion, PALETA_PREDEFINIDA,
};
pub use panel_admin::{
    indice_de, CampoEditor, CampoInterfaz, CampoTemas, EstadoPanelAdmin, FocoPanelAdmin,
    OpcionExterna, ResultadoBusquedaAdmin, Seccion,
};
pub use pliegues_guardados::{directorio_estado, huella, ruta_pliegues, PlieguesGuardados, MAX_ARCHIVOS};
pub use proyecto::{
    buscar_config_proyecto, cargar_config_proyecto, directorio_inicio_proyecto, mezclar_toml, ConfigProyecto,
};
pub use selector::{EstadoSelectorTema, FiltroTipoTema};
pub use tema::{
    cargar_tema, descubrir_temas_usuario, duplicar_tema_para_editar, guardar_tema, tema_por_defecto, EstiloToken,
    InfoTema, InfoTemaListado, ResultadoDuplicarTema, Tema, TemaBusqueda, TemaDiagnosticos, TemaGit, TemaSintaxis,
    TemaStatusbar, TemaUi, TEMAS_EMBEBIDOS, TEMA_POR_DEFECTO,
};
