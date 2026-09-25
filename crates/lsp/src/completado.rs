//! Autocompletado vía `textDocument/completion` (BACKLOG.md P1 #17): la
//! traducción de la respuesta a [`ItemCompletado`] y el estado del popup
//! ([`EstadoCompletado`]: filtro local mientras se sigue escribiendo,
//! selección). Sin UI ni `tcode-core`: `app` decide cuándo pedir y cómo
//! aplicar el item elegido sobre el buffer.

use lsp_types::{Position, Range};
use serde_json::Value;

/// Cuántos items se muestran como mucho después de filtrar: pyright
/// devuelve miles con el prefijo vacío (todo lo importable), y nadie
/// recorre más que unas decenas con las flechas.
pub const MAX_ITEMS_VISIBLES: usize = 200;

/// Un item de la lista de completado, ya sin las partes del protocolo
/// que `tcode` no usa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemCompletado {
    pub etiqueta: String,
    /// `detail` (la firma o el tipo, en una línea), si vino.
    pub detalle: Option<String>,
    /// Nombre corto del `CompletionItemKind` ("fn", "var", "mod"...).
    pub tipo: &'static str,
    /// Contra qué se filtra (`filterText`, o la etiqueta).
    pub texto_filtro: String,
    /// Orden sin filtro (`sortText`, o la etiqueta).
    pub orden: String,
    /// El texto a insertar: `textEdit.newText`, `insertText` o la
    /// etiqueta, en ese orden, ya sin marcas de snippet.
    pub insertar: String,
    /// Rango de `textEdit` (o de `insert` en un `InsertReplaceEdit`), si
    /// vino: qué parte de la línea reemplaza `insertar`. Sin él se
    /// reemplaza la palabra a la izquierda del cursor.
    pub rango: Option<Range>,
    /// `additionalTextEdits` (un `use`/`import` al principio del archivo,
    /// típicamente), ya sin marcas de snippet.
    pub adicionales: Vec<(Range, String)>,
}

/// Parsea la respuesta a `textDocument/completion`
/// (`CompletionItem[] | CompletionList | null`). Devuelve los items y si
/// la lista está incompleta (`isIncomplete`: el servidor recortó y
/// conviene volver a pedir al seguir escribiendo).
pub fn parsear_completado(resultado: &Value) -> (Vec<ItemCompletado>, bool) {
    let (lista, incompleto) = match resultado {
        Value::Array(lista) => (lista.as_slice(), false),
        Value::Object(_) => (
            resultado["items"].as_array().map(Vec::as_slice).unwrap_or_default(),
            resultado["isIncomplete"].as_bool().unwrap_or(false),
        ),
        _ => (&[][..], false),
    };
    (lista.iter().filter_map(parsear_item).collect(), incompleto)
}

fn parsear_item(item: &Value) -> Option<ItemCompletado> {
    let etiqueta = item["label"].as_str()?.to_string();
    // `insertTextFormat` 2 = snippet: `tcode` anuncia no soportarlos,
    // pero un servidor puede mandarlos igual.
    let es_snippet = item["insertTextFormat"].as_u64() == Some(2);
    let limpiar = |texto: &str| {
        let texto = texto.replace("\r\n", "\n");
        if es_snippet { snippet_a_texto(&texto) } else { texto }
    };
    let edicion = &item["textEdit"];
    let rango_edicion = edicion.get("range").or_else(|| edicion.get("insert"));
    let rango = rango_edicion.and_then(rango_de);
    let insertar = edicion["newText"]
        .as_str()
        .or_else(|| item["insertText"].as_str())
        .map(limpiar)
        .unwrap_or_else(|| etiqueta.clone());
    let adicionales = item["additionalTextEdits"]
        .as_array()
        .map(|lista| {
            lista
                .iter()
                .filter_map(|e| {
                    let rango = rango_de(&e["range"])?;
                    Some((rango, limpiar(e["newText"].as_str()?)))
                })
                .collect()
        })
        .unwrap_or_default();
    Some(ItemCompletado {
        detalle: item["detail"].as_str().map(|d| d.lines().next().unwrap_or("").trim().to_string()).filter(|d| !d.is_empty()),
        tipo: nombre_tipo(item["kind"].as_u64().unwrap_or(0)),
        texto_filtro: item["filterText"].as_str().map(str::to_string).unwrap_or_else(|| etiqueta.clone()),
        orden: item["sortText"].as_str().map(str::to_string).unwrap_or_else(|| etiqueta.clone()),
        insertar,
        rango,
        adicionales,
        etiqueta,
    })
}

/// Un `Range` LSP leído directo del JSON, sin clonar el valor para
/// deserializarlo (con miles de items, se nota).
fn rango_de(valor: &Value) -> Option<Range> {
    let posicion = |p: &Value| Some(Position { line: p["line"].as_u64()? as u32, character: p["character"].as_u64()? as u32 });
    Some(Range { start: posicion(&valor["start"])?, end: posicion(&valor["end"])? })
}

/// Nombre corto de un `CompletionItemKind` para la columna de tipo del
/// popup (ASCII, ancho fijo razonable).
fn nombre_tipo(kind: u64) -> &'static str {
    match kind {
        1 => "texto",
        2 => "método",
        3 => "fn",
        4 => "constructor",
        5 => "campo",
        6 => "var",
        7 => "clase",
        8 => "interfaz",
        9 => "módulo",
        10 => "propiedad",
        11 => "unidad",
        12 => "valor",
        13 => "enum",
        14 => "palabra",
        15 => "snippet",
        16 => "color",
        17 => "archivo",
        18 => "referencia",
        19 => "carpeta",
        20 => "variante",
        21 => "const",
        22 => "struct",
        23 => "evento",
        24 => "operador",
        25 => "tipo",
        _ => "",
    }
}

/// Texto plano de un snippet LSP: `$1`/`$0` desaparecen, `${1:valor}` y
/// `${1|a,b|}` dejan su valor por defecto (la primera opción), y `\$`,
/// `\}`, `\\` quedan como el carácter solo. `tcode` no tiene saltos entre
/// placeholders: el cursor queda al final de lo insertado.
pub fn snippet_a_texto(snippet: &str) -> String {
    fn procesar(caracteres: &mut std::iter::Peekable<std::str::Chars>, salida: &mut String, dentro: bool) {
        while let Some(c) = caracteres.next() {
            match c {
                '\\' => {
                    if let Some(siguiente) = caracteres.next() {
                        salida.push(siguiente);
                    }
                }
                '}' if dentro => return,
                '$' => match caracteres.peek() {
                    Some(d) if d.is_ascii_digit() => {
                        while caracteres.peek().is_some_and(|d| d.is_ascii_digit()) {
                            caracteres.next();
                        }
                    }
                    Some('{') => {
                        caracteres.next();
                        // Número o nombre de variable, después `:`
                        // (valor), `|` (opciones) o `}` (vacío).
                        while caracteres.peek().is_some_and(|d| d.is_ascii_alphanumeric() || *d == '_') {
                            caracteres.next();
                        }
                        match caracteres.next() {
                            Some(':') => procesar(caracteres, salida, true),
                            Some('|') => {
                                let mut primera = true;
                                for d in caracteres.by_ref() {
                                    match d {
                                        '|' => {
                                            primera = false;
                                        }
                                        '}' => break,
                                        ',' => primera = false,
                                        _ if primera => salida.push(d),
                                        _ => {}
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => salida.push('$'),
                },
                _ => salida.push(c),
            }
        }
    }
    let mut salida = String::with_capacity(snippet.len());
    procesar(&mut snippet.chars().peekable(), &mut salida, false);
    salida
}

/// Un item que pasa el filtro: su índice en la lista completa y las
/// posiciones (en caracteres de la etiqueta) que coincidieron.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemVisible {
    pub indice: usize,
    pub posiciones: Vec<usize>,
}

/// Estado del popup de completado: la lista que devolvió el servidor, lo
/// que pasa el filtro con lo escrito desde que empezó la palabra, y la
/// selección. `inicio_palabra` (byte absoluto en el buffer) y `linea`
/// son el contexto que deja `app` al abrirlo, para saber si el cursor
/// sigue en la misma palabra y qué reemplazar al aceptar.
#[derive(Debug, Default)]
pub struct EstadoCompletado {
    activo: bool,
    items: Vec<ItemCompletado>,
    visibles: Vec<ItemVisible>,
    /// Todos los items que pasaron el último filtro (sin el recorte a
    /// [`MAX_ITEMS_VISIBLES`]) y con qué prefijo: si el próximo lo
    /// extiende (se siguió escribiendo), solo hace falta mirar estos —
    /// lo que no coincidía con `pu` tampoco coincide con `pus`.
    coincidentes: Vec<usize>,
    ultimo_prefijo: String,
    seleccion: usize,
    pub incompleto: bool,
    pub ruta: String,
    pub linea: usize,
    pub inicio_palabra: usize,
    /// Columna UTF-16 de la posición en la que se pidió el completado:
    /// un `textEdit` que termina más allá reemplaza también lo que
    /// seguía a la derecha del cursor.
    pub caracter_pedido: u32,
}

impl EstadoCompletado {
    pub fn nuevo() -> Self {
        Self::default()
    }

    pub fn activo(&self) -> bool {
        self.activo
    }

    /// Abre (o reemplaza) la lista con `items`, filtrada por `prefijo`
    /// (lo escrito de la palabra hasta el cursor). Si nada pasa el
    /// filtro no queda abierto: un popup vacío no le sirve a nadie.
    pub fn abrir(&mut self, items: Vec<ItemCompletado>, incompleto: bool, prefijo: &str) {
        self.coincidentes = (0..items.len()).collect();
        self.ultimo_prefijo.clear();
        self.items = items;
        self.incompleto = incompleto;
        self.activo = true;
        self.filtrar(prefijo);
    }

    pub fn cerrar(&mut self) {
        self.activo = false;
        self.items.clear();
        self.visibles.clear();
        self.coincidentes.clear();
    }

    /// Vuelve a filtrar con `prefijo`: con prefijo vacío, todo en el
    /// orden del servidor (`sortText`); con prefijo, lo que coincide con
    /// `tcode-fuzzy` de mejor a peor puntaje (empates por `sortText`).
    /// Se cierra solo si no queda nada.
    pub fn filtrar(&mut self, prefijo: &str) {
        if !prefijo.starts_with(self.ultimo_prefijo.as_str()) {
            self.coincidentes = (0..self.items.len()).collect();
        }
        self.ultimo_prefijo = prefijo.to_string();
        let items = &self.items;
        let mut candidatos: Vec<(i32, &str, ItemVisible)> = self
            .coincidentes
            .iter()
            .filter_map(|&indice| {
                let item = &items[indice];
                let coincidencia = tcode_fuzzy::coincidir(prefijo, &item.texto_filtro)?;
                // Las posiciones se marcan sobre la etiqueta, que es lo
                // que se ve; si el filtro es otro texto, sin marcas.
                let posiciones = if item.texto_filtro == item.etiqueta { coincidencia.posiciones } else { Vec::new() };
                Some((coincidencia.puntaje, item.orden.as_str(), ItemVisible { indice, posiciones }))
            })
            .collect();
        candidatos.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
        let mut coincidentes: Vec<usize> = candidatos.iter().map(|(_, _, v)| v.indice).collect();
        coincidentes.sort_unstable();
        candidatos.truncate(MAX_ITEMS_VISIBLES);
        self.visibles = candidatos.into_iter().map(|(_, _, visible)| visible).collect();
        self.coincidentes = coincidentes;
        self.seleccion = 0;
        if self.visibles.is_empty() {
            self.cerrar();
        }
    }

    pub fn visibles(&self) -> &[ItemVisible] {
        &self.visibles
    }

    pub fn item(&self, indice: usize) -> &ItemCompletado {
        &self.items[indice]
    }

    pub fn seleccion(&self) -> usize {
        self.seleccion
    }

    pub fn mover_abajo(&mut self) {
        if !self.visibles.is_empty() {
            self.seleccion = (self.seleccion + 1) % self.visibles.len();
        }
    }

    pub fn mover_arriba(&mut self) {
        if !self.visibles.is_empty() {
            self.seleccion = (self.seleccion + self.visibles.len() - 1) % self.visibles.len();
        }
    }

    /// Cierra el popup y devuelve el item elegido.
    pub fn aceptar(&mut self) -> Option<ItemCompletado> {
        let item = self.visibles.get(self.seleccion).map(|v| self.items[v.indice].clone());
        self.cerrar();
        item
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(etiqueta: &str, orden: &str) -> Value {
        json!({ "label": etiqueta, "sortText": orden, "kind": 2 })
    }

    #[test]
    fn parsea_lista_y_arreglo() {
        let (items, incompleto) = parsear_completado(&json!({ "isIncomplete": true, "items": [item("push", "b")] }));
        assert!(incompleto);
        assert_eq!(items[0].etiqueta, "push");
        assert_eq!(items[0].tipo, "método");
        assert_eq!(items[0].insertar, "push");
        let (items, incompleto) = parsear_completado(&json!([item("a", "a"), { "sin": "label" }]));
        assert!(!incompleto);
        assert_eq!(items.len(), 1);
        assert!(parsear_completado(&Value::Null).0.is_empty());
    }

    #[test]
    fn text_edit_insert_text_y_adicionales() {
        let rango = json!({ "start": { "line": 2, "character": 4 }, "end": { "line": 2, "character": 6 } });
        let respuesta = json!([
            { "label": "len", "textEdit": { "range": rango, "newText": "len()" }, "detail": "fn len(&self) -> usize\nmás" },
            { "label": "x", "insertText": "x_largo", "filterText": "xl" },
            { "label": "y", "textEdit": { "insert": rango, "replace": rango, "newText": "y" },
              "additionalTextEdits": [{ "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } }, "newText": "use a::y;\r\n" }] },
        ]);
        let (items, _) = parsear_completado(&respuesta);
        assert_eq!(items[0].insertar, "len()");
        assert_eq!(items[0].rango.unwrap().start.character, 4);
        assert_eq!(items[0].detalle.as_deref(), Some("fn len(&self) -> usize"));
        assert_eq!(items[1].insertar, "x_largo");
        assert_eq!(items[1].texto_filtro, "xl");
        assert!(items[2].rango.is_some(), "InsertReplaceEdit usa el rango de insert");
        assert_eq!(items[2].adicionales[0].1, "use a::y;\n");
    }

    #[test]
    fn snippets_como_texto_plano() {
        assert_eq!(snippet_a_texto("foo(${1:a}, ${2:b})$0"), "foo(a, b)");
        assert_eq!(snippet_a_texto("if $1 {\n\t$0\n}"), "if  {\n\t\n}");
        assert_eq!(snippet_a_texto("${1|uno,dos|} \\$x ${2:${3:anidado}}"), "uno $x anidado");
        assert_eq!(snippet_a_texto("cuesta \\\\ $ 5"), "cuesta \\ $ 5");
        let (items, _) = parsear_completado(&json!([{ "label": "f", "insertText": "f($1)", "insertTextFormat": 2 }]));
        assert_eq!(items[0].insertar, "f()");
    }

    fn estado_con(etiquetas: &[(&str, &str)]) -> EstadoCompletado {
        let lista: Vec<Value> = etiquetas.iter().map(|(e, o)| item(e, o)).collect();
        let mut estado = EstadoCompletado::nuevo();
        estado.abrir(parsear_completado(&Value::Array(lista)).0, false, "");
        estado
    }

    #[test]
    fn sin_prefijo_respeta_el_orden_del_servidor() {
        let estado = estado_con(&[("zeta", "1"), ("alfa", "2")]);
        let etiquetas: Vec<&str> = estado.visibles().iter().map(|v| estado.item(v.indice).etiqueta.as_str()).collect();
        assert_eq!(etiquetas, ["zeta", "alfa"]);
    }

    #[test]
    fn filtrar_mientras_se_escribe_y_cerrar_si_no_queda_nada() {
        let mut estado = estado_con(&[("push", "1"), ("pop", "2"), ("len", "3")]);
        estado.filtrar("p");
        assert_eq!(estado.visibles().len(), 2);
        estado.filtrar("pu");
        assert_eq!(estado.visibles().len(), 1);
        assert_eq!(estado.visibles()[0].posiciones, vec![0, 1]);
        estado.filtrar("pux");
        assert!(!estado.activo());
    }

    #[test]
    fn borrar_una_letra_vuelve_a_mirar_todos_los_items() {
        let mut estado = estado_con(&[("push", "1"), ("pop", "2"), ("len", "3")]);
        estado.filtrar("pu");
        assert_eq!(estado.visibles().len(), 1);
        estado.filtrar("p");
        assert_eq!(estado.visibles().len(), 2, "pop vuelve aunque no coincidía con pu");
        estado.filtrar("");
        assert_eq!(estado.visibles().len(), 3);
    }

    #[test]
    fn navegar_da_la_vuelta_y_aceptar_cierra() {
        let mut estado = estado_con(&[("a", "1"), ("b", "2"), ("c", "3")]);
        estado.mover_arriba();
        assert_eq!(estado.seleccion(), 2);
        estado.mover_abajo();
        estado.mover_abajo();
        assert_eq!(estado.aceptar().unwrap().etiqueta, "b");
        assert!(!estado.activo());
        assert_eq!(estado.aceptar(), None);
    }
}
