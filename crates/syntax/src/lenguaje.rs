/// Lenguajes con resaltado de sintaxis vía tree-sitter en M1 (PLAN.md §11:
/// "5 lenguajes iniciales"). El resto de los 13 lenguajes objetivo de
/// PLAN.md §6 se van sumando en fases posteriores — añadir uno nuevo es
/// agregar una variante aquí y un caso en `Resaltador::config_para`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lenguaje {
    Rust,
    Python,
    JavaScript,
    Go,
    Markdown,
}

impl Lenguaje {
    /// Detecta el lenguaje por la extensión del archivo. `None` si no es
    /// uno de los 5 lenguajes con resaltado en M1 (el texto sigue
    /// mostrándose normalmente, solo que sin colorear).
    pub fn detectar_por_extension(ruta: &str) -> Option<Lenguaje> {
        let extension = ruta.rsplit('.').next().unwrap_or("");
        match extension {
            "rs" => Some(Lenguaje::Rust),
            "py" => Some(Lenguaje::Python),
            "js" | "jsx" | "mjs" | "cjs" => Some(Lenguaje::JavaScript),
            "go" => Some(Lenguaje::Go),
            "md" | "markdown" => Some(Lenguaje::Markdown),
            _ => None,
        }
    }

    pub fn nombre_mostrado(&self) -> &'static str {
        match self {
            Lenguaje::Rust => "Rust",
            Lenguaje::Python => "Python",
            Lenguaje::JavaScript => "JavaScript",
            Lenguaje::Go => "Go",
            Lenguaje::Markdown => "Markdown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detecta_los_5_lenguajes_de_m1() {
        assert_eq!(Lenguaje::detectar_por_extension("main.rs"), Some(Lenguaje::Rust));
        assert_eq!(Lenguaje::detectar_por_extension("script.py"), Some(Lenguaje::Python));
        assert_eq!(Lenguaje::detectar_por_extension("app.js"), Some(Lenguaje::JavaScript));
        assert_eq!(Lenguaje::detectar_por_extension("componente.jsx"), Some(Lenguaje::JavaScript));
        assert_eq!(Lenguaje::detectar_por_extension("main.go"), Some(Lenguaje::Go));
        assert_eq!(Lenguaje::detectar_por_extension("README.md"), Some(Lenguaje::Markdown));
    }

    #[test]
    fn extension_desconocida_no_tiene_lenguaje() {
        assert_eq!(Lenguaje::detectar_por_extension("datos.csv"), None);
        assert_eq!(Lenguaje::detectar_por_extension("sin_extension"), None);
    }
}
