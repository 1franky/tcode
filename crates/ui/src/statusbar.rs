use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use tcode_config::ConfigInterfaz;
use tcode_core::{Editor, Modo};
use tcode_lsp::{DiagnosticoSimple, Severidad};

use crate::Paleta;

/// Barra de estado inferior (PLAN.md §1): posición del cursor, total de
/// líneas, codificación, fin de línea, lenguaje detectado, modo y — desde
/// M2 — el conteo de diagnósticos LSP del archivo. La rama git llega en
/// fase posterior. Cada uno de esos elementos (salvo la ruta y el total
/// de líneas, que se consideran base) se puede ocultar desde la sección
/// "Interfaz" del panel de administración (PLAN.md §5.5, M4) — `interfaz`
/// es lo que decide cuáles entran.
pub fn dibujar(
    frame: &mut Frame,
    area: Rect,
    editor: &Editor,
    ruta_mostrada: &str,
    paleta: &Paleta,
    diagnosticos: &[DiagnosticoSimple],
    interfaz: &ConfigInterfaz,
) {
    let cursor = editor.cursor();
    // ASCII a propósito (`*`, no `●`): la statusbar se redibuja en cada
    // frame junto a varios segmentos más, así que un carácter de ancho
    // "ambiguo" que una terminal renderice distinto a como lo calcula
    // `ratatui` correría todo lo que sigue — ver `panel_archivos` sobre
    // el mismo problema, sospechoso de un bug de desalineación en
    // Windows Terminal.
    let marca_modificado = if editor.buffer().modificado() { " *" } else { "" };

    let mut partes = vec![format!("{ruta_mostrada}{marca_modificado}")];

    if interfaz.statusbar_posicion_cursor {
        // Solo se muestra la cantidad de cursores cuando hay más de uno
        // activo (`Ctrl+D`/`Ctrl+Shift+L`/`Ctrl+Alt+↑↓`, PLAN.md §11 M3)
        // — con uno solo es ruido, ya lo dice "Ln/Col".
        let resumen_cursores =
            if editor.tiene_multiples_cursores() { format!(", {} cursores", editor.cursores().len()) } else { String::new() };
        partes.push(format!("Ln {}, Col {}{resumen_cursores}", cursor.linea + 1, cursor.columna + 1));
    }

    partes.push(format!("{} líneas", editor.buffer().num_lineas()));

    if interfaz.statusbar_codificacion {
        partes.push("UTF-8".to_string());
    }
    if interfaz.statusbar_eol {
        partes.push("LF".to_string());
    }
    if interfaz.statusbar_lenguaje {
        partes.push(detectar_lenguaje(ruta_mostrada).to_string());
    }
    if interfaz.statusbar_diagnosticos {
        if let Some(resumen) = resumir_diagnosticos(diagnosticos) {
            partes.push(resumen);
        }
    }
    if interfaz.statusbar_modo {
        partes.push(
            match editor.modo() {
                Modo::Insertar => "INSERTAR",
            }
            .to_string(),
        );
    }

    // Separador ASCII (`|`, no `│`): el de box-drawing tiene ancho
    // "ambiguo" en Unicode (ver `marca_modificado` más arriba y
    // `panel_archivos::dibujar`) y aparece varias veces por frame en la
    // fila que más segmentos concatena de toda la UI — el candidato más
    // fuerte para un desalineamiento progresivo hacia la derecha si una
    // terminal lo renderiza con un ancho distinto al que calcula
    // `ratatui`.
    let texto = format!(" {} ", partes.join("  |  "));

    frame.render_widget(
        Paragraph::new(Line::from(texto))
            .style(Style::default().bg(paleta.statusbar_fondo).fg(paleta.statusbar_texto)),
        area,
    );
}

/// "2 errores, 1 aviso" (o `None` si no hay LSP corriendo o no hay nada
/// que reportar — no se distingue "sin LSP" de "sin errores", como
/// VSCode).
fn resumir_diagnosticos(diagnosticos: &[DiagnosticoSimple]) -> Option<String> {
    if diagnosticos.is_empty() {
        return None;
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
        return None;
    }
    Some(partes.join(", "))
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
