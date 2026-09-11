use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use tcode_core::{Editor, Modo};
use tcode_lsp::{DiagnosticoSimple, Severidad};

use crate::Paleta;

/// Barra de estado inferior (PLAN.md §1): posición del cursor, total de
/// líneas, codificación, fin de línea, lenguaje detectado, modo y — desde
/// M2 — el conteo de diagnósticos LSP del archivo. La rama git llega en
/// fase posterior.
pub fn dibujar(
    frame: &mut Frame,
    area: Rect,
    editor: &Editor,
    ruta_mostrada: &str,
    paleta: &Paleta,
    diagnosticos: &[DiagnosticoSimple],
) {
    let cursor = editor.cursor();
    let marca_modificado = if editor.buffer().modificado() {
        " ●"
    } else {
        ""
    };
    let lenguaje = detectar_lenguaje(ruta_mostrada);
    let modo = match editor.modo() {
        Modo::Insertar => "INSERTAR",
    };
    let resumen_diagnosticos = resumir_diagnosticos(diagnosticos);

    let texto = format!(
        " {ruta}{marca_modificado}  │  Ln {ln}, Col {col}  │  {total} líneas  │  UTF-8  │  LF  │  {lenguaje}{resumen_diagnosticos}  │  {modo} ",
        ruta = ruta_mostrada,
        ln = cursor.linea + 1,
        col = cursor.columna + 1,
        total = editor.buffer().num_lineas(),
    );

    frame.render_widget(
        Paragraph::new(Line::from(texto))
            .style(Style::default().bg(paleta.statusbar_fondo).fg(paleta.statusbar_texto)),
        area,
    );
}

/// " │ 2 errores, 1 aviso" (o "" si no hay LSP corriendo o no hay nada que
/// reportar — no se distingue "sin LSP" de "sin errores", como VSCode.
fn resumir_diagnosticos(diagnosticos: &[DiagnosticoSimple]) -> String {
    if diagnosticos.is_empty() {
        return String::new();
    }
    let errores = diagnosticos.iter().filter(|d| d.severidad == Severidad::Error).count();
    let avisos = diagnosticos.iter().filter(|d| d.severidad == Severidad::Advertencia).count();

    let mut partes = Vec::new();
    if errores > 0 {
        partes.push(format!("{errores} error{}", if errores == 1 { "" } else { "es" }));
    }
    if avisos > 0 {
        partes.push(format!("{avisos} aviso{}", if avisos == 1 { "" } else { "s" }));
    }
    if partes.is_empty() {
        return String::new();
    }
    format!("  │  {}", partes.join(", "))
}

fn detectar_lenguaje(ruta: &str) -> &'static str {
    let extension = ruta.rsplit('.').next().unwrap_or("");
    match extension {
        "rs" => "Rust",
        "py" => "Python",
        "js" | "jsx" => "JavaScript",
        "ts" | "tsx" => "TypeScript",
        "go" => "Go",
        "md" => "Markdown",
        "toml" => "TOML",
        "json" => "JSON",
        "c" | "h" => "C",
        "cpp" | "hpp" | "cc" => "C++",
        "cs" => "C#",
        "java" => "Java",
        "kt" => "Kotlin",
        "rb" => "Ruby",
        "php" => "PHP",
        "html" => "HTML",
        "css" => "CSS",
        "sql" => "SQL",
        "csv" => "CSV",
        "tsv" => "TSV",
        _ => "Texto sin formato",
    }
}
