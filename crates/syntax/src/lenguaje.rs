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
    /// Los 5 lenguajes de M1, en el mismo orden que PLAN.md §11 — usado
    /// por la sección "Lenguajes / LSP" del panel de administración
    /// (PLAN.md §5.3) para listarlos todos sin tener que enumerarlos de
    /// nuevo a mano en `app`.
    pub const TODOS: [Lenguaje; 5] =
        [Lenguaje::Rust, Lenguaje::Python, Lenguaje::JavaScript, Lenguaje::Go, Lenguaje::Markdown];

    /// Identificador estable en minúsculas, para usar como clave de
    /// almacenamiento (`config.toml`, sección "Lenguajes / LSP") en vez
    /// de depender del nombre mostrado (`nombre_mostrado`), que es texto
    /// para humanos y podría cambiar de redacción sin que eso deba
    /// invalidar una configuración ya guardada.
    pub fn id(&self) -> &'static str {
        match self {
            Lenguaje::Rust => "rust",
            Lenguaje::Python => "python",
            Lenguaje::JavaScript => "javascript",
            Lenguaje::Go => "go",
            Lenguaje::Markdown => "markdown",
        }
    }

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

    /// Detecta el lenguaje por la etiqueta de un bloque de código cercado
    /// de Markdown (` ```rust `, ` ```py `...) — usado por la vista de
    /// preview de Markdown (PLAN.md §8: "code blocks → resaltado con
    /// tree-sitter del lenguaje declarado"). Acepta alias comunes además
    /// del nombre completo.
    pub fn detectar_por_etiqueta(etiqueta: &str) -> Option<Lenguaje> {
        match etiqueta.trim().to_lowercase().as_str() {
            "rust" | "rs" => Some(Lenguaje::Rust),
            "python" | "py" => Some(Lenguaje::Python),
            "javascript" | "js" | "jsx" => Some(Lenguaje::JavaScript),
            "go" | "golang" => Some(Lenguaje::Go),
            "markdown" | "md" => Some(Lenguaje::Markdown),
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
    fn todos_tiene_un_id_unico_por_lenguaje() {
        let mut ids: Vec<&str> = Lenguaje::TODOS.iter().map(|l| l.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), Lenguaje::TODOS.len());
    }

    #[test]
    fn extension_desconocida_no_tiene_lenguaje() {
        assert_eq!(Lenguaje::detectar_por_extension("datos.csv"), None);
        assert_eq!(Lenguaje::detectar_por_extension("sin_extension"), None);
    }

    #[test]
    fn detecta_lenguaje_por_etiqueta_de_bloque_de_codigo() {
        assert_eq!(Lenguaje::detectar_por_etiqueta("rust"), Some(Lenguaje::Rust));
        assert_eq!(Lenguaje::detectar_por_etiqueta("py"), Some(Lenguaje::Python));
        assert_eq!(Lenguaje::detectar_por_etiqueta("JS"), Some(Lenguaje::JavaScript));
        assert_eq!(Lenguaje::detectar_por_etiqueta("golang"), Some(Lenguaje::Go));
        assert_eq!(Lenguaje::detectar_por_etiqueta("brainfuck"), None);
        assert_eq!(Lenguaje::detectar_por_etiqueta(""), None);
    }
}
