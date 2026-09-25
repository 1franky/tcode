use anyhow::{bail, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// La tecla base de una combinación, sin contar modificadores. `Caracter`
/// siempre se normaliza a minúscula: `Combinacion.shift` es lo que indica
/// si hacía falta mayúscula, así "p" y "Shift+p" no son la misma clave de
/// mapa pero ambas usan `Tecla::Caracter('p')`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tecla {
    Caracter(char),
    Flecha(Direccion),
    Enter,
    Tab,
    Backspace,
    Delete,
    Esc,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
    /// Cualquier tecla que no reconocemos todavía (multimedia, etc.). Nunca
    /// coincide con un atajo configurado.
    Otra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direccion {
    Arriba,
    Abajo,
    Izquierda,
    Derecha,
}

/// Una combinación de teclas ya resuelta: modificadores + tecla base. Es la
/// unidad mínima de un atajo; una secuencia de combinaciones (`Vec<Combinacion>`)
/// representa un atajo encadenado tipo `Ctrl+K Ctrl+O` (PLAN.md §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Combinacion {
    pub tecla: Tecla,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

/// Convierte un evento real de `crossterm` en una `Combinacion`.
///
/// Nota conocida: en terminales clásicas (sin el protocolo extendido de
/// Kitty), `Ctrl+<letra>` colapsa a un único byte de control que no
/// distingue si además se sostenía Shift — por eso el keymap por defecto de
/// M1 no depende de esa distinción en combinaciones con Ctrl.
pub fn desde_evento(key: KeyEvent) -> Combinacion {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let mut shift = key.modifiers.contains(KeyModifiers::SHIFT);

    let tecla = match key.code {
        KeyCode::Char(c) => {
            if c.is_uppercase() {
                shift = true;
            }
            Tecla::Caracter(c.to_ascii_lowercase())
        }
        KeyCode::Enter => Tecla::Enter,
        // `Shift+Tab` no llega como `Tab` + modificador Shift: los
        // terminales lo codifican como una tecla aparte (`BackTab`,
        // `CSI Z`) que sí trae el modificador Shift marcado — se
        // normaliza aquí a la misma `Tecla::Tab` para que
        // `"Shift+Tab"` en el keymap funcione igual que cualquier otra
        // combinación con Shift.
        KeyCode::Tab | KeyCode::BackTab => Tecla::Tab,
        KeyCode::Backspace => Tecla::Backspace,
        KeyCode::Delete => Tecla::Delete,
        KeyCode::Esc => Tecla::Esc,
        KeyCode::Home => Tecla::Home,
        KeyCode::End => Tecla::End,
        KeyCode::PageUp => Tecla::PageUp,
        KeyCode::PageDown => Tecla::PageDown,
        KeyCode::Up => Tecla::Flecha(Direccion::Arriba),
        KeyCode::Down => Tecla::Flecha(Direccion::Abajo),
        KeyCode::Left => Tecla::Flecha(Direccion::Izquierda),
        KeyCode::Right => Tecla::Flecha(Direccion::Derecha),
        KeyCode::F(n) => Tecla::F(n),
        _ => Tecla::Otra,
    };

    Combinacion { tecla, ctrl, alt, shift }
}

fn parsear_tecla(cruda: &str) -> Result<Tecla> {
    let normalizada = cruda.trim();
    if normalizada.chars().count() == 1 && !normalizada.chars().next().unwrap().is_alphanumeric() {
        // Símbolos sueltos como "/", "[", "]", ",", "`", "-": se toman tal cual.
        return Ok(Tecla::Caracter(normalizada.chars().next().unwrap()));
    }

    let tecla = match normalizada.to_lowercase().as_str() {
        "enter" | "return" => Tecla::Enter,
        "tab" => Tecla::Tab,
        "backspace" => Tecla::Backspace,
        "delete" | "supr" => Tecla::Delete,
        "esc" | "escape" => Tecla::Esc,
        "home" | "inicio" => Tecla::Home,
        "end" | "fin" => Tecla::End,
        "pageup" | "repag" => Tecla::PageUp,
        "pagedown" | "avpag" => Tecla::PageDown,
        "up" | "↑" => Tecla::Flecha(Direccion::Arriba),
        "down" | "↓" => Tecla::Flecha(Direccion::Abajo),
        "left" | "←" => Tecla::Flecha(Direccion::Izquierda),
        "right" | "→" => Tecla::Flecha(Direccion::Derecha),
        "space" | "espacio" => Tecla::Caracter(' '),
        otro => {
            if let Some(numero) = otro.strip_prefix('f') {
                if let Ok(n) = numero.parse::<u8>() {
                    return Ok(Tecla::F(n));
                }
            }
            let caracteres: Vec<char> = normalizada.chars().collect();
            if caracteres.len() == 1 {
                return Ok(Tecla::Caracter(caracteres[0].to_ascii_lowercase()));
            }
            bail!("tecla desconocida '{cruda}'");
        }
    };
    Ok(tecla)
}

/// Parsea una única combinación como `"Ctrl+Shift+P"`. Los modificadores se
/// aceptan en cualquier orden, antes de la tecla base.
pub fn parsear_combinacion(cadena: &str) -> Result<Combinacion> {
    let cadena = cadena.trim();
    if cadena.is_empty() {
        bail!("combinación de teclas vacía");
    }

    // Caso especial: "Ctrl++" termina en un '+' literal, no en un separador.
    let (resto, tecla_cruda) = if let Some(prefijo) = cadena.strip_suffix("++") {
        (prefijo, "+")
    } else {
        match cadena.rsplit_once('+') {
            Some((prefijo, tecla)) if !tecla.is_empty() => (prefijo, tecla),
            _ => ("", cadena),
        }
    };

    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    if !resto.is_empty() {
        for modificador in resto.split('+') {
            match modificador.trim().to_lowercase().as_str() {
                "ctrl" | "control" => ctrl = true,
                "alt" => alt = true,
                "shift" => shift = true,
                "" => {}
                otro => bail!("modificador desconocido '{otro}' en '{cadena}'"),
            }
        }
    }

    let tecla = parsear_tecla(tecla_cruda)?;
    Ok(Combinacion { tecla, ctrl, alt, shift })
}

/// Parsea un atajo completo, que puede ser una combinación simple o una
/// secuencia encadenada separada por espacios (`"Ctrl+K Ctrl+O"`).
pub fn parsear_atajo(cadena: &str) -> Result<Vec<Combinacion>> {
    let secuencia: Result<Vec<Combinacion>> = cadena.split_whitespace().map(parsear_combinacion).collect();
    let secuencia = secuencia?;
    if secuencia.is_empty() {
        bail!("atajo vacío");
    }
    Ok(secuencia)
}

/// Texto legible de una combinación, inverso de [`parsear_combinacion`]
/// (`Ctrl+Shift+P`). Se usa para mostrar el atajo de un comando en la
/// paleta de comandos (`Ctrl+Shift+P`, M2) y, más adelante, en el editor
/// visual de atajos del panel de administración (M4).
pub fn formatear_combinacion(c: &Combinacion) -> String {
    let mut partes = Vec::new();
    if c.ctrl {
        partes.push("Ctrl".to_string());
    }
    if c.alt {
        partes.push("Alt".to_string());
    }
    if c.shift {
        partes.push("Shift".to_string());
    }
    partes.push(formatear_tecla(&c.tecla));
    partes.join("+")
}

/// Texto legible de un atajo completo, inverso de [`parsear_atajo`]
/// (`Ctrl+K Ctrl+O`).
pub fn formatear_atajo(secuencia: &[Combinacion]) -> String {
    secuencia.iter().map(formatear_combinacion).collect::<Vec<_>>().join(" ")
}

fn formatear_tecla(t: &Tecla) -> String {
    match t {
        // Un espacio literal no se puede volver a parsear (`"Ctrl+ "` se
        // recorta): se escribe con nombre, como lo acepta `parsear_tecla`.
        Tecla::Caracter(' ') => "Space".to_string(),
        Tecla::Caracter(c) => c.to_uppercase().to_string(),
        Tecla::Flecha(Direccion::Arriba) => "Up".to_string(),
        Tecla::Flecha(Direccion::Abajo) => "Down".to_string(),
        Tecla::Flecha(Direccion::Izquierda) => "Left".to_string(),
        Tecla::Flecha(Direccion::Derecha) => "Right".to_string(),
        Tecla::Enter => "Enter".to_string(),
        Tecla::Tab => "Tab".to_string(),
        Tecla::Backspace => "Backspace".to_string(),
        Tecla::Delete => "Delete".to_string(),
        Tecla::Esc => "Esc".to_string(),
        Tecla::Home => "Home".to_string(),
        Tecla::End => "End".to_string(),
        Tecla::PageUp => "PageUp".to_string(),
        Tecla::PageDown => "PageDown".to_string(),
        Tecla::F(n) => format!("F{n}"),
        Tecla::Otra => "?".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_espacio_se_formatea_con_nombre_y_vuelve_a_parsear() {
        let c = parsear_combinacion("Ctrl+Space").unwrap();
        assert_eq!(c.tecla, Tecla::Caracter(' '));
        assert_eq!(formatear_combinacion(&c), "Ctrl+Space");
        assert_eq!(parsear_combinacion(&formatear_combinacion(&c)).unwrap(), c);
    }

    #[test]
    fn parsea_combinacion_simple() {
        let c = parsear_combinacion("Ctrl+S").unwrap();
        assert_eq!(c, Combinacion { tecla: Tecla::Caracter('s'), ctrl: true, alt: false, shift: false });
    }

    #[test]
    fn parsea_combinacion_con_varios_modificadores() {
        let c = parsear_combinacion("Ctrl+Shift+P").unwrap();
        assert_eq!(c.tecla, Tecla::Caracter('p'));
        assert!(c.ctrl && c.shift && !c.alt);
    }

    #[test]
    fn parsea_teclas_especiales_y_simbolos() {
        assert_eq!(parsear_combinacion("F12").unwrap().tecla, Tecla::F(12));
        assert_eq!(parsear_combinacion("Ctrl+/").unwrap().tecla, Tecla::Caracter('/'));
        assert_eq!(parsear_combinacion("Ctrl+,").unwrap().tecla, Tecla::Caracter(','));
        assert_eq!(parsear_combinacion("Esc").unwrap().tecla, Tecla::Esc);
        assert_eq!(parsear_combinacion("Ctrl+Home").unwrap().tecla, Tecla::Home);
    }

    #[test]
    fn parsea_el_caso_especial_ctrl_mas_mas() {
        let c = parsear_combinacion("Ctrl++").unwrap();
        assert_eq!(c.tecla, Tecla::Caracter('+'));
        assert!(c.ctrl);
    }

    #[test]
    fn parsea_atajo_encadenado() {
        let secuencia = parsear_atajo("Ctrl+K Ctrl+O").unwrap();
        assert_eq!(secuencia.len(), 2);
        assert_eq!(secuencia[0].tecla, Tecla::Caracter('k'));
        assert_eq!(secuencia[1].tecla, Tecla::Caracter('o'));
    }

    #[test]
    fn rechaza_atajos_invalidos() {
        assert!(parsear_atajo("").is_err());
        assert!(parsear_combinacion("Ctrl+NoExiste").is_err());
    }

    #[test]
    fn desde_evento_normaliza_mayusculas() {
        let evento = KeyEvent::new(KeyCode::Char('P'), KeyModifiers::NONE);
        let c = desde_evento(evento);
        assert_eq!(c.tecla, Tecla::Caracter('p'));
        assert!(c.shift);
    }

    #[test]
    fn desde_evento_normaliza_backtab_a_shift_tab() {
        let evento = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
        let c = desde_evento(evento);
        assert_eq!(c.tecla, Tecla::Tab);
        assert!(c.shift);
        assert_eq!(c, parsear_combinacion("Shift+Tab").unwrap());
    }

    #[test]
    fn formatear_es_el_inverso_de_parsear() {
        for texto in ["Ctrl+S", "Ctrl+Shift+P", "F12", "Esc", "Ctrl+Home", "Ctrl+K Ctrl+O"] {
            let secuencia = parsear_atajo(texto).unwrap();
            assert_eq!(formatear_atajo(&secuencia), texto);
        }
    }
}
