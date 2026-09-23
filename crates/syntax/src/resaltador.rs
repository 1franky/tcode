use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::ops::{ControlFlow, Range};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use tree_sitter::{
    InputEdit, Language, ParseOptions, ParseState, Parser, Point, Query, QueryCursor, StreamingIterator, Tree,
};

use crate::lenguaje::Lenguaje;
use crate::plegado::{rangos_de_arbol, rangos_por_indentacion, RangoPlegable};

/// Nombres de token canónicos, en el mismo vocabulario que
/// `tcode_config::TemaSintaxis` (PLAN.md §7: keyword, string, number,
/// comment, function, type, variable, constant, operator). Es lo único que
/// necesita conocer el resto del editor (p. ej. `ui::Paleta` para mapear
/// colores) — un [`Token::nombre`] siempre es uno de estos 9.
pub const NOMBRES_RESALTADO: &[&str] = &[
    "keyword", "string", "number", "comment", "function", "type", "variable", "constant", "operator",
];

/// Nombres que de verdad se reconocen (vía [`nombre_para_captura`]), para
/// hacer *matching* por partes contra las capturas de
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
    "punctuation.bracket",
    "tag",
    "attribute",
    "property",
    "spell",
];

fn nombre_canonico(indice: usize) -> &'static str {
    match NOMBRES_RECONOCIDOS[indice] {
        // Markdown (gramática de bloque): sin categorías propias en el
        // tema, se reasignan a la categoría genérica más parecida.
        "text.title" => "keyword",
        "text.literal" => "string",
        "text.uri" | "text.reference" => "constant",
        "punctuation.special" | "punctuation.delimiter" | "punctuation.bracket" => "operator",
        // HTML/CSS: nombres de etiqueta (`div`, seudo-elementos) como
        // palabra clave, nombres de atributo (`class`, `href`...) como
        // tipo, y nombres de propiedad CSS (`color`, `background`...)
        // como variable — ninguno tiene una categoría propia entre las 9
        // de `NOMBRES_RESALTADO`, así que se reasignan a la más
        // parecida visualmente.
        "tag" => "keyword",
        "attribute" => "type",
        "property" => "variable",
        // SQL: `(comment) @comment @spell` marca el mismo nodo con dos
        // capturas sobre el mismo nodo, y gana la última (ver
        // `tokens_de_capturas`) — si `spell` no estuviera entre las
        // reconocidas, el nodo quedaría sin resaltar, así que el comentario
        // desaparecía por completo en vez de solo perder el marcado de
        // "revisar ortografía" (que este editor no usa igual). Se
        // reconoce y se reasigna a la misma categoría que su compañera.
        "spell" => "comment",
        otro => otro,
    }
}

/// Un tramo de código fuente (en bytes, extremo derecho exclusivo) al que
/// le corresponde el token `nombre` (uno de [`NOMBRES_RESALTADO`]). No hay
/// solapes entre tokens y vienen en orden: cada tramo lleva el resaltado
/// más interno que le aplica (ver [`tokens_de_capturas`]).
#[derive(Debug, Clone, Copy)]
pub struct Token {
    pub inicio: usize,
    pub fin: usize,
    pub nombre: &'static str,
}

/// Resalta código fuente con tree-sitter, cacheando la query de cada
/// lenguaje (compilar un highlights.scm tiene costo, cachearla evita
/// repetirlo en cada frame).
///
/// Dos formas de uso:
///
/// - [`Resaltador::resaltar_documento`] para el archivo que se está
///   editando (la vista de código): guarda el árbol de cada documento y lo
///   re-parsea de forma INCREMENTAL después de cada edición (tree-sitter
///   reutiliza todo lo que no cambió), y solo corre la query sobre el
///   rango visible. Con archivos de miles de líneas, re-parsear todo en
///   cada tecla costaba ~40 ms por frame (BACKLOG.md P1 #14).
/// - [`Resaltador::resaltar`] para fragmentos chicos sin identidad propia
///   (los bloques de código de la vista Markdown): parsea desde cero,
///   con una cache por texto para no repetirlo en cada frame.
///
/// Las dos producen exactamente los mismos tokens que producía
/// `tree-sitter-highlight` (el motor anterior, sin parseo incremental ni
/// consulta por rango) — ver [`tokens_de_capturas`] y el test que los
/// compara lenguaje por lenguaje.
pub struct Resaltador {
    parser: Parser,
    cursor: QueryCursor,
    configs: HashMap<Lenguaje, ConfigLenguaje>,
    /// Estado incremental por documento, indexado por la clave que pasa
    /// la UI (la ruta del archivo).
    documentos: HashMap<String, Documento>,
    /// Contador para desalojar el documento usado hace más tiempo cuando
    /// se pasa de [`MAX_DOCUMENTOS`].
    reloj: u64,
    /// Últimos resultados de `resaltar`, el más reciente primero. La vista
    /// Markdown resalta sus bloques de código en cada frame; sin esto se
    /// re-parsearían aunque el texto fuera idéntico.
    cache: Vec<EntradaCache>,
    /// Tiempo máximo de un re-parseo incremental (ver
    /// [`PRESUPUESTO_PARSEO`]); campo en vez de usar la constante directo
    /// para que los tests puedan forzar el camino "se pasó del
    /// presupuesto".
    presupuesto_parseo: Duration,
}

struct ConfigLenguaje {
    language: Language,
    query: Query,
    /// Para cada captura de `query` (por índice), el nombre canónico que
    /// le corresponde (uno de [`NOMBRES_RESALTADO`]), o `None` si no se
    /// reconoce — mismo criterio que `HighlightConfiguration::configure`.
    nombres_por_captura: Vec<Option<&'static str>>,
}

struct Documento {
    lenguaje: Lenguaje,
    fuente: String,
    arbol: Tree,
    /// Revisión de `fuente` según quien llama (ver
    /// [`Resaltador::resaltar_documento_versionado`]), `None` si no la
    /// informó.
    revision: Option<u64>,
    /// `Some(momento)` si el último re-parseo intentado se pasó de
    /// [`PRESUPUESTO_PARSEO`] y se canceló: `arbol` quedó "atrasado"
    /// (editado con `Tree::edit` para que sus posiciones sigan al texto,
    /// pero sin re-parsear lo cambiado) y no se vuelve a intentar hasta
    /// que pase [`REINTENTO_TRAS_CANCELAR`] desde ese momento.
    parseo_cancelado: Option<Instant>,
    ultimo_uso: u64,
}

struct EntradaCache {
    lenguaje: Lenguaje,
    fuente: String,
    tokens: Vec<Token>,
}

/// Cuántos resultados recuerda `Resaltador::cache` — alcanza de sobra
/// para los bloques de código visibles de una vista Markdown, sin retener
/// memoria sin límite.
const TAMANO_CACHE: usize = 16;

/// Cuántos documentos con árbol incremental se recuerdan a la vez (cada
/// uno retiene una copia del texto y su árbol) — de sobra para los
/// archivos abiertos en los paneles de un split.
const MAX_DOCUMENTOS: usize = 16;

/// Tiempo máximo que puede tardar re-parsear un documento después de una
/// edición antes de cancelarlo y seguir con el árbol anterior (BACKLOG.md
/// P1 #14). Un archivo con MUCHOS errores de sintaxis para su lenguaje
/// (p. ej. Rust guardado como `.py`) hace que la recuperación de errores
/// de tree-sitter tarde lo mismo que parsear de cero aunque sea
/// incremental: con 10.000 líneas, ~1,7 s por tecla, con la pantalla
/// congelada. Un archivo válido nunca llega a esto: re-parsear tras una
/// tecla tarda unos pocos ms y pegar 10.000 líneas de una ~150 ms — de
/// ahí el margen. Solo aplica al re-parseo incremental: el primer parseo
/// de un documento no tiene árbol anterior al que volver y se hace
/// entero.
const PRESUPUESTO_PARSEO: Duration = Duration::from_millis(250);

/// Después de cancelar un re-parseo por [`PRESUPUESTO_PARSEO`], cuánto
/// esperar antes de volver a intentarlo. Mientras tanto cada frame usa el
/// árbol atrasado (colores aproximados en lo editado: en un archivo
/// lleno de errores ya lo eran), así que tipear sigue fluido, con una
/// pausa de a lo sumo el presupuesto cada tanto en vez de en cada tecla.
/// Un intento que termina a tiempo vuelve todo a la normalidad.
const REINTENTO_TRAS_CANCELAR: Duration = Duration::from_secs(2);

impl Resaltador {
    pub fn nuevo() -> Self {
        Self {
            parser: Parser::new(),
            cursor: QueryCursor::new(),
            configs: HashMap::new(),
            documentos: HashMap::new(),
            reloj: 0,
            cache: Vec::new(),
            presupuesto_parseo: PRESUPUESTO_PARSEO,
        }
    }

    fn construir_config(lenguaje: Lenguaje) -> Result<ConfigLenguaje> {
        let (language, highlights_query) = Self::gramatica_y_query(lenguaje);
        let query = Query::new(&language, &highlights_query)?;
        let nombres_por_captura = query.capture_names().iter().map(|nombre| nombre_para_captura(nombre)).collect();
        Ok(ConfigLenguaje { language, query, nombres_por_captura })
    }

    fn gramatica_y_query(lenguaje: Lenguaje) -> (Language, Cow<'static, str>) {
        // `Cow` porque la mayoría de las queries son el `&'static str` que
        // ya trae cada crate de gramática, pero TypeScript y C++ necesitan
        // una concatenada en el momento (ver los dos casos de abajo).
        let (language, highlights_query): (Language, Cow<'static, str>) = match lenguaje {
            Lenguaje::Rust => (tree_sitter_rust::LANGUAGE.into(), tree_sitter_rust::HIGHLIGHTS_QUERY.into()),
            Lenguaje::Python => {
                (tree_sitter_python::LANGUAGE.into(), tree_sitter_python::HIGHLIGHTS_QUERY.into())
            }
            Lenguaje::JavaScript => (
                tree_sitter_javascript::LANGUAGE.into(),
                tree_sitter_javascript::HIGHLIGHT_QUERY.into(),
            ),
            Lenguaje::Go => (tree_sitter_go::LANGUAGE.into(), tree_sitter_go::HIGHLIGHTS_QUERY.into()),
            // Solo la gramática de bloque: encabezados, listas, citas,
            // bloques de código, etc. El contenido inline (negrita,
            // cursiva, enlaces) necesita la gramática inyectada aparte y
            // llega junto con la vista Markdown doble de M3.
            Lenguaje::Markdown => {
                (tree_sitter_md::LANGUAGE.into(), tree_sitter_md::HIGHLIGHT_QUERY_BLOCK.into())
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
                format!("{}\n{}", tree_sitter_javascript::HIGHLIGHT_QUERY, tree_sitter_typescript::HIGHLIGHTS_QUERY)
                    .into(),
            ),
            Lenguaje::Java => (tree_sitter_java::LANGUAGE.into(), tree_sitter_java::HIGHLIGHTS_QUERY.into()),
            Lenguaje::C => (tree_sitter_c::LANGUAGE.into(), tree_sitter_c::HIGHLIGHT_QUERY.into()),
            // Mismo caso que TypeScript/JavaScript: el highlights.scm de
            // `tree-sitter-cpp` es un complemento sobre el de C (la
            // gramática de C++ extiende la de C).
            Lenguaje::Cpp => (
                tree_sitter_cpp::LANGUAGE.into(),
                format!("{}\n{}", tree_sitter_c::HIGHLIGHT_QUERY, tree_sitter_cpp::HIGHLIGHT_QUERY).into(),
            ),
            // `tree-sitter-kotlin-sg` (mantenida por ast-grep) en vez de la
            // original de fwcd/tree-sitter-kotlin: esa última fija
            // `tree-sitter` <0.23, incompatible con la 0.27 que usa el
            // resto del crate (conflicto de la librería nativa "links").
            // Su query es autocontenida (basada en la de nvim-treesitter).
            Lenguaje::Kotlin => {
                (tree_sitter_kotlin_sg::LANGUAGE.into(), tree_sitter_kotlin_sg::HIGHLIGHTS_QUERY.into())
            }
            Lenguaje::CSharp => {
                (tree_sitter_c_sharp::LANGUAGE.into(), tree_sitter_c_sharp::HIGHLIGHTS_QUERY.into())
            }
            Lenguaje::Ruby => (tree_sitter_ruby::LANGUAGE.into(), tree_sitter_ruby::HIGHLIGHTS_QUERY.into()),
            // La gramática "PHP" completa (a diferencia de "PHP_ONLY")
            // reconoce el archivo típico que arranca con `<?php` sin
            // necesitar tratarlo como HTML con PHP incrustado.
            Lenguaje::Php => {
                (tree_sitter_php::LANGUAGE_PHP.into(), tree_sitter_php::HIGHLIGHTS_QUERY.into())
            }
            Lenguaje::Html => (tree_sitter_html::LANGUAGE.into(), tree_sitter_html::HIGHLIGHTS_QUERY.into()),
            Lenguaje::Css => (tree_sitter_css::LANGUAGE.into(), tree_sitter_css::HIGHLIGHTS_QUERY.into()),
            // El nombre del crate es "sequel" ("SQL" se lee igual en
            // inglés) porque "tree-sitter-sql" ya estaba tomado en
            // crates.io por una gramática distinta/menos completa.
            //
            // Limitación conocida de su `highlights.scm`: el predicado
            // que distingue números (`#match? @number "^[-+]?%d+$"`)
            // usa `%d`, sintaxis de patrones de Lua (viene de
            // nvim-treesitter) — el motor de regex de los predicados de
            // `tree-sitter` en Rust no la entiende, así que nunca matchea
            // y los números terminan cayendo en la captura genérica
            // `(literal) @string` de la línea anterior. No es corregible
            // desde acá sin mantener un fork de la query; cosmético
            // nomás (los números igual se ven, solo que del color de
            // los strings en vez de un color propio).
            Lenguaje::Sql => (tree_sitter_sequel::LANGUAGE.into(), tree_sitter_sequel::HIGHLIGHTS_QUERY.into()),
        };

        (language, highlights_query)
    }

    fn preparar(&mut self, lenguaje: Lenguaje) -> Result<()> {
        if let std::collections::hash_map::Entry::Vacant(hueco) = self.configs.entry(lenguaje) {
            hueco.insert(Self::construir_config(lenguaje)?);
        }
        let config = &self.configs[&lenguaje];
        self.parser.set_language(&config.language)?;
        Ok(())
    }

    /// Resalta el código fuente completo, devolviendo los tramos con token
    /// reconocido en orden de aparición. Las regiones sin token (texto
    /// "plano" para el resaltador, como espacios o puntuación sin captura)
    /// simplemente no aparecen en el resultado.
    pub fn resaltar(&mut self, lenguaje: Lenguaje, fuente: &str) -> Result<Vec<Token>> {
        if let Some(pos) = self.cache.iter().position(|e| e.lenguaje == lenguaje && e.fuente == fuente) {
            let entrada = self.cache.remove(pos);
            let tokens = entrada.tokens.clone();
            self.cache.insert(0, entrada);
            return Ok(tokens);
        }
        let tokens = self.resaltar_sin_cache(lenguaje, fuente)?;
        self.cache.insert(0, EntradaCache { lenguaje, fuente: fuente.to_string(), tokens: tokens.clone() });
        self.cache.truncate(TAMANO_CACHE);
        Ok(tokens)
    }

    fn resaltar_sin_cache(&mut self, lenguaje: Lenguaje, fuente: &str) -> Result<Vec<Token>> {
        self.preparar(lenguaje)?;
        let arbol = self.parser.parse(fuente, None).ok_or_else(|| anyhow!("tree-sitter no pudo parsear"))?;
        let config = &self.configs[&lenguaje];
        Ok(tokens_de_capturas(&mut self.cursor, config, &arbol, fuente, 0..fuente.len()))
    }

    /// Resalta `rango` (offsets de byte) de un documento que se edita,
    /// identificado por `clave` (la ruta del archivo). La primera vez
    /// parsea el texto completo; las siguientes calcula qué cambió
    /// respecto del texto de la llamada anterior (prefijo y sufijo
    /// comunes: una tecla, un pegado o un "reemplazar todo" son siempre
    /// UNA región contigua o se tratan como tal) y re-parsea de forma
    /// incremental reutilizando el árbol anterior. Si el texto no cambió,
    /// no se parsea nada.
    ///
    /// Los tokens devueltos cubren al menos todo `rango` (pueden empezar
    /// antes o terminar después, p. ej. un comentario de varias líneas que
    /// arranca más arriba de lo visible), en orden y sin solaparse, igual
    /// que [`Resaltador::resaltar`].
    pub fn resaltar_documento(
        &mut self,
        clave: &str,
        lenguaje: Lenguaje,
        fuente: &str,
        rango: Range<usize>,
    ) -> Result<Vec<Token>> {
        self.resaltar_documento_tramos(clave, lenguaje, fuente, &[rango])
    }

    /// Igual que [`Resaltador::resaltar_documento`], pero para varios
    /// tramos del documento (en orden, sin solaparse) de una sola vez:
    /// con bloques plegados (BACKLOG.md P2 #7) lo visible son pedazos
    /// separados por miles de líneas ocultas, y resaltar desde la primera
    /// fila visible hasta la última recorrería también todo lo oculto. Un
    /// token que cruza de un tramo al siguiente (un comentario de bloque
    /// largo) se recorta para que el resultado siga ordenado y sin
    /// solapes.
    pub fn resaltar_documento_tramos(
        &mut self,
        clave: &str,
        lenguaje: Lenguaje,
        fuente: &str,
        tramos: &[Range<usize>],
    ) -> Result<Vec<Token>> {
        self.resaltar_documento_tramos_versionado(clave, lenguaje, None, || fuente.to_string(), tramos)
    }

    /// [`Resaltador::resaltar_documento`] sabiendo la revisión del texto
    /// (ver [`Resaltador::resaltar_documento_tramos_versionado`]).
    pub fn resaltar_documento_versionado(
        &mut self,
        clave: &str,
        lenguaje: Lenguaje,
        revision: Option<u64>,
        obtener_fuente: impl FnOnce() -> String,
        rango: Range<usize>,
    ) -> Result<Vec<Token>> {
        self.resaltar_documento_tramos_versionado(clave, lenguaje, revision, obtener_fuente, &[rango])
    }

    /// Igual que [`Resaltador::resaltar_documento_tramos`], pero sabiendo
    /// la revisión del texto (`tcode_core::Buffer::revision`): si coincide
    /// con la de la llamada anterior para la misma `clave`, el texto es
    /// el mismo y se consulta directo el árbol guardado — sin pedir el
    /// texto (`obtener_fuente` ni se llama, así que la vista de código se
    /// ahorra `Buffer::a_texto()`) ni compararlo byte a byte con el
    /// anterior. Es el caso de casi todos los frames: mover el cursor,
    /// scrollear, cualquier redibujado sin edición (BACKLOG.md P1 #14).
    /// `revision: None` = desconocida, se compara el texto como siempre.
    pub fn resaltar_documento_tramos_versionado(
        &mut self,
        clave: &str,
        lenguaje: Lenguaje,
        revision: Option<u64>,
        obtener_fuente: impl FnOnce() -> String,
        tramos: &[Range<usize>],
    ) -> Result<Vec<Token>> {
        self.actualizar_documento(clave, lenguaje, revision, obtener_fuente)?;
        let documento = &self.documentos[clave];
        let (arbol, fuente) = (&documento.arbol, documento.fuente.as_str());
        let config = &self.configs[&lenguaje];
        let mut tokens: Vec<Token> = Vec::new();
        for rango in tramos {
            let inicio = rango.start.min(fuente.len());
            let fin = rango.end.clamp(inicio, fuente.len());
            let hasta = tokens.last().map_or(0, |t| t.fin);
            for mut token in tokens_de_capturas(&mut self.cursor, config, arbol, fuente, inicio..fin) {
                if token.fin <= hasta {
                    continue;
                }
                token.inicio = token.inicio.max(hasta);
                tokens.push(token);
            }
        }
        Ok(tokens)
    }

    /// Rangos plegables del documento `clave` (BACKLOG.md P2 #7, ver
    /// `crate::plegado`), a partir del mismo árbol incremental que usa el
    /// resaltado — no hace falta volver a parsear el archivo. Sin
    /// lenguaje (o si el parseo falla), por indentación. Si el último
    /// re-parseo se pasó de [`PRESUPUESTO_PARSEO`] (un archivo lleno de
    /// errores de sintaxis), el árbol está atrasado y los rangos son
    /// aproximados en lo recién editado, igual que los colores.
    pub fn rangos_plegables(&mut self, clave: &str, lenguaje: Option<Lenguaje>, fuente: &str) -> Vec<RangoPlegable> {
        let Some(lenguaje) = lenguaje else {
            return rangos_por_indentacion(fuente);
        };
        if self.actualizar_documento(clave, lenguaje, None, || fuente.to_string()).is_err() {
            return rangos_por_indentacion(fuente);
        }
        rangos_de_arbol(&self.documentos[clave].arbol, lenguaje, fuente)
    }

    /// Deja en `self.documentos[clave]` el árbol del texto actual. Con
    /// `revision` igual a la de la llamada anterior no hace nada (ni pide
    /// el texto). Si no, pide el texto (`obtener_fuente`): la primera vez
    /// parsea el texto completo; las siguientes calcula qué cambió
    /// respecto del texto de la llamada anterior (prefijo y sufijo
    /// comunes: una tecla, un pegado o un "reemplazar todo" son siempre
    /// UNA región contigua o se tratan como tal) y re-parsea de forma
    /// incremental reutilizando el árbol anterior, con el tope de
    /// [`PRESUPUESTO_PARSEO`]. Si el texto no cambió, no se parsea nada.
    fn actualizar_documento(
        &mut self,
        clave: &str,
        lenguaje: Lenguaje,
        revision: Option<u64>,
        obtener_fuente: impl FnOnce() -> String,
    ) -> Result<()> {
        self.preparar(lenguaje)?;
        self.reloj += 1;

        let anterior = self.documentos.remove(clave).filter(|d| d.lenguaje == lenguaje);
        let documento = match anterior {
            Some(doc) if revision.is_some() && doc.revision == revision => doc,
            anterior => {
                let fuente = obtener_fuente();
                let (arbol, parseo_cancelado, revision) = match anterior {
                    // Mismo texto: si quien llama no sabe la revisión
                    // (`rangos_plegables`), se conserva la que ya había
                    // para que el próximo frame de la vista de código
                    // siga salteándose la comparación.
                    Some(doc) if doc.fuente == fuente => (doc.arbol, doc.parseo_cancelado, revision.or(doc.revision)),
                    Some(mut doc) => {
                        doc.arbol.edit(&edicion_entre(&doc.fuente, &fuente));
                        let reintentar = doc.parseo_cancelado.is_none_or(|t| t.elapsed() >= REINTENTO_TRAS_CANCELAR);
                        match reintentar.then(|| self.reparsear_con_presupuesto(&fuente, &doc.arbol)).flatten() {
                            Some(arbol) => (arbol, None, revision),
                            None if reintentar => (doc.arbol, Some(Instant::now()), revision),
                            None => (doc.arbol, doc.parseo_cancelado, revision),
                        }
                    }
                    None => (
                        self.parser.parse(&fuente, None).ok_or_else(|| anyhow!("tree-sitter no pudo parsear"))?,
                        None,
                        revision,
                    ),
                };
                Documento { lenguaje, fuente, arbol, revision, parseo_cancelado, ultimo_uso: 0 }
            }
        };

        if self.documentos.len() >= MAX_DOCUMENTOS {
            if let Some(mas_viejo) = self.documentos.iter().min_by_key(|(_, d)| d.ultimo_uso).map(|(k, _)| k.clone()) {
                self.documentos.remove(&mas_viejo);
            }
        }
        self.documentos.insert(clave.to_string(), Documento { ultimo_uso: self.reloj, ..documento });
        Ok(())
    }
}

impl Resaltador {
    /// Re-parsea `fuente` de forma incremental desde `anterior` (ya
    /// editado con `Tree::edit`), cancelando si tarda más que
    /// `presupuesto_parseo` (ver [`PRESUPUESTO_PARSEO`]) — `None` en ese
    /// caso. Un parseo cancelado deja estado a medio hacer en el parser
    /// (tree-sitter lo retomaría en la próxima llamada si el texto fuera
    /// el mismo, que acá no se puede garantizar): se descarta con `reset`.
    fn reparsear_con_presupuesto(&mut self, fuente: &str, anterior: &Tree) -> Option<Tree> {
        let inicio = Instant::now();
        let presupuesto = self.presupuesto_parseo;
        let mut vigilar = |_: &ParseState| {
            if inicio.elapsed() > presupuesto { ControlFlow::Break(()) } else { ControlFlow::Continue(()) }
        };
        let bytes = fuente.as_bytes();
        let arbol = self.parser.parse_with_options(
            &mut |i, _| if i < bytes.len() { &bytes[i..] } else { &[] },
            Some(anterior),
            Some(ParseOptions::new().progress_callback(&mut vigilar)),
        );
        if arbol.is_none() {
            self.parser.reset();
        }
        arbol
    }
}

/// Nombre canónico para una captura de un highlights.scm (p. ej.
/// `function.method.builtin`), con el mismo criterio de
/// `HighlightConfiguration::configure` de `tree-sitter-highlight`: de los
/// [`NOMBRES_RECONOCIDOS`] cuyas partes (separadas por `.`) aparecen
/// TODAS entre las partes de la captura, gana el que tiene más partes.
fn nombre_para_captura(captura: &str) -> Option<&'static str> {
    let partes: Vec<&str> = captura.split('.').collect();
    let mut mejor = None;
    let mut mejor_largo = 0;
    for (i, reconocido) in NOMBRES_RECONOCIDOS.iter().enumerate() {
        let partes_reconocido: Vec<&str> = reconocido.split('.').collect();
        if partes_reconocido.iter().all(|p| partes.contains(p)) && partes_reconocido.len() > mejor_largo {
            mejor = Some(i);
            mejor_largo = partes_reconocido.len();
        }
    }
    mejor.map(nombre_canonico)
}

/// La edición que convierte `viejo` en `nuevo`, como la necesita
/// `Tree::edit`: la región entre el prefijo común y el sufijo común más
/// largos. Recorre los dos textos una vez (O(n) comparando bytes — mucho
/// más barato que re-parsear), sin necesitar que el `Buffer` avise de
/// cada edición.
fn edicion_entre(viejo: &str, nuevo: &str) -> InputEdit {
    let (v, n) = (viejo.as_bytes(), nuevo.as_bytes());
    let prefijo = v.iter().zip(n).take_while(|(a, b)| a == b).count();
    let maximo_sufijo = v.len().min(n.len()) - prefijo;
    let sufijo = v.iter().rev().zip(n.iter().rev()).take(maximo_sufijo).take_while(|(a, b)| a == b).count();
    InputEdit {
        start_byte: prefijo,
        old_end_byte: v.len() - sufijo,
        new_end_byte: n.len() - sufijo,
        start_position: punto_en(v, prefijo),
        old_end_position: punto_en(v, v.len() - sufijo),
        new_end_position: punto_en(n, n.len() - sufijo),
    }
}

/// Fila/columna (columna en bytes, como la usa tree-sitter) del offset
/// `byte` dentro de `texto`.
fn punto_en(texto: &[u8], byte: usize) -> Point {
    let antes = &texto[..byte];
    let fila = antes.iter().filter(|&&b| b == b'\n').count();
    let columna = antes.iter().rposition(|&b| b == b'\n').map_or(byte, |i| byte - i - 1);
    Point { row: fila, column: columna }
}

/// Una captura de la query, copiada a un struct propio para poder mirar
/// las siguientes antes de procesarla (el iterador de tree-sitter es
/// "streaming", no permite `peek`).
struct Captura {
    inicio: usize,
    fin: usize,
    nodo: usize,
    indice: u32,
    id_match: u32,
}

/// Corre la query de resaltado sobre `rango` de `arbol` y arma los tokens
/// con el MISMO algoritmo que `tree-sitter-highlight` (versión 0.27,
/// `HighlightIter::next`, sin inyecciones ni variables locales, que este
/// editor nunca usó):
///
/// - Las capturas llegan ordenadas por posición; una pila guarda las que
///   siguen "abiertas". Cada tramo de texto toma el nombre de la captura
///   de más arriba de la pila: gana la más interna.
/// - Si varios patrones capturan el MISMO nodo, gana el último (y el
///   match del que se descarta se elimina entero, junto con sus otras
///   capturas pendientes).
/// - Una captura sin nombre reconocido no se apila (no tapa a la de
///   afuera).
fn tokens_de_capturas(
    cursor: &mut QueryCursor,
    config: &ConfigLenguaje,
    arbol: &Tree,
    fuente: &str,
    rango: Range<usize>,
) -> Vec<Token> {
    cursor.set_byte_range(rango);
    let mut capturas = Vec::new();
    let mut iterador = cursor.captures(&config.query, arbol.root_node(), fuente.as_bytes());
    while let Some((m, i)) = iterador.next() {
        let c = m.captures()[*i];
        capturas.push(Captura {
            inicio: c.node.start_byte(),
            fin: c.node.end_byte(),
            nodo: c.node.id(),
            indice: c.index,
            id_match: m.id(),
        });
    }

    let mut tokens = Vec::new();
    let mut pila: Vec<(usize, &'static str)> = Vec::new();
    let mut posicion = 0usize;
    let mut eliminados: HashSet<u32> = HashSet::new();
    // El tramo `desde..hasta` lleva el nombre de la captura más interna
    // abierta (la de más arriba de la pila), si hay alguna.
    fn emitir(tokens: &mut Vec<Token>, pila: &[(usize, &'static str)], desde: usize, hasta: usize) {
        if let Some(&(_, nombre)) = pila.last() {
            if desde < hasta {
                tokens.push(Token { inicio: desde, fin: hasta, nombre });
            }
        }
    }

    let mut i = 0;
    while i < capturas.len() {
        if eliminados.contains(&capturas[i].id_match) {
            i += 1;
            continue;
        }
        let mut actual = i;
        i += 1;

        // Cierra lo que terminó antes de que empiece esta captura.
        while let Some(&(fin, _)) = pila.last() {
            if fin > capturas[actual].inicio {
                break;
            }
            emitir(&mut tokens, &pila, posicion, fin);
            posicion = posicion.max(fin);
            pila.pop();
        }

        // Patrones posteriores sobre el mismo nodo: gana el último.
        while i < capturas.len() {
            if eliminados.contains(&capturas[i].id_match) {
                i += 1;
                continue;
            }
            if capturas[i].nodo != capturas[actual].nodo {
                break;
            }
            eliminados.insert(capturas[actual].id_match);
            actual = i;
            i += 1;
        }

        let c = &capturas[actual];
        if let Some(nombre) = config.nombres_por_captura[c.indice as usize] {
            emitir(&mut tokens, &pila, posicion, c.inicio);
            posicion = posicion.max(c.inicio);
            pila.push((c.fin, nombre));
        }
    }
    while let Some(&(fin, _)) = pila.last() {
        emitir(&mut tokens, &pila, posicion, fin);
        posicion = posicion.max(fin);
        pila.pop();
    }
    tokens
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

    fn muestra(lenguaje: Lenguaje) -> &'static str {
        match lenguaje {
            Lenguaje::Rust => include_str!("../../core/src/editor.rs"),
            Lenguaje::Markdown => include_str!("../../../MANUAL.md"),
            Lenguaje::Python => include_str!("../tests/muestras/muestra.py"),
            Lenguaje::JavaScript => include_str!("../tests/muestras/muestra.js"),
            Lenguaje::TypeScript => include_str!("../tests/muestras/muestra.ts"),
            Lenguaje::Go => include_str!("../tests/muestras/muestra.go"),
            Lenguaje::Java => include_str!("../tests/muestras/muestra.java"),
            Lenguaje::C => include_str!("../tests/muestras/muestra.c"),
            Lenguaje::Cpp => include_str!("../tests/muestras/muestra.cpp"),
            Lenguaje::Kotlin => include_str!("../tests/muestras/muestra.kt"),
            Lenguaje::CSharp => include_str!("../tests/muestras/muestra.cs"),
            Lenguaje::Ruby => include_str!("../tests/muestras/muestra.rb"),
            Lenguaje::Php => include_str!("../tests/muestras/muestra.php"),
            Lenguaje::Html => include_str!("../tests/muestras/muestra.html"),
            Lenguaje::Css => include_str!("../tests/muestras/muestra.css"),
            Lenguaje::Sql => include_str!("../tests/muestras/muestra.sql"),
        }
    }

    /// Los tokens que producía el motor anterior (`tree-sitter-highlight`
    /// completo, sin parseo incremental): la referencia contra la que se
    /// compara el motor actual.
    fn tokens_de_referencia(lenguaje: Lenguaje, fuente: &str) -> Vec<(usize, usize, &'static str)> {
        use tree_sitter_highlight::{Highlight, HighlightConfiguration, HighlightEvent, Highlighter};
        let (language, query) = Resaltador::gramatica_y_query(lenguaje);
        let mut config = HighlightConfiguration::new(language, "x", &query, "", "").unwrap();
        config.configure(NOMBRES_RECONOCIDOS);
        let mut highlighter = Highlighter::new();
        let mut tokens = Vec::new();
        let mut pila = Vec::new();
        for evento in highlighter.highlight(&config, fuente.as_bytes(), None, None, |_| None).unwrap() {
            match evento.unwrap() {
                HighlightEvent::HighlightStart(Highlight(idx)) => pila.push(nombre_canonico(idx)),
                HighlightEvent::HighlightEnd => {
                    pila.pop();
                }
                HighlightEvent::Source { start, end } => {
                    if let Some(&nombre) = pila.last() {
                        tokens.push((start, end, nombre));
                    }
                }
            }
        }
        tokens
    }

    fn como_tuplas(tokens: &[Token]) -> Vec<(usize, usize, &'static str)> {
        tokens.iter().map(|t| (t.inicio, t.fin, t.nombre)).collect()
    }

    /// Nombre de token de cada byte de `rango` — para comparar resultados
    /// que pueden estar partidos en tramos distintos pero pintan igual.
    fn nombre_por_byte(tokens: &[Token], rango: Range<usize>) -> Vec<Option<&'static str>> {
        let mut mapa = vec![None; rango.len()];
        for t in tokens {
            for b in t.inicio.max(rango.start)..t.fin.min(rango.end) {
                mapa[b - rango.start] = Some(t.nombre);
            }
        }
        mapa
    }

    #[test]
    fn mismos_tokens_que_tree_sitter_highlight_en_todos_los_lenguajes() {
        let mut resaltador = Resaltador::nuevo();
        for lenguaje in Lenguaje::TODOS {
            let fuente = muestra(lenguaje);
            let esperado = tokens_de_referencia(lenguaje, fuente);
            assert!(!esperado.is_empty(), "{lenguaje:?}: la muestra no tiene ningún token");
            let obtenido = como_tuplas(&resaltador.resaltar(lenguaje, fuente).unwrap());
            assert_eq!(obtenido, esperado, "{lenguaje:?}: difiere del motor anterior");
            let documento = resaltador.resaltar_documento("doc", lenguaje, fuente, 0..fuente.len()).unwrap();
            assert_eq!(como_tuplas(&documento), esperado, "{lenguaje:?}: resaltar_documento difiere");
        }
    }

    /// Con código VÁLIDO, el árbol incremental tiene que ser idéntico al
    /// de parsear de cero. Con errores de sintaxis no se puede exigir: la
    /// recuperación de errores de tree-sitter depende del camino de
    /// ediciones por el que se llegó al texto (pasa igual en Helix, Zed o
    /// Neovim), y se vuelve a alinear en cuanto el código es válido otra
    /// vez. Por eso cada edición aleatoria (que casi siempre rompe la
    /// sintaxis) se revierte con otra edición chica, y se compara en el
    /// texto válido resultante — eso sí ejercita `edicion_entre` y el
    /// parseo incremental con ediciones chicas en cualquier posición. Las
    /// líneas en blanco agregadas en los cortes de línea no rompen la
    /// sintaxis y se acumulan, para que los offsets del documento vayan
    /// cambiando entre paso y paso.
    #[test]
    fn el_resaltado_incremental_coincide_con_parsear_de_cero() {
        for lenguaje in [Lenguaje::Rust, Lenguaje::Python, Lenguaje::Markdown] {
            let mut resaltador = Resaltador::nuevo();
            let mut parser = Parser::new();
            parser.set_language(&Resaltador::construir_config(lenguaje).unwrap().language).unwrap();
            let mut texto = muestra(lenguaje).to_string();
            assert!(!parser.parse(&texto, None).unwrap().root_node().has_error(), "{lenguaje:?}: la muestra no es válida");
            // Generador pseudoaleatorio fijo: el test es reproducible.
            let mut semilla: u64 = 0x5eed;
            let mut azar = |max: usize| {
                semilla = semilla.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                ((semilla >> 33) as usize) % max.max(1)
            };
            let limite_de_caracter = |texto: &str, mut i: usize| {
                while !texto.is_char_boundary(i) {
                    i -= 1;
                }
                i
            };
            let insertos = ["x", "\n", "{", "}", "\"", "/*", "# ", "fn f() {}\n", "    ", "é"];
            let resaltar = |resaltador: &mut Resaltador, texto: &str, en: usize| {
                let rango = en.saturating_sub(1000)..(en + 1000).min(texto.len());
                (resaltador.resaltar_documento("doc", lenguaje, texto, rango.clone()).unwrap(), rango)
            };
            for paso in 0..60 {
                // Edición que rompe (o no) la sintaxis, y su reversión.
                let en = limite_de_caracter(&texto, azar(texto.len() + 1));
                let valido = texto.clone();
                match paso % 3 {
                    0 => texto.insert_str(en, insertos[azar(insertos.len())]),
                    1 => {
                        let fin = limite_de_caracter(&texto, (en + azar(40)).min(texto.len()));
                        texto.replace_range(en..fin, "");
                    }
                    _ => {
                        let desde = limite_de_caracter(&texto, azar(texto.len()));
                        let hasta = limite_de_caracter(&texto, (desde + azar(300)).min(texto.len()));
                        let bloque = texto[desde..hasta].to_string();
                        texto.insert_str(en, &bloque);
                    }
                }
                resaltar(&mut resaltador, &texto, en);
                texto = valido;

                // Línea en blanco en un corte de línea: sigue siendo válido.
                if let Some(corte) = texto[..en].rfind('\n') {
                    texto.insert(corte + 1, '\n');
                }

                let (incremental, rango) = resaltar(&mut resaltador, &texto, en);
                let arbol_incremental = resaltador.documentos["doc"].arbol.root_node().to_sexp();
                let arbol_de_cero = parser.parse(&texto, None).unwrap().root_node().to_sexp();
                assert!(arbol_incremental == arbol_de_cero, "{lenguaje:?}, paso {paso}: el árbol incremental difiere");
                let de_cero = tokens_de_referencia(lenguaje, &texto);
                let de_cero: Vec<Token> =
                    de_cero.into_iter().map(|(inicio, fin, nombre)| Token { inicio, fin, nombre }).collect();
                assert_eq!(
                    nombre_por_byte(&incremental, rango.clone()),
                    nombre_por_byte(&de_cero, rango),
                    "{lenguaje:?}, paso {paso}: el resaltado incremental difiere de parsear de cero"
                );
            }
        }
    }

    #[test]
    fn edicion_entre_calcula_la_region_cambiada() {
        let e = edicion_entre("ab\ncd\nef", "ab\nXYZ\nef");
        assert_eq!((e.start_byte, e.old_end_byte, e.new_end_byte), (3, 5, 6));
        assert_eq!(e.start_position, Point { row: 1, column: 0 });
        assert_eq!(e.old_end_position, Point { row: 1, column: 2 });
        assert_eq!(e.new_end_position, Point { row: 1, column: 3 });
        // Texto repetido: prefijo y sufijo no se pisan.
        let e = edicion_entre("aaa", "aaaa");
        assert_eq!((e.start_byte, e.old_end_byte, e.new_end_byte), (3, 3, 4));
    }

    #[test]
    fn la_cache_no_devuelve_tokens_de_otro_texto() {
        let mut resaltador = Resaltador::nuevo();
        let a = "fn main() {}\n";
        let b = "let x = 1;\n";
        let tokens_a = nombres_en(&resaltador.resaltar(Lenguaje::Rust, a).unwrap(), a);
        let tokens_b = nombres_en(&resaltador.resaltar(Lenguaje::Rust, b).unwrap(), b);
        assert_ne!(tokens_a, tokens_b);
        // Segunda vuelta: sale de la cache, idéntico al cálculo original.
        assert_eq!(nombres_en(&resaltador.resaltar(Lenguaje::Rust, a).unwrap(), a), tokens_a);
        assert_eq!(nombres_en(&resaltador.resaltar(Lenguaje::Rust, b).unwrap(), b), tokens_b);
        // Mismo texto, otro lenguaje: no debe reutilizar la entrada de Rust.
        let tokens_py = resaltador.resaltar(Lenguaje::Python, a).unwrap();
        assert_ne!(nombres_en(&tokens_py, a), tokens_a);
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
    fn resalta_la_segunda_tanda_de_lenguajes_agregados_en_m4() {
        let mut resaltador = Resaltador::nuevo();

        let kotlin = "// comentario\nfun saludar(): String = \"hola\"\n";
        let tokens_kt = resaltador.resaltar(Lenguaje::Kotlin, kotlin).unwrap();
        assert!(nombres_en(&tokens_kt, kotlin).iter().any(|(n, _)| *n == "comment"));
        assert!(nombres_en(&tokens_kt, kotlin).iter().any(|(n, _)| *n == "string"));

        let csharp = "class Principal {\n    // comentario\n    static void Main() { int x = 42; }\n}\n";
        let tokens_cs = resaltador.resaltar(Lenguaje::CSharp, csharp).unwrap();
        assert!(nombres_en(&tokens_cs, csharp).iter().any(|(n, _)| *n == "comment"));
        assert!(nombres_en(&tokens_cs, csharp).iter().any(|(n, texto)| *n == "number" && texto == "42"));

        let ruby = "# comentario\ndef saludar\n  \"hola\"\nend\n";
        let tokens_rb = resaltador.resaltar(Lenguaje::Ruby, ruby).unwrap();
        assert!(nombres_en(&tokens_rb, ruby).iter().any(|(n, _)| *n == "comment"));
        assert!(nombres_en(&tokens_rb, ruby).iter().any(|(n, _)| *n == "string"));

        let php = "<?php\n// comentario\n$x = 42;\necho \"hola\";\n";
        let tokens_php = resaltador.resaltar(Lenguaje::Php, php).unwrap();
        assert!(nombres_en(&tokens_php, php).iter().any(|(n, _)| *n == "comment"));
        assert!(nombres_en(&tokens_php, php).iter().any(|(n, texto)| *n == "number" && texto == "42"));
    }

    #[test]
    fn resalta_la_tercera_tanda_de_lenguajes_agregados_en_m4() {
        let mut resaltador = Resaltador::nuevo();

        let html = "<!-- comentario -->\n<div class=\"main\">hola</div>\n";
        let tokens_html = resaltador.resaltar(Lenguaje::Html, html).unwrap();
        assert!(nombres_en(&tokens_html, html).iter().any(|(n, _)| *n == "comment"));
        assert!(nombres_en(&tokens_html, html).iter().any(|(n, texto)| *n == "keyword" && texto == "div"));
        assert!(nombres_en(&tokens_html, html).iter().any(|(n, _)| *n == "string"));

        let css = "/* comentario */\n.main {\n  color: red;\n}\n";
        let tokens_css = resaltador.resaltar(Lenguaje::Css, css).unwrap();
        assert!(nombres_en(&tokens_css, css).iter().any(|(n, _)| *n == "comment"));
        assert!(nombres_en(&tokens_css, css).iter().any(|(n, texto)| *n == "variable" && texto == "color"));

        let sql = "-- comentario\nSELECT * FROM usuarios WHERE id = 42;\n";
        let tokens_sql = resaltador.resaltar(Lenguaje::Sql, sql).unwrap();
        assert!(nombres_en(&tokens_sql, sql).iter().any(|(n, _)| *n == "comment"));
        assert!(nombres_en(&tokens_sql, sql).iter().any(|(n, texto)| *n == "keyword" && texto.eq_ignore_ascii_case("select")));
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

    /// Con la misma revisión, `resaltar_documento_versionado` no pide el
    /// texto (la vista de código se ahorra `Buffer::a_texto()`); con una
    /// revisión nueva lo pide y re-parsea, igual que sin revisión.
    #[test]
    fn resaltar_documento_versionado_no_pide_el_texto_si_la_revision_no_cambio() {
        let mut resaltador = Resaltador::nuevo();
        let v1 = "fn a() {}\n";
        let v2 = "fn a() { 1 }\n// fin\n";
        let pedidos = std::cell::Cell::new(0);
        let mut resaltar = |revision: u64, fuente: &str| {
            let obtener = || {
                pedidos.set(pedidos.get() + 1);
                fuente.to_string()
            };
            resaltador.resaltar_documento_versionado("doc", Lenguaje::Rust, Some(revision), obtener, 0..usize::MAX)
        };

        let primero = resaltar(1, v1).unwrap();
        assert_eq!(pedidos.get(), 1);
        // Misma revisión: se reutiliza el texto guardado aunque quien
        // llama "tenga" otro (nunca pasa en la práctica: misma revisión
        // implica mismo texto) — prueba que de verdad no se lo pidió.
        let repetido = resaltar(1, "texto que no se debería leer").unwrap();
        assert_eq!(pedidos.get(), 1);
        assert_eq!(como_tuplas(&repetido), como_tuplas(&primero));

        let editado = resaltar(2, v2).unwrap();
        assert_eq!(pedidos.get(), 2);
        let desde_cero = Resaltador::nuevo().resaltar(Lenguaje::Rust, v2).unwrap();
        assert_eq!(como_tuplas(&editado), como_tuplas(&desde_cero));
    }

    /// Si re-parsear se pasa del presupuesto, se sigue con el árbol
    /// anterior (editado): nunca falla ni se rompe, los tokens quedan
    /// dentro del texto nuevo y en orden, y cuando un re-parseo vuelve a
    /// terminar a tiempo el resultado es otra vez exactamente el de
    /// parsear de cero.
    #[test]
    fn reparseo_que_se_pasa_del_presupuesto_sigue_con_el_arbol_anterior() {
        let mut resaltador = Resaltador::nuevo();
        // Lo bastante grande como para que el parser llegue a consultar
        // el callback de progreso antes de terminar.
        let base: String = (0..400).map(|i| format!("fn f{i}(a: i32) -> i32 {{ a + {i} }}\n")).collect();
        resaltador.resaltar_documento("doc", Lenguaje::Rust, &base, 0..base.len()).unwrap();

        resaltador.presupuesto_parseo = Duration::ZERO;
        let mut texto = base.clone();
        for i in 0..20 {
            let medio = (texto.len() / 2 + i..).find(|&b| texto.is_char_boundary(b)).unwrap();
            texto.insert_str(medio, "é😀 x");
            let tokens = resaltador.resaltar_documento("doc", Lenguaje::Rust, &texto, 0..texto.len()).unwrap();
            assert!(tokens.iter().all(|t| t.inicio <= t.fin && t.fin <= texto.len()));
            assert!(tokens.windows(2).all(|par| par[0].fin <= par[1].inicio));
        }
        assert!(resaltador.documentos["doc"].parseo_cancelado.is_some(), "el presupuesto 0 debería haber cancelado");

        // Presupuesto normal y ya pasado el tiempo de reintento: vuelve a
        // re-parsear y coincide con parsear de cero.
        resaltador.presupuesto_parseo = PRESUPUESTO_PARSEO;
        resaltador.documentos.get_mut("doc").unwrap().parseo_cancelado =
            Some(Instant::now() - REINTENTO_TRAS_CANCELAR);
        texto.push_str("// fin\n");
        let tokens = resaltador.resaltar_documento("doc", Lenguaje::Rust, &texto, 0..texto.len()).unwrap();
        let desde_cero = Resaltador::nuevo().resaltar(Lenguaje::Rust, &texto).unwrap();
        assert_eq!(como_tuplas(&tokens), como_tuplas(&desde_cero));
        assert!(resaltador.documentos["doc"].parseo_cancelado.is_none());
    }
}
