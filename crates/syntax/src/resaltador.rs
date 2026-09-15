use std::borrow::Cow;
use std::collections::HashMap;

use anyhow::Result;
use tree_sitter_highlight::{Highlight, HighlightConfiguration, HighlightEvent, Highlighter};

use crate::lenguaje::Lenguaje;

/// Nombres de token canónicos, en el mismo vocabulario que
/// `tcode_config::TemaSintaxis` (PLAN.md §7: keyword, string, number,
/// comment, function, type, variable, constant, operator). Es lo único que
/// necesita conocer el resto del editor (p. ej. `ui::Paleta` para mapear
/// colores) — un [`Token::nombre`] siempre es uno de estos 9.
pub const NOMBRES_RESALTADO: &[&str] = &[
    "keyword", "string", "number", "comment", "function", "type", "variable", "constant", "operator",
];

/// Nombres que de verdad se le piden a `tree-sitter-highlight` (vía
/// `configure`), para hacer *matching* por prefijos contra las capturas de
/// cada `highlights.scm`. Incluye alias específicos de alguna gramática
/// (p. ej. Markdown usa `text.title`, `punctuation.special`...) que no
/// existen en las demás — `nombre_canonico` los reduce a uno de los 9 de
/// [`NOMBRES_RESALTADO`] antes de exponerlos como [`Token`].
const NOMBRES_RECONOCIDOS: &[&str] = &[
    "keyword",
    "string",
    "number",
    "comment",
    "function",
    "type",
    "variable",
    "constant",
    "operator",
    "text.title",
    "text.literal",
    "text.uri",
    "text.reference",
    "punctuation.special",
    "punctuation.delimiter",
];

fn nombre_canonico(indice: usize) -> &'static str {
    match NOMBRES_RECONOCIDOS[indice] {
        // Markdown (gramática de bloque): sin categorías propias en el
        // tema, se reasignan a la categoría genérica más parecida.
        "text.title" => "keyword",
        "text.literal" => "string",
        "text.uri" | "text.reference" => "constant",
        "punctuation.special" | "punctuation.delimiter" => "operator",
        otro => otro,
    }
}

/// Un tramo de código fuente (en bytes, extremo derecho exclusivo) al que
/// le corresponde el token `nombre` (uno de [`NOMBRES_RESALTADO`]). No hay
/// solapes entre tokens: tree-sitter-highlight ya entrega el resaltado más
/// interno aplicable a cada tramo.
#[derive(Debug, Clone, Copy)]
pub struct Token {
    pub inicio: usize,
    pub fin: usize,
    pub nombre: &'static str,
}

/// Resalta código fuente completo con tree-sitter, cacheando la
/// configuración de cada lenguaje (parsear las queries de highlights.scm
/// tiene costo, cachearla evita repetirlo en cada frame).
///
/// Recalcula el árbol completo en cada llamada a `resaltar` — el parsing
/// incremental real de tree-sitter (reutilizar el árbol anterior con los
/// rangos editados) es una optimización de rendimiento pendiente para
/// cuando haga falta con archivos grandes; no es necesaria para M1.
pub struct Resaltador {
    highlighter: Highlighter,
    configs: HashMap<Lenguaje, HighlightConfiguration>,
}

impl Resaltador {
    pub fn nuevo() -> Self {
        Self { highlighter: Highlighter::new(), configs: HashMap::new() }
    }

    fn construir_config(lenguaje: Lenguaje) -> Result<HighlightConfiguration> {
        // `Cow` porque la mayoría de las queries son el `&'static str` que
        // ya trae cada crate de gramática, pero TypeScript y C++ necesitan
        // una concatenada en el momento (ver los dos casos de abajo).
        let (language, nombre, highlights_query): (tree_sitter::Language, &str, Cow<'static, str>) = match lenguaje
        {
            Lenguaje::Rust => (tree_sitter_rust::LANGUAGE.into(), "rust", tree_sitter_rust::HIGHLIGHTS_QUERY.into()),
            Lenguaje::Python => {
                (tree_sitter_python::LANGUAGE.into(), "python", tree_sitter_python::HIGHLIGHTS_QUERY.into())
            }
            Lenguaje::JavaScript => (
                tree_sitter_javascript::LANGUAGE.into(),
                "javascript",
                tree_sitter_javascript::HIGHLIGHT_QUERY.into(),
            ),
            Lenguaje::Go => (tree_sitter_go::LANGUAGE.into(), "go", tree_sitter_go::HIGHLIGHTS_QUERY.into()),
            // Solo la gramática de bloque: encabezados, listas, citas,
            // bloques de código, etc. El contenido inline (negrita,
            // cursiva, enlaces) necesita la gramática inyectada aparte y
            // llega junto con la vista Markdown doble de M3.
            Lenguaje::Markdown => {
                (tree_sitter_md::LANGUAGE.into(), "markdown", tree_sitter_md::HIGHLIGHT_QUERY_BLOCK.into())
            }
            // El highlights.scm que trae `tree-sitter-typescript` es solo
            // un complemento (tipos y palabras clave propias de TS) —
            // asume que se combina con el de JavaScript para lo demás
            // (strings, números, funciones...), igual que hacen
            // nvim-treesitter y el resto del ecosistema. TSX es superset
            // de TypeScript (además acepta JSX), así que una sola gramática
            // alcanza para .ts y .tsx, igual que decidió
            // `Lenguaje::detectar_por_extension`.
            Lenguaje::TypeScript => (
                tree_sitter_typescript::LANGUAGE_TSX.into(),
                "typescript",
                format!("{}\n{}", tree_sitter_javascript::HIGHLIGHT_QUERY, tree_sitter_typescript::HIGHLIGHTS_QUERY)
                    .into(),
            ),
            Lenguaje::Java => (tree_sitter_java::LANGUAGE.into(), "java", tree_sitter_java::HIGHLIGHTS_QUERY.into()),
            Lenguaje::C => (tree_sitter_c::LANGUAGE.into(), "c", tree_sitter_c::HIGHLIGHT_QUERY.into()),
            // Mismo caso que TypeScript/JavaScript: el highlights.scm de
            // `tree-sitter-cpp` es un complemento sobre el de C (la
            // gramática de C++ extiende la de C).
            Lenguaje::Cpp => (
                tree_sitter_cpp::LANGUAGE.into(),
                "cpp",
                format!("{}\n{}", tree_sitter_c::HIGHLIGHT_QUERY, tree_sitter_cpp::HIGHLIGHT_QUERY).into(),
            ),
        };

        let mut config = HighlightConfiguration::new(language, nombre, &highlights_query, "", "")?;
        config.configure(NOMBRES_RECONOCIDOS);
        Ok(config)
    }

    fn config_para(&mut self, lenguaje: Lenguaje) -> Result<&HighlightConfiguration> {
        if let std::collections::hash_map::Entry::Vacant(hueco) = self.configs.entry(lenguaje) {
            hueco.insert(Self::construir_config(lenguaje)?);
        }
        Ok(self.configs.get(&lenguaje).expect("se acaba de insertar"))
    }

    /// Resalta el código fuente completo, devolviendo los tramos con token
    /// reconocido en orden de aparición. Las regiones sin token (texto
    /// "plano" para el resaltador, como espacios o puntuación sin captura)
    /// simplemente no aparecen en el resultado.
    pub fn resaltar(&mut self, lenguaje: Lenguaje, fuente: &str) -> Result<Vec<Token>> {
        self.config_para(lenguaje)?;
        // Se separan los campos para no pedir prestado `self` dos veces
        // (una vez para `configs`, otra para `highlighter`) al mismo tiempo.
        let Resaltador { highlighter, configs } = self;
        let config = configs.get(&lenguaje).expect("config_para ya la insertó");

        let mut tokens = Vec::new();
        let mut pila: Vec<&'static str> = Vec::new();

        let eventos = highlighter.highlight(config, fuente.as_bytes(), None, None, |_| None)?;
        for evento in eventos {
            match evento? {
                HighlightEvent::HighlightStart(Highlight(idx)) => pila.push(nombre_canonico(idx)),
                HighlightEvent::HighlightEnd => {
                    pila.pop();
                }
                HighlightEvent::Source { start, end } => {
                    if let Some(&nombre) = pila.last() {
                        tokens.push(Token { inicio: start, fin: end, nombre });
                    }
                }
            }
        }

        Ok(tokens)
    }
}

impl Default for Resaltador {
    fn default() -> Self {
        Self::nuevo()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nombres_en(tokens: &[Token], fuente: &str) -> Vec<(&'static str, String)> {
        tokens
            .iter()
            .map(|t| (t.nombre, fuente[t.inicio..t.fin].to_string()))
            .collect()
    }

    #[test]
    fn resalta_palabras_clave_de_rust() {
        let mut resaltador = Resaltador::nuevo();
        let fuente = "fn main() {}\n";
        let tokens = resaltador.resaltar(Lenguaje::Rust, fuente).unwrap();
        let nombres = nombres_en(&tokens, fuente);
        assert!(
            nombres.iter().any(|(n, texto)| *n == "keyword" && texto == "fn"),
            "se esperaba un token 'keyword' para 'fn', se obtuvo: {nombres:?}"
        );
    }

    #[test]
    fn resalta_comentarios_de_python() {
        let mut resaltador = Resaltador::nuevo();
        let fuente = "# comentario\ndef f():\n    pass\n";
        let tokens = resaltador.resaltar(Lenguaje::Python, fuente).unwrap();
        let nombres = nombres_en(&tokens, fuente);
        assert!(
            nombres.iter().any(|(n, _)| *n == "comment"),
            "se esperaba al menos un token 'comment', se obtuvo: {nombres:?}"
        );
        assert!(nombres.iter().any(|(n, texto)| *n == "keyword" && texto == "def"));
    }

    #[test]
    fn resalta_strings_de_javascript_y_go() {
        let mut resaltador = Resaltador::nuevo();

        let js = "const x = \"hola\";\n";
        let tokens_js = resaltador.resaltar(Lenguaje::JavaScript, js).unwrap();
        assert!(nombres_en(&tokens_js, js).iter().any(|(n, _)| *n == "string"));

        let go = "package main\nfunc main() { s := \"hola\" }\n";
        let tokens_go = resaltador.resaltar(Lenguaje::Go, go).unwrap();
        assert!(nombres_en(&tokens_go, go).iter().any(|(n, _)| *n == "string"));
    }

    #[test]
    fn resalta_encabezados_de_markdown() {
        let mut resaltador = Resaltador::nuevo();
        let fuente = "# Título\n\ntexto normal\n";
        let tokens = resaltador.resaltar(Lenguaje::Markdown, fuente).unwrap();
        assert!(!tokens.is_empty(), "se esperaba al menos un token para el encabezado");
    }

    #[test]
    fn resalta_la_primera_tanda_de_lenguajes_agregados_en_m4() {
        let mut resaltador = Resaltador::nuevo();

        let ts = "const x: string = \"hola\";\n";
        let tokens_ts = resaltador.resaltar(Lenguaje::TypeScript, ts).unwrap();
        assert!(nombres_en(&tokens_ts, ts).iter().any(|(n, _)| *n == "string"));

        let tsx = "const f = () => <div>hola</div>;\n";
        assert!(resaltador.resaltar(Lenguaje::TypeScript, tsx).is_ok());

        let java = "// comentario\nclass Principal {}\n";
        let tokens_java = resaltador.resaltar(Lenguaje::Java, java).unwrap();
        assert!(nombres_en(&tokens_java, java).iter().any(|(n, _)| *n == "comment"));

        let c = "int main() { return 0; }\n";
        let tokens_c = resaltador.resaltar(Lenguaje::C, c).unwrap();
        assert!(nombres_en(&tokens_c, c).iter().any(|(n, texto)| *n == "keyword" && texto == "return"));

        let cpp = "#include <string>\nint main() { return 0; }\n";
        let tokens_cpp = resaltador.resaltar(Lenguaje::Cpp, cpp).unwrap();
        assert!(nombres_en(&tokens_cpp, cpp).iter().any(|(n, texto)| *n == "keyword" && texto == "return"));
    }

    #[test]
    fn reutiliza_la_configuracion_entre_llamadas() {
        // No debe fallar ni recompilar la query al resaltar dos veces el
        // mismo lenguaje con el mismo Resaltador.
        let mut resaltador = Resaltador::nuevo();
        resaltador.resaltar(Lenguaje::Rust, "fn a() {}\n").unwrap();
        resaltador.resaltar(Lenguaje::Rust, "fn b() {}\n").unwrap();
        assert_eq!(resaltador.configs.len(), 1);
    }
}
