//! Cliente LSP de `tcode` (PLAN.md §2, §6, §11 M2): habla el protocolo por
//! stdio con servers externos (`pyright`, `rust-analyzer`...). No lanza
//! ningún server por su cuenta — `app` decide cuándo, según el lenguaje
//! del archivo activo.
//!
//! No sabe nada de terminal/`ratatui`; tampoco de `tcode-core` — expone
//! diagnósticos en coordenadas línea/columna simples, que quien lo use
//! traduce a lo que haga falta (resaltar en la vista de código, contar en
//! la statusbar...).

mod cliente;
mod completado;
mod diagnostico;
mod estado_logs;
mod formateo;
mod navegacion;
mod protocolo;
mod sincronizacion;

pub use completado::{parsear_completado, snippet_a_texto, EstadoCompletado, ItemCompletado, ItemVisible};
pub use cliente::{comando_para, Cliente, MensajeEntrante};
pub use diagnostico::{parsear_diagnosticos, DiagnosticoSimple, Severidad};
pub use estado_logs::EstadoLogsLsp;
pub use formateo::{parsear_ediciones_formateo, soporta_formateo, EdicionTexto};
pub use navegacion::{
    byte_de_posicion, byte_en_linea, parsear_ubicaciones, parsear_workspace_edit, posicion_en_linea, ruta_desde_uri,
    texto_hover, CapacidadesLsp, EdicionArchivo, Ubicacion,
};
pub use protocolo::{escribir_mensaje, leer_mensaje};
pub use sincronizacion::{cambio_entre, ModoSincronizacion};
