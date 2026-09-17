/// Estado del prompt "Guardar como" (`Ctrl+Shift+S`/`Ctrl+K S`) — hasta
/// ahora `Ctrl+S` sobre un buffer nuevo sin ruta asociada simplemente no
/// hacía nada (`Buffer::guardar` fallaba en silencio), sin ninguna forma
/// de ponerle nombre a un archivo nuevo desde la UI. Al no haber selector
/// de archivos en esta TUI, es un campo de texto de una sola línea con la
/// ruta destino — mismo criterio que el resto del editor (export/import
/// de temas y de atajos también son "escribí la ruta a mano").
pub struct EstadoGuardarComo {
    activa: bool,
    ruta: String,
    /// Motivo del último intento de guardado fallido (permiso denegado,
    /// directorio inexistente...) — se muestra en vez de cerrar el
    /// prompt sin explicación, para poder corregir la ruta sin perder lo
    /// ya escrito.
    error: Option<String>,
}

impl EstadoGuardarComo {
    pub fn nueva() -> Self {
        Self { activa: false, ruta: String::new(), error: None }
    }

    pub fn activa(&self) -> bool {
        self.activa
    }

    pub fn ruta(&self) -> &str {
        &self.ruta
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Abre el prompt precargado con `ruta_inicial` — la ruta ya asociada
    /// al buffer si tenía una (permite "Guardar como" para renombrar o
    /// duplicar un archivo existente, no solo ponerle nombre a uno
    /// nuevo), o vacía si es un buffer sin nombre todavía.
    pub fn abrir(&mut self, ruta_inicial: &str) {
        self.activa = true;
        self.ruta = ruta_inicial.to_string();
        self.error = None;
    }

    pub fn cerrar(&mut self) {
        self.activa = false;
    }

    pub fn escribir(&mut self, c: char) {
        self.error = None;
        self.ruta.push(c);
    }

    pub fn borrar(&mut self) {
        self.error = None;
        self.ruta.pop();
    }

    /// `app` la llama cuando `Editor::guardar_como` devuelve un error —
    /// deja el prompt abierto con el motivo visible en vez de cerrarlo
    /// como si nada.
    pub fn establecer_error(&mut self, error: String) {
        self.error = Some(error);
    }
}

impl Default for EstadoGuardarComo {
    fn default() -> Self {
        Self::nueva()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arranca_cerrado_y_sin_ruta() {
        let estado = EstadoGuardarComo::nueva();
        assert!(!estado.activa());
        assert_eq!(estado.ruta(), "");
        assert!(estado.error().is_none());
    }

    #[test]
    fn abrir_precarga_la_ruta_inicial_y_limpia_el_error_previo() {
        let mut estado = EstadoGuardarComo::nueva();
        estado.establecer_error("algo falló".to_string());
        estado.abrir("archivo.rs");
        assert!(estado.activa());
        assert_eq!(estado.ruta(), "archivo.rs");
        assert!(estado.error().is_none());
    }

    #[test]
    fn escribir_y_borrar_modifican_la_ruta() {
        let mut estado = EstadoGuardarComo::nueva();
        estado.abrir("");
        for c in "nuevo.txt".chars() {
            estado.escribir(c);
        }
        assert_eq!(estado.ruta(), "nuevo.txt");
        estado.borrar();
        assert_eq!(estado.ruta(), "nuevo.tx");
    }

    #[test]
    fn escribir_y_borrar_limpian_un_error_previo() {
        let mut estado = EstadoGuardarComo::nueva();
        estado.abrir("a");
        estado.establecer_error("permiso denegado".to_string());
        estado.escribir('b');
        assert!(estado.error().is_none());

        estado.establecer_error("permiso denegado".to_string());
        estado.borrar();
        assert!(estado.error().is_none());
    }

    #[test]
    fn cerrar_desactiva_sin_borrar_la_ruta() {
        let mut estado = EstadoGuardarComo::nueva();
        estado.abrir("a.rs");
        estado.cerrar();
        assert!(!estado.activa());
        assert_eq!(estado.ruta(), "a.rs");
    }
}
