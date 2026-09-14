use crate::config::Config;

/// Las 6 secciones de PLAN.md §5 menos "Extensiones" (fase 2, fuera de
/// alcance de M4). El orden es el de la tabla del plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seccion {
    Atajos,
    Temas,
    Lenguajes,
    Editor,
    Interfaz,
}

impl Seccion {
    pub const TODAS: [Seccion; 5] =
        [Seccion::Atajos, Seccion::Temas, Seccion::Lenguajes, Seccion::Editor, Seccion::Interfaz];

    pub fn nombre(&self) -> &'static str {
        match self {
            Seccion::Atajos => "Atajos de teclado",
            Seccion::Temas => "Temas",
            Seccion::Lenguajes => "Lenguajes / LSP",
            Seccion::Editor => "Editor",
            Seccion::Interfaz => "Interfaz",
        }
    }

    /// Si la sección ya tiene contenido editable real. Aparece igual en
    /// la barra lateral (para que la navegación y el diseño final del
    /// panel ya se vean completos, PLAN.md §5), pero `entrar` no hace
    /// nada sobre ella y el área central muestra un aviso en vez de filas
    /// editables — mismo patrón de "se suma incrementalmente" que el
    /// cliente LSP (solo Python por ahora) o los lenguajes de
    /// tree-sitter.
    pub fn implementada(&self) -> bool {
        matches!(self, Seccion::Editor | Seccion::Temas)
    }

    /// Resumen de qué va a traer una sección todavía no implementada
    /// (PLAN.md §5), para mostrar en el área central en vez de dejarla en
    /// blanco.
    pub fn resumen_pendiente(&self) -> &'static str {
        match self {
            Seccion::Atajos => {
                "Próximamente: lista buscable de atajos, edición en línea, \
                 detección de conflictos en tiempo real, exportar/importar \
                 keymap."
            }
            Seccion::Lenguajes => {
                "Próximamente: habilitar/deshabilitar LSPs por lenguaje, ver \
                 estado de conexión y logs en vivo, indicador de si el \
                 binario está en el PATH."
            }
            Seccion::Interfaz => {
                "Próximamente: densidad de UI, mostrar/ocultar statusbar y \
                 tabs, elegir qué se muestra en la barra de estado."
            }
            Seccion::Editor | Seccion::Temas => "",
        }
    }
}

/// Un campo editable de la sección "Editor" (la única implementada por
/// ahora) — corresponde 1 a 1 con `tcode_config::ConfigEditor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampoEditor {
    TamanoTabulacion,
    UsarEspacios,
    AjusteLinea,
    NumerosDeLinea,
}

impl CampoEditor {
    pub const TODOS: [CampoEditor; 4] = [
        CampoEditor::TamanoTabulacion,
        CampoEditor::UsarEspacios,
        CampoEditor::AjusteLinea,
        CampoEditor::NumerosDeLinea,
    ];

    pub fn nombre(&self) -> &'static str {
        match self {
            CampoEditor::TamanoTabulacion => "Tamaño de tabulación",
            CampoEditor::UsarEspacios => "Usar espacios en vez de tabs",
            CampoEditor::AjusteLinea => "Ajuste de línea (wrap)",
            CampoEditor::NumerosDeLinea => "Números de línea",
        }
    }

    /// Nota aparte de un campo, si hace falta aclarar algo sobre su
    /// estado actual — por ahora solo "ajuste de línea", que se persiste
    /// pero todavía no reflowa el texto en pantalla (llega en una pieza
    /// aparte de M4).
    pub fn nota(&self) -> Option<&'static str> {
        match self {
            CampoEditor::AjusteLinea => Some("todavía no reflowa el texto en pantalla"),
            _ => None,
        }
    }

    /// Valor actual como texto, para dibujarlo en la fila.
    pub fn valor_actual(&self, config: &Config) -> String {
        match self {
            CampoEditor::TamanoTabulacion => config.editor.tamano_tabulacion.to_string(),
            CampoEditor::UsarEspacios => etiqueta_bool(config.editor.usar_espacios),
            CampoEditor::AjusteLinea => etiqueta_bool(config.editor.ajuste_linea),
            CampoEditor::NumerosDeLinea => etiqueta_bool(config.editor.numeros_de_linea),
        }
    }

    /// `Enter`/`←`/`→` sobre esta fila: alterna un booleano, o
    /// incrementa/decrementa (`delta` = -1 o 1) el tamaño de tabulación,
    /// recortado a 1..=16 — un rango razonable (0 dejaría de indentar
    /// nada; no tiene sentido práctico ir mucho más allá de 16).
    pub fn aplicar(&self, config: &mut Config, delta: i32) {
        match self {
            CampoEditor::TamanoTabulacion => {
                let nuevo = config.editor.tamano_tabulacion as i32 + delta;
                config.editor.tamano_tabulacion = nuevo.clamp(1, 16) as usize;
            }
            CampoEditor::UsarEspacios => config.editor.usar_espacios = !config.editor.usar_espacios,
            CampoEditor::AjusteLinea => config.editor.ajuste_linea = !config.editor.ajuste_linea,
            CampoEditor::NumerosDeLinea => config.editor.numeros_de_linea = !config.editor.numeros_de_linea,
        }
    }
}

fn etiqueta_bool(valor: bool) -> String {
    if valor { "Sí".to_string() } else { "No".to_string() }
}

/// Una fila de la sección "Temas": a diferencia de `CampoEditor`, no
/// alterna un valor en el sitio — dispara una acción (`app` decide qué
/// hacer, porque cruza a estado que este crate no conoce: abrir el
/// selector de temas ya activo, o escribir un archivo en disco).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampoTemas {
    ElegirTema,
    DuplicarActivo,
}

impl CampoTemas {
    pub const TODOS: [CampoTemas; 2] = [CampoTemas::ElegirTema, CampoTemas::DuplicarActivo];

    pub fn nombre(&self) -> &'static str {
        match self {
            CampoTemas::ElegirTema => "Elegir tema (con preview en vivo)",
            CampoTemas::DuplicarActivo => "Duplicar tema activo para editar/exportar",
        }
    }
}

/// Una fila que la búsqueda global del panel (`Ctrl+F`, PLAN.md §5) puede
/// encontrar. Solo hay filas de las secciones "Editor" y "Temas" por
/// ahora — las demás secciones todavía no tienen campos que buscar.
struct OpcionBuscable {
    seccion: usize,
    campo: usize,
    nombre: &'static str,
}

fn indice_de(seccion: Seccion) -> usize {
    Seccion::TODAS.iter().position(|s| *s == seccion).expect("la sección buscada está en TODAS")
}

fn opciones_buscables() -> Vec<OpcionBuscable> {
    let indice_editor = indice_de(Seccion::Editor);
    let indice_temas = indice_de(Seccion::Temas);
    CampoEditor::TODOS
        .iter()
        .enumerate()
        .map(|(campo, c)| OpcionBuscable { seccion: indice_editor, campo, nombre: c.nombre() })
        .chain(
            CampoTemas::TODOS
                .iter()
                .enumerate()
                .map(|(campo, c)| OpcionBuscable { seccion: indice_temas, campo, nombre: c.nombre() }),
        )
        .collect()
}

/// Un resultado de la búsqueda dentro del panel, con las posiciones que
/// matchearon (para resaltarlas en la UI, igual que la paleta de
/// comandos/buscador de archivos).
#[derive(Debug, Clone)]
pub struct ResultadoBusquedaAdmin {
    pub seccion: usize,
    pub campo: usize,
    pub nombre: &'static str,
    pub posiciones: Vec<usize>,
}

/// Dónde está el foco del teclado dentro del panel (PLAN.md §5: "Tab
/// cambia foco").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocoPanelAdmin {
    Barra,
    Central,
    Busqueda,
}

/// Estado del panel de administración (`Ctrl+,`, PLAN.md §5): sin
/// dependencias de terminal/`ratatui` — el crate `ui` lo dibuja, `app`
/// decide qué tecla llega aquí. Vive en `tcode-config` (no en un crate
/// aparte) porque por ahora solo orquesta campos de `Config`; el día que
/// la sección "Atajos" o "Lenguajes/LSP" necesiten datos de
/// `tcode-keymap`/`tcode-lsp`, esos valores/ediciones los resuelve quien
/// llame (mismo patrón que `CampoEditor::valor_actual`/`aplicar` ya usan
/// con `Config`) — este struct solo rastrea QUÉ fila está seleccionada,
/// nunca los valores en sí.
pub struct EstadoPanelAdmin {
    activo: bool,
    seccion: usize,
    foco: FocoPanelAdmin,
    campo: usize,
    busqueda: String,
    /// Mensaje transitorio de la última acción disparada en el área
    /// central (por ahora, solo "Temas: Duplicar tema activo" lo usa,
    /// para confirmar dónde quedó el archivo — ver `establecer_mensaje`).
    /// Se limpia solo al navegar a otro lado, no automáticamente con el
    /// tiempo: no hay una noción de "frame" en este struct sin `ratatui`.
    mensaje: Option<String>,
}

impl EstadoPanelAdmin {
    pub fn nueva() -> Self {
        Self {
            activo: false,
            seccion: 0,
            foco: FocoPanelAdmin::Barra,
            campo: 0,
            busqueda: String::new(),
            mensaje: None,
        }
    }

    pub fn activo(&self) -> bool {
        self.activo
    }

    pub fn foco(&self) -> FocoPanelAdmin {
        self.foco
    }

    pub fn campo(&self) -> usize {
        self.campo
    }

    pub fn busqueda(&self) -> &str {
        &self.busqueda
    }

    pub fn mensaje(&self) -> Option<&str> {
        self.mensaje.as_deref()
    }

    /// `app` la llama tras ejecutar una acción de una fila (por ahora,
    /// solo `CampoTemas::DuplicarActivo`) para dejar constancia de qué
    /// pasó — dónde quedó el archivo, o el error si falló.
    pub fn establecer_mensaje(&mut self, mensaje: String) {
        self.mensaje = Some(mensaje);
    }

    pub fn seccion_actual(&self) -> Seccion {
        Seccion::TODAS[self.seccion]
    }

    pub fn indice_seccion(&self) -> usize {
        self.seccion
    }

    /// Abre el panel siempre desde el mismo estado (barra lateral,
    /// primera sección) — más simple de razonar y de probar que recordar
    /// la última posición entre una apertura y la siguiente.
    pub fn abrir(&mut self) {
        self.activo = true;
        self.seccion = 0;
        self.foco = FocoPanelAdmin::Barra;
        self.campo = 0;
        self.busqueda.clear();
        self.mensaje = None;
    }

    pub fn cerrar(&mut self) {
        self.activo = false;
    }

    pub fn mover_seccion_abajo(&mut self) {
        if self.seccion + 1 < Seccion::TODAS.len() {
            self.seccion += 1;
            self.campo = 0;
            self.mensaje = None;
        }
    }

    pub fn mover_seccion_arriba(&mut self) {
        if self.seccion > 0 {
            self.seccion -= 1;
            self.campo = 0;
            self.mensaje = None;
        }
    }

    /// Cuántas filas editables tiene el área central de la sección
    /// actual (0 si todavía no está implementada).
    pub fn num_campos(&self) -> usize {
        match self.seccion_actual() {
            Seccion::Editor => CampoEditor::TODOS.len(),
            Seccion::Temas => CampoTemas::TODOS.len(),
            _ => 0,
        }
    }

    /// Cuántas filas puede recorrer `mover_campo_arriba`/`mover_campo_abajo`
    /// ahora mismo: las del área central si el foco está ahí, o los
    /// resultados de la búsqueda si está en la búsqueda — `campo` hace
    /// doble uso como índice de fila seleccionada en cualquiera de los
    /// dos casos, porque nunca están activos al mismo tiempo.
    fn total_filas_navegables(&self) -> usize {
        match self.foco {
            FocoPanelAdmin::Central => self.num_campos(),
            FocoPanelAdmin::Busqueda => self.resultados_busqueda().len(),
            FocoPanelAdmin::Barra => 0,
        }
    }

    pub fn mover_campo_abajo(&mut self) {
        let total = self.total_filas_navegables();
        if total > 0 {
            self.campo = (self.campo + 1).min(total - 1);
        }
    }

    pub fn mover_campo_arriba(&mut self) {
        self.campo = self.campo.saturating_sub(1);
    }

    pub fn campo_editor_actual(&self) -> Option<CampoEditor> {
        if self.seccion_actual() == Seccion::Editor {
            CampoEditor::TODOS.get(self.campo).copied()
        } else {
            None
        }
    }

    pub fn campo_temas_actual(&self) -> Option<CampoTemas> {
        if self.seccion_actual() == Seccion::Temas {
            CampoTemas::TODOS.get(self.campo).copied()
        } else {
            None
        }
    }

    /// `Tab`: alterna entre la barra lateral y el área central. Desde la
    /// barra solo entra si la sección tiene contenido (ver `entrar`);
    /// desde la búsqueda no hace nada (cerrarla es cosa de `Esc`/`Enter`).
    pub fn alternar_foco(&mut self) {
        match self.foco {
            FocoPanelAdmin::Barra => self.entrar(),
            FocoPanelAdmin::Central => self.foco = FocoPanelAdmin::Barra,
            FocoPanelAdmin::Busqueda => {}
        }
    }

    /// `Enter`/`→` sobre la barra lateral: entra al área central, solo si
    /// la sección ya tiene contenido editable.
    pub fn entrar(&mut self) {
        if self.foco == FocoPanelAdmin::Barra && self.seccion_actual().implementada() {
            self.foco = FocoPanelAdmin::Central;
            self.campo = 0;
            self.mensaje = None;
        }
    }

    /// `Esc` dentro del panel: cierra la búsqueda si estaba abierta; si
    /// no, vuelve del área central a la barra lateral; si no, cierra el
    /// panel entero. Devuelve `true` si el panel sigue abierto después de
    /// este `Esc` (para que quien llama sepa si debe seguir capturando el
    /// teclado o devolver el foco al editor).
    pub fn escape(&mut self) -> bool {
        match self.foco {
            FocoPanelAdmin::Busqueda => {
                self.busqueda.clear();
                self.foco = FocoPanelAdmin::Barra;
                true
            }
            FocoPanelAdmin::Central => {
                self.foco = FocoPanelAdmin::Barra;
                self.mensaje = None;
                true
            }
            FocoPanelAdmin::Barra => {
                self.cerrar();
                false
            }
        }
    }

    /// `Ctrl+F` dentro del panel: búsqueda global de opciones por nombre
    /// en español (PLAN.md §5).
    pub fn abrir_busqueda(&mut self) {
        self.foco = FocoPanelAdmin::Busqueda;
        self.busqueda.clear();
        self.campo = 0;
        self.mensaje = None;
    }

    pub fn escribir_busqueda(&mut self, c: char) {
        self.busqueda.push(c);
        self.campo = 0;
    }

    pub fn borrar_busqueda(&mut self) {
        self.busqueda.pop();
        self.campo = 0;
    }

    /// Filas que coinciden con la búsqueda actual, mejor coincidencia
    /// primero (mismo algoritmo — y misma sensación de uso — que la
    /// paleta de comandos y el buscador de archivos, PLAN.md §4).
    pub fn resultados_busqueda(&self) -> Vec<ResultadoBusquedaAdmin> {
        let opciones = opciones_buscables();
        tcode_fuzzy::filtrar_y_ordenar(&self.busqueda, &opciones, |o| o.nombre)
            .into_iter()
            .map(|(o, coincidencia)| ResultadoBusquedaAdmin {
                seccion: o.seccion,
                campo: o.campo,
                nombre: o.nombre,
                posiciones: coincidencia.posiciones,
            })
            .collect()
    }

    /// `Enter` sobre el resultado de búsqueda seleccionado (`campo`,
    /// mismo índice que usan `mover_campo_arriba`/`mover_campo_abajo`
    /// mientras el foco está en la búsqueda): salta directo a esa
    /// sección/campo en el área central y cierra la búsqueda. Si el
    /// filtro actual no deja ningún resultado, no hace nada — la
    /// búsqueda sigue abierta.
    pub fn confirmar_busqueda(&mut self) {
        if let Some(resultado) = self.resultados_busqueda().get(self.campo) {
            self.seccion = resultado.seccion;
            self.campo = resultado.campo;
            self.foco = FocoPanelAdmin::Central;
            self.busqueda.clear();
        }
    }
}

impl Default for EstadoPanelAdmin {
    fn default() -> Self {
        Self::nueva()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abrir_arranca_en_la_barra_con_la_primera_seccion() {
        let mut panel = EstadoPanelAdmin::nueva();
        panel.abrir();
        assert!(panel.activo());
        assert_eq!(panel.foco(), FocoPanelAdmin::Barra);
        assert_eq!(panel.seccion_actual(), Seccion::Atajos);
    }

    #[test]
    fn mover_seccion_se_recorta_a_los_limites() {
        let mut panel = EstadoPanelAdmin::nueva();
        panel.abrir();
        panel.mover_seccion_arriba();
        assert_eq!(panel.indice_seccion(), 0);

        for _ in 0..(Seccion::TODAS.len() + 5) {
            panel.mover_seccion_abajo();
        }
        assert_eq!(panel.indice_seccion(), Seccion::TODAS.len() - 1);
        assert_eq!(panel.seccion_actual(), Seccion::Interfaz);
    }

    #[test]
    fn entrar_no_hace_nada_en_una_seccion_no_implementada() {
        let mut panel = EstadoPanelAdmin::nueva();
        panel.abrir();
        assert_eq!(panel.seccion_actual(), Seccion::Atajos);
        panel.entrar();
        assert_eq!(panel.foco(), FocoPanelAdmin::Barra);
    }

    #[test]
    fn entrar_funciona_en_la_seccion_editor() {
        let mut panel = EstadoPanelAdmin::nueva();
        panel.abrir();
        for _ in 0..3 {
            panel.mover_seccion_abajo();
        }
        assert_eq!(panel.seccion_actual(), Seccion::Editor);
        panel.entrar();
        assert_eq!(panel.foco(), FocoPanelAdmin::Central);
        assert_eq!(panel.campo(), 0);
    }

    #[test]
    fn entrar_funciona_en_la_seccion_temas() {
        let mut panel = EstadoPanelAdmin::nueva();
        panel.abrir();
        panel.mover_seccion_abajo();
        assert_eq!(panel.seccion_actual(), Seccion::Temas);
        panel.entrar();
        assert_eq!(panel.foco(), FocoPanelAdmin::Central);
        assert_eq!(panel.num_campos(), CampoTemas::TODOS.len());
        assert_eq!(panel.campo_temas_actual(), Some(CampoTemas::ElegirTema));
        panel.mover_campo_abajo();
        assert_eq!(panel.campo_temas_actual(), Some(CampoTemas::DuplicarActivo));
        // Fuera de la sección Temas no hay campo de temas que devolver,
        // aunque el índice numérico coincida.
        assert_eq!(panel.campo_editor_actual(), None);
    }

    #[test]
    fn establecer_mensaje_se_limpia_al_navegar() {
        let mut panel = EstadoPanelAdmin::nueva();
        panel.abrir();
        panel.mover_seccion_abajo();
        panel.entrar();
        panel.establecer_mensaje("Copia creada".to_string());
        assert_eq!(panel.mensaje(), Some("Copia creada"));

        panel.mover_campo_abajo();
        // Moverse de fila no descarta el mensaje: sigue siendo relevante
        // hasta que se cambie de sección o se vuelva a la barra.
        assert_eq!(panel.mensaje(), Some("Copia creada"));

        panel.escape();
        assert_eq!(panel.mensaje(), None);
    }

    #[test]
    fn alternar_foco_va_y_vuelve_entre_barra_y_central() {
        let mut panel = EstadoPanelAdmin::nueva();
        panel.abrir();
        for _ in 0..3 {
            panel.mover_seccion_abajo();
        }
        panel.alternar_foco();
        assert_eq!(panel.foco(), FocoPanelAdmin::Central);
        panel.alternar_foco();
        assert_eq!(panel.foco(), FocoPanelAdmin::Barra);
    }

    #[test]
    fn mover_campo_se_recorta_a_los_limites_de_la_seccion_editor() {
        let mut panel = EstadoPanelAdmin::nueva();
        panel.abrir();
        for _ in 0..3 {
            panel.mover_seccion_abajo();
        }
        panel.entrar();
        panel.mover_campo_arriba();
        assert_eq!(panel.campo(), 0);

        for _ in 0..(CampoEditor::TODOS.len() + 5) {
            panel.mover_campo_abajo();
        }
        assert_eq!(panel.campo(), CampoEditor::TODOS.len() - 1);
    }

    #[test]
    fn escape_recorre_busqueda_luego_central_luego_cierra() {
        let mut panel = EstadoPanelAdmin::nueva();
        panel.abrir();
        for _ in 0..3 {
            panel.mover_seccion_abajo();
        }
        panel.entrar();
        panel.abrir_busqueda();
        panel.escribir_busqueda('x');

        assert!(panel.escape());
        assert_eq!(panel.foco(), FocoPanelAdmin::Barra);
        assert_eq!(panel.busqueda(), "");

        // Sin `entrar` de nuevo, ya estamos en la barra: el próximo Esc
        // cierra el panel entero.
        assert!(!panel.escape());
        assert!(!panel.activo());
    }

    #[test]
    fn campo_editor_toggle_de_booleano() {
        let mut config = Config::default();
        assert!(config.editor.numeros_de_linea);
        CampoEditor::NumerosDeLinea.aplicar(&mut config, 1);
        assert!(!config.editor.numeros_de_linea);
        CampoEditor::NumerosDeLinea.aplicar(&mut config, -1);
        assert!(config.editor.numeros_de_linea);
    }

    #[test]
    fn campo_editor_tamano_tabulacion_se_recorta_entre_1_y_16() {
        let mut config = Config::default();
        config.editor.tamano_tabulacion = 1;
        CampoEditor::TamanoTabulacion.aplicar(&mut config, -1);
        assert_eq!(config.editor.tamano_tabulacion, 1);

        config.editor.tamano_tabulacion = 16;
        CampoEditor::TamanoTabulacion.aplicar(&mut config, 1);
        assert_eq!(config.editor.tamano_tabulacion, 16);
    }

    #[test]
    fn busqueda_filtra_y_confirmar_salta_a_la_seccion_editor() {
        let mut panel = EstadoPanelAdmin::nueva();
        panel.abrir();
        panel.abrir_busqueda();
        for c in "tabula".chars() {
            panel.escribir_busqueda(c);
        }
        let resultados = panel.resultados_busqueda();
        assert!(resultados.iter().any(|r| r.nombre == CampoEditor::TamanoTabulacion.nombre()));

        panel.confirmar_busqueda();
        assert_eq!(panel.foco(), FocoPanelAdmin::Central);
        assert_eq!(panel.seccion_actual(), Seccion::Editor);
        assert_eq!(panel.campo_editor_actual(), Some(CampoEditor::TamanoTabulacion));
    }

    #[test]
    fn busqueda_sin_coincidencias_no_rompe_confirmar() {
        let mut panel = EstadoPanelAdmin::nueva();
        panel.abrir();
        panel.abrir_busqueda();
        for c in "esto-no-existe-como-opcion".chars() {
            panel.escribir_busqueda(c);
        }
        assert!(panel.resultados_busqueda().is_empty());
        panel.confirmar_busqueda();
        // No pasa nada: sigue en la búsqueda, no salta a ningún lado.
        assert_eq!(panel.foco(), FocoPanelAdmin::Busqueda);
    }

    #[test]
    fn mover_campo_navega_los_resultados_de_busqueda_no_los_del_area_central() {
        let mut panel = EstadoPanelAdmin::nueva();
        panel.abrir();
        panel.abrir_busqueda();
        // Consulta vacía: coincide con todas las opciones buscables
        // (Editor + Temas, únicas secciones con campos registrados).
        let total = panel.resultados_busqueda().len();
        assert_eq!(total, CampoEditor::TODOS.len() + CampoTemas::TODOS.len());

        for _ in 0..(total + 5) {
            panel.mover_campo_abajo();
        }
        assert_eq!(panel.campo(), total - 1);

        panel.mover_campo_arriba();
        assert_eq!(panel.campo(), total - 2);
    }
}
