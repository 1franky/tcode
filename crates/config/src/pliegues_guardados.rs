//! Pliegues guardados entre sesiones: qué bloques estaban plegados en
//! cada archivo la última vez que se cerró, para restaurarlos al volver a
//! abrirlo. NO es configuración — es estado del usuario, y vive aparte
//! de `config.toml`, en el directorio de datos
//! (`<datos>/tcode/estado/pliegues.toml`, ver [`ruta_pliegues`]).
//!
//! Cada entrada es la ruta canónica del archivo, una huella de su texto
//! ([`huella`]) y los rangos de líneas plegados. Si al abrir la huella no
//! coincide (el archivo cambió por fuera: otro editor, `git checkout`...)
//! los pliegues se descartan en vez de plegar líneas equivocadas. Se
//! eligió una huella del contenido y no la fecha de modificación: un
//! `touch` o ir y volver de rama no invalida nada, y un cambio real
//! siempre lo hace.
//!
//! Tamaño acotado: a lo sumo [`MAX_ARCHIVOS`] entradas, la más reciente
//! primero (LRU) — la que se usa se mueve adelante y las del final se
//! caen. Cada lectura/escritura toca el disco una vez, y solo al abrir o
//! cerrar un documento (nunca por frame). Leer o escribir mal no es un
//! error visible: un archivo roto o ilegible es "sin pliegues guardados".

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Máximo de archivos recordados (ver la documentación del módulo).
pub const MAX_ARCHIVOS: usize = 200;

/// Directorio del estado del usuario (lo que tcode recuerda entre
/// sesiones y no es configuración): `~/.local/state/tcode/estado` en
/// Linux, la carpeta de datos local del SO en macOS/Windows.
pub fn directorio_estado() -> PathBuf {
    dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .map(|base| base.join("tcode").join("estado"))
        .unwrap_or_else(|| PathBuf::from(".tcode").join("estado"))
}

/// Archivo donde se guardan los pliegues.
pub fn ruta_pliegues() -> PathBuf {
    directorio_estado().join("pliegues.toml")
}

/// Huella de un texto (FNV-1a de 64 bits sobre sus bytes), dado en
/// trozos para no tener que armarlo entero (los chunks del rope). No es
/// criptográfica — solo tiene que cambiar cuando cambia el texto — y,
/// a diferencia de `DefaultHasher`, es estable entre versiones de Rust.
pub fn huella<'a>(trozos: impl Iterator<Item = &'a str>) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for trozo in trozos {
        for &b in trozo.as_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Entrada {
    ruta: String,
    /// La huella en hexadecimal: TOML no tiene enteros sin signo de 64
    /// bits.
    huella: String,
    pliegues: Vec<(usize, usize)>,
}

/// Los pliegues recordados de todos los archivos, del más reciente al
/// más viejo.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlieguesGuardados {
    #[serde(default, rename = "archivo")]
    archivos: Vec<Entrada>,
}

impl PlieguesGuardados {
    /// Lee `ruta`; si no existe o está rota, vacío.
    pub fn cargar(ruta: &Path) -> Self {
        std::fs::read_to_string(ruta).ok().and_then(|texto| toml::from_str(&texto).ok()).unwrap_or_default()
    }

    /// Escribe en `ruta` (creando la carpeta), primero a un temporal y
    /// después renombrando, para que un corte a mitad de camino no deje
    /// un archivo a medias.
    pub fn guardar(&self, ruta: &Path) -> std::io::Result<()> {
        if let Some(carpeta) = ruta.parent() {
            std::fs::create_dir_all(carpeta)?;
        }
        let texto = toml::to_string(self).map_err(std::io::Error::other)?;
        let temporal = ruta.with_extension("toml.tmp");
        std::fs::write(&temporal, format!("{ENCABEZADO}{texto}"))?;
        std::fs::rename(&temporal, ruta)
    }

    /// Si hay algo recordado para `archivo` (ruta canónica) — para no
    /// calcular la huella de un archivo del que no hay nada guardado.
    pub fn tiene(&self, archivo: &Path) -> bool {
        let clave = archivo.to_string_lossy();
        self.archivos.iter().any(|e| e.ruta == clave)
    }

    /// Los pliegues de `archivo` (ruta canónica), si los hay y su huella
    /// coincide con `huella_actual`.
    pub fn restaurar(&self, archivo: &Path, huella_actual: u64) -> Option<&[(usize, usize)]> {
        let clave = archivo.to_string_lossy();
        let entrada = self.archivos.iter().find(|e| e.ruta == clave)?;
        (entrada.huella == format!("{huella_actual:016x}")).then_some(entrada.pliegues.as_slice())
    }

    /// Recuerda `pliegues` para `archivo` (ruta canónica) con su huella,
    /// como el más reciente. Sin pliegues, lo olvida: al abrir de nuevo
    /// arranca todo desplegado igual. Devuelve si cambió algo (para no
    /// reescribir el archivo en vano).
    pub fn recordar(&mut self, archivo: &Path, huella_actual: u64, pliegues: &[(usize, usize)]) -> bool {
        let clave = archivo.to_string_lossy().into_owned();
        let posicion = self.archivos.iter().position(|e| e.ruta == clave);
        let anterior = posicion.map(|i| self.archivos.remove(i));
        if pliegues.is_empty() {
            return anterior.is_some();
        }
        let entrada = Entrada { ruta: clave, huella: format!("{huella_actual:016x}"), pliegues: pliegues.to_vec() };
        // Igual que antes y ya primera: no hay nada que reescribir.
        let cambio = posicion != Some(0) || anterior.as_ref() != Some(&entrada);
        self.archivos.insert(0, entrada);
        self.archivos.truncate(MAX_ARCHIVOS);
        cambio
    }

    pub fn len(&self) -> usize {
        self.archivos.len()
    }

    pub fn is_empty(&self) -> bool {
        self.archivos.is_empty()
    }
}

const ENCABEZADO: &str = "# Pliegues de código recordados por tcode entre sesiones (estado, no\n\
# configuración): se puede borrar sin problema.\n\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_huella_no_depende_de_como_esta_partido_el_texto() {
        assert_eq!(huella(["hola ", "mundo"].into_iter()), huella(std::iter::once("hola mundo")));
        assert_ne!(huella(std::iter::once("hola mundo")), huella(std::iter::once("hola mundo!")));
        // Valor conocido de FNV-1a 64 ("a"): estable entre versiones.
        assert_eq!(huella(std::iter::once("a")), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn restaura_solo_si_la_huella_coincide() {
        let mut guardados = PlieguesGuardados::default();
        let ruta = Path::new("/proyecto/a.rs");
        assert!(guardados.recordar(ruta, 42, &[(2, 5), (10, 20)]));
        assert!(guardados.tiene(ruta));
        assert_eq!(guardados.restaurar(ruta, 42), Some(&[(2, 5), (10, 20)][..]));
        assert_eq!(guardados.restaurar(ruta, 43), None);
        assert_eq!(guardados.restaurar(Path::new("/proyecto/b.rs"), 42), None);
    }

    #[test]
    fn sin_pliegues_se_olvida_la_entrada() {
        let mut guardados = PlieguesGuardados::default();
        let ruta = Path::new("/a.rs");
        guardados.recordar(ruta, 1, &[(0, 3)]);
        assert!(guardados.recordar(ruta, 1, &[]));
        assert!(!guardados.tiene(ruta));
        // Olvidar algo que no estaba no cambia nada.
        assert!(!guardados.recordar(ruta, 1, &[]));
    }

    #[test]
    fn es_lru_y_tiene_tope() {
        let mut guardados = PlieguesGuardados::default();
        for i in 0..MAX_ARCHIVOS + 10 {
            guardados.recordar(Path::new(&format!("/f{i}.rs")), 1, &[(0, 1)]);
        }
        assert_eq!(guardados.len(), MAX_ARCHIVOS);
        // Los más viejos se cayeron; el último recordado va primero.
        assert!(!guardados.tiene(Path::new("/f0.rs")));
        assert!(guardados.tiene(Path::new(&format!("/f{}.rs", MAX_ARCHIVOS + 9))));
        // Volver a recordar uno viejo lo sube al frente y no duplica.
        guardados.recordar(Path::new("/f10.rs"), 1, &[(0, 2)]);
        assert_eq!(guardados.len(), MAX_ARCHIVOS);
        assert_eq!(guardados.archivos[0].ruta, "/f10.rs");
    }

    #[test]
    fn recordar_lo_mismo_que_ya_estaba_primero_no_cambia_nada() {
        let mut guardados = PlieguesGuardados::default();
        guardados.recordar(Path::new("/a.rs"), 1, &[(0, 3)]);
        assert!(!guardados.recordar(Path::new("/a.rs"), 1, &[(0, 3)]));
    }

    #[test]
    fn guardar_y_cargar_ida_y_vuelta_y_archivo_roto() {
        let dir = tempfile::tempdir().unwrap();
        let ruta = dir.path().join("sub/estado/pliegues.toml");
        let mut guardados = PlieguesGuardados::default();
        guardados.recordar(Path::new("/a.rs"), u64::MAX, &[(1, 4)]);
        guardados.guardar(&ruta).unwrap();
        assert_eq!(PlieguesGuardados::cargar(&ruta), guardados);

        std::fs::write(&ruta, "esto no es [toml").unwrap();
        assert!(PlieguesGuardados::cargar(&ruta).is_empty());
        assert!(PlieguesGuardados::cargar(&dir.path().join("no_existe.toml")).is_empty());
    }
}
