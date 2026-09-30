//! Sesión guardada por carpeta de proyecto (BACKLOG.md P2 #20): qué
//! pestañas había abiertas en cada panel, cómo estaban divididos los
//! paneles y dónde estaba el cursor de cada documento, para reabrir todo
//! igual la próxima vez que se lance `tcode` sin archivo en esa carpeta.
//!
//! Estado, no configuración (igual que los pliegues): vive en
//! `<estado>/sesiones/<huella de la carpeta>.json`, una por carpeta, y se
//! puede borrar sin problema. JSON y no TOML porque el árbol de paneles es
//! recursivo y nadie lo edita a mano. Un archivo roto o de una versión
//! incompatible es "sin sesión": arranca como siempre.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::pliegues_guardados::{directorio_estado, huella};

/// Una pestaña: el archivo (ruta tal como se mostraba, relativa a la
/// carpeta del proyecto si estaba adentro) y el cursor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PestanaSesion {
    pub ruta: String,
    pub linea: usize,
    pub columna: usize,
}

/// El árbol de paneles, con la misma forma que `tcode_ui::Layout`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum NodoSesion {
    Hoja {
        pestanas: Vec<PestanaSesion>,
        activa: usize,
    },
    Division {
        /// `true` = lado a lado (`DireccionSplit::Vertical`).
        vertical: bool,
        primero: Box<NodoSesion>,
        segundo: Box<NodoSesion>,
    },
}

impl NodoSesion {
    /// Si no hay ninguna pestaña con archivo en todo el árbol (no vale la
    /// pena guardarla: restaurarla daría lo mismo que no hacer nada).
    pub fn vacio(&self) -> bool {
        match self {
            NodoSesion::Hoja { pestanas, .. } => pestanas.is_empty(),
            NodoSesion::Division { primero, segundo, .. } => primero.vacio() && segundo.vacio(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sesion {
    /// Carpeta del proyecto (canónica), para no confundir dos carpetas
    /// cuyas huellas coincidieran.
    pub proyecto: String,
    pub paneles: NodoSesion,
    /// Índice del panel activo en el recorrido en profundidad.
    pub panel_activo: usize,
    pub explorador_visible: bool,
}

/// Archivo de sesión de la carpeta `proyecto` (canónica).
fn ruta_sesion(dir_estado: &Path, proyecto: &Path) -> PathBuf {
    let clave = huella(std::iter::once(proyecto.to_string_lossy().as_ref()));
    dir_estado.join("sesiones").join(format!("{clave:016x}.json"))
}

impl Sesion {
    /// La sesión guardada de `proyecto` (canónica), si hay una válida.
    pub fn cargar(proyecto: &Path) -> Option<Sesion> {
        Self::cargar_en(&directorio_estado(), proyecto)
    }

    /// Guarda (o, si no tiene ningún archivo, borra) la sesión de su
    /// proyecto. Errores de disco se ignoran: en el peor caso la próxima
    /// vez arranca sin sesión.
    pub fn guardar(&self) {
        let _ = self.guardar_en(&directorio_estado());
    }

    fn cargar_en(dir_estado: &Path, proyecto: &Path) -> Option<Sesion> {
        let texto = std::fs::read_to_string(ruta_sesion(dir_estado, proyecto)).ok()?;
        let sesion: Sesion = serde_json::from_str(&texto).ok()?;
        (Path::new(&sesion.proyecto) == proyecto).then_some(sesion)
    }

    fn guardar_en(&self, dir_estado: &Path) -> std::io::Result<()> {
        let ruta = ruta_sesion(dir_estado, Path::new(&self.proyecto));
        if self.paneles.vacio() {
            return match std::fs::remove_file(&ruta) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            };
        }
        if let Some(carpeta) = ruta.parent() {
            std::fs::create_dir_all(carpeta)?;
        }
        let texto = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let temporal = ruta.with_extension("json.tmp");
        std::fs::write(&temporal, texto)?;
        std::fs::rename(&temporal, ruta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pestana(ruta: &str, linea: usize) -> PestanaSesion {
        PestanaSesion { ruta: ruta.to_string(), linea, columna: 2 }
    }

    fn sesion(proyecto: &str) -> Sesion {
        Sesion {
            proyecto: proyecto.to_string(),
            paneles: NodoSesion::Division {
                vertical: true,
                primero: Box::new(NodoSesion::Hoja { pestanas: vec![pestana("a.rs", 3), pestana("b.rs", 0)], activa: 1 }),
                segundo: Box::new(NodoSesion::Hoja { pestanas: vec![pestana("src/c.py", 40)], activa: 0 }),
            },
            panel_activo: 1,
            explorador_visible: true,
        }
    }

    #[test]
    fn ida_y_vuelta_por_disco() {
        let dir = tempfile::tempdir().unwrap();
        let s = sesion("/proyecto/uno");
        s.guardar_en(dir.path()).unwrap();
        assert_eq!(Sesion::cargar_en(dir.path(), Path::new("/proyecto/uno")), Some(s));
        assert_eq!(Sesion::cargar_en(dir.path(), Path::new("/proyecto/otro")), None);
    }

    #[test]
    fn sin_archivos_borra_la_sesion_guardada() {
        let dir = tempfile::tempdir().unwrap();
        sesion("/p").guardar_en(dir.path()).unwrap();
        let vacia = Sesion {
            paneles: NodoSesion::Hoja { pestanas: vec![], activa: 0 },
            ..sesion("/p")
        };
        vacia.guardar_en(dir.path()).unwrap();
        assert_eq!(Sesion::cargar_en(dir.path(), Path::new("/p")), None);
        // Borrar lo que no existe tampoco es un error.
        vacia.guardar_en(dir.path()).unwrap();
    }

    #[test]
    fn un_archivo_roto_es_sin_sesion() {
        let dir = tempfile::tempdir().unwrap();
        let ruta = ruta_sesion(dir.path(), Path::new("/p"));
        std::fs::create_dir_all(ruta.parent().unwrap()).unwrap();
        std::fs::write(&ruta, "{ no es json").unwrap();
        assert_eq!(Sesion::cargar_en(dir.path(), Path::new("/p")), None);
    }
}
