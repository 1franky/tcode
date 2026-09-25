//! Cómo se comenta una línea en cada lenguaje (BACKLOG.md P0 #19,
//! "comentar/descomentar"). Vive en `tcode-core` y no en `tcode-syntax`
//! (junto a `Lenguaje`) porque se decide por la extensión del archivo,
//! igual que `csv::delimitador_por_extension`: así también se pueden
//! comentar archivos que no tienen resaltado de sintaxis (`.sh`,
//! `.toml`, `.yaml`, `.lua`, `Makefile`...), que son justamente los que
//! más se editan a mano, y `Editor::alternar_comentario` recibe el
//! estilo ya resuelto sin depender de ningún otro crate.

/// Estilo de comentario de un lenguaje.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstiloComentario {
    /// Comentario de línea (`//`, `#`, `--`, `;`...): se antepone a
    /// cada línea.
    Linea(&'static str),
    /// Solo comentario de bloque (HTML/Markdown `<!-- -->`, CSS `/* */`):
    /// se envuelve CADA línea por separado, no el bloque entero — así
    /// comentar y descomentar son simétricos línea a línea, igual que con
    /// `Linea`, y se puede descomentar una sola línea de un bloque
    /// comentado sin romper el resto.
    Bloque(&'static str, &'static str),
}

/// Estilo de comentario según la ruta (o el nombre) del archivo — `None`
/// si no se sabe (texto plano, JSON, sin nombre...): ahí comentar no hace
/// nada y `app` avisa.
pub fn estilo_comentario_por_extension(ruta: &str) -> Option<EstiloComentario> {
    let nombre = ruta.rsplit(['/', '\\']).next().unwrap_or(ruta).to_lowercase();
    // Archivos conocidos sin extensión (o con una que no dice nada).
    match nombre.as_str() {
        "makefile" | "gnumakefile" | "dockerfile" | "containerfile" | "cmakelists.txt" | "gemfile" | "rakefile"
        | "vagrantfile" | "procfile" => return Some(EstiloComentario::Linea("#")),
        _ => {}
    }
    let (_, extension) = nombre.rsplit_once('.')?;
    let estilo = match extension {
        "rs" | "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts" | "go" | "java" | "c" | "h" | "cpp"
        | "cc" | "cxx" | "c++" | "hpp" | "hh" | "hxx" | "kt" | "kts" | "cs" | "php" | "phtml" | "swift"
        | "scala" | "dart" | "zig" | "proto" | "groovy" | "gradle" | "jsonc" | "scss" | "less" => {
            EstiloComentario::Linea("//")
        }
        "py" | "pyi" | "rb" | "sh" | "bash" | "zsh" | "fish" | "toml" | "yaml" | "yml" | "pl" | "pm" | "r"
        | "cmake" | "mk" | "ps1" | "nix" | "tf" | "conf" | "gitignore" | "dockerignore" | "env" | "bashrc"
        | "zshrc" | "profile" | "editorconfig" | "ex" | "exs" | "jl" | "tcl" => EstiloComentario::Linea("#"),
        "sql" | "lua" | "hs" | "elm" => EstiloComentario::Linea("--"),
        "lisp" | "clj" | "cljs" | "el" | "scm" | "ini" | "asm" | "s" => EstiloComentario::Linea(";"),
        "tex" | "erl" | "hrl" => EstiloComentario::Linea("%"),
        "vim" | "vimrc" => EstiloComentario::Linea("\""),
        "html" | "htm" | "xml" | "svg" | "vue" | "md" | "markdown" => EstiloComentario::Bloque("<!--", "-->"),
        "css" => EstiloComentario::Bloque("/*", "*/"),
        _ => return None,
    };
    Some(estilo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detecta_el_estilo_por_extension_y_por_nombre() {
        assert_eq!(estilo_comentario_por_extension("src/main.rs"), Some(EstiloComentario::Linea("//")));
        assert_eq!(estilo_comentario_por_extension("a/b.PY"), Some(EstiloComentario::Linea("#")));
        assert_eq!(estilo_comentario_por_extension("consulta.sql"), Some(EstiloComentario::Linea("--")));
        assert_eq!(estilo_comentario_por_extension("init.lua"), Some(EstiloComentario::Linea("--")));
        assert_eq!(estilo_comentario_por_extension("x.ini"), Some(EstiloComentario::Linea(";")));
        assert_eq!(estilo_comentario_por_extension("Cargo.toml"), Some(EstiloComentario::Linea("#")));
        assert_eq!(estilo_comentario_por_extension("/proyecto/Makefile"), Some(EstiloComentario::Linea("#")));
        assert_eq!(estilo_comentario_por_extension("C:\\p\\.gitignore"), Some(EstiloComentario::Linea("#")));
        assert_eq!(estilo_comentario_por_extension("index.html"), Some(EstiloComentario::Bloque("<!--", "-->")));
        assert_eq!(estilo_comentario_por_extension("estilos.css"), Some(EstiloComentario::Bloque("/*", "*/")));
    }

    #[test]
    fn texto_plano_y_desconocidos_no_tienen_estilo() {
        assert_eq!(estilo_comentario_por_extension("notas.txt"), None);
        assert_eq!(estilo_comentario_por_extension("datos.json"), None);
        assert_eq!(estilo_comentario_por_extension("LEEME"), None);
        assert_eq!(estilo_comentario_por_extension(""), None);
        // Una carpeta con punto no le presta la extensión al archivo.
        assert_eq!(estilo_comentario_por_extension("dir.rs/LEEME"), None);
    }
}
