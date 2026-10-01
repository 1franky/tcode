//! Snippets propios por lenguaje (BACKLOG.md P2 #24): archivos TOML en
//! `<config>/snippets/`, uno por lenguaje (`rust.toml`, `python.toml`...,
//! con el mismo nombre que en `[lenguajes]`) más `global.toml` para
//! todos. Cada snippet tiene un prefijo (lo que se escribe antes de
//! `Tab`), el cuerpo con la sintaxis de snippets de LSP/VSCode, y una
//! descripción opcional:
//!
//! ```toml
//! [[snippet]]
//! prefijo = "fn"
//! descripcion = "Función"
//! cuerpo = "fn ${1:nombre}(${2}) {\n\t$0\n}"
//! ```
//!
//! Un archivo roto se ignora entero (los demás siguen valiendo); el
//! motivo queda en [`SnippetsCargados::errores`].

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::config::directorio_config;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SnippetUsuario {
    pub prefijo: String,
    pub cuerpo: String,
    #[serde(default)]
    pub descripcion: String,
}

#[derive(Debug, Default, Deserialize)]
struct ArchivoSnippets {
    #[serde(default, rename = "snippet")]
    snippets: Vec<SnippetUsuario>,
}

/// Los snippets que valen para un lenguaje: los suyos primero (ganan si
/// repiten un prefijo de `global.toml`) y después los globales.
#[derive(Debug, Default, Clone)]
pub struct SnippetsCargados {
    pub snippets: Vec<SnippetUsuario>,
    pub errores: Vec<String>,
}

pub fn directorio_snippets() -> PathBuf {
    directorio_config().join("snippets")
}

/// Carga los snippets de `id_lenguaje` (`None` = un archivo sin
/// lenguaje: solo los globales).
pub fn cargar_snippets(id_lenguaje: Option<&str>) -> SnippetsCargados {
    cargar_snippets_en(&directorio_snippets(), id_lenguaje)
}

fn cargar_snippets_en(carpeta: &Path, id_lenguaje: Option<&str>) -> SnippetsCargados {
    let mut cargados = SnippetsCargados::default();
    let nombres = id_lenguaje.into_iter().chain(std::iter::once("global"));
    for nombre in nombres {
        let ruta = carpeta.join(format!("{nombre}.toml"));
        let Ok(texto) = std::fs::read_to_string(&ruta) else { continue };
        match toml::from_str::<ArchivoSnippets>(&texto) {
            Ok(archivo) => {
                for snippet in archivo.snippets {
                    if !snippet.prefijo.is_empty() && !cargados.snippets.iter().any(|s| s.prefijo == snippet.prefijo) {
                        cargados.snippets.push(snippet);
                    }
                }
            }
            Err(error) => cargados.errores.push(format!("{}: {error}", ruta.display())),
        }
    }
    cargados
}

/// El snippet cuyo prefijo termina justo en el cursor: `antes` es el
/// texto de la línea antes del cursor. El prefijo tiene que empezar en un
/// borde de palabra (no se expande `fn` en `confn`). Si varios coinciden,
/// el más largo.
pub fn snippet_para_prefijo<'a>(snippets: &'a [SnippetUsuario], antes: &str) -> Option<&'a SnippetUsuario> {
    snippets
        .iter()
        .filter(|s| {
            antes.strip_suffix(s.prefijo.as_str()).is_some_and(|resto| {
                let palabra = |c: char| c.is_alphanumeric() || c == '_';
                let empieza_palabra = s.prefijo.chars().next().is_some_and(palabra);
                !(empieza_palabra && resto.chars().last().is_some_and(palabra))
            })
        })
        .max_by_key(|s| s.prefijo.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carga_los_del_lenguaje_y_los_globales_sin_repetir_prefijos() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("rust.toml"),
            "[[snippet]]\nprefijo = \"fn\"\ncuerpo = \"fn $1() {}\"\n[[snippet]]\nprefijo = \"\"\ncuerpo = \"x\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("global.toml"),
            "[[snippet]]\nprefijo = \"fn\"\ncuerpo = \"pisado\"\n[[snippet]]\nprefijo = \"fecha\"\ncuerpo = \"hoy\"\ndescripcion = \"d\"\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("python.toml"), "esto no es toml [[").unwrap();
        let rust = cargar_snippets_en(dir.path(), Some("rust"));
        let prefijos: Vec<_> = rust.snippets.iter().map(|s| (s.prefijo.as_str(), s.cuerpo.as_str())).collect();
        assert_eq!(prefijos, [("fn", "fn $1() {}"), ("fecha", "hoy")]);
        assert!(rust.errores.is_empty());
        let python = cargar_snippets_en(dir.path(), Some("python"));
        assert_eq!(python.snippets.len(), 2);
        assert_eq!(python.errores.len(), 1);
        assert_eq!(cargar_snippets_en(dir.path(), None).snippets.len(), 2);
    }

    #[test]
    fn prefijo_en_borde_de_palabra_y_el_mas_largo() {
        let snippet = |p: &str| SnippetUsuario { prefijo: p.to_string(), cuerpo: String::new(), descripcion: String::new() };
        let lista = [snippet("fn"), snippet("pfn"), snippet("#inc")];
        assert_eq!(snippet_para_prefijo(&lista, "    fn").map(|s| s.prefijo.as_str()), Some("fn"));
        assert_eq!(snippet_para_prefijo(&lista, "pfn").map(|s| s.prefijo.as_str()), Some("pfn"));
        assert_eq!(snippet_para_prefijo(&lista, "confn"), None);
        assert_eq!(snippet_para_prefijo(&lista, "x#inc").map(|s| s.prefijo.as_str()), Some("#inc"));
        assert_eq!(snippet_para_prefijo(&lista, "fn "), None);
    }
}
