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

/// `#rrggbb` en minúsculas a partir de sus componentes RGB — inverso de
/// `analizar_color_hex`.
pub fn formatear_color_hex(r: u8, g: u8, b: u8) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// RGB (0..=255 cada uno) a HSL: matiz en grados (0..360), saturación y
/// luminosidad en porcentaje (0..100) — las tres redondeadas a entero,
/// que es todo lo que necesita el ajuste con flechas del editor visual
/// de tema (PLAN.md §7: "ajustar HSL con flechas"). Fórmula estándar
/// (ver https://en.wikipedia.org/wiki/HSL_and_HSV#From_RGB).
pub fn rgb_a_hsl(r: u8, g: u8, b: u8) -> (u16, u8, u8) {
    let (r, g, b) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    let maximo = r.max(g).max(b);
    let minimo = r.min(g).min(b);
    let delta = maximo - minimo;

    let l = (maximo + minimo) / 2.0;

    let h = if delta == 0.0 {
        0.0
    } else if maximo == r {
        60.0 * (((g - b) / delta).rem_euclid(6.0))
    } else if maximo == g {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };

    let s = if delta == 0.0 { 0.0 } else { delta / (1.0 - (2.0 * l - 1.0).abs()) };

    (h.round() as u16 % 360, (s * 100.0).round() as u8, (l * 100.0).round() as u8)
}

/// HSL (matiz 0..360, saturación/luminosidad 0..100) a RGB — inverso de
/// `rgb_a_hsl`. Fórmula estándar, misma referencia.
pub fn hsl_a_rgb(h: u16, s: u8, l: u8) -> (u8, u8, u8) {
    let h = (h % 360) as f64;
    let s = s.min(100) as f64 / 100.0;
    let l = l.min(100) as f64 / 100.0;

    if s == 0.0 {
        let gris = (l * 255.0).round() as u8;
        return (gris, gris, gris);
    }

    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = l - c / 2.0;

    let (r1, g1, b1) = match h as u32 {
        0..=59 => (c, x, 0.0),
        60..=119 => (x, c, 0.0),
        120..=179 => (0.0, c, x),
        180..=239 => (0.0, x, c),
        240..=299 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    (((r1 + m) * 255.0).round() as u8, ((g1 + m) * 255.0).round() as u8, ((b1 + m) * 255.0).round() as u8)
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

    #[test]
    fn formatear_color_hex_es_el_inverso_de_analizar() {
        assert_eq!(formatear_color_hex(0xa3, 0xb1, 0xc6), "#a3b1c6");
        assert_eq!(formatear_color_hex(0, 0, 0), "#000000");
        assert_eq!(formatear_color_hex(255, 255, 255), "#ffffff");
    }

    #[test]
    fn rgb_a_hsl_de_colores_conocidos() {
        assert_eq!(rgb_a_hsl(255, 0, 0), (0, 100, 50)); // rojo puro
        assert_eq!(rgb_a_hsl(0, 255, 0), (120, 100, 50)); // verde puro
        assert_eq!(rgb_a_hsl(0, 0, 255), (240, 100, 50)); // azul puro
        assert_eq!(rgb_a_hsl(255, 255, 255), (0, 0, 100)); // blanco
        assert_eq!(rgb_a_hsl(0, 0, 0), (0, 0, 0)); // negro
        assert_eq!(rgb_a_hsl(128, 128, 128), (0, 0, 50)); // gris medio
    }

    #[test]
    fn hsl_a_rgb_de_colores_conocidos() {
        assert_eq!(hsl_a_rgb(0, 100, 50), (255, 0, 0));
        assert_eq!(hsl_a_rgb(120, 100, 50), (0, 255, 0));
        assert_eq!(hsl_a_rgb(240, 100, 50), (0, 0, 255));
        assert_eq!(hsl_a_rgb(0, 0, 100), (255, 255, 255));
        assert_eq!(hsl_a_rgb(0, 0, 0), (0, 0, 0));
    }

    #[test]
    fn hsl_y_rgb_son_razonablemente_inversos_en_un_muestreo_de_colores() {
        // La conversión redondea a enteros en cada paso — no es
        // perfectamente reversible, pero el error debería ser mínimo (a
        // lo sumo 1-2 unidades de RGB) para cualquier color, no solo los
        // "de libro" de los tests de arriba.
        for hex in ["#282a36", "#f8f8f2", "#ff79c6", "#50fa7b", "#6272a4", "#123456", "#abcdef"] {
            let (r, g, b) = analizar_color_hex(hex).unwrap();
            let (h, s, l) = rgb_a_hsl(r, g, b);
            let (r2, g2, b2) = hsl_a_rgb(h, s, l);
            let diff = |a: u8, b: u8| (a as i16 - b as i16).abs();
            assert!(diff(r, r2) <= 2 && diff(g, g2) <= 2 && diff(b, b2) <= 2, "'{hex}' -> hsl({h},{s},{l}) -> #{r2:02x}{g2:02x}{b2:02x}");
        }
    }
}
