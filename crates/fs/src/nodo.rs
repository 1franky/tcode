use std::path::PathBuf;

use anyhow::{Context, Result};

/// Un nodo del árbol de archivos: una carpeta o un archivo. Los hijos de
/// una carpeta se cargan perezosamente, la primera vez que se expande
/// (`alternar`) — abrir una carpeta con miles de archivos no debería
/// bloquear el arranque del editor.
pub struct Nodo {
    pub nombre: String,
    pub ruta: PathBuf,
    pub es_carpeta: bool,
    pub expandida: bool,
    pub hijos: Vec<Nodo>,
    hijos_cargados: bool,
}

impl Nodo {
    /// Crea el nodo raíz del árbol: siempre una carpeta, ya expandida y con
    /// su primer nivel de hijos cargado.
    pub fn raiz(ruta: impl Into<PathBuf>) -> Result<Nodo> {
        let ruta = ruta.into();
        let nombre = ruta
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| ruta.display().to_string());
        let mut nodo = Nodo {
            nombre,
            ruta,
            es_carpeta: true,
            expandida: true,
            hijos: Vec::new(),
            hijos_cargados: false,
        };
        nodo.cargar_hijos()?;
        Ok(nodo)
    }

    fn cargar_hijos(&mut self) -> Result<()> {
        let mut entradas: Vec<_> = std::fs::read_dir(&self.ruta)
            .with_context(|| format!("no se pudo leer '{}'", self.ruta.display()))?
            .filter_map(|entrada| entrada.ok())
            .filter(|entrada| !nombre_oculto(&entrada.file_name().to_string_lossy()))
            .collect();

        // Carpetas primero, luego archivos; alfabético e insensible a
        // mayúsculas dentro de cada grupo (convención habitual de
        // exploradores de archivos).
        entradas.sort_by(|a, b| {
            let a_es_dir = a.path().is_dir();
            let b_es_dir = b.path().is_dir();
            b_es_dir.cmp(&a_es_dir).then_with(|| {
                a.file_name()
                    .to_string_lossy()
                    .to_lowercase()
                    .cmp(&b.file_name().to_string_lossy().to_lowercase())
            })
        });

        self.hijos = entradas
            .into_iter()
            .map(|entrada| {
                let ruta = entrada.path();
                let es_carpeta = ruta.is_dir();
                Nodo {
                    nombre: entrada.file_name().to_string_lossy().to_string(),
                    ruta,
                    es_carpeta,
                    expandida: false,
                    hijos: Vec::new(),
                    hijos_cargados: false,
                }
            })
            .collect();

        self.hijos_cargados = true;
        Ok(())
    }

    /// Nodo raíz "vacío" (carpeta sin nombre ni hijos), para cuando no se
    /// pudo leer el directorio real y no se quiere hacer fallar el
    /// arranque del editor por eso.
    pub fn vacio() -> Nodo {
        Nodo {
            nombre: String::new(),
            ruta: PathBuf::new(),
            es_carpeta: true,
            expandida: true,
            hijos: Vec::new(),
            hijos_cargados: true,
        }
    }

    /// Expande o colapsa una carpeta, cargando sus hijos la primera vez
    /// que se expande. No hace nada sobre un archivo.
    pub fn alternar(&mut self) -> Result<()> {
        if !self.es_carpeta {
            return Ok(());
        }
        if self.expandida {
            self.expandida = false;
            return Ok(());
        }
        if !self.hijos_cargados {
            self.cargar_hijos()?;
        }
        self.expandida = true;
        Ok(())
    }
}

/// Oculta dotfiles (`.git`, `.DS_Store`, ...) por convención — igual que la
/// mayoría de exploradores antes de cualquier configuración adicional. Qué
/// más ocultar (p. ej. `target/`, `node_modules/`) es configurable desde el
/// panel de administración en M4 (PLAN.md §5).
fn nombre_oculto(nombre: &str) -> bool {
    nombre.starts_with('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crear_arbol_de_prueba() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("carpeta_b")).unwrap();
        std::fs::create_dir(dir.path().join("Carpeta_A")).unwrap();
        std::fs::write(dir.path().join("archivo_z.txt"), "").unwrap();
        std::fs::write(dir.path().join("archivo_a.txt"), "").unwrap();
        std::fs::write(dir.path().join(".oculto"), "").unwrap();
        dir
    }

    #[test]
    fn carga_el_primer_nivel_con_carpetas_primero_y_orden_alfabetico() {
        let dir = crear_arbol_de_prueba();
        let raiz = Nodo::raiz(dir.path()).unwrap();

        let nombres: Vec<&str> = raiz.hijos.iter().map(|n| n.nombre.as_str()).collect();
        assert_eq!(nombres, vec!["Carpeta_A", "carpeta_b", "archivo_a.txt", "archivo_z.txt"]);
        assert!(raiz.hijos[0].es_carpeta);
        assert!(raiz.hijos[1].es_carpeta);
        assert!(!raiz.hijos[2].es_carpeta);
    }

    #[test]
    fn oculta_dotfiles_por_defecto() {
        let dir = crear_arbol_de_prueba();
        let raiz = Nodo::raiz(dir.path()).unwrap();
        assert!(!raiz.hijos.iter().any(|n| n.nombre == ".oculto"));
    }

    #[test]
    fn alternar_expande_y_colapsa_una_carpeta() {
        let dir = crear_arbol_de_prueba();
        std::fs::write(dir.path().join("carpeta_b").join("dentro.txt"), "").unwrap();
        let mut raiz = Nodo::raiz(dir.path()).unwrap();

        let carpeta_b = raiz.hijos.iter_mut().find(|n| n.nombre == "carpeta_b").unwrap();
        assert!(!carpeta_b.expandida);
        assert!(carpeta_b.hijos.is_empty());

        carpeta_b.alternar().unwrap();
        assert!(carpeta_b.expandida);
        assert_eq!(carpeta_b.hijos.len(), 1);
        assert_eq!(carpeta_b.hijos[0].nombre, "dentro.txt");

        carpeta_b.alternar().unwrap();
        assert!(!carpeta_b.expandida);
        // Los hijos ya cargados se conservan (no hace falta releer el
        // disco si se vuelve a expandir).
        assert_eq!(carpeta_b.hijos.len(), 1);
    }

    #[test]
    fn alternar_sobre_un_archivo_no_hace_nada() {
        let dir = crear_arbol_de_prueba();
        let mut raiz = Nodo::raiz(dir.path()).unwrap();
        let archivo = raiz.hijos.iter_mut().find(|n| !n.es_carpeta).unwrap();
        archivo.alternar().unwrap();
        assert!(!archivo.expandida);
    }
}
