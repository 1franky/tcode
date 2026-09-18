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
        // Las 5 secciones de M4 (todas menos "Extensiones", fase 2) ya
        // tienen contenido real. Este método se queda igual (siempre
        // `true` por ahora) para cuando se agregue una sección nueva
        // que todavía no lo tenga.
        true
    }

    /// Resumen de qué va a traer una sección todavía no implementada
    /// (PLAN.md §5), para mostrar en el área central en vez de dejarla en
    /// blanco. Sin uso real por ahora (las 5 secciones ya están
    /// implementadas), pero se mantiene para la próxima sección que
    /// llegue sin contenido todavía.
    pub fn resumen_pendiente(&self) -> &'static str {
        ""
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
    ModoVim,
}

impl CampoEditor {
    pub const TODOS: [CampoEditor; 5] = [
        CampoEditor::TamanoTabulacion,
        CampoEditor::UsarEspacios,
        CampoEditor::AjusteLinea,
        CampoEditor::NumerosDeLinea,
        CampoEditor::ModoVim,
    ];

    pub fn nombre(&self) -> &'static str {
        match self {
            CampoEditor::TamanoTabulacion => "Tamaño de tabulación",
            CampoEditor::UsarEspacios => "Usar espacios en vez de tabs",
            CampoEditor::AjusteLinea => "Ajuste de línea (wrap)",
            CampoEditor::NumerosDeLinea => "Números de línea",
            CampoEditor::ModoVim => "Modo VIM (hjkl, Normal/Insertar)",
        }
    }

    /// Nota aparte de un campo, si hace falta aclarar algo sobre su
    /// estado actual.
    pub fn nota(&self) -> Option<&'static str> {
        match self {
            CampoEditor::ModoVim => Some("Alcance inicial: sin operadores combinables (dw, d$), sin conteos (3dd), sin :"),
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
            CampoEditor::ModoVim => etiqueta_bool(config.editor.modo_vim),
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
            CampoEditor::ModoVim => config.editor.modo_vim = !config.editor.modo_vim,
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

/// Un campo editable de la sección "Interfaz" (PLAN.md §5.5) —
/// corresponde 1 a 1 con `tcode_config::ConfigInterfaz`, salvo `tema`
/// (que tiene su propia sección "Temas") y lo que todavía no existe
/// como feature (densidad de UI, tabs, breadcrumbs, rama git en la
/// statusbar).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampoInterfaz {
    MostrarStatusbar,
    StatusbarPosicionCursor,
    StatusbarCodificacion,
    StatusbarEol,
    StatusbarLenguaje,
    StatusbarDiagnosticos,
    StatusbarModo,
}

impl CampoInterfaz {
    pub const TODOS: [CampoInterfaz; 7] = [
        CampoInterfaz::MostrarStatusbar,
        CampoInterfaz::StatusbarPosicionCursor,
        CampoInterfaz::StatusbarCodificacion,
        CampoInterfaz::StatusbarEol,
        CampoInterfaz::StatusbarLenguaje,
        CampoInterfaz::StatusbarDiagnosticos,
        CampoInterfaz::StatusbarModo,
    ];

    pub fn nombre(&self) -> &'static str {
        match self {
            CampoInterfaz::MostrarStatusbar => "Mostrar barra de estado",
            CampoInterfaz::StatusbarPosicionCursor => "Statusbar: posición del cursor",
            CampoInterfaz::StatusbarCodificacion => "Statusbar: codificación",
            CampoInterfaz::StatusbarEol => "Statusbar: fin de línea (EOL)",
            CampoInterfaz::StatusbarLenguaje => "Statusbar: lenguaje detectado",
            CampoInterfaz::StatusbarDiagnosticos => "Statusbar: resumen de diagnósticos LSP",
            CampoInterfaz::StatusbarModo => "Statusbar: modo",
        }
    }

    pub fn valor_actual(&self, config: &Config) -> String {
        let activo = match self {
            CampoInterfaz::MostrarStatusbar => config.interfaz.mostrar_statusbar,
            CampoInterfaz::StatusbarPosicionCursor => config.interfaz.statusbar_posicion_cursor,
            CampoInterfaz::StatusbarCodificacion => config.interfaz.statusbar_codificacion,
            CampoInterfaz::StatusbarEol => config.interfaz.statusbar_eol,
            CampoInterfaz::StatusbarLenguaje => config.interfaz.statusbar_lenguaje,
            CampoInterfaz::StatusbarDiagnosticos => config.interfaz.statusbar_diagnosticos,
            CampoInterfaz::StatusbarModo => config.interfaz.statusbar_modo,
        };
        etiqueta_bool(activo)
    }

    /// `Enter`/`←`/`→` sobre esta fila: todas las de "Interfaz" son
    /// booleanas, así que solo alternan — a diferencia de `CampoEditor`,
    /// no hace falta un `delta`.
    pub fn aplicar(&self, config: &mut Config) {
        let campo = match self {
            CampoInterfaz::MostrarStatusbar => &mut config.interfaz.mostrar_statusbar,
            CampoInterfaz::StatusbarPosicionCursor => &mut config.interfaz.statusbar_posicion_cursor,
            CampoInterfaz::StatusbarCodificacion => &mut config.interfaz.statusbar_codificacion,
            CampoInterfaz::StatusbarEol => &mut config.interfaz.statusbar_eol,
            CampoInterfaz::StatusbarLenguaje => &mut config.interfaz.statusbar_lenguaje,
            CampoInterfaz::StatusbarDiagnosticos => &mut config.interfaz.statusbar_diagnosticos,
            CampoInterfaz::StatusbarModo => &mut config.interfaz.statusbar_modo,
        };
        *campo = !*campo;
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

/// Índice de `seccion` dentro de [`Seccion::TODAS`] — lo necesita `app`
/// para construir [`OpcionExterna`]s (Atajos) sin tener que reimplementar
/// esta búsqueda.
pub fn indice_de(seccion: Seccion) -> usize {
    Seccion::TODAS.iter().position(|s| *s == seccion).expect("la sección buscada está en TODAS")
}

fn opciones_buscables() -> Vec<OpcionBuscable> {
    let indice_editor = indice_de(Seccion::Editor);
    let indice_temas = indice_de(Seccion::Temas);
    let indice_interfaz = indice_de(Seccion::Interfaz);
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
        .chain(
            CampoInterfaz::TODOS
                .iter()
                .enumerate()
                .map(|(campo, c)| OpcionBuscable { seccion: indice_interfaz, campo, nombre: c.nombre() }),
        )
        .collect()
}

/// Una fila buscable que este crate no puede describir por sí solo,
/// porque su contenido vive en otro crate (por ahora, "Atajos": un
/// comando de `tcode-commands` con su combinación actual en
/// `tcode-keymap`). `app` la construye una sola vez — la lista de
/// comandos es fija durante toda la sesión, así que no hace falta
/// recalcularla en cada tecla — y la registra con
/// [`EstadoPanelAdmin::fijar_opciones_externas`].
#[derive(Debug, Clone, Copy)]
pub struct OpcionExterna {
    pub seccion: usize,
    pub campo: usize,
    pub nombre: &'static str,
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
/// aparte) porque orquesta campos de `Config` directamente (Editor); para
/// lo que necesita datos de `tcode-keymap`/`tcode-commands` (Atajos) o
/// mañana `tcode-lsp` (Lenguajes/LSP), este struct no los conoce — solo
/// expone puntos de extensión genéricos (`fijar_num_filas_atajos`,
/// `fijar_opciones_externas`) que `app` llena una vez al arrancar, y el
/// propio render/edición de esas filas vive en `ui`/`app` con acceso
/// directo a `Keymap`. Este struct solo rastrea QUÉ fila está
/// seleccionada, nunca los valores en sí (salvo los de `Config`, que sí
/// resuelve directo porque ya los tiene disponibles).
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
    /// Cantidad de filas de la sección "Atajos" (la fila especial
    /// "restablecer todos" más un comando por fila) — fijada una sola
    /// vez por `app` al arrancar, porque la lista de comandos es fija
    /// durante toda la sesión (ver `OpcionExterna`).
    num_filas_atajos: usize,
    /// Igual que `num_filas_atajos`, pero para "Lenguajes / LSP" — un
    /// lenguaje por fila (`tcode_syntax::Lenguaje::TODOS`).
    num_filas_lenguajes: usize,
    opciones_externas: Vec<OpcionExterna>,
    /// `true` mientras el panel espera que se presione la tecla que va a
    /// convertirse en el nuevo atajo de la fila seleccionada (`Enter`
    /// sobre un comando en "Atajos", PLAN.md §5: "presionás la nueva
    /// combinación y se guarda"). Es un flag genérico a propósito — este
    /// crate no sabe qué significa "capturar una tecla nueva" más allá de
    /// prender/apagar el modo; `app` es quien interpreta la tecla
    /// siguiente y decide qué hacer con ella.
    capturando: bool,
    /// `Some(buffer)` mientras se edita el comando LSP personalizado de un
    /// lenguaje en la sección "Lenguajes / LSP" (PLAN.md §5.3) — el
    /// `buffer` es la línea completa tal como se está escribiendo
    /// ("comando arg1 arg2 ..."), igual al formato que espera
    /// `ConfigLenguajes::fijar_comando_desde_linea`. Igual que
    /// `capturando`, es un flag genérico: este crate no sabe de teclas
    /// concretas, solo guarda el texto que `app` va acumulando.
    editando_comando_lsp: Option<String>,
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
            num_filas_atajos: 0,
            num_filas_lenguajes: 0,
            opciones_externas: Vec::new(),
            capturando: false,
            editando_comando_lsp: None,
        }
    }

    /// `app` la llama una sola vez al arrancar, con `1 + comandos_
    /// disponibles().len()` (la fila especial "restablecer todos" más un
    /// comando por fila).
    pub fn fijar_num_filas_atajos(&mut self, num: usize) {
        self.num_filas_atajos = num;
    }

    /// `app` la llama una sola vez al arrancar, con `tcode_syntax::
    /// Lenguaje::TODOS.len()` (un lenguaje por fila, sin fila especial —
    /// a diferencia de "Atajos", acá no hace falta un "restablecer
    /// todos": alternar habilitado/deshabilitado no tiene un valor "por
    /// defecto" que perder).
    pub fn fijar_num_filas_lenguajes(&mut self, num: usize) {
        self.num_filas_lenguajes = num;
    }

    /// `app` la llama una sola vez al arrancar con una fila por comando
    /// de `tcode_commands::comandos_disponibles()`, para que la búsqueda
    /// global (`Ctrl+F`) también encuentre atajos por nombre en español.
    pub fn fijar_opciones_externas(&mut self, opciones: Vec<OpcionExterna>) {
        self.opciones_externas = opciones;
    }

    pub fn capturando(&self) -> bool {
        self.capturando
    }

    /// `Enter` sobre un comando en "Atajos": el panel pasa a esperar la
    /// próxima tecla, que `app` interpreta como la nueva combinación.
    pub fn iniciar_captura(&mut self) {
        self.capturando = true;
        self.mensaje = None;
    }

    /// Cualquier tecla mientras se está esperando (haya terminado en un
    /// nuevo atajo, un error, o un `Esc` que cancela) apaga el modo de
    /// captura — siempre se llama exactamente una vez por tecla recibida
    /// en ese estado.
    pub fn terminar_captura(&mut self) {
        self.capturando = false;
    }

    /// Buffer actual mientras se edita un comando LSP personalizado, o
    /// `None` si no se está editando ninguno.
    pub fn editando_comando_lsp(&self) -> Option<&str> {
        self.editando_comando_lsp.as_deref()
    }

    /// Empieza a editar el comando LSP de la fila seleccionada en
    /// "Lenguajes / LSP", con `valor_inicial` precargado en el buffer —
    /// `app` decide qué precargar (el comando personalizado si ya hay
    /// uno, o el efectivo por defecto, o vacío si no hay ninguno).
    pub fn iniciar_edicion_comando_lsp(&mut self, valor_inicial: String) {
        self.editando_comando_lsp = Some(valor_inicial);
        self.mensaje = None;
    }

    pub fn escribir_comando_lsp(&mut self, c: char) {
        if let Some(buffer) = &mut self.editando_comando_lsp {
            buffer.push(c);
        }
    }

    pub fn borrar_comando_lsp(&mut self) {
        if let Some(buffer) = &mut self.editando_comando_lsp {
            buffer.pop();
        }
    }

    /// `Esc` durante la edición: descarta el buffer sin guardar nada.
    pub fn cancelar_edicion_comando_lsp(&mut self) {
        self.editando_comando_lsp = None;
    }

    /// `Enter` durante la edición: devuelve el buffer para que `app` lo
    /// guarde vía `ConfigLenguajes::fijar_comando_desde_linea` y cierra el
    /// modo de edición. `None` si no se estaba editando nada (no debería
    /// pasar si quien llama ya chequeó `editando_comando_lsp().is_some()`).
    pub fn confirmar_edicion_comando_lsp(&mut self) -> Option<String> {
        self.editando_comando_lsp.take()
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
        self.capturando = false;
        self.editando_comando_lsp = None;
    }

    pub fn cerrar(&mut self) {
        self.activo = false;
    }

    pub fn mover_seccion_abajo(&mut self) {
        if self.seccion + 1 < Seccion::TODAS.len() {
            self.seccion += 1;
            self.campo = 0;
            self.mensaje = None;
            self.capturando = false;
            self.editando_comando_lsp = None;
        }
    }

    pub fn mover_seccion_arriba(&mut self) {
        if self.seccion > 0 {
            self.seccion -= 1;
            self.campo = 0;
            self.mensaje = None;
            self.capturando = false;
            self.editando_comando_lsp = None;
        }
    }

    /// Cuántas filas editables tiene el área central de la sección
    /// actual (0 si todavía no está implementada).
    pub fn num_campos(&self) -> usize {
        match self.seccion_actual() {
            Seccion::Editor => CampoEditor::TODOS.len(),
            Seccion::Temas => CampoTemas::TODOS.len(),
            Seccion::Atajos => self.num_filas_atajos,
            Seccion::Lenguajes => self.num_filas_lenguajes,
            Seccion::Interfaz => CampoInterfaz::TODOS.len(),
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

    pub fn campo_interfaz_actual(&self) -> Option<CampoInterfaz> {
        if self.seccion_actual() == Seccion::Interfaz {
            CampoInterfaz::TODOS.get(self.campo).copied()
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
            self.capturando = false;
            self.editando_comando_lsp = None;
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
                self.capturando = false;
                self.editando_comando_lsp = None;
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
        self.capturando = false;
        self.editando_comando_lsp = None;
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
        let internas = opciones_buscables();
        let externas: Vec<OpcionBuscable> = self
            .opciones_externas
            .iter()
            .map(|o| OpcionBuscable { seccion: o.seccion, campo: o.campo, nombre: o.nombre })
            .collect();
        let todas: Vec<OpcionBuscable> = internas.into_iter().chain(externas).collect();
        tcode_fuzzy::filtrar_y_ordenar(&self.busqueda, &todas, |o| o.nombre)
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
    fn entrar_funciona_en_la_seccion_interfaz() {
        let mut panel = EstadoPanelAdmin::nueva();
        panel.abrir();
        for _ in 0..4 {
            panel.mover_seccion_abajo();
        }
        assert_eq!(panel.seccion_actual(), Seccion::Interfaz);
        panel.entrar();
        assert_eq!(panel.foco(), FocoPanelAdmin::Central);
        assert_eq!(panel.num_campos(), CampoInterfaz::TODOS.len());
        assert_eq!(panel.campo_interfaz_actual(), Some(CampoInterfaz::MostrarStatusbar));
    }

    #[test]
    fn campo_interfaz_alterna_booleano() {
        let mut config = Config::default();
        assert!(config.interfaz.mostrar_statusbar);
        CampoInterfaz::MostrarStatusbar.aplicar(&mut config);
        assert!(!config.interfaz.mostrar_statusbar);
        CampoInterfaz::MostrarStatusbar.aplicar(&mut config);
        assert!(config.interfaz.mostrar_statusbar);
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
    fn entrar_funciona_en_atajos_con_las_filas_fijadas_externamente() {
        let mut panel = EstadoPanelAdmin::nueva();
        panel.fijar_num_filas_atajos(16); // "restablecer todos" + 15 comandos, por ejemplo
        panel.abrir();
        assert_eq!(panel.seccion_actual(), Seccion::Atajos);
        panel.entrar();
        assert_eq!(panel.foco(), FocoPanelAdmin::Central);
        assert_eq!(panel.num_campos(), 16);
    }

    #[test]
    fn entrar_funciona_en_lenguajes_con_las_filas_fijadas_externamente() {
        let mut panel = EstadoPanelAdmin::nueva();
        panel.fijar_num_filas_lenguajes(5); // los 5 lenguajes de M1
        panel.abrir();
        panel.mover_seccion_abajo();
        panel.mover_seccion_abajo();
        assert_eq!(panel.seccion_actual(), Seccion::Lenguajes);
        panel.entrar();
        assert_eq!(panel.foco(), FocoPanelAdmin::Central);
        assert_eq!(panel.num_campos(), 5);
    }

    #[test]
    fn capturar_atajo_se_puede_iniciar_y_cancelar() {
        let mut panel = EstadoPanelAdmin::nueva();
        panel.fijar_num_filas_atajos(2);
        panel.abrir();
        panel.entrar();
        assert!(!panel.capturando());
        panel.iniciar_captura();
        assert!(panel.capturando());
        panel.terminar_captura();
        assert!(!panel.capturando());
    }

    #[test]
    fn opciones_externas_aparecen_en_la_busqueda_global() {
        let mut panel = EstadoPanelAdmin::nueva();
        let indice_atajos = indice_de(Seccion::Atajos);
        panel.fijar_opciones_externas(vec![OpcionExterna {
            seccion: indice_atajos,
            campo: 1,
            nombre: "Archivo: Guardar",
        }]);
        panel.abrir();
        panel.abrir_busqueda();
        for c in "guardar".chars() {
            panel.escribir_busqueda(c);
        }
        let resultados = panel.resultados_busqueda();
        assert!(resultados.iter().any(|r| r.nombre == "Archivo: Guardar" && r.seccion == indice_atajos));

        panel.confirmar_busqueda();
        assert_eq!(panel.seccion_actual(), Seccion::Atajos);
        assert_eq!(panel.campo(), 1);
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
        // internas (Editor + Temas + Interfaz — las únicas secciones
        // que este crate resuelve directo, sin opciones externas).
        let total = panel.resultados_busqueda().len();
        assert_eq!(total, CampoEditor::TODOS.len() + CampoTemas::TODOS.len() + CampoInterfaz::TODOS.len());

        for _ in 0..(total + 5) {
            panel.mover_campo_abajo();
        }
        assert_eq!(panel.campo(), total - 1);

        panel.mover_campo_arriba();
        assert_eq!(panel.campo(), total - 2);
    }
}
