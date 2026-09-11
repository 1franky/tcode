use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use tcode_core::{Editor, Modo};

use crate::Paleta;

/// Barra de estado inferior (PLAN.md §1): posición del cursor, total de
/// líneas, codificación, fin de línea, lenguaje detectado y modo. La rama
/// git y el estado del LSP llegan en fases posteriores.
pub fn dibujar(frame: &mut Frame, area: Rect, editor: &Editor, ruta_mostrada: &str, paleta: &Paleta) {
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

    let texto = format!(
        " {ruta}{marca_modificado}  │  Ln {ln}, Col {col}  │  {total} líneas  │  UTF-8  │  LF  │  {lenguaje}  │  {modo} ",
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
