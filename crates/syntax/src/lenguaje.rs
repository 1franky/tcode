/// Lenguajes con resaltado de sintaxis vía tree-sitter. Los 5 primeros son
/// los de M1 (PLAN.md §11: "5 lenguajes iniciales"); TypeScript, Java, C y
/// C++ son la primera tanda de los 13 lenguajes objetivo de PLAN.md §6 que
/// se suma en M4 — el resto (Kotlin, C#, Ruby, PHP, HTML/CSS, SQL) queda
/// para tandas siguientes. Añadir uno nuevo es agregar una variante aquí y
/// un caso en `Resaltador::config_para`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lenguaje {
    Rust,
    Python,
    JavaScript,
    Go,
    Markdown,
    TypeScript,
    Java,
    C,
    Cpp,
}

impl Lenguaje {
    /// Todos los lenguajes con resaltado — usado por la sección
    /// "Lenguajes / LSP" del panel de administración (PLAN.md §5.3) para
    /// listarlos todos sin tener que enumerarlos de nuevo a mano en `app`.
    pub const TODOS: [Lenguaje; 9] = [
        Lenguaje::Rust,
        Lenguaje::Python,
        Lenguaje::JavaScript,
        Lenguaje::Go,
        Lenguaje::Markdown,
        Lenguaje::TypeScript,
        Lenguaje::Java,
        Lenguaje::C,
        Lenguaje::Cpp,
    ];

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
            Lenguaje::TypeScript => "typescript",
            Lenguaje::Java => "java",
            Lenguaje::C => "c",
            Lenguaje::Cpp => "cpp",
        }
    }

    /// Detecta el lenguaje por la extensión del archivo. `None` si no es
    /// ninguno de los lenguajes con resaltado (el texto sigue
    /// mostrándose normalmente, solo que sin colorear).
    pub fn detectar_por_extension(ruta: &str) -> Option<Lenguaje> {
        let extension = ruta.rsplit('.').next().unwrap_or("");
        match extension {
            "rs" => Some(Lenguaje::Rust),
            "py" => Some(Lenguaje::Python),
            "js" | "jsx" | "mjs" | "cjs" => Some(Lenguaje::JavaScript),
            "go" => Some(Lenguaje::Go),
            "md" | "markdown" => Some(Lenguaje::Markdown),
            // Una sola variante para .ts/.tsx: la gramática TSX es un
            // superset de TypeScript (acepta JSX además de lo que ya
            // acepta .ts), así que sirve para resaltar ambas extensiones
            // sin necesitar una variante separada — mismo criterio que
            // JavaScript con .jsx más arriba.
            "ts" | "tsx" | "mts" | "cts" => Some(Lenguaje::TypeScript),
            "java" => Some(Lenguaje::Java),
            // ".h" es ambiguo entre C y C++ (headers de C++ lo usan
            // seguido) — se resuelve a C por default, más simple que
            // adivinar por contenido; ".hpp"/".hh"/".hxx" no son
            // ambiguos y van directo a C++.
            "c" | "h" => Some(Lenguaje::C),
            "cpp" | "cc" | "cxx" | "c++" | "hpp" | "hh" | "hxx" => Some(Lenguaje::Cpp),
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
            "typescript" | "ts" | "tsx" => Some(Lenguaje::TypeScript),
            "java" => Some(Lenguaje::Java),
            "c" => Some(Lenguaje::C),
            "cpp" | "c++" | "cxx" => Some(Lenguaje::Cpp),
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
            Lenguaje::TypeScript => "TypeScript",
            Lenguaje::Java => "Java",
            Lenguaje::C => "C",
            Lenguaje::Cpp => "C++",
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
    fn detecta_la_primera_tanda_de_lenguajes_agregados_en_m4() {
        assert_eq!(Lenguaje::detectar_por_extension("app.ts"), Some(Lenguaje::TypeScript));
        assert_eq!(Lenguaje::detectar_por_extension("componente.tsx"), Some(Lenguaje::TypeScript));
        assert_eq!(Lenguaje::detectar_por_extension("Principal.java"), Some(Lenguaje::Java));
        assert_eq!(Lenguaje::detectar_por_extension("main.c"), Some(Lenguaje::C));
        assert_eq!(Lenguaje::detectar_por_extension("cabecera.h"), Some(Lenguaje::C));
        assert_eq!(Lenguaje::detectar_por_extension("main.cpp"), Some(Lenguaje::Cpp));
        assert_eq!(Lenguaje::detectar_por_extension("cabecera.hpp"), Some(Lenguaje::Cpp));
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
