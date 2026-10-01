//! Copias de respaldo de los buffers con cambios sin guardar (BACKLOG.md
//! P2 #21), para recuperarlos si `tcode` se cierra de golpe (la terminal
//! se cerró, `kill`, un panic, se cortó la luz).
//!
//! Cada proceso tiene su carpeta `<estado>/respaldos/<id>/` con:
//! - `bloqueo`: un archivo que el proceso mantiene bloqueado
//!   (`File::try_lock`) mientras vive. El sistema operativo suelta el
//!   bloqueo cuando el proceso termina, de la forma que sea — así otro
//!   `tcode` distingue "este sigue abierto" (bloqueado) de "este se murió"
//!   (se puede bloquear), también entre varias instancias a la vez.
//! - `respaldos.json`: la carpeta del proyecto y el texto de cada buffer
//!   modificado, reescrito entero (temporal + renombrar) cada vez que
//!   cambia; se borra cuando ya no queda nada sin guardar.
//!
//! Al salir normalmente la carpeta se borra. Si queda una sin bloquear es
//! de un cierre inesperado: un "huérfano" ([`buscar_huerfanos`]), que se
//! ofrece recuperar al abrir `tcode` en la misma carpeta de proyecto.
//! Mientras se decide, el huérfano queda bloqueado por quien lo encontró:
//! dos `tcode` que arrancan a la vez no lo ofrecen los dos.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::pliegues_guardados::directorio_estado;

const ARCHIVO_BLOQUEO: &str = "bloqueo";
const ARCHIVO_RESPALDOS: &str = "respaldos.json";

/// Una carpeta sin `respaldos.json` y con el bloqueo libre es de un
/// proceso que no tenía nada sin guardar, o de uno que la está creando en
/// este instante (entre crear `bloqueo` y bloquearlo): solo se borra si es
/// más vieja que esto.
const GRACIA_CARPETA_VACIA: Duration = Duration::from_secs(60);

/// Los huérfanos de OTRAS carpetas de proyecto se dejan para cuando se
/// abra esa carpeta — salvo que sean más viejos que esto.
const VIDA_MAXIMA_HUERFANO: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// Un buffer con cambios sin guardar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Respaldo {
    /// Ruta del archivo (tal como se abrió), o `None` si era un
    /// "[Sin nombre]".
    pub ruta: Option<String>,
    /// Lo que se mostraba en la pestaña.
    pub ruta_mostrada: String,
    pub texto: String,
    /// Dónde estaba el cursor (línea, columna en caracteres).
    pub linea: usize,
    pub columna: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ArchivoRespaldos {
    proyecto: String,
    respaldos: Vec<Respaldo>,
}

/// `<estado>/respaldos`.
pub fn directorio_respaldos() -> PathBuf {
    directorio_estado().join("respaldos")
}

/// La carpeta de respaldos de ESTE proceso, bloqueada mientras viva.
#[derive(Debug)]
pub struct CarpetaRespaldos {
    dir: PathBuf,
    proyecto: String,
    _bloqueo: File,
}

impl CarpetaRespaldos {
    /// Crea y bloquea `<base>/<pid>-<nanos>/`.
    pub fn crear(base: &Path, proyecto: &Path) -> std::io::Result<Self> {
        let nanos = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let dir = base.join(format!("{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let bloqueo = File::create(dir.join(ARCHIVO_BLOQUEO))?;
        bloqueo.try_lock().map_err(std::io::Error::other)?;
        Ok(Self { dir, proyecto: proyecto.to_string_lossy().into_owned(), _bloqueo: bloqueo })
    }

    /// Reescribe los respaldos (sin ninguno, borra el archivo).
    pub fn escribir(&self, respaldos: Vec<Respaldo>) -> std::io::Result<()> {
        let ruta = self.dir.join(ARCHIVO_RESPALDOS);
        if respaldos.is_empty() {
            return match std::fs::remove_file(&ruta) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            };
        }
        let archivo = ArchivoRespaldos { proyecto: self.proyecto.clone(), respaldos };
        let texto = serde_json::to_string(&archivo).map_err(std::io::Error::other)?;
        let temporal = ruta.with_extension("json.tmp");
        std::fs::write(&temporal, texto)?;
        std::fs::rename(&temporal, ruta)
    }

    /// Salida normal: borra la carpeta entera.
    pub fn terminar(self) {
        let dir = self.dir.clone();
        drop(self);
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// Los respaldos de un proceso que se cerró sin terminar bien. Mientras
/// exista, está bloqueado por este proceso.
#[derive(Debug)]
pub struct Huerfano {
    dir: PathBuf,
    pub respaldos: Vec<Respaldo>,
    _bloqueo: File,
}

impl Huerfano {
    /// Ya se recuperó o se decidió descartarlo: borra la carpeta.
    pub fn borrar(self) {
        let dir = self.dir.clone();
        drop(self);
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// Los huérfanos de `proyecto` con algo que recuperar, del más viejo al
/// más nuevo. De paso limpia las carpetas que no sirven para nada (sin
/// respaldos, o huérfanas de hace más de un mes). Los de otros proyectos
/// quedan como están, sin bloquear. Nunca falla: lo ilegible se ignora.
pub fn buscar_huerfanos(base: &Path, proyecto: &Path) -> Vec<Huerfano> {
    let Ok(entradas) = std::fs::read_dir(base) else { return Vec::new() };
    let mut huerfanos: Vec<(SystemTime, Huerfano)> = Vec::new();
    for entrada in entradas.flatten() {
        let dir = entrada.path();
        if !dir.is_dir() {
            continue;
        }
        let Ok(bloqueo) = File::options().read(true).write(true).open(dir.join(ARCHIVO_BLOQUEO)) else { continue };
        if bloqueo.try_lock().is_err() {
            continue; // Su proceso sigue vivo.
        }
        let edad = |ruta: &Path| {
            std::fs::metadata(ruta)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| SystemTime::now().duration_since(t).ok())
                .unwrap_or_default()
        };
        let ruta_respaldos = dir.join(ARCHIVO_RESPALDOS);
        let archivo = std::fs::read_to_string(&ruta_respaldos)
            .ok()
            .and_then(|t| serde_json::from_str::<ArchivoRespaldos>(&t).ok());
        let Some(archivo) = archivo.filter(|a| !a.respaldos.is_empty()) else {
            if edad(&dir.join(ARCHIVO_BLOQUEO)) > GRACIA_CARPETA_VACIA {
                drop(bloqueo);
                let _ = std::fs::remove_dir_all(&dir);
            }
            continue;
        };
        if Path::new(&archivo.proyecto) != proyecto {
            if edad(&ruta_respaldos) > VIDA_MAXIMA_HUERFANO {
                drop(bloqueo);
                let _ = std::fs::remove_dir_all(&dir);
            }
            continue;
        }
        let cuando = std::fs::metadata(&ruta_respaldos).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
        huerfanos.push((cuando, Huerfano { dir, respaldos: archivo.respaldos, _bloqueo: bloqueo }));
    }
    huerfanos.sort_by_key(|(cuando, _)| *cuando);
    huerfanos.into_iter().map(|(_, h)| h).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn respaldo(ruta: &str, texto: &str) -> Respaldo {
        Respaldo {
            ruta: Some(ruta.to_string()),
            ruta_mostrada: ruta.to_string(),
            texto: texto.to_string(),
            linea: 0,
            columna: 0,
        }
    }

    #[test]
    fn un_proceso_vivo_no_es_huerfano_y_al_morir_si() {
        let base = tempfile::tempdir().unwrap();
        let proyecto = Path::new("/p");
        let carpeta = CarpetaRespaldos::crear(base.path(), proyecto).unwrap();
        carpeta.escribir(vec![respaldo("a.rs", "hola")]).unwrap();
        // Vivo (bloqueado): nadie más lo toma.
        assert!(buscar_huerfanos(base.path(), proyecto).is_empty());
        // "Muere" sin terminar: el bloqueo se suelta y queda huérfano.
        drop(carpeta);
        let huerfanos = buscar_huerfanos(base.path(), proyecto);
        assert_eq!(huerfanos.len(), 1);
        assert_eq!(huerfanos[0].respaldos, vec![respaldo("a.rs", "hola")]);
        // Mientras alguien lo tiene encontrado, otro no lo ve.
        assert!(buscar_huerfanos(base.path(), proyecto).is_empty());
        // Otro proyecto no lo ve ni lo borra.
        drop(huerfanos);
        assert!(buscar_huerfanos(base.path(), Path::new("/otro")).is_empty());
        let huerfanos = buscar_huerfanos(base.path(), proyecto);
        assert_eq!(huerfanos.len(), 1);
        for h in huerfanos {
            h.borrar();
        }
        assert!(buscar_huerfanos(base.path(), proyecto).is_empty());
        assert_eq!(std::fs::read_dir(base.path()).unwrap().count(), 0);
    }

    #[test]
    fn terminar_bien_no_deja_nada() {
        let base = tempfile::tempdir().unwrap();
        let carpeta = CarpetaRespaldos::crear(base.path(), Path::new("/p")).unwrap();
        carpeta.escribir(vec![respaldo("a.rs", "x")]).unwrap();
        carpeta.terminar();
        assert_eq!(std::fs::read_dir(base.path()).unwrap().count(), 0);
    }

    #[test]
    fn sin_cambios_pendientes_no_hay_nada_que_recuperar() {
        let base = tempfile::tempdir().unwrap();
        let carpeta = CarpetaRespaldos::crear(base.path(), Path::new("/p")).unwrap();
        carpeta.escribir(vec![respaldo("a.rs", "x")]).unwrap();
        // Se guardó todo: el archivo de respaldos desaparece.
        carpeta.escribir(vec![]).unwrap();
        drop(carpeta);
        // Carpeta reciente sin respaldos: no se ofrece, y todavía no se
        // borra (podría estar creándose).
        assert!(buscar_huerfanos(base.path(), Path::new("/p")).is_empty());
        assert_eq!(std::fs::read_dir(base.path()).unwrap().count(), 1);
    }
}
