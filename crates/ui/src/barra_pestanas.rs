//! Barra de pestañas de un panel (BACKLOG.md P3 #10): una fila arriba del
//! código con un título por documento abierto en ese panel, la activa
//! resaltada y un `*` en los que tienen cambios sin guardar. Todo ASCII
//! (`*`, `<`, `>`): la barra se redibuja en cada frame, y un carácter de
//! ancho "ambiguo" desalinea Windows Terminal (ver `statusbar` y
//! `panel_archivos` sobre el mismo problema).

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::{Paleta, PanelEditor};

/// Dibuja la barra en `area` (una fila). `activa` es el índice de la
/// pestaña visible; `panel_activo` si este panel tiene el foco — la
/// pestaña activa de un panel sin foco se resalta sin negrita, para que
/// con splits se note en cuál se está escribiendo. Cuesta O(pestañas)
/// por frame, nunca O(archivo): solo lee rutas y el flag de modificado.
pub fn dibujar(
    frame: &mut Frame,
    area: Rect,
    documentos: &[PanelEditor],
    activa: usize,
    panel_activo: bool,
    paleta: &Paleta,
) {
    let rutas: Vec<&str> = documentos.iter().map(|d| d.ruta_mostrada.as_str()).collect();
    let etiquetas: Vec<String> = titulos(&rutas)
        .into_iter()
        .zip(documentos)
        .map(|(titulo, d)| if d.editor.buffer().modificado() { format!(" {titulo}* ") } else { format!(" {titulo} ") })
        .collect();
    let anchos: Vec<usize> = etiquetas.iter().map(|e| Span::raw(e.as_str()).width()).collect();
    let (inicio, fin) = ventana_visible(&anchos, activa, area.width as usize);

    let estilo_barra = Style::default().bg(paleta.statusbar_fondo).fg(paleta.statusbar_texto);
    let mut estilo_activa = Style::default().bg(paleta.fondo).fg(paleta.texto);
    if panel_activo {
        estilo_activa = estilo_activa.add_modifier(Modifier::BOLD);
    }

    let mut spans = Vec::new();
    if inicio > 0 {
        spans.push(Span::styled("<", estilo_barra));
    }
    for (i, etiqueta) in etiquetas.into_iter().enumerate().take(fin).skip(inicio) {
        spans.push(Span::styled(etiqueta, if i == activa { estilo_activa } else { estilo_barra }));
    }
    if fin < anchos.len() {
        // Pegado al borde derecho: el hueco que quede en el medio se
        // rellena con el fondo de la barra.
        let usado: usize = spans.iter().map(Span::width).sum();
        let hueco = (area.width as usize).saturating_sub(usado + 1);
        spans.push(Span::styled(" ".repeat(hueco), estilo_barra));
        spans.push(Span::styled(">", estilo_barra));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)).style(estilo_barra), area);
}

/// Título de cada pestaña: el nombre del archivo, o `carpeta/nombre` si
/// hay otro con el mismo nombre en el mismo panel (dos `mod.rs`, dos
/// `README.md`...) — así se distinguen sin ocupar la ruta entera. Un
/// documento sin ruta es "[Sin nombre]".
pub(crate) fn titulos(rutas: &[&str]) -> Vec<String> {
    let nombres: Vec<&str> = rutas.iter().map(|r| nombre_de(r)).collect();
    rutas
        .iter()
        .zip(&nombres)
        .map(|(ruta, nombre)| {
            if ruta.is_empty() {
                return "[Sin nombre]".to_string();
            }
            let repetido = nombres.iter().filter(|n| *n == nombre).count() > 1;
            match carpeta_de(ruta) {
                Some(carpeta) if repetido => format!("{carpeta}/{nombre}"),
                _ => nombre.to_string(),
            }
        })
        .collect()
}

/// Último componente de la ruta, aceptando `/` y `\` (Windows).
fn nombre_de(ruta: &str) -> &str {
    ruta.rsplit(['/', '\\']).next().unwrap_or(ruta)
}

/// Nombre de la carpeta que contiene el archivo, si la ruta la dice.
fn carpeta_de(ruta: &str) -> Option<&str> {
    let (padre, _) = ruta.rsplit_once(['/', '\\'])?;
    let carpeta = nombre_de(padre);
    (!carpeta.is_empty()).then_some(carpeta)
}

/// Qué pestañas entran en `ancho` columnas (rango `inicio..fin`),
/// garantizando que la activa siempre se vea. Si no entran todas se
/// reservan dos columnas para los indicadores `<`/`>` y la ventana se
/// corre lo justo para que la activa quede adentro; después se suman
/// pestañas a la derecha mientras quepan. Sin estado: se recalcula en
/// cada frame, así que cambiar de pestaña o cerrar una nunca deja la
/// barra desplazada a un lugar raro.
pub(crate) fn ventana_visible(anchos: &[usize], activa: usize, ancho: usize) -> (usize, usize) {
    if anchos.iter().sum::<usize>() <= ancho {
        return (0, anchos.len());
    }
    let disponible = ancho.saturating_sub(2);
    let mut inicio = 0;
    while inicio < activa && anchos[inicio..=activa].iter().sum::<usize>() > disponible {
        inicio += 1;
    }
    let mut fin = activa + 1;
    let mut usado: usize = anchos[inicio..fin].iter().sum();
    while fin < anchos.len() && usado + anchos[fin] <= disponible {
        usado += anchos[fin];
        fin += 1;
    }
    (inicio, fin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titulos_usan_el_nombre_y_desambiguan_con_la_carpeta() {
        let rutas = ["src/a/mod.rs", "src/b/mod.rs", "src/main.rs", ""];
        assert_eq!(titulos(&rutas), vec!["a/mod.rs", "b/mod.rs", "main.rs", "[Sin nombre]"]);
    }

    #[test]
    fn titulo_repetido_sin_carpeta_queda_como_esta() {
        assert_eq!(titulos(&["mod.rs", "x/mod.rs"]), vec!["mod.rs", "x/mod.rs"]);
    }

    #[test]
    fn titulos_aceptan_barras_de_windows() {
        assert_eq!(titulos(&["C:\\proy\\main.rs"]), vec!["main.rs"]);
    }

    #[test]
    fn si_entran_todas_se_ven_todas() {
        assert_eq!(ventana_visible(&[5, 5, 5], 2, 15), (0, 3));
    }

    #[test]
    fn la_activa_al_final_corre_la_ventana_a_la_derecha() {
        // 10 columnas, 2 reservadas para los indicadores: entran dos de 4.
        assert_eq!(ventana_visible(&[4, 4, 4, 4], 3, 10), (2, 4));
    }

    #[test]
    fn la_activa_al_principio_muestra_lo_que_quepa_a_su_derecha() {
        assert_eq!(ventana_visible(&[4, 4, 4, 4], 0, 10), (0, 2));
    }

    #[test]
    fn una_activa_mas_ancha_que_la_barra_igual_se_ve_sola() {
        assert_eq!(ventana_visible(&[4, 30, 4], 1, 10), (1, 2));
    }
}
