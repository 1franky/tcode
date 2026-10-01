//! Bloques de cambios respecto de `HEAD` (BACKLOG.md P2 #25): ver el
//! diff del bloque del cursor, revertirlo e ir al siguiente/anterior. La
//! base es la misma que usa el gutter (`DiffGit`, ya leída en segundo
//! plano), y los bloques se calculan en el momento contra el texto
//! actual del buffer (`tcode_fs::bloques_git`): solo al usar un comando,
//! nunca por frame.

use tcode_core::Cursor;
use tcode_fs::{bloques_git, BloqueGit};
use tcode_ui::Layout as PanelLayout;

use crate::EstadoApp;

/// Máximo de líneas del popup de "ver cambio" (el resto se resume).
const MAX_LINEAS_POPUP: usize = 20;

/// El popup de "ver cambio": cada línea con si es agregada (`+`) o
/// borrada (`-`). Se cierra con cualquier tecla, como el hover.
pub type LineasCambio = Vec<(bool, String)>;

fn avisar(layout: &mut PanelLayout, texto: impl Into<String>) {
    layout.panel_activo_mut().mensaje_estado = Some(texto.into());
}

/// Los bloques del documento activo y su base, o un aviso de por qué no
/// hay (fuera de un repo, sin commitear, todavía leyendo).
fn bloques_del_activo(layout: &mut PanelLayout) -> Option<(String, String, Vec<BloqueGit>)> {
    let panel = layout.panel_activo();
    let Some(base) = panel.git.base().map(str::to_string) else {
        let motivo = if panel.editor.buffer().ruta().is_none() {
            "Git: el archivo no tiene nombre"
        } else if panel.git.pendiente() {
            "Git: todavía leyendo la versión de HEAD"
        } else {
            "Git: el archivo no está en HEAD (fuera de un repo o sin commitear)"
        };
        avisar(layout, motivo);
        return None;
    };
    let texto = panel.editor.buffer().a_texto();
    let bloques = bloques_git(&base, &texto);
    if bloques.is_empty() {
        avisar(layout, "Git: sin cambios respecto de HEAD");
        return None;
    }
    Some((base, texto, bloques))
}

/// El bloque que contiene la línea del cursor, o un aviso.
fn bloque_del_cursor(layout: &mut PanelLayout) -> Option<(String, String, BloqueGit)> {
    let (base, texto, bloques) = bloques_del_activo(layout)?;
    let linea = layout.editor_activo().cursor().linea;
    match bloques.into_iter().find(|b| b.contiene(linea)) {
        Some(bloque) => Some((base, texto, bloque)),
        None => {
            avisar(layout, "Git: la línea del cursor no tiene cambios");
            None
        }
    }
}

/// `git.ver_cambio`: las líneas borradas (`-`) y agregadas (`+`) del
/// bloque del cursor, en un popup.
pub fn ver(layout: &mut PanelLayout, estado: &mut EstadoApp) {
    let Some((base, texto, bloque)) = bloque_del_cursor(layout) else { return };
    let borradas = base[bloque.bytes_base.clone()].lines().map(|l| (false, l.to_string()));
    let agregadas = texto[bloque.bytes_actual.clone()].lines().map(|l| (true, l.to_string()));
    let mut lineas: LineasCambio = borradas.chain(agregadas).collect();
    if lineas.len() > MAX_LINEAS_POPUP {
        let resto = lineas.len() - MAX_LINEAS_POPUP + 1;
        lineas.truncate(MAX_LINEAS_POPUP - 1);
        lineas.push((true, format!("... y {resto} líneas más")));
    }
    estado.cambio_git = Some(lineas);
}

/// `git.revertir_cambio`: devuelve el bloque del cursor a como está en
/// `HEAD`, como una sola edición (un `Ctrl+Z` la deshace).
pub fn revertir(layout: &mut PanelLayout) {
    let Some((base, _, bloque)) = bloque_del_cursor(layout) else { return };
    let original = base[bloque.bytes_base.clone()].to_string();
    let editor = layout.editor_activo_mut();
    if editor.aplicar_ediciones(&[(bloque.bytes_actual.clone(), original)]) {
        avisar(layout, "Git: bloque revertido a HEAD (Ctrl+Z lo deshace)");
    }
}

/// `git.siguiente_cambio`/`git.anterior_cambio`: lleva el cursor al
/// principio del bloque siguiente (o anterior) a su línea, dando la
/// vuelta al llegar a un extremo.
pub fn saltar(adelante: bool, layout: &mut PanelLayout) {
    let Some((_, _, bloques)) = bloques_del_activo(layout) else { return };
    let linea = layout.editor_activo().cursor().linea;
    let indice = if adelante {
        bloques.iter().position(|b| b.linea_marca > linea).unwrap_or(0)
    } else {
        bloques.iter().rposition(|b| b.linea_marca < linea).unwrap_or(bloques.len() - 1)
    };
    let destino = Cursor { linea: bloques[indice].linea_marca, columna: 0 };
    layout.editor_activo_mut().fijar_seleccion(destino, destino);
    avisar(layout, format!("Git: cambio {} de {}", indice + 1, bloques.len()));
}
