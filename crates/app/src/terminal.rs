//! Terminal integrada (BACKLOG.md P3 #26) del lado de `app`: abrirla y
//! cerrarla, el foco, traducir cada tecla a los bytes que espera una
//! shell (las secuencias de xterm) y lo pegado. La sesión (la shell en su
//! PTY y la pantalla interpretada) es `tcode_terminal`; el dibujo,
//! `tcode_ui::panel_terminal`.
//!
//! Con el foco en la terminal TODAS las teclas van a la shell (`Ctrl+C`,
//! `Ctrl+K`, `Ctrl+W`... tienen que llegarle), salvo una: `Ctrl+`` (con
//! protocolo Kitty) o `Ctrl+Espacio` (lo que mandan las terminales sin él
//! para `Ctrl+``), que oculta el panel y vuelve al editor. Un clic en el
//! editor también le devuelve el foco, dejando la terminal a la vista.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tcode_terminal::{shell_por_defecto, SesionTerminal};
use tcode_ui::Layout as PanelLayout;

use crate::{EstadoApp, Foco};

/// Filas de la pantalla de la terminal más el título, como fracción del
/// alto: un tercio, con un mínimo.
const ALTO_MINIMO: u16 = 8;

#[derive(Default)]
pub struct EstadoTerminal {
    pub sesion: Option<SesionTerminal>,
    /// Si el panel se ve (la sesión sigue viva aunque esté oculto).
    pub visible: bool,
}

impl EstadoTerminal {
    /// Filas del panel para una pantalla de `alto` filas (0 si no se ve).
    pub fn alto(&self, alto_pantalla: u16) -> u16 {
        if self.visible && self.sesion.is_some() {
            (alto_pantalla / 3).max(ALTO_MINIMO)
        } else {
            0
        }
    }

    pub fn titulo(&self, enfocada: bool) -> String {
        let nombre = self.sesion.as_ref().map(|s| s.nombre.as_str()).unwrap_or("");
        if enfocada {
            format!("Terminal: {nombre}  (Ctrl+` o Ctrl+Espacio vuelve al editor)")
        } else {
            format!("Terminal: {nombre}")
        }
    }
}

fn avisar(layout: &mut PanelLayout, texto: impl Into<String>) {
    layout.panel_activo_mut().mensaje_estado = Some(texto.into());
}

/// `terminal.alternar` (`Ctrl+``, `Ctrl+K ``), como en VSCode: si está
/// oculta, la muestra (lanzando la shell la primera vez) y le da el foco;
/// si se ve pero el foco está en otro lado, se lo da; si ya lo tiene, la
/// oculta y vuelve al editor.
pub fn alternar(layout: &mut PanelLayout, estado: &mut EstadoApp) {
    let terminal = &mut estado.terminal;
    if terminal.visible && terminal.sesion.is_some() {
        if estado.foco == Foco::Terminal {
            terminal.visible = false;
            estado.foco = Foco::Editor;
        } else {
            estado.foco = Foco::Terminal;
        }
        return;
    }
    if terminal.sesion.is_none() {
        let carpeta = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        // Un tamaño inicial cualquiera: el primer dibujo lo ajusta.
        match SesionTerminal::lanzar(&shell_por_defecto(), &carpeta, 10, 80) {
            Ok(sesion) => terminal.sesion = Some(sesion),
            Err(error) => return avisar(layout, format!("Terminal: {error:#}")),
        }
    }
    terminal.visible = true;
    estado.foco = Foco::Terminal;
}

/// `terminal.cerrar`: termina la shell y oculta el panel.
pub fn cerrar(estado: &mut EstadoApp) {
    if let Some(mut sesion) = estado.terminal.sesion.take() {
        sesion.cerrar();
    }
    estado.terminal.visible = false;
    if estado.foco == Foco::Terminal {
        estado.foco = Foco::Editor;
    }
}

/// La shell terminó sola (`exit`): se cierra el panel.
pub fn termino(layout: &mut PanelLayout, estado: &mut EstadoApp) {
    estado.terminal.sesion = None;
    estado.terminal.visible = false;
    if estado.foco == Foco::Terminal {
        estado.foco = Foco::Editor;
    }
    avisar(layout, "La terminal se cerró");
}

/// Si `key` es la tecla que saca el foco de la terminal.
fn es_tecla_salida(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('`') | KeyCode::Char(' '))
}

/// Una tecla con el foco en la terminal. Devuelve si cambió algo visible
/// en `tcode` (lo de la shell se redibuja cuando llega su salida).
pub fn tecla(key: KeyEvent, layout: &mut PanelLayout, estado: &mut EstadoApp) -> bool {
    if es_tecla_salida(&key) {
        alternar(layout, estado);
        return true;
    }
    let Some(sesion) = &mut estado.terminal.sesion else { return false };
    if let Some(bytes) = bytes_de_tecla(&key, sesion.pantalla().application_cursor()) {
        sesion.escribir(&bytes);
    }
    false
}

/// Texto pegado con el foco en la terminal: a la shell, con los saltos de
/// línea como `Enter` y entre las marcas de bracketed paste si la shell
/// las pidió (así no ejecuta cada línea al pegar).
pub fn pegar(texto: &str, estado: &mut EstadoApp) {
    let Some(sesion) = &mut estado.terminal.sesion else { return };
    let texto = texto.replace("\r\n", "\r").replace('\n', "\r");
    if sesion.pantalla().bracketed_paste() {
        sesion.escribir(format!("\x1b[200~{texto}\x1b[201~").as_bytes());
    } else {
        sesion.escribir(texto.as_bytes());
    }
}

/// Los bytes que manda xterm para `key` (`None` si no manda nada).
/// `cursor_aplicacion`: la shell pidió el modo "application cursor keys"
/// (lo hacen `vim`, `less`...): las flechas van como `ESC O A`.
pub fn bytes_de_tecla(key: &KeyEvent, cursor_aplicacion: bool) -> Option<Vec<u8>> {
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    // Parámetro de modificadores de xterm (`ESC [ 1 ; m A`): 1 + shift +
    // 2·alt + 4·ctrl.
    let modificador = 1 + shift as u8 + 2 * alt as u8 + 4 * ctrl as u8;
    let con_esc = |mut bytes: Vec<u8>| {
        if alt {
            bytes.insert(0, 0x1b);
        }
        bytes
    };
    let flecha = |letra: char| {
        if modificador > 1 {
            format!("\x1b[1;{modificador}{letra}").into_bytes()
        } else if cursor_aplicacion {
            format!("\x1bO{letra}").into_bytes()
        } else {
            format!("\x1b[{letra}").into_bytes()
        }
    };
    let tilde = |n: u8| {
        if modificador > 1 { format!("\x1b[{n};{modificador}~") } else { format!("\x1b[{n}~") }.into_bytes()
    };
    Some(match key.code {
        KeyCode::Char(c) if ctrl => {
            let control = match c.to_ascii_lowercase() {
                l @ 'a'..='z' => l as u8 - b'a' + 1,
                '@' | ' ' | '2' => 0,
                '[' | '3' => 0x1b,
                '\\' | '4' => 0x1c,
                ']' | '5' => 0x1d,
                '^' | '6' => 0x1e,
                '_' | '-' | '7' | '/' => 0x1f,
                '?' | '8' => 0x7f,
                _ => return Some(con_esc(c.to_string().into_bytes())),
            };
            con_esc(vec![control])
        }
        KeyCode::Char(c) => con_esc(c.to_string().into_bytes()),
        KeyCode::Enter => con_esc(vec![b'\r']),
        KeyCode::Tab => vec![b'\t'],
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Backspace => con_esc(vec![0x7f]),
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => flecha('A'),
        KeyCode::Down => flecha('B'),
        KeyCode::Right => flecha('C'),
        KeyCode::Left => flecha('D'),
        KeyCode::Home => flecha('H'),
        KeyCode::End => flecha('F'),
        KeyCode::Insert => tilde(2),
        KeyCode::Delete => tilde(3),
        KeyCode::PageUp => tilde(5),
        KeyCode::PageDown => tilde(6),
        KeyCode::F(n @ 1..=4) => {
            let letra = (b'P' + n - 1) as char;
            if modificador > 1 { format!("\x1b[1;{modificador}{letra}").into_bytes() } else { format!("\x1bO{letra}").into_bytes() }
        }
        KeyCode::F(n @ 5..=12) => tilde([15, 17, 18, 19, 20, 21, 23, 24][(n - 5) as usize]),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tecla(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn teclas_como_las_manda_xterm() {
        let b = |code, m| bytes_de_tecla(&tecla(code, m), false).unwrap();
        let ninguno = KeyModifiers::NONE;
        assert_eq!(b(KeyCode::Char('a'), ninguno), b"a");
        assert_eq!(b(KeyCode::Char('ñ'), ninguno), "ñ".as_bytes());
        assert_eq!(b(KeyCode::Char('c'), KeyModifiers::CONTROL), [3]);
        assert_eq!(b(KeyCode::Char('K'), KeyModifiers::CONTROL), [11]);
        assert_eq!(b(KeyCode::Char('['), KeyModifiers::CONTROL), [0x1b]);
        assert_eq!(b(KeyCode::Char('b'), KeyModifiers::ALT), b"\x1bb");
        assert_eq!(b(KeyCode::Enter, ninguno), b"\r");
        assert_eq!(b(KeyCode::Backspace, ninguno), [0x7f]);
        assert_eq!(b(KeyCode::BackTab, KeyModifiers::SHIFT), b"\x1b[Z");
        assert_eq!(b(KeyCode::Up, ninguno), b"\x1b[A");
        assert_eq!(bytes_de_tecla(&tecla(KeyCode::Up, ninguno), true).unwrap(), b"\x1bOA");
        assert_eq!(b(KeyCode::Right, KeyModifiers::CONTROL), b"\x1b[1;5C");
        assert_eq!(b(KeyCode::Delete, ninguno), b"\x1b[3~");
        assert_eq!(b(KeyCode::PageDown, KeyModifiers::SHIFT), b"\x1b[6;2~");
        assert_eq!(b(KeyCode::F(1), ninguno), b"\x1bOP");
        assert_eq!(b(KeyCode::F(5), ninguno), b"\x1b[15~");
        assert_eq!(b(KeyCode::F(12), ninguno), b"\x1b[24~");
        assert!(bytes_de_tecla(&tecla(KeyCode::CapsLock, ninguno), false).is_none());
    }

    #[test]
    fn tecla_de_salida() {
        assert!(es_tecla_salida(&tecla(KeyCode::Char('`'), KeyModifiers::CONTROL)));
        assert!(es_tecla_salida(&tecla(KeyCode::Char(' '), KeyModifiers::CONTROL)));
        assert!(!es_tecla_salida(&tecla(KeyCode::Char('c'), KeyModifiers::CONTROL)));
        assert!(!es_tecla_salida(&tecla(KeyCode::Char('`'), KeyModifiers::NONE)));
    }
}
