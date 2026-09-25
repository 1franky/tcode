/// Estado del prompt "Ir a línea" (`Ctrl+G`, BACKLOG.md P0 #19) — hasta
/// ahora solo existía `:{n}` en el modo VIM. Mismo patrón que
/// `EstadoGuardarComo`: un campo de texto de una sola línea que acepta
/// `n` o `n:col` (base uno, como los muestra la statusbar).
pub struct EstadoIrALinea {
    activo: bool,
    texto: String,
    /// Qué estaba mal en el último `Enter` (no es un número...) — el
    /// prompt queda abierto mostrándolo, sin perder lo escrito.
    error: Option<String>,
}

impl EstadoIrALinea {
    pub fn nuevo() -> Self {
        Self { activo: false, texto: String::new(), error: None }
    }

    pub fn activo(&self) -> bool {
        self.activo
    }

    pub fn texto(&self) -> &str {
        &self.texto
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Abre el prompt vacío (a diferencia de "Guardar como", no hay nada
    /// útil que precargar: la línea actual ya está en la statusbar).
    pub fn abrir(&mut self) {
        self.activo = true;
        self.texto.clear();
        self.error = None;
    }

    pub fn cerrar(&mut self) {
        self.activo = false;
    }

    pub fn escribir(&mut self, c: char) {
        self.error = None;
        self.texto.push(c);
    }

    pub fn borrar(&mut self) {
        self.error = None;
        self.texto.pop();
    }

    pub fn establecer_error(&mut self, error: String) {
        self.error = Some(error);
    }
}

impl Default for EstadoIrALinea {
    fn default() -> Self {
        Self::nuevo()
    }
}

/// Interpreta lo escrito en el prompt "Ir a línea": `n` o `n:col` (base
/// uno, espacios alrededor permitidos). Devuelve `(línea, columna)` en
/// base cero, con la línea recortada a `1..=num_lineas` (escribir un
/// número enorme va al final, igual que `:{n}` en VIM y que VSCode); la
/// columna la recorta después `Editor::ir_a_linea` al largo de esa línea.
/// `Ok(None)` si está vacío (`Enter` sin nada cierra sin moverse).
pub fn interpretar_ir_a_linea(texto: &str, num_lineas: usize) -> Result<Option<(usize, usize)>, String> {
    let texto = texto.trim();
    if texto.is_empty() {
        return Ok(None);
    }
    let (linea, columna) = match texto.split_once(':') {
        Some((linea, columna)) => (linea.trim(), Some(columna.trim())),
        None => (texto, None),
    };
    let invalido = || "no es un número de línea (n o n:col)".to_string();
    let linea: usize = linea.parse().map_err(|_| invalido())?;
    let columna: usize = match columna {
        Some(c) => c.parse().map_err(|_| invalido())?,
        None => 1,
    };
    let linea = linea.clamp(1, num_lineas.max(1)) - 1;
    Ok(Some((linea, columna.max(1) - 1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acepta_linea_y_linea_con_columna() {
        assert_eq!(interpretar_ir_a_linea("12", 100), Ok(Some((11, 0))));
        assert_eq!(interpretar_ir_a_linea(" 3:7 ", 100), Ok(Some((2, 6))));
        assert_eq!(interpretar_ir_a_linea("3 : 7", 100), Ok(Some((2, 6))));
    }

    #[test]
    fn recorta_al_rango_valido() {
        assert_eq!(interpretar_ir_a_linea("0", 10), Ok(Some((0, 0))));
        assert_eq!(interpretar_ir_a_linea("99999", 10), Ok(Some((9, 0))));
        assert_eq!(interpretar_ir_a_linea("2:0", 10), Ok(Some((1, 0))));
    }

    #[test]
    fn vacio_no_se_mueve_y_basura_es_error() {
        assert_eq!(interpretar_ir_a_linea("   ", 10), Ok(None));
        assert!(interpretar_ir_a_linea("abc", 10).is_err());
        assert!(interpretar_ir_a_linea("3:", 10).is_err());
        assert!(interpretar_ir_a_linea("-2", 10).is_err());
        assert!(interpretar_ir_a_linea("1:2:3", 10).is_err());
    }

    #[test]
    fn escribir_y_borrar_limpian_el_error() {
        let mut estado = EstadoIrALinea::nuevo();
        estado.abrir();
        estado.escribir('x');
        estado.establecer_error("mal".to_string());
        estado.borrar();
        assert!(estado.error().is_none());
        assert_eq!(estado.texto(), "");
        estado.escribir('4');
        estado.cerrar();
        assert!(!estado.activo());
        estado.abrir();
        assert_eq!(estado.texto(), "");
    }
}
