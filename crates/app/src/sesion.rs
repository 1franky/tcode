//! Restaurar la sesión anterior (BACKLOG.md P2 #20): al lanzar `tcode`
//! sin archivo, reabre las pestañas, los splits, el cursor de cada
//! documento y la visibilidad del explorador de la última vez en esta
//! carpeta; al salir, los guarda (ver `tcode_config::Sesion`).
//!
//! Solo cuando se lanzó sin archivo: `tcode archivo.rs` abre ese archivo
//! y nada más, y tampoco pisa la sesión guardada de la carpeta. Se apaga
//! con `editor.restaurar_sesion`. Cualquier error (sesión rota, archivo
//! borrado, sin permiso) se saltea: en el peor caso arranca como siempre.

use std::path::{Path, PathBuf};

use tcode_config::{Config, NodoSesion, PestanaSesion, Sesion};
use tcode_core::{Cursor, Editor};
use tcode_fs::Explorador;
use tcode_ui::{Layout as PanelLayout, PanelEditor};

use crate::pliegues;

/// La carpeta del proyecto: el directorio actual, canónico.
fn proyecto() -> Option<PathBuf> {
    std::env::current_dir().ok().and_then(|d| std::fs::canonicalize(d).ok())
}

/// Si corresponde restaurar/guardar la sesión en este arranque.
pub fn aplica(ruta_arg: Option<&str>, config: &Config) -> bool {
    ruta_arg.is_none() && config.editor.restaurar_sesion
}

/// El layout de la sesión guardada de esta carpeta, con la visibilidad
/// del explorador, si hay una y queda al menos un archivo que abrir.
pub fn restaurar(config: &Config) -> Option<(PanelLayout, bool)> {
    let proyecto = proyecto()?;
    let sesion = Sesion::cargar(&proyecto)?;
    let layout = PanelLayout::desde_sesion(&sesion.paneles, sesion.panel_activo, |p| abrir(p, config))?;
    Some((layout, sesion.explorador_visible))
}

fn abrir(pestana: &PestanaSesion, config: &Config) -> Option<PanelEditor> {
    // La ruta tal como se abrió (relativa al directorio actual, que es la
    // carpeta del proyecto): es la que se muestra en la pestaña.
    if !Path::new(&pestana.ruta).is_file() {
        return None;
    }
    let mut editor = Editor::abrir(&pestana.ruta).ok()?;
    pliegues::restaurar(&mut editor);
    // `fijar_seleccion` recorta a lo que exista (el archivo pudo achicarse
    // por fuera) y despliega el bloque que oculte al cursor.
    let cursor = Cursor { linea: pestana.linea, columna: pestana.columna };
    editor.fijar_seleccion(cursor, cursor);
    if config.editor.modo_vim {
        editor.entrar_modo_normal();
    }
    Some(PanelEditor::nuevo(editor, pestana.ruta.clone()))
}

/// Guarda la sesión de esta carpeta tal como quedó al salir.
pub fn guardar(layout: &PanelLayout, explorador: &Explorador) {
    let Some(proyecto) = proyecto() else { return };
    let (mut paneles, panel_activo) = layout.a_sesion();
    relativizar(&mut paneles, &proyecto);
    Sesion {
        proyecto: proyecto.to_string_lossy().into_owned(),
        paneles,
        panel_activo,
        explorador_visible: explorador.visible(),
    }
    .guardar();
}

/// Las rutas de adentro del proyecto, relativas a él (`Ctrl+P` y el
/// explorador abren con rutas absolutas): más cortas en la pestaña al
/// restaurar, y siguen valiendo si la carpeta se mueve o se renombra.
fn relativizar(nodo: &mut NodoSesion, proyecto: &Path) {
    match nodo {
        NodoSesion::Hoja { pestanas, .. } => {
            for pestana in pestanas {
                let canonica = std::fs::canonicalize(&pestana.ruta).unwrap_or_else(|_| PathBuf::from(&pestana.ruta));
                if let Ok(relativa) = canonica.strip_prefix(proyecto) {
                    pestana.ruta = relativa.display().to_string();
                }
            }
        }
        NodoSesion::Division { primero, segundo, .. } => {
            relativizar(primero, proyecto);
            relativizar(segundo, proyecto);
        }
    }
}
