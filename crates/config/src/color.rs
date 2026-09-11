use anyhow::{bail, Result};

/// Convierte un color en formato hexadecimal (`#rrggbb`, con o sin `#`) a
/// sus componentes RGB. Es lo único que `tcode-config` sabe sobre colores:
/// la conversión a un tipo de color de la librería de UI vive en el crate
/// `ui`, para no acoplar la configuración a `ratatui`.
pub fn analizar_color_hex(valor: &str) -> Result<(u8, u8, u8)> {
    let limpio = valor.trim().trim_start_matches('#');
    if limpio.len() != 6 || !limpio.is_ascii() {
        bail!("color inválido '{valor}': se espera el formato #rrggbb");
    }
    let r = u8::from_str_radix(&limpio[0..2], 16)
        .map_err(|_| anyhow::anyhow!("color inválido '{valor}': se espera el formato #rrggbb"))?;
    let g = u8::from_str_radix(&limpio[2..4], 16)
        .map_err(|_| anyhow::anyhow!("color inválido '{valor}': se espera el formato #rrggbb"))?;
    let b = u8::from_str_radix(&limpio[4..6], 16)
        .map_err(|_| anyhow::anyhow!("color inválido '{valor}': se espera el formato #rrggbb"))?;
    Ok((r, g, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analiza_colores_validos() {
        assert_eq!(analizar_color_hex("#a3b1c6").unwrap(), (0xa3, 0xb1, 0xc6));
        assert_eq!(analizar_color_hex("ffffff").unwrap(), (255, 255, 255));
        assert_eq!(analizar_color_hex("  #000000  ").unwrap(), (0, 0, 0));
    }

    #[test]
    fn rechaza_colores_invalidos() {
        assert!(analizar_color_hex("#fff").is_err());
        assert!(analizar_color_hex("no-es-un-color").is_err());
        assert!(analizar_color_hex("#gggggg").is_err());
    }
}
