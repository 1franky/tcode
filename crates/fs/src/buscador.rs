use std::path::{Path, PathBuf};

use crate::nodo::nombre_oculto;

/// Un archivo que coincidió con la consulta actual del buscador, con la
/// ruta relativa a la raíz del proyecto (lo que se muestra) y las
/// posiciones que matchearon (para resaltarlas en la UI).
#[derive(Debug, Clone)]
pub struct ResultadoBusqueda {
    pub ruta: PathBuf,
    pub ruta_mostrada: String,
    pub posiciones: Vec<usize>,
}

/// Buscador difuso de archivos (`Ctrl+P`, PLAN.md §4): filtra sobre la
/// lista completa de archivos del proyecto mientras se escribe, vía
/// `tcode-fuzzy`. No sabe nada de terminal/`ratatui` — el crate `ui` lo
/// dibuja.
pub struct BuscadorArchivos {
    raiz: PathBuf,
    archivos: Vec<PathBuf>,
    activo: bool,
    consulta: String,
    seleccion: usize,
}

impl BuscadorArchivos {
    pub fn nuevo(raiz: impl Into<PathBuf>) -> Self {
        Self { raiz: raiz.into(), archivos: Vec::new(), activo: false, consulta: String::new(), seleccion: 0 }
    }

    pub fn activo(&self) -> bool {
        self.activo
    }

    pub fn consulta(&self) -> &str {
        &self.consulta
    }

    pub fn seleccion(&self) -> usize {
        self.seleccion
    }

    /// Abre el buscador con la consulta en blanco y recarga la lista de
    /// archivos del proyecto (recorrido recursivo completo — por eso se
    /// hace al abrir, no en cada tecla).
    pub fn abrir(&mut self) {
        self.activo = true;
        self.consulta.clear();
        self.seleccion = 0;
        self.archivos = listar_archivos_recursivo(&self.raiz);
    }

    pub fn cerrar(&mut self) {
        self.activo = false;
    }

    pub fn escribir(&mut self, c: char) {
        self.consulta.push(c);
        self.seleccion = 0;
    }

    pub fn borrar(&mut self) {
        self.consulta.pop();
        self.seleccion = 0;
    }

    fn ruta_relativa(&self, ruta: &Path) -> String {
        ruta.strip_prefix(&self.raiz).unwrap_or(ruta).display().to_string()
    }

    /// Archivos que coinciden con la consulta actual, de mejor a peor
    /// coincidencia (consulta vacía = todos, en el orden del recorrido).
    pub fn resultados(&self) -> Vec<ResultadoBusqueda> {
        let candidatos: Vec<(PathBuf, String)> =
            self.archivos.iter().map(|ruta| (ruta.clone(), self.ruta_relativa(ruta))).collect();

        tcode_fuzzy::filtrar_y_ordenar(&self.consulta, &candidatos, |(_, relativa)| relativa.as_str())
            .into_iter()
            .map(|((ruta, relativa), coincidencia)| ResultadoBusqueda {
                ruta: ruta.clone(),
                ruta_mostrada: relativa.clone(),
                posiciones: coincidencia.posiciones,
            })
            .collect()
    }

    pub fn mover_abajo(&mut self) {
        let total = self.resultados().len();
        if total == 0 {
            return;
        }
        self.seleccion = (self.seleccion + 1).min(total - 1);
    }

    pub fn mover_arriba(&mut self) {
        self.seleccion = self.seleccion.saturating_sub(1);
    }

    /// Confirma el resultado seleccionado: cierra el buscador y devuelve
    /// la ruta a abrir (`None` si no hay resultados que coincidan).
    pub fn confirmar(&mut self) -> Option<PathBuf> {
        let ruta = self.resultados().get(self.seleccion).map(|r| r.ruta.clone());
        self.cerrar();
        ruta
    }
}

/// Carpetas que nunca vale la pena recorrer para el buscador de archivos:
/// a diferencia del [`crate::Explorador`] (que carga bajo demanda),
/// `listar_archivos_recursivo` recorre TODO de una — sin este filtro,
/// cualquier proyecto con dependencias instaladas o binarios compilados
/// (`target/`, `node_modules/`) haría el buscador inservible.
const CARPETAS_IGNORADAS: &[&str] = &["target", "node_modules", ".git"];

/// Recorre `raiz` recursivamente y devuelve las rutas de todos los
/// archivos (no carpetas) que contiene — la lista sobre la que filtra el
/// buscador difuso (`Ctrl+P`, PLAN.md §4). Silencioso ante errores de
/// lectura (permisos, symlinks rotos...): simplemente hay menos
/// resultados, nunca rompe.
pub fn listar_archivos_recursivo(raiz: impl AsRef<Path>) -> Vec<PathBuf> {
    let mut salida = Vec::new();
    recorrer(raiz.as_ref(), &mut salida);
    salida
}

fn recorrer(dir: &Path, salida: &mut Vec<PathBuf>) {
    let Ok(entradas) = std::fs::read_dir(dir) else {
        return;
    };
    for entrada in entradas.filter_map(Result::ok) {
        let nombre_os = entrada.file_name();
        let nombre = nombre_os.to_string_lossy();
        let nombre_ref: &str = nombre.as_ref();
        if nombre_oculto(nombre_ref) || CARPETAS_IGNORADAS.contains(&nombre_ref) {
            continue;
        }

        let ruta = entrada.path();
        if ruta.is_dir() {
            recorrer(&ruta, salida);
        } else {
            salida.push(ruta);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lista_archivos_recursivamente_ignorando_ocultos_y_target() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src").join("main.rs"), "").unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        std::fs::write(dir.path().join(".gitignore"), "").unwrap();
        std::fs::create_dir_all(dir.path().join("target").join("debug")).unwrap();
        std::fs::write(dir.path().join("target").join("debug").join("binario"), "").unwrap();

        let mut archivos = listar_archivos_recursivo(dir.path());
        archivos.sort();

        let esperado = {
            let mut v = vec![dir.path().join("Cargo.toml"), dir.path().join("src").join("main.rs")];
            v.sort();
            v
        };
        assert_eq!(archivos, esperado);
    }

    fn crear_proyecto_de_prueba() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src").join("main.rs"), "").unwrap();
        std::fs::write(dir.path().join("src").join("lib.rs"), "").unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        dir
    }

    #[test]
    fn abrir_carga_archivos_y_resetea_consulta_y_seleccion() {
        let dir = crear_proyecto_de_prueba();
        let mut buscador = BuscadorArchivos::nuevo(dir.path());
        buscador.abrir();
        assert!(buscador.activo());
        assert_eq!(buscador.consulta(), "");
        assert_eq!(buscador.resultados().len(), 3);
    }

    #[test]
    fn muestra_rutas_relativas_a_la_raiz() {
        let dir = crear_proyecto_de_prueba();
        let mut buscador = BuscadorArchivos::nuevo(dir.path());
        buscador.abrir();
        let rutas: Vec<String> = buscador.resultados().into_iter().map(|r| r.ruta_mostrada).collect();
        assert!(rutas.contains(&"Cargo.toml".to_string()));
        assert!(rutas.iter().any(|r| r == "src/main.rs" || r == "src\\main.rs"));
    }

    #[test]
    fn escribir_filtra_por_coincidencia_difusa() {
        let dir = crear_proyecto_de_prueba();
        let mut buscador = BuscadorArchivos::nuevo(dir.path());
        buscador.abrir();
        for c in "main".chars() {
            buscador.escribir(c);
        }
        let resultados = buscador.resultados();
        assert_eq!(resultados.len(), 1);
        assert_eq!(resultados[0].ruta_mostrada, "src/main.rs".replace('/', std::path::MAIN_SEPARATOR_STR));
    }

    #[test]
    fn confirmar_devuelve_la_ruta_absoluta_y_cierra_el_buscador() {
        let dir = crear_proyecto_de_prueba();
        let mut buscador = BuscadorArchivos::nuevo(dir.path());
        buscador.abrir();
        for c in "Cargo.toml".chars() {
            buscador.escribir(c);
        }
        let ruta = buscador.confirmar();
        assert_eq!(ruta, Some(dir.path().join("Cargo.toml")));
        assert!(!buscador.activo());
    }
}
