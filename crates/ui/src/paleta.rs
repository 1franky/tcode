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
    pub statusbar_fondo: Color,
    pub statusbar_texto: Color,
    /// Estilo por token de sintaxis (PLAN.md §7), indexado por uno de los
    /// nombres canónicos de [`tcode_syntax::NOMBRES_RESALTADO`].
    sintaxis: HashMap<&'static str, Style>,
}

impl Paleta {
    pub fn desde_tema(tema: &Tema) -> Result<Self> {
        let rgb = |c: (u8, u8, u8)| Color::Rgb(c.0, c.1, c.2);

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
            statusbar_fondo: rgb(tema.statusbar.background_rgb()?),
            statusbar_texto: rgb(tema.statusbar.foreground_rgb()?),
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
            statusbar_fondo: Color::DarkGray,
            statusbar_texto: Color::White,
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
