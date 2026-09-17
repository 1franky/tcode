use anyhow::{Context, Result};
use lsp_types::{Diagnostic, DiagnosticSeverity, PublishDiagnosticsParams};
use serde_json::Value;

/// Severidad de un diagnóstico, tal como la define LSP (error, aviso,
/// información, sugerencia).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severidad {
    Error,
    Advertencia,
    Informacion,
    Sugerencia,
}

/// Un diagnóstico ya extraído de un mensaje LSP, en coordenadas simples de
/// línea/columna base-0 (igual que `tcode_core::Cursor`: `columna` es un
/// índice de CARÁCTER Unicode, no de byte ni de unidad UTF-16).
///
/// LSP mide `character` en unidades UTF-16, no en caracteres Unicode —
/// `DiagnosticoSimple::desde` hace la conversión de verdad usando el texto
/// del documento (`parsear_diagnosticos` la recibe para eso), en vez de
/// tratar el offset UTF-16 como si ya fuera un índice de carácter. Sin
/// esa conversión, cualquier carácter fuera del plano básico (la mayoría
/// de los emoji, algunos alfabetos) antes de un diagnóstico en la misma
/// línea corría la columna reportada.
#[derive(Debug, Clone)]
pub struct DiagnosticoSimple {
    pub linea_inicio: u32,
    pub columna_inicio: u32,
    pub linea_fin: u32,
    pub columna_fin: u32,
    pub severidad: Severidad,
    pub mensaje: String,
}

impl DiagnosticoSimple {
    /// `lineas` es el texto del documento ya partido en líneas (mismo
    /// documento que el servidor tenía cuando calculó `d.range` — hace
    /// falta para la conversión UTF-16 → carácter de cada extremo del
    /// rango). Si `d.range` referencia una línea que ya no existe (una
    /// carrera entre una edición y un diagnóstico que llega tarde, poco
    /// común pero posible), esa columna se deja tal cual llegó de LSP en
    /// vez de fallar — un desalineamiento en un caso así ya de por sí
    /// transitorio es preferible a perder el diagnóstico entero.
    fn desde(d: &Diagnostic, lineas: &[&str]) -> Self {
        let columna = |linea: u32, caracter: u32| {
            lineas.get(linea as usize).map(|l| utf16_a_indice_char(l, caracter)).unwrap_or(caracter)
        };
        Self {
            linea_inicio: d.range.start.line,
            columna_inicio: columna(d.range.start.line, d.range.start.character),
            linea_fin: d.range.end.line,
            columna_fin: columna(d.range.end.line, d.range.end.character),
            severidad: match d.severity {
                Some(DiagnosticSeverity::WARNING) => Severidad::Advertencia,
                Some(DiagnosticSeverity::INFORMATION) => Severidad::Informacion,
                Some(DiagnosticSeverity::HINT) => Severidad::Sugerencia,
                _ => Severidad::Error, // por spec, la ausencia de `severity` se trata como error
            },
            mensaje: d.message.clone(),
        }
    }
}

/// Convierte un offset `character` de LSP (cuenta unidades de código
/// UTF-16 desde el inicio de `linea`) al índice de carácter Unicode
/// equivalente — la mayoría de los caracteres (ASCII y el resto del plano
/// básico) ocupan una sola unidad UTF-16, así que ahí el offset ya
/// coincide con el índice de carácter; los que quedan fuera del plano
/// básico (la mayoría de los emoji, algunos alfabetos históricos) ocupan
/// dos, corriendo todo lo que venga después si no se los cuenta bien.
/// `utf16_offset` más allá del final de la línea (algunos servidores lo
/// usan para "fin de línea") devuelve la cantidad total de caracteres.
fn utf16_a_indice_char(linea: &str, utf16_offset: u32) -> u32 {
    let mut unidades_utf16 = 0u32;
    for (idx_char, c) in linea.chars().enumerate() {
        if unidades_utf16 >= utf16_offset {
            return idx_char as u32;
        }
        unidades_utf16 += c.len_utf16() as u32;
    }
    linea.chars().count() as u32
}

/// Parsea los parámetros de una notificación `textDocument/publishDiagnostics`,
/// devolviendo el URI del archivo (para saber a qué panel corresponden) y
/// sus diagnósticos ya convertidos. `texto_documento` es el contenido tal
/// como lo tiene `tcode` en este momento (`EstadoLsp::sesion.ultimo_texto_
/// enviado`, ver `app/src/lsp.rs`) — se usa únicamente para la conversión
/// UTF-16 → carácter de las columnas, ver [`DiagnosticoSimple::desde`].
pub fn parsear_diagnosticos(params: &Value, texto_documento: &str) -> Result<(String, Vec<DiagnosticoSimple>)> {
    let params: PublishDiagnosticsParams =
        serde_json::from_value(params.clone()).context("params de publishDiagnostics con forma inesperada")?;
    let lineas: Vec<&str> = texto_documento.lines().collect();
    let diagnosticos = params.diagnostics.iter().map(|d| DiagnosticoSimple::desde(d, &lineas)).collect();
    Ok((params.uri.to_string(), diagnosticos))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parsea_diagnosticos_de_una_notificacion_real() {
        let params = json!({
            "uri": "file:///proyecto/main.py",
            "diagnostics": [
                {
                    "range": { "start": { "line": 4, "character": 2 }, "end": { "line": 4, "character": 10 } },
                    "severity": 1,
                    "message": "\"x\" no está definido"
                },
                {
                    "range": { "start": { "line": 7, "character": 0 }, "end": { "line": 7, "character": 5 } },
                    "severity": 2,
                    "message": "import sin usar"
                }
            ]
        });

        let texto = "\n\n\n\nx = noexiste\n\n\nimport os\n";
        let (uri, diagnosticos) = parsear_diagnosticos(&params, texto).unwrap();
        assert_eq!(uri, "file:///proyecto/main.py");
        assert_eq!(diagnosticos.len(), 2);
        assert_eq!(diagnosticos[0].severidad, Severidad::Error);
        assert_eq!(diagnosticos[0].linea_inicio, 4);
        assert_eq!(diagnosticos[0].mensaje, "\"x\" no está definido");
        assert_eq!(diagnosticos[1].severidad, Severidad::Advertencia);
    }

    #[test]
    fn sin_severidad_explicita_se_trata_como_error() {
        let params = json!({
            "uri": "file:///a.py",
            "diagnostics": [
                { "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 1 } }, "message": "algo" }
            ]
        });
        let (_, diagnosticos) = parsear_diagnosticos(&params, "x").unwrap();
        assert_eq!(diagnosticos[0].severidad, Severidad::Error);
    }

    #[test]
    fn lista_vacia_de_diagnosticos_es_valida() {
        let params = json!({ "uri": "file:///a.py", "diagnostics": [] });
        let (_, diagnosticos) = parsear_diagnosticos(&params, "").unwrap();
        assert!(diagnosticos.is_empty());
    }

    #[test]
    fn utf16_a_indice_char_con_solo_ascii_coincide_con_el_offset() {
        assert_eq!(utf16_a_indice_char("hola mundo", 5), 5);
    }

    #[test]
    fn utf16_a_indice_char_despues_de_un_emoji_no_coincide_con_el_offset_utf16() {
        // "😀" ocupa 2 unidades UTF-16 pero es 1 solo carácter Unicode:
        // el offset UTF-16 de "x" (después del emoji) es 3 (2 del emoji +
        // 1 de la "a" que lo precede), pero su índice de carácter es 2
        // ('a', '😀', 'x' → índices 0, 1, 2).
        let linea = "a😀x";
        assert_eq!(utf16_a_indice_char(linea, 3), 2);
    }

    #[test]
    fn utf16_a_indice_char_mas_alla_del_final_da_el_total_de_caracteres() {
        assert_eq!(utf16_a_indice_char("abc", 100), 3);
    }

    #[test]
    fn parsear_diagnosticos_corrige_la_columna_tras_un_emoji_en_la_misma_linea() {
        // El servidor reporta el error empezando en el offset UTF-16 3
        // (justo en la "x", tras "a😀") — sin la conversión, columna_inicio
        // quedaría en 3 (un carácter de más) en vez de 2.
        let params = json!({
            "uri": "file:///a.py",
            "diagnostics": [
                { "range": { "start": { "line": 0, "character": 3 }, "end": { "line": 0, "character": 4 } }, "message": "algo" }
            ]
        });
        let (_, diagnosticos) = parsear_diagnosticos(&params, "a😀x = 1").unwrap();
        assert_eq!(diagnosticos[0].columna_inicio, 2);
        assert_eq!(diagnosticos[0].columna_fin, 3);
    }

    #[test]
    fn parsear_diagnosticos_con_linea_fuera_de_rango_deja_la_columna_sin_convertir() {
        // Línea 5 no existe en un texto de una sola línea: en vez de
        // fallar, se queda con el offset UTF-16 tal cual llegó.
        let params = json!({
            "uri": "file:///a.py",
            "diagnostics": [
                { "range": { "start": { "line": 5, "character": 2 }, "end": { "line": 5, "character": 3 } }, "message": "algo" }
            ]
        });
        let (_, diagnosticos) = parsear_diagnosticos(&params, "una sola línea").unwrap();
        assert_eq!(diagnosticos[0].columna_inicio, 2);
    }
}
