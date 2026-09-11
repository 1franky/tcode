use anyhow::Result;
use ratatui::style::Color;

use tcode_config::Tema;

/// Colores de un [`Tema`] ya resueltos a [`ratatui::style::Color`],
/// calculados una sola vez al cargar el tema en vez de re-parsear hex en
/// cada frame.
pub struct Paleta {
    pub fondo: Color,
    pub texto: Color,
    pub cursor: Color,
    pub linea_actual: Color,
    pub statusbar_fondo: Color,
    pub statusbar_texto: Color,
}

impl Paleta {
    pub fn desde_tema(tema: &Tema) -> Result<Self> {
        let rgb = |c: (u8, u8, u8)| Color::Rgb(c.0, c.1, c.2);
        Ok(Self {
            fondo: rgb(tema.ui.background_rgb()?),
            texto: rgb(tema.ui.foreground_rgb()?),
            cursor: rgb(tema.ui.cursor_rgb()?),
            linea_actual: rgb(tema.ui.linea_actual_rgb()?),
            statusbar_fondo: rgb(tema.statusbar.background_rgb()?),
            statusbar_texto: rgb(tema.statusbar.foreground_rgb()?),
        })
    }

    /// Paleta de emergencia (blanco sobre negro) para el caso extremo en que
    /// ni el tema configurado ni el tema por defecto se puedan resolver a
    /// colores válidos. No debería usarse nunca en la práctica: los 3 temas
    /// embebidos están validados por tests en `tcode-config`.
    pub fn basica() -> Self {
        Self {
            fondo: Color::Black,
            texto: Color::White,
            cursor: Color::White,
            linea_actual: Color::DarkGray,
            statusbar_fondo: Color::DarkGray,
            statusbar_texto: Color::White,
        }
    }
}
