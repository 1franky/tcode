use anyhow::{bail, Context, Result};
use lsp_types::{Position, TextEdit};
use serde_json::Value;

use crate::diagnostico::utf16_a_indice_char;

/// Una edición de texto ya traducida de coordenadas LSP (línea +
/// carácter UTF-16) a offsets de bytes absolutos sobre el documento —
/// el mismo sistema de coordenadas que usan `tcode_core::Buffer::
/// reemplazar_rango_bytes`, la búsqueda y el multi-cursor, así que quien
/// la recibe (`app`) la puede aplicar sin saber nada de UTF-16.
/// `[inicio_byte, fin_byte)` siempre cae en límites de carácter válidos
/// del documento con el que se calculó.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdicionTexto {
    pub inicio_byte: usize,
    pub fin_byte: usize,
    pub texto: String,
}

/// Si el servidor anunció `documentFormattingProvider` en la respuesta a
/// `initialize` (`resultado_initialize` es el `result` completo, con
/// `capabilities` adentro). La spec permite tanto `true` como un objeto
/// de opciones (`DocumentFormattingOptions`) para decir que sí; `false`
/// o la ausencia del campo, que no — pyright, por ejemplo, no formatea
/// y no lo anuncia (BACKLOG.md P2 #5).
pub fn soporta_formateo(resultado_initialize: &Value) -> bool {
    match resultado_initialize.pointer("/capabilities/documentFormattingProvider") {
        Some(Value::Bool(soporta)) => *soporta,
        Some(Value::Object(_)) => true,
        _ => false,
    }
}

/// Parsea la respuesta a `textDocument/formatting` (`TextEdit[] | null`)
/// y la traduce a [`EdicionTexto`] sobre `texto_documento` — el texto
/// EXACTO que el servidor tenía cuando formateó (el último que se le
/// mandó por `didOpen`/`didChange`): todas las posiciones de la
/// respuesta se refieren a ese documento original, no a uno con las
/// ediciones anteriores ya aplicadas (spec LSP, `TextEdit[]`). `null`
/// (algunos servidores lo usan para "no hay nada que cambiar") da una
/// lista vacía.
///
/// El `newText` de cada edición se normaliza a `\n`: el buffer de
/// `tcode` nunca contiene `\r` (ver `tcode_core::Eol`) y un servidor que
/// devuelva `\r\n` (porque el proyecto usa CRLF, por ejemplo) rompería
/// esa invariante — el CRLF original se reconstruye igual al guardar.
pub fn parsear_ediciones_formateo(resultado: &Value, texto_documento: &str) -> Result<Vec<EdicionTexto>> {
    if resultado.is_null() {
        return Ok(Vec::new());
    }
    let ediciones: Vec<TextEdit> =
        serde_json::from_value(resultado.clone()).context("respuesta de textDocument/formatting con forma inesperada")?;
    let inicios = inicios_de_linea(texto_documento);
    ediciones
        .iter()
        .map(|e| {
            let inicio_byte = posicion_a_byte(texto_documento, &inicios, e.range.start);
            let fin_byte = posicion_a_byte(texto_documento, &inicios, e.range.end);
            if fin_byte < inicio_byte {
                bail!("edición de formateo con el rango invertido");
            }
            let texto = e.new_text.replace("\r\n", "\n").replace('\r', "\n");
            Ok(EdicionTexto { inicio_byte, fin_byte, texto })
        })
        .collect()
}

/// Offset de bytes donde empieza cada línea de `texto` (la primera en 0,
/// y una más después de cada `\n` — incluida la línea vacía que queda
/// tras un `\n` final, que LSP también cuenta como línea).
fn inicios_de_linea(texto: &str) -> Vec<usize> {
    std::iter::once(0).chain(texto.match_indices('\n').map(|(i, _)| i + 1)).collect()
}

/// Traduce una `Position` de LSP (línea + carácter UTF-16) a un offset de
/// bytes absoluto en `texto`, reusando la misma conversión UTF-16 →
/// carácter de los diagnósticos (`utf16_a_indice_char`). Como pide la
/// spec, un `character` más allá del final de la línea se recorta al
/// final de esa línea (sin el `\n`), y una `line` más allá del final del
/// documento se interpreta como el final del documento — algunos
/// servidores devuelven un único `TextEdit` "todo el archivo" con un
/// rango de fin generoso en vez de calcularlo exacto.
fn posicion_a_byte(texto: &str, inicios: &[usize], posicion: Position) -> usize {
    let linea = posicion.line as usize;
    let Some(&inicio) = inicios.get(linea) else { return texto.len() };
    let fin = inicios.get(linea + 1).map(|siguiente| siguiente - 1).unwrap_or(texto.len());
    let contenido = &texto[inicio..fin];
    let idx_char = utf16_a_indice_char(contenido, posicion.character) as usize;
    inicio + contenido.char_indices().nth(idx_char).map(|(byte, _)| byte).unwrap_or(contenido.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn edicion(l1: u32, c1: u32, l2: u32, c2: u32, texto: &str) -> Value {
        json!({ "range": { "start": { "line": l1, "character": c1 }, "end": { "line": l2, "character": c2 } }, "newText": texto })
    }

    #[test]
    fn soporta_formateo_con_true_o_con_objeto_de_opciones() {
        assert!(soporta_formateo(&json!({ "capabilities": { "documentFormattingProvider": true } })));
        assert!(soporta_formateo(&json!({ "capabilities": { "documentFormattingProvider": { "workDoneProgress": false } } })));
    }

    #[test]
    fn no_soporta_formateo_con_false_o_sin_el_campo() {
        assert!(!soporta_formateo(&json!({ "capabilities": { "documentFormattingProvider": false } })));
        assert!(!soporta_formateo(&json!({ "capabilities": { "hoverProvider": true } })));
        assert!(!soporta_formateo(&Value::Null));
    }

    #[test]
    fn respuesta_null_no_tiene_ediciones() {
        assert!(parsear_ediciones_formateo(&Value::Null, "fn main() {}\n").unwrap().is_empty());
    }

    #[test]
    fn traduce_lineas_y_columnas_ascii_a_bytes() {
        let texto = "fn main(){\nlet x=1;\n}\n";
        let respuesta = json!([edicion(0, 9, 0, 9, " "), edicion(1, 0, 1, 0, "    ")]);
        let ediciones = parsear_ediciones_formateo(&respuesta, texto).unwrap();
        assert_eq!(ediciones[0], EdicionTexto { inicio_byte: 9, fin_byte: 9, texto: " ".into() });
        assert_eq!(ediciones[1], EdicionTexto { inicio_byte: 11, fin_byte: 11, texto: "    ".into() });
    }

    #[test]
    fn columna_utf16_tras_emoji_y_acentos_cae_en_el_byte_correcto() {
        // "😀" = 2 unidades UTF-16 / 4 bytes, "é" = 1 unidad UTF-16 / 2
        // bytes: el carácter UTF-16 4 (la "x") empieza en el byte 7.
        let texto = "a😀éx=1\n";
        let ediciones = parsear_ediciones_formateo(&json!([edicion(0, 4, 0, 5, "y")]), texto).unwrap();
        assert_eq!(ediciones[0].inicio_byte, 7);
        assert_eq!(ediciones[0].fin_byte, 8);
        assert_eq!(&texto[7..8], "x");
    }

    #[test]
    fn caracter_mas_alla_del_final_de_linea_se_recorta_sin_comerse_el_salto() {
        let texto = "abc\ndef\n";
        let ediciones = parsear_ediciones_formateo(&json!([edicion(0, 99, 0, 99, ";")]), texto).unwrap();
        assert_eq!(ediciones[0].inicio_byte, 3); // antes del '\n', no después
    }

    #[test]
    fn linea_mas_alla_del_final_es_el_final_del_documento() {
        // El típico "reemplazar todo el archivo" con un fin generoso.
        let texto = "abc\ndef";
        let ediciones = parsear_ediciones_formateo(&json!([edicion(0, 0, 50, 0, "nuevo")]), texto).unwrap();
        assert_eq!(ediciones[0], EdicionTexto { inicio_byte: 0, fin_byte: texto.len(), texto: "nuevo".into() });
    }

    #[test]
    fn linea_vacia_tras_el_salto_final_existe() {
        let texto = "abc\n";
        let ediciones = parsear_ediciones_formateo(&json!([edicion(1, 0, 1, 0, "x")]), texto).unwrap();
        assert_eq!(ediciones[0].inicio_byte, 4);
    }

    #[test]
    fn crlf_en_el_texto_nuevo_se_normaliza_a_lf() {
        let ediciones = parsear_ediciones_formateo(&json!([edicion(0, 0, 0, 0, "a\r\nb\rc")]), "").unwrap();
        assert_eq!(ediciones[0].texto, "a\nb\nc");
    }

    #[test]
    fn rango_invertido_es_un_error() {
        assert!(parsear_ediciones_formateo(&json!([edicion(1, 0, 0, 0, "")]), "a\nb\n").is_err());
    }

    #[test]
    fn respuesta_con_forma_inesperada_es_un_error() {
        assert!(parsear_ediciones_formateo(&json!({ "no": "es una lista" }), "").is_err());
    }
}
