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
/// línea/columna base-0 (igual que `tcode_core::Cursor`).
///
/// Nota: LSP mide `character` en unidades UTF-16, no en caracteres
/// Unicode; aquí se trata como un offset de carácter simple — una
/// simplificación válida para código fuente mayormente ASCII. Para líneas
/// con caracteres fuera del plano básico (emoji, ciertos alfabetos) podría
/// desalinearse ligeramente; ajustarlo con precisión queda pendiente.
#[derive(Debug, Clone)]
pub struct DiagnosticoSimple {
    pub linea_inicio: u32,
    pub columna_inicio: u32,
    pub linea_fin: u32,
    pub columna_fin: u32,
    pub severidad: Severidad,
    pub mensaje: String,
}

impl From<&Diagnostic> for DiagnosticoSimple {
    fn from(d: &Diagnostic) -> Self {
        Self {
            linea_inicio: d.range.start.line,
            columna_inicio: d.range.start.character,
            linea_fin: d.range.end.line,
            columna_fin: d.range.end.character,
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

/// Parsea los parámetros de una notificación `textDocument/publishDiagnostics`,
/// devolviendo el URI del archivo (para saber a qué panel corresponden) y
/// sus diagnósticos ya convertidos.
pub fn parsear_diagnosticos(params: &Value) -> Result<(String, Vec<DiagnosticoSimple>)> {
    let params: PublishDiagnosticsParams =
        serde_json::from_value(params.clone()).context("params de publishDiagnostics con forma inesperada")?;
    let diagnosticos = params.diagnostics.iter().map(DiagnosticoSimple::from).collect();
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

        let (uri, diagnosticos) = parsear_diagnosticos(&params).unwrap();
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
        let (_, diagnosticos) = parsear_diagnosticos(&params).unwrap();
        assert_eq!(diagnosticos[0].severidad, Severidad::Error);
    }

    #[test]
    fn lista_vacia_de_diagnosticos_es_valida() {
        let params = json!({ "uri": "file:///a.py", "diagnostics": [] });
        let (_, diagnosticos) = parsear_diagnosticos(&params).unwrap();
        assert!(diagnosticos.is_empty());
    }
}
