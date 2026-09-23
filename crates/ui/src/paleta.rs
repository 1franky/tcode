use std::collections::HashMap;

use anyhow::Result;
use ratatui::style::{Color, Modifier, Style};

use tcode_config::{EstiloToken, Tema};
use tcode_syntax::NOMBRES_RESALTADO;

/// Colores de un [`Tema`] ya resueltos a tipos de `ratatui` una sola vez al
/// cargar el tema, en vez de re-parsear hex (o volver a decidir negrita vs.
/// cursiva) en cada frame.
pub struct Paleta {
    pub fondo: Color,
    pub texto: Color,
    pub cursor: Color,
    pub linea_actual: Color,
    /// Fondo de selección de texto (multi-cursor, PLAN.md §11 M3) — el
    /// campo `ui.selection` del tema existe desde M0 pero no se había
    /// conectado a nada hasta que hubo selección real que dibujar.
    pub seleccion: Color,
    pub statusbar_fondo: Color,
    pub statusbar_texto: Color,
    /// Colores del gutter de números de línea (`config.editor.
    /// numeros_de_linea`, PLAN.md §5 M4) — el campo del tema existe desde
    /// M0 pero no se había conectado a nada hasta que hubo un gutter real
    /// que dibujar.
    pub numero_linea: Color,
    pub numero_linea_activo: Color,
    pub diagnostico_error: Color,
    pub diagnostico_advertencia: Color,
    pub diagnostico_info: Color,
    pub diagnostico_sugerencia: Color,
    /// Fondo resaltado de la coincidencia de búsqueda sobre la que está
    /// el cursor de búsqueda, y de las demás coincidencias visibles
    /// (`Ctrl+F`/`Ctrl+H`, PLAN.md §4).
    pub busqueda_actual: Color,
    pub busqueda_otras: Color,
    /// Fondo de la columna de la regla vertical (`config.editor.
    /// columna_regla`, BACKLOG.md P1 #5) — no es un campo nuevo de
    /// `Tema` (evitaría tener que editar los 13 temas existentes):
    /// [`color_regla_vertical`] lo deriva de `background`/`foreground`,
    /// así se adapta solo a temas oscuros y claros por igual.
    pub regla_vertical: Color,
    /// Estilo por token de sintaxis (PLAN.md §7), indexado por uno de los
    /// nombres canónicos de [`tcode_syntax::NOMBRES_RESALTADO`].
    sintaxis: HashMap<&'static str, Style>,
}

impl Paleta {
    pub fn desde_tema(tema: &Tema) -> Result<Self> {
        let rgb = |c: (u8, u8, u8)| Color::Rgb(c.0, c.1, c.2);
        let fondo_rgb = tema.ui.background_rgb()?;
        let texto_rgb = tema.ui.foreground_rgb()?;

        let mut sintaxis = HashMap::new();
        for nombre in NOMBRES_RESALTADO {
            if let Some(estilo_token) = campo_sintaxis(tema, nombre) {
                if let Ok(estilo) = estilo_desde_token(estilo_token) {
                    sintaxis.insert(*nombre, estilo);
                }
            }
        }

        Ok(Self {
            fondo: rgb(tema.ui.background_rgb()?),
            texto: rgb(tema.ui.foreground_rgb()?),
            cursor: rgb(tema.ui.cursor_rgb()?),
            linea_actual: rgb(tema.ui.linea_actual_rgb()?),
            seleccion: rgb(tema.ui.seleccion_rgb()?),
            statusbar_fondo: rgb(tema.statusbar.background_rgb()?),
            statusbar_texto: rgb(tema.statusbar.foreground_rgb()?),
            numero_linea: rgb(tema.ui.numero_linea_rgb()?),
            numero_linea_activo: rgb(tema.ui.numero_linea_activa_rgb()?),
            diagnostico_error: color_diagnostico(&tema.diagnostics.error, Color::Red),
            diagnostico_advertencia: color_diagnostico(&tema.diagnostics.warning, Color::Yellow),
            diagnostico_info: color_diagnostico(&tema.diagnostics.info, Color::Cyan),
            diagnostico_sugerencia: color_diagnostico(&tema.diagnostics.hint, Color::Gray),
            busqueda_actual: color_diagnostico(&tema.search.coincidencia_actual, Color::Yellow),
            busqueda_otras: color_diagnostico(&tema.search.otras_coincidencias, Color::DarkGray),
            regla_vertical: color_regla_vertical(fondo_rgb, texto_rgb),
            sintaxis,
        })
    }

    /// Paleta de emergencia (blanco sobre negro, sin colores de sintaxis)
    /// para el caso extremo en que ni el tema configurado ni el tema por
    /// defecto se puedan resolver a colores válidos. No debería usarse
    /// nunca en la práctica: los 3 temas embebidos están validados por
    /// tests en `tcode-config`.
    pub fn basica() -> Self {
        Self {
            fondo: Color::Black,
            texto: Color::White,
            cursor: Color::White,
            linea_actual: Color::DarkGray,
            seleccion: Color::Blue,
            statusbar_fondo: Color::DarkGray,
            statusbar_texto: Color::White,
            numero_linea: Color::DarkGray,
            numero_linea_activo: Color::White,
            diagnostico_error: Color::Red,
            diagnostico_advertencia: Color::Yellow,
            diagnostico_info: Color::Cyan,
            diagnostico_sugerencia: Color::Gray,
            busqueda_actual: Color::Yellow,
            busqueda_otras: Color::DarkGray,
            regla_vertical: Color::DarkGray,
            sintaxis: HashMap::new(),
        }
    }

    /// Estilo para un token de sintaxis (uno de [`NOMBRES_RESALTADO`]); si
    /// el tema no define ese token, cae al color de texto normal.
    pub fn estilo_sintaxis(&self, nombre: &str) -> Style {
        self.sintaxis
            .get(nombre)
            .copied()
            .unwrap_or_else(|| Style::default().fg(self.texto))
    }
}

fn campo_sintaxis<'t>(tema: &'t Tema, nombre: &str) -> Option<&'t EstiloToken> {
    match nombre {
        "keyword" => tema.syntax.keyword.as_ref(),
        "string" => tema.syntax.string.as_ref(),
        "number" => tema.syntax.number.as_ref(),
        "comment" => tema.syntax.comment.as_ref(),
        "function" => tema.syntax.function.as_ref(),
        "type" => tema.syntax.tipo.as_ref(),
        "variable" => tema.syntax.variable.as_ref(),
        "constant" => tema.syntax.constant.as_ref(),
        "operator" => tema.syntax.operator.as_ref(),
        _ => None,
    }
}

/// Resuelve un color opcional del tema (`tema.diagnostics.*`, siempre
/// `Option<String>` porque no todos los temas de usuario lo definen) a un
/// `Color`, cayendo a `fallback` si falta o no se puede parsear.
fn color_diagnostico(valor: &Option<String>, fallback: Color) -> Color {
    valor
        .as_deref()
        .and_then(|hex| tcode_config::analizar_color_hex(hex).ok())
        .map(|(r, g, b)| Color::Rgb(r, g, b))
        .unwrap_or(fallback)
}

/// Deriva el color de fondo de la regla vertical (BACKLOG.md P1 #5) de
/// los dos colores que cualquier tema ya tiene, en vez de agregar un
/// campo `Tema` nuevo que habría que sumarle a los 13 temas existentes
/// (12 embebidos + los que un usuario haya duplicado a mano):
/// mayormente el color de fondo, con un toque del color de texto — sutil
/// en cualquier tema (oscuro o claro) sin que nadie tenga que
/// configurarlo, y sin arriesgarse a un color fijo que quede invisible
/// en un tema y chillón en otro.
fn color_regla_vertical(fondo: (u8, u8, u8), texto: (u8, u8, u8)) -> Color {
    // 88% fondo + 12% texto: se distingue de un vistazo del fondo liso,
    // pero no compite con el resaltado de sintaxis ni con el fondo de la
    // línea actual si coinciden en esa columna.
    let mezclar = |c_fondo: u8, c_texto: u8| ((c_fondo as f32) * 0.88 + (c_texto as f32) * 0.12).round() as u8;
    Color::Rgb(mezclar(fondo.0, texto.0), mezclar(fondo.1, texto.1), mezclar(fondo.2, texto.2))
}

fn estilo_desde_token(token: &EstiloToken) -> Result<Style> {
    let (r, g, b) = token.color_rgb()?;
    let mut estilo = Style::default().fg(Color::Rgb(r, g, b));
    if token.negrita() {
        estilo = estilo.add_modifier(Modifier::BOLD);
    }
    if token.cursiva() {
        estilo = estilo.add_modifier(Modifier::ITALIC);
    }
    Ok(estilo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_regla_vertical_en_tema_oscuro_queda_mas_claro_que_el_fondo() {
        // Dracula-ish: fondo casi negro, texto casi blanco.
        let fondo = (40, 42, 54);
        let texto = (248, 248, 242);
        let Color::Rgb(r, g, b) = color_regla_vertical(fondo, texto) else { panic!("debería ser Rgb") };
        assert!(r > fondo.0 && g > fondo.1 && b > fondo.2, "debe ser más claro que el fondo, no más oscuro");
        // Pero mucho más cerca del fondo que del texto (sutil, no un
        // gris a mitad de camino).
        assert!(r < 90, "no debería acercarse al blanco del texto");
    }

    #[test]
    fn color_regla_vertical_en_tema_claro_queda_mas_oscuro_que_el_fondo() {
        // GitHub Light-ish: fondo casi blanco, texto casi negro.
        let fondo = (255, 255, 255);
        let texto = (36, 41, 46);
        let Color::Rgb(r, g, b) = color_regla_vertical(fondo, texto) else { panic!("debería ser Rgb") };
        assert!(r < fondo.0 && g < fondo.1 && b < fondo.2, "debe ser más oscuro que el fondo, no más claro");
        assert!(r > 200, "no debería acercarse al negro del texto");
    }

    #[test]
    fn color_regla_vertical_con_fondo_y_texto_iguales_no_cambia() {
        let gris = (128, 128, 128);
        assert_eq!(color_regla_vertical(gris, gris), Color::Rgb(128, 128, 128));
    }
}
