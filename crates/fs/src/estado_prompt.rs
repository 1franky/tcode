use std::path::PathBuf;

/// Qué se está pidiendo por texto en el prompt del explorador (`Ctrl+K
/// N`/`Ctrl+K C`/`Ctrl+K M`, BACKLOG.md P0 "explorador de solo lectura")
/// — nuevo archivo, nueva carpeta, o renombrar la selección actual.
/// Mismo patrón que `tcode_core::EstadoGuardarComo` (recuadro con un
/// campo de una línea, `Enter` confirma, `Esc` cancela, error visible sin
/// cerrar el prompt) pero para operaciones del árbol de archivos, no del
/// buffer activo — se mantienen separados porque no comparten ni el
/// disparador ni la acción de confirmar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModoPromptExplorador {
    NuevoArchivo,
    NuevaCarpeta,
    Renombrar,
}

impl ModoPromptExplorador {
    /// Título del recuadro (`tcode_ui::panel_prompt_explorador`).
    pub fn titulo(&self) -> &'static str {
        match self {
            ModoPromptExplorador::NuevoArchivo => "Nuevo archivo",
            ModoPromptExplorador::NuevaCarpeta => "Nueva carpeta",
            ModoPromptExplorador::Renombrar => "Renombrar",
        }
    }
}

/// Estado del prompt de texto del explorador — activo solo si `modo` es
/// `Some`; `None` es el estado de reposo (nada abierto).
#[derive(Debug, Clone, Default)]
pub struct EstadoPromptExplorador {
    modo: Option<ModoPromptExplorador>,
    texto: String,
    error: Option<String>,
}

impl EstadoPromptExplorador {
    pub fn nuevo() -> Self {
        Self::default()
    }

    pub fn activo(&self) -> bool {
        self.modo.is_some()
    }

    pub fn modo(&self) -> Option<ModoPromptExplorador> {
        self.modo
    }

    pub fn texto(&self) -> &str {
        &self.texto
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Abre el prompt en `modo`, precargado con `texto_inicial` (vacío
    /// para "nuevo archivo/carpeta"; el nombre actual para "renombrar",
    /// así alcanza con ajustar en vez de reescribirlo entero).
    pub fn abrir(&mut self, modo: ModoPromptExplorador, texto_inicial: &str) {
        self.modo = Some(modo);
        self.texto = texto_inicial.to_string();
        self.error = None;
    }

    pub fn cerrar(&mut self) {
        self.modo = None;
    }

    pub fn escribir(&mut self, c: char) {
        self.error = None;
        self.texto.push(c);
    }

    pub fn borrar(&mut self) {
        self.error = None;
        self.texto.pop();
    }

    /// `app` la llama cuando la operación de filesystem correspondiente
    /// (`Explorador::crear_archivo`/`crear_carpeta`/`renombrar_seleccion`)
    /// devuelve un error — deja el prompt abierto con el motivo visible
    /// en vez de cerrarlo como si nada, igual que "Guardar como".
    pub fn establecer_error(&mut self, error: String) {
        self.error = Some(error);
    }
}

/// Confirmación de borrado (`Delete` con el explorador enfocado,
/// reinterpretado igual que el resto de la navegación genérica) — acción
/// destructiva e irreversible (`std::fs::remove_file`/`remove_dir_all`
/// no pasan por ninguna papelera de reciclaje), así que no se ejecuta
/// nunca directo desde la tecla: siempre pasa por este prompt explícito
/// primero ("¿Borrar 'x.txt'? (y/n)", `tcode_ui::panel_confirmar_borrado`).
#[derive(Debug, Clone, Default)]
pub struct EstadoConfirmarBorrado {
    objetivo: Option<PathBuf>,
    nombre: String,
    es_carpeta: bool,
}

impl EstadoConfirmarBorrado {
    pub fn nuevo() -> Self {
        Self::default()
    }

    pub fn activo(&self) -> bool {
        self.objetivo.is_some()
    }

    /// Nombre del archivo/carpeta a punto de borrarse, para mostrar en el
    /// prompt sin tener que volver a consultar el árbol.
    pub fn nombre(&self) -> &str {
        &self.nombre
    }

    pub fn es_carpeta(&self) -> bool {
        self.es_carpeta
    }

    pub fn abrir(&mut self, ruta: PathBuf, nombre: String, es_carpeta: bool) {
        self.objetivo = Some(ruta);
        self.nombre = nombre;
        self.es_carpeta = es_carpeta;
    }

    pub fn cerrar(&mut self) {
        self.objetivo = None;
    }

    /// Confirma el borrado: devuelve la ruta a borrar y cierra el prompt
    /// (`None` si no había ninguno abierto). Quien llama es responsable
    /// de borrarla de verdad (`Explorador::borrar_seleccion`) — este
    /// struct no toca el filesystem.
    pub fn confirmar(&mut self) -> Option<PathBuf> {
        self.objetivo.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_arranca_inactivo() {
        let estado = EstadoPromptExplorador::nuevo();
        assert!(!estado.activo());
        assert_eq!(estado.modo(), None);
    }

    #[test]
    fn abrir_prompt_precarga_texto_y_limpia_error_previo() {
        let mut estado = EstadoPromptExplorador::nuevo();
        estado.establecer_error("algo falló".to_string());
        estado.abrir(ModoPromptExplorador::Renombrar, "viejo.txt");
        assert!(estado.activo());
        assert_eq!(estado.modo(), Some(ModoPromptExplorador::Renombrar));
        assert_eq!(estado.texto(), "viejo.txt");
        assert!(estado.error().is_none());
    }

    #[test]
    fn escribir_y_borrar_modifican_el_texto_y_limpian_error() {
        let mut estado = EstadoPromptExplorador::nuevo();
        estado.abrir(ModoPromptExplorador::NuevoArchivo, "");
        for c in "a.txt".chars() {
            estado.escribir(c);
        }
        assert_eq!(estado.texto(), "a.txt");
        estado.establecer_error("ya existe".to_string());
        estado.borrar();
        assert_eq!(estado.texto(), "a.tx");
        assert!(estado.error().is_none());
    }

    #[test]
    fn cerrar_desactiva_sin_borrar_el_texto() {
        let mut estado = EstadoPromptExplorador::nuevo();
        estado.abrir(ModoPromptExplorador::NuevaCarpeta, "carpeta");
        estado.cerrar();
        assert!(!estado.activo());
        assert_eq!(estado.texto(), "carpeta");
    }

    #[test]
    fn confirmar_borrado_arranca_inactivo() {
        let estado = EstadoConfirmarBorrado::nuevo();
        assert!(!estado.activo());
    }

    #[test]
    fn abrir_confirmar_borrado_guarda_nombre_y_tipo() {
        let mut estado = EstadoConfirmarBorrado::nuevo();
        estado.abrir(PathBuf::from("/tmp/a.txt"), "a.txt".to_string(), false);
        assert!(estado.activo());
        assert_eq!(estado.nombre(), "a.txt");
        assert!(!estado.es_carpeta());
    }

    #[test]
    fn confirmar_devuelve_la_ruta_y_cierra() {
        let mut estado = EstadoConfirmarBorrado::nuevo();
        estado.abrir(PathBuf::from("/tmp/a.txt"), "a.txt".to_string(), false);
        let ruta = estado.confirmar();
        assert_eq!(ruta, Some(PathBuf::from("/tmp/a.txt")));
        assert!(!estado.activo());
    }

    #[test]
    fn cancelar_no_devuelve_nada_al_confirmar_despues() {
        let mut estado = EstadoConfirmarBorrado::nuevo();
        estado.abrir(PathBuf::from("/tmp/a.txt"), "a.txt".to_string(), false);
        estado.cerrar();
        assert_eq!(estado.confirmar(), None);
    }
}
