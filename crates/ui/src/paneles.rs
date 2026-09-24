use std::path::Path;

use ratatui::layout::{Constraint, Direction, Rect};
use ratatui::Frame;

use tcode_config::ConfigInterfaz;
use tcode_core::{delimitador_por_extension, Editor, EstadoBusqueda, EstadoCsv};
use tcode_fs::DiffGit;
use tcode_lsp::DiagnosticoSimple;
use tcode_syntax::{Lenguaje, Resaltador};

use crate::{barra_pestanas, statusbar, vista_codigo, vista_csv, vista_markdown, EstadoUi, Paleta};

/// Cómo se divide un panel (`Ctrl+\`/`Ctrl+K Ctrl+\`, PLAN.md §4): en
/// paneles lado a lado (una línea divisoria vertical entre ellos) o
/// apilados (línea divisoria horizontal). Ojo: es lo opuesto al
/// `Direction` de `ratatui` — un split "vertical" reparte el ANCHO, que
/// en `ratatui` es `Direction::Horizontal` (ver `dibujar_panel` en este
/// módulo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DireccionSplit {
    Vertical,
    Horizontal,
}

/// Vista de un panel cuyo archivo es Markdown (PLAN.md §8, `Ctrl+K V` /
/// `Ctrl+Shift+V`): solo el código fuente (comportamiento normal de
/// cualquier otro archivo), fuente + preview lado a lado, o solo el
/// preview. Se ignora por completo si el archivo activo no es
/// `.md`/`.markdown` — no hay forma de "quedar atascado" en modo preview
/// al cambiar a otro archivo, cada `PanelEditor` es dueño de su propio
/// modo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModoMarkdown {
    #[default]
    Fuente,
    Dividido,
    SoloPreview,
}

/// Vista de un panel cuyo archivo es CSV/TSV (PLAN.md §9, `Ctrl+K T`): a
/// diferencia de Markdown, el modo por defecto es `Tabla` — un CSV como
/// texto plano es justo lo que esta vista existe para evitar tener que
/// leer — y `Ctrl+K T` es el escape hatch hacia el texto plano cuando
/// hace falta (p. ej. arreglar una fila corrupta a mano).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModoCsv {
    #[default]
    Tabla,
    Fuente,
}

/// Un documento abierto en un panel — una pestaña (BACKLOG.md P3 #10):
/// su editor, la ruta que se muestra en la statusbar, su propio
/// desplazamiento vertical (cada documento se desplaza de forma
/// independiente, y lo conserva al cambiar de pestaña), los diagnósticos
/// LSP más recientes para ese archivo (M2, PLAN.md §2: "diagnósticos
/// inline") y, según el tipo de archivo, su [`ModoMarkdown`] (PLAN.md §8)
/// o su [`ModoCsv`] + [`EstadoCsv`] (selección/edición de celda, PLAN.md
/// §9). El nombre es de antes de las pestañas, cuando cada panel tenía un
/// solo documento; se mantuvo para no tocar a todos los que lo usan.
pub struct PanelEditor {
    pub editor: Editor,
    pub ruta_mostrada: String,
    pub estado_ui: EstadoUi,
    pub diagnosticos: Vec<DiagnosticoSimple>,
    pub modo_markdown: ModoMarkdown,
    pub modo_csv: ModoCsv,
    pub estado_csv: EstadoCsv,
    /// Base de `HEAD` + marcas por línea para los indicadores de git del
    /// gutter (BACKLOG.md P2 #6). Se carga sola la primera vez que se
    /// dibuja el panel con un archivo con ruta (ver `dibujar_panel`).
    pub git: DiffGit,
    /// Motivo del último intento de guardar este documento que falló
    /// (permiso denegado, carpeta borrada...), mostrado en la statusbar
    /// hasta que un guardado posterior funcione o se abra otro archivo
    /// en el panel. Lo fija `app` (`guardar_panel` en `main.rs`) tanto
    /// para `Ctrl+S` como para el guardado automático (BACKLOG.md P2 #4)
    /// — este último corre solo, sin ningún prompt donde mostrar el error.
    pub aviso_guardado: Option<String>,
    /// Aviso corto y transitorio para la barra de estado de este panel
    /// (por ahora solo lo deja el guardado con "formatear al guardar"
    /// prendido, BACKLOG.md P2 #5: "Formateado al guardar" o por qué no
    /// se formateó). `app` lo limpia con la siguiente tecla.
    pub mensaje_estado: Option<String>,
}

impl PanelEditor {
    pub fn nuevo(editor: Editor, ruta_mostrada: String) -> Self {
        let modo_csv = if es_csv(&ruta_mostrada) { ModoCsv::Tabla } else { ModoCsv::Fuente };
        Self {
            editor,
            ruta_mostrada,
            estado_ui: EstadoUi::default(),
            diagnosticos: Vec::new(),
            modo_markdown: ModoMarkdown::default(),
            modo_csv,
            estado_csv: EstadoCsv::nuevo(),
            git: DiffGit::nuevo(),
            aviso_guardado: None,
            mensaje_estado: None,
        }
    }

    fn vacio() -> Self {
        Self::nuevo(Editor::nuevo(), String::new())
    }

    fn es_markdown(&self) -> bool {
        Lenguaje::detectar_por_extension(&self.ruta_mostrada) == Some(Lenguaje::Markdown)
    }

    pub fn es_csv(&self) -> bool {
        es_csv(&self.ruta_mostrada)
    }

    /// Si es el "[Sin nombre]" vacío e intacto con el que arranca `tcode`
    /// sin argumentos (o un panel recién dividido): abrir un archivo lo
    /// reemplaza en vez de dejarlo como una pestaña más que nadie pidió
    /// (mismo criterio que VSCode con su pestaña "Untitled" sin tocar).
    fn es_descartable(&self) -> bool {
        let buffer = self.editor.buffer();
        buffer.ruta().is_none() && !buffer.modificado() && buffer.len_bytes() == 0
    }

    /// Si este documento es el archivo `ruta` — comparando rutas
    /// canónicas, porque el mismo archivo puede llegar escrito distinto
    /// (relativo desde la línea de comandos, absoluto desde el
    /// explorador o el buscador). Solo se llama al abrir un archivo, nunca
    /// por frame: `canonicalize` toca el disco.
    fn es_archivo(&self, ruta: &Path) -> bool {
        let Some(propia) = self.editor.buffer().ruta() else { return false };
        propia == ruta
            || matches!((std::fs::canonicalize(propia), std::fs::canonicalize(ruta)), (Ok(a), Ok(b)) if a == b)
    }

    /// La tabla CSV/TSV analizada a partir del contenido actual del
    /// buffer — se recalcula cada vez que hace falta (igual que
    /// `vista_markdown` re-parsea en cada frame): un archivo CSV de
    /// tamaño razonable es barato de volver a analizar, y así la vista
    /// nunca puede desincronizarse del contenido real tras una edición.
    pub fn tabla_csv(&self) -> tcode_core::TablaCsv {
        let delimitador = delimitador_por_extension(&self.ruta_mostrada);
        tcode_core::analizar_csv(&self.editor.buffer().a_texto(), delimitador).unwrap_or_default()
    }
}

fn es_csv(ruta: &str) -> bool {
    matches!(ruta.rsplit('.').next().map(|e| e.to_ascii_lowercase()), Some(e) if e == "csv" || e == "tsv")
}

/// Las pestañas de un panel (BACKLOG.md P3 #10): los documentos abiertos
/// en él, en el orden de la barra de pestañas, y cuál se ve. Nunca está
/// vacía — cerrar la última pestaña deja un "[Sin nombre]" en blanco (o
/// cierra el panel entero, si hay otros: ver
/// [`Layout::cerrar_pestana_activa`]). Cada `PanelEditor` es dueño de
/// todo su estado (cursor, scroll, historial, pliegues, diagnósticos,
/// git, vista Markdown/CSV), así que cambiar de pestaña es solo cambiar
/// `activa`: no se copia ni se recalcula nada del documento.
struct Pestanas {
    documentos: Vec<PanelEditor>,
    activa: usize,
}

impl Pestanas {
    fn con(documento: PanelEditor) -> Self {
        Self { documentos: vec![documento], activa: 0 }
    }

    fn activo(&self) -> &PanelEditor {
        &self.documentos[self.activa]
    }

    fn activo_mut(&mut self) -> &mut PanelEditor {
        &mut self.documentos[self.activa]
    }

    /// Agrega `documento` como pestaña nueva, justo a la derecha de la
    /// activa (como VSCode: lo recién abierto queda al lado de lo que se
    /// estaba mirando), y la activa — salvo que la activa sea un "[Sin
    /// nombre]" descartable, que se reemplaza.
    fn abrir(&mut self, documento: PanelEditor) {
        if self.activo().es_descartable() {
            self.documentos[self.activa] = documento;
        } else {
            self.activa += 1;
            self.documentos.insert(self.activa, documento);
        }
    }

    /// Activa la pestaña del archivo `ruta`, si ya está abierto en este
    /// panel. Devuelve si la encontró.
    fn activar_ruta(&mut self, ruta: &Path) -> bool {
        match self.documentos.iter().position(|d| d.es_archivo(ruta)) {
            Some(indice) => {
                self.activa = indice;
                true
            }
            None => false,
        }
    }

    /// Cierra la pestaña activa y activa la de su derecha (o la de su
    /// izquierda, si era la última). Con una sola pestaña no hace nada:
    /// eso lo decide `Layout::cerrar_pestana_activa`.
    fn cerrar_activa(&mut self) {
        if self.documentos.len() <= 1 {
            return;
        }
        self.documentos.remove(self.activa);
        self.activa = self.activa.min(self.documentos.len() - 1);
    }
}

/// Árbol de paneles: una hoja con sus pestañas, o una división en dos
/// sub-árboles. Privado — quien usa `tcode-ui` solo interactúa con
/// [`Layout`], nunca navega el árbol directamente. Antes de las pestañas
/// la hoja era un `Box<PanelEditor>` (un `Editor` completo es mucho más
/// grande que `Division`, `clippy::large_enum_variant`); ahora los
/// documentos viven en el `Vec` de [`Pestanas`], que ya está en el heap,
/// así que la hoja quedó chica sin necesitar el `Box`.
enum Panel {
    Hoja(Pestanas),
    Division { direccion: DireccionSplit, primero: Box<Panel>, segundo: Box<Panel> },
}

impl Panel {
    fn vacio() -> Self {
        Panel::Hoja(Pestanas::con(PanelEditor::vacio()))
    }

    fn contar_hojas(&self) -> usize {
        match self {
            Panel::Hoja(_) => 1,
            Panel::Division { primero, segundo, .. } => primero.contar_hojas() + segundo.contar_hojas(),
        }
    }
}

/// `tcode` puede tener varios paneles de edición abiertos a la vez
/// (`Ctrl+\`/`Ctrl+K Ctrl+\`, PLAN.md §4): este es el árbol de paneles
/// más cuál de ellos tiene el foco. Los índices de panel (para
/// `Ctrl+1`/`Ctrl+2`/`Ctrl+3`) son su posición en el recorrido en
/// profundidad del árbol, de izquierda/arriba a derecha/abajo.
pub struct Layout {
    raiz: Panel,
    activo: usize,
    /// "Pantalla completa" (`F11`/`Ctrl+K G`, BACKLOG.md P3 #11): el panel
    /// activo ocupa toda el área de edición, como el zoom de tmux
    /// (`prefix z`). Solo cambia cómo se dibuja — el árbol de splits queda
    /// intacto detrás, así que al salir vuelve exactamente igual. Cualquier
    /// cosa que cambie la estructura o el panel activo (dividir, cerrar,
    /// `Ctrl+1/2/3`) sale primero del maximizado, igual que tmux.
    maximizado: bool,
}

impl Layout {
    pub fn nuevo(editor: Editor, ruta_mostrada: String) -> Self {
        Self {
            raiz: Panel::Hoja(Pestanas::con(PanelEditor::nuevo(editor, ruta_mostrada))),
            activo: 0,
            maximizado: false,
        }
    }

    /// Si el panel activo está maximizado (ver campo `maximizado`).
    pub fn maximizado(&self) -> bool {
        self.maximizado
    }

    /// `F11`/`Ctrl+K G`: maximiza el panel activo o lo restaura. Con un
    /// solo panel no hace nada — ya ocupa toda el área, y un `[MAX]`
    /// prendido sin nada que restaurar solo confundiría.
    pub fn alternar_maximizado(&mut self) {
        self.maximizado = !self.maximizado && self.num_paneles() > 1;
    }

    pub fn num_paneles(&self) -> usize {
        self.raiz.contar_hojas()
    }

    pub fn indice_activo(&self) -> usize {
        self.activo
    }

    fn hojas(&self) -> Vec<&Pestanas> {
        fn recorrer<'a>(panel: &'a Panel, salida: &mut Vec<&'a Pestanas>) {
            match panel {
                Panel::Hoja(p) => salida.push(p),
                Panel::Division { primero, segundo, .. } => {
                    recorrer(primero, salida);
                    recorrer(segundo, salida);
                }
            }
        }
        let mut salida = Vec::new();
        recorrer(&self.raiz, &mut salida);
        salida
    }

    /// Todos los documentos abiertos: cada pestaña de cada panel, en el
    /// orden de los paneles de `ir_a_panel` y, dentro de cada uno, en el
    /// de su barra de pestañas — para lo que tiene que recorrerlos a
    /// todos, no solo el visible (el guardado automático, BACKLOG.md P2
    /// #4: una pestaña que se dejó de mirar también tiene que guardarse).
    pub fn paneles_mut(&mut self) -> Vec<&mut PanelEditor> {
        self.hojas_mut().into_iter().flat_map(|p| p.documentos.iter_mut()).collect()
    }

    fn hojas_mut(&mut self) -> Vec<&mut Pestanas> {
        fn recorrer<'a>(panel: &'a mut Panel, salida: &mut Vec<&'a mut Pestanas>) {
            match panel {
                Panel::Hoja(p) => salida.push(p),
                Panel::Division { primero, segundo, .. } => {
                    recorrer(primero, salida);
                    recorrer(segundo, salida);
                }
            }
        }
        let mut salida = Vec::new();
        recorrer(&mut self.raiz, &mut salida);
        salida
    }

    fn pestanas_activas(&self) -> &Pestanas {
        self.hojas()[self.activo]
    }

    fn pestanas_activas_mut(&mut self) -> &mut Pestanas {
        let activo = self.activo;
        self.hojas_mut().remove(activo)
    }

    /// El documento visible del panel activo (su pestaña activa).
    pub fn panel_activo(&self) -> &PanelEditor {
        self.pestanas_activas().activo()
    }

    pub fn panel_activo_mut(&mut self) -> &mut PanelEditor {
        self.pestanas_activas_mut().activo_mut()
    }

    pub fn editor_activo(&self) -> &Editor {
        &self.panel_activo().editor
    }

    pub fn editor_activo_mut(&mut self) -> &mut Editor {
        &mut self.panel_activo_mut().editor
    }

    /// Abre un documento en el panel activo (explorador, buscador de
    /// archivos) como pestaña nueva y la activa (BACKLOG.md P3 #10). Si
    /// ese archivo ya tenía pestaña en este panel, solo la activa y
    /// `editor` se descarta — mejor todavía es preguntar antes con
    /// [`Layout::activar_pestana_de`], para no leer el archivo de disco en
    /// vano. Un "[Sin nombre]" vacío e intacto se reemplaza en vez de
    /// quedar como pestaña de más.
    pub fn abrir_en_activo(&mut self, editor: Editor, ruta_mostrada: String) {
        let pestanas = self.pestanas_activas_mut();
        if let Some(ruta) = editor.buffer().ruta() {
            if pestanas.activar_ruta(ruta) {
                return;
            }
        }
        pestanas.abrir(PanelEditor::nuevo(editor, ruta_mostrada));
    }

    /// Si el archivo `ruta` ya está abierto en alguna pestaña del panel
    /// activo, la activa y devuelve `true`. Solo mira el panel activo:
    /// con splits, cada panel tiene sus propias pestañas (igual que los
    /// grupos de VSCode), y abrir en uno un archivo que está en otro le
    /// agrega una pestaña propia.
    pub fn activar_pestana_de(&mut self, ruta: &Path) -> bool {
        self.pestanas_activas_mut().activar_ruta(ruta)
    }

    /// Cantidad de pestañas del panel activo.
    pub fn num_pestanas(&self) -> usize {
        self.pestanas_activas().documentos.len()
    }

    /// Índice (0-based) de la pestaña activa del panel activo.
    pub fn indice_pestana_activa(&self) -> usize {
        self.pestanas_activas().activa
    }

    /// `Ctrl+PageDown`: la pestaña de la derecha, dando la vuelta al
    /// llegar a la última (como VSCode y los navegadores).
    pub fn siguiente_pestana(&mut self) {
        let pestanas = self.pestanas_activas_mut();
        pestanas.activa = (pestanas.activa + 1) % pestanas.documentos.len();
    }

    /// `Ctrl+PageUp`: la pestaña de la izquierda, dando la vuelta.
    pub fn anterior_pestana(&mut self) {
        let pestanas = self.pestanas_activas_mut();
        let total = pestanas.documentos.len();
        pestanas.activa = (pestanas.activa + total - 1) % total;
    }

    /// `Alt+1`..`Alt+9`: va a la pestaña `indice` (0-based) del panel
    /// activo; no hace nada si está fuera de rango.
    pub fn ir_a_pestana(&mut self, indice: usize) {
        let pestanas = self.pestanas_activas_mut();
        if indice < pestanas.documentos.len() {
            pestanas.activa = indice;
        }
    }

    /// `Ctrl+W`: cierra la pestaña activa, descartando sus cambios — la
    /// confirmación si hay cambios sin guardar la pide `app` antes de
    /// llamar esto. Si era la última del panel: con otros paneles
    /// abiertos se cierra el panel entero (como un grupo vacío de
    /// VSCode, un panel sin nada adentro no sirve para nada); si es el
    /// único panel, queda un "[Sin nombre]" en blanco — `tcode` siempre
    /// tiene algo abierto.
    pub fn cerrar_pestana_activa(&mut self) {
        if self.num_pestanas() > 1 {
            self.pestanas_activas_mut().cerrar_activa();
        } else if self.num_paneles() > 1 {
            self.cerrar_activo();
        } else {
            *self.pestanas_activas_mut() = Pestanas::con(PanelEditor::vacio());
        }
    }

    /// Cuántos documentos (todas las pestañas de todos los paneles)
    /// tienen cambios sin guardar — para que `Ctrl+Q` avise aunque lo
    /// modificado no sea lo que se está viendo.
    pub fn documentos_modificados(&self) -> usize {
        self.hojas().iter().flat_map(|p| p.documentos.iter()).filter(|d| d.editor.buffer().modificado()).count()
    }

    /// Si alguna pestaña del panel activo tiene cambios sin guardar (lo
    /// que se perdería al cerrarlo con `Ctrl+K F`).
    pub fn panel_activo_modificado(&self) -> bool {
        self.pestanas_activas().documentos.iter().any(|d| d.editor.buffer().modificado())
    }

    /// Vuelve a leer de `HEAD` la base de los indicadores de git de todos
    /// los documentos (BACKLOG.md P2 #6) — la app lo llama al guardar: es
    /// el momento natural en que un commit hecho desde otra terminal se
    /// vuelve visible. Todos y no solo el activo porque el mismo archivo
    /// puede estar abierto en más de un panel, y una pestaña que no se
    /// está viendo también tiene que tener la base al día cuando se
    /// vuelva a ella; cada uno es un `git cat-file` en segundo plano.
    pub fn refrescar_bases_git(&mut self) {
        for documento in self.paneles_mut() {
            documento.git.refrescar_base();
        }
    }

    /// Si algún documento está esperando que `git` devuelva su base o que
    /// termine de calcularse su diff: mientras tanto la app vuelve a
    /// dibujar cada tanto aunque no lleguen teclas, para que las marcas
    /// aparezcan solas al terminar.
    pub fn cargas_git_pendientes(&self) -> bool {
        self.hojas().iter().flat_map(|p| p.documentos.iter()).any(|p| p.git.pendiente())
    }

    /// `Ctrl+K T`: alterna el panel activo entre la vista de tabla y el
    /// texto plano (PLAN.md §9, "toggle a modo raw"). No hace nada si el
    /// archivo activo no es CSV/TSV.
    pub fn alternar_vista_tabla_csv(&mut self) {
        let panel = self.panel_activo_mut();
        if !panel.es_csv() {
            return;
        }
        panel.modo_csv = match panel.modo_csv {
            ModoCsv::Tabla => ModoCsv::Fuente,
            ModoCsv::Fuente => ModoCsv::Tabla,
        };
    }

    /// `Ctrl+K V`: alterna el panel activo entre solo-fuente y
    /// fuente+preview lado a lado (PLAN.md §8). No hace nada si el
    /// archivo activo no es Markdown.
    pub fn alternar_preview_markdown(&mut self) {
        let panel = self.panel_activo_mut();
        if !panel.es_markdown() {
            return;
        }
        panel.modo_markdown = match panel.modo_markdown {
            ModoMarkdown::Dividido => ModoMarkdown::Fuente,
            ModoMarkdown::Fuente | ModoMarkdown::SoloPreview => ModoMarkdown::Dividido,
        };
    }

    /// `Ctrl+Shift+V`: alterna el panel activo entre solo-preview y
    /// solo-fuente (PLAN.md §8). No hace nada si el archivo activo no es
    /// Markdown.
    pub fn alternar_preview_solo_markdown(&mut self) {
        let panel = self.panel_activo_mut();
        if !panel.es_markdown() {
            return;
        }
        panel.modo_markdown = match panel.modo_markdown {
            ModoMarkdown::SoloPreview => ModoMarkdown::Fuente,
            ModoMarkdown::Fuente | ModoMarkdown::Dividido => ModoMarkdown::SoloPreview,
        };
    }

    /// Reemplaza los diagnósticos LSP del panel activo (llega una
    /// notificación `textDocument/publishDiagnostics` nueva).
    pub fn establecer_diagnosticos_activo(&mut self, diagnosticos: Vec<DiagnosticoSimple>) {
        self.panel_activo_mut().diagnosticos = diagnosticos;
    }

    /// Divide el panel activo en dos: el documento actual se queda en el
    /// primer sub-panel, un buffer nuevo en blanco en el segundo, que
    /// pasa a ser el panel activo (igual que VSCode).
    pub fn dividir(&mut self, direccion: DireccionSplit) {
        self.maximizado = false;
        let raiz = std::mem::replace(&mut self.raiz, Panel::vacio());
        self.raiz = dividir_en_indice(raiz, self.activo, direccion);
        self.activo += 1;
    }

    /// Cierra el panel activo. No hace nada si es el único que queda —
    /// siempre debe sobrevivir al menos uno.
    pub fn cerrar_activo(&mut self) {
        if self.num_paneles() <= 1 {
            return;
        }
        self.maximizado = false;
        let raiz = std::mem::replace(&mut self.raiz, Panel::vacio());
        let (nueva_raiz, _) = cerrar_en_indice(raiz, self.activo);
        self.raiz = nueva_raiz;
        self.activo = self.activo.min(self.num_paneles().saturating_sub(1));
    }

    /// Va al panel `indice` (0-based, `Ctrl+1`/`Ctrl+2`/`Ctrl+3`); no hace
    /// nada si está fuera de rango.
    pub fn ir_a_panel(&mut self, indice: usize) {
        if indice < self.num_paneles() {
            self.maximizado = false;
            self.activo = indice;
        }
    }

    /// Dibuja el árbol de paneles completo dentro de `area`, recursivo:
    /// cada división reparte el espacio 50/50 entre sus dos sub-árboles.
    /// Solo el panel activo recibe el cursor real de la terminal.
    /// Maximizado, se dibuja solo la hoja activa en toda `area`, con
    /// `[MAX]` al final de su barra de estado (si la barra se ve).
    #[allow(clippy::too_many_arguments)]
    pub fn dibujar(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        paleta: &Paleta,
        resaltador: &mut Resaltador,
        estado_busqueda: &EstadoBusqueda,
        mostrar_numeros: bool,
        ajuste_linea: bool,
        columna_regla: Option<usize>,
        indicadores_git: bool,
        interfaz: &ConfigInterfaz,
    ) {
        // Maximizado: la hoja activa se dibuja sola, como si fuera la raíz
        // (índice 0 de un árbol de un solo panel) — el resto del árbol ni
        // se recorre.
        let (raiz, activo) = if self.maximizado {
            (hoja_en_indice(&mut self.raiz, self.activo), 0)
        } else {
            (&mut self.raiz, self.activo)
        };
        let mut indice_actual = 0;
        dibujar_panel(
            frame,
            area,
            raiz,
            activo,
            &mut indice_actual,
            paleta,
            resaltador,
            estado_busqueda,
            mostrar_numeros,
            ajuste_linea,
            columna_regla,
            indicadores_git,
            interfaz,
        );
        if self.maximizado && interfaz.mostrar_statusbar && area.height > 0 {
            // Pegado a la derecha de la última fila (la de la statusbar),
            // encima de su relleno: así no hace falta tocar
            // `statusbar::dibujar` para un indicador que solo existe acá.
            let indicador = " [MAX] ";
            let ancho = (indicador.len() as u16).min(area.width);
            let fila = Rect { x: area.x + area.width - ancho, y: area.y + area.height - 1, width: ancho, height: 1 };
            frame.render_widget(
                ratatui::widgets::Paragraph::new(indicador)
                    .style(ratatui::style::Style::default().bg(paleta.statusbar_fondo).fg(paleta.statusbar_texto)),
                fila,
            );
        }
    }
}

/// La hoja (nodo `Panel::Hoja`, no su `PanelEditor`) en la posición
/// `indice` del recorrido en profundidad — para dibujarla sola al
/// maximizar. Fuera de rango devuelve la última (no pasa: `activo`
/// siempre es válido).
fn hoja_en_indice(panel: &mut Panel, indice: usize) -> &mut Panel {
    match panel {
        Panel::Hoja(_) => panel,
        Panel::Division { primero, segundo, .. } => {
            let n = primero.contar_hojas();
            if indice < n {
                hoja_en_indice(primero, indice)
            } else {
                hoja_en_indice(segundo, indice - n)
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn dibujar_panel(
    frame: &mut Frame,
    area: Rect,
    panel: &mut Panel,
    activo: usize,
    indice_actual: &mut usize,
    paleta: &Paleta,
    resaltador: &mut Resaltador,
    estado_busqueda: &EstadoBusqueda,
    mostrar_numeros: bool,
    ajuste_linea: bool,
    columna_regla: Option<usize>,
    indicadores_git: bool,
    interfaz: &ConfigInterfaz,
) {
    match panel {
        Panel::Hoja(pestanas) => {
            let es_activo = *indice_actual == activo;
            *indice_actual += 1;

            // Barra de pestañas (BACKLOG.md P3 #10): una fila arriba del
            // código, también con una sola pestaña — es además donde se
            // ve qué archivo tiene cada panel sin mirar la statusbar.
            // "Mostrar pestañas" apagado la saca y devuelve la fila.
            let area = if interfaz.mostrar_pestanas && area.height > 1 {
                let (barra, resto) = (Rect { height: 1, ..area }, Rect { y: area.y + 1, height: area.height - 1, ..area });
                barra_pestanas::dibujar(frame, barra, &pestanas.documentos, pestanas.activa, es_activo, paleta);
                resto
            } else {
                area
            };
            let panel_editor = pestanas.activo_mut();

            // "Mostrar barra de estado" (PLAN.md §5.5, M4): si está
            // apagado, el panel de código/tabla usa el área completa —
            // no se reserva ninguna fila para la statusbar ni se la
            // dibuja.
            let (area_contenido, area_statusbar) = if interfaz.mostrar_statusbar {
                let partes = ratatui::layout::Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(1), Constraint::Length(1)])
                    .split(area);
                (partes[0], Some(partes[1]))
            } else {
                (area, None)
            };
            let partes = [area_contenido];

            // La búsqueda opera solo sobre el buffer del panel activo: los
            // demás paneles no reciben coincidencias que resaltar.
            let (coincidencias, indice_coincidencia): (&[_], Option<usize>) = if es_activo {
                (estado_busqueda.coincidencias(), estado_busqueda.indice_actual())
            } else {
                (&[], None)
            };

            // Mientras la barra de búsqueda está abierta, el cursor real
            // de la terminal se posiciona en ella (ver
            // `panel_busqueda::dibujar`), no en el código.
            let mostrar_cursor = es_activo && !estado_busqueda.activa();

            if panel_editor.es_csv() && panel_editor.modo_csv == ModoCsv::Tabla {
                let tabla = panel_editor.tabla_csv();
                // Un `Ctrl+Z` (global, no pasa por la vista) o un filtro
                // que dejó menos filas pueden dejar la selección fuera de
                // la tabla: se recorta acá, justo antes de dibujar, igual
                // para cualquier cosa que haya cambiado el buffer.
                let num_visibles = panel_editor.estado_csv.filas_visibles(&tabla).len();
                panel_editor.estado_csv.recortar(num_visibles, tabla.num_columnas());
                vista_csv::dibujar(
                    frame,
                    partes[0],
                    &tabla,
                    &panel_editor.estado_csv,
                    &mut panel_editor.estado_ui,
                    paleta,
                    mostrar_cursor,
                );
                if let Some(area_statusbar) = area_statusbar {
                    statusbar::dibujar(
                        frame,
                        area_statusbar,
                        &panel_editor.editor,
                        &panel_editor.ruta_mostrada,
                        panel_editor.aviso_guardado.as_deref(),
                        paleta,
                        &panel_editor.diagnosticos,
                        interfaz,
                        panel_editor.mensaje_estado.as_deref(),
                    );
                }
                return;
            }

            let area_markdown = if panel_editor.es_markdown() { panel_editor.modo_markdown } else { ModoMarkdown::Fuente };

            // Indicadores de git (BACKLOG.md P2 #6): se ponen al día acá,
            // al dibujar, y solo para los paneles que muestran código —
            // `DiffGit::actualizar` no copia nada si el texto no cambió
            // desde el frame anterior. Sin base (archivo fuera de un repo,
            // sin trackear, o todavía cargando) no se le reserva columna.
            let marcas_git = if indicadores_git && area_markdown != ModoMarkdown::SoloPreview {
                let buffer = panel_editor.editor.buffer();
                panel_editor.git.actualizar(buffer.ruta(), buffer.rope().chunks());
                panel_editor.git.tiene_base().then(|| panel_editor.git.marcas())
            } else {
                None
            };

            match area_markdown {
                ModoMarkdown::Fuente => {
                    vista_codigo::dibujar(
                        frame,
                        partes[0],
                        &panel_editor.editor,
                        &mut panel_editor.estado_ui,
                        paleta,
                        resaltador,
                        &panel_editor.ruta_mostrada,
                        mostrar_cursor,
                        &panel_editor.diagnosticos,
                        coincidencias,
                        indice_coincidencia,
                        mostrar_numeros,
                        ajuste_linea,
                        columna_regla,
                        marcas_git,
                    );
                }
                ModoMarkdown::Dividido => {
                    let columnas = ratatui::layout::Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                        .split(partes[0]);
                    vista_codigo::dibujar(
                        frame,
                        columnas[0],
                        &panel_editor.editor,
                        &mut panel_editor.estado_ui,
                        paleta,
                        resaltador,
                        &panel_editor.ruta_mostrada,
                        mostrar_cursor,
                        &panel_editor.diagnosticos,
                        coincidencias,
                        indice_coincidencia,
                        mostrar_numeros,
                        // `false` fijo, no `ajuste_linea`: `estado_ui.
                        // scroll` se comparte con
                        // `vista_markdown::dibujar` de acá abajo, que
                        // asume una fila de scroll por línea lógica (no
                        // sabe de filas visuales) — activar el ajuste acá
                        // desincronizaría las dos mitades del split en
                        // cuanto una línea larga se partiera en más de
                        // una fila. El ajuste de línea sigue andando
                        // normal en "solo fuente" (arriba), donde no hay
                        // ninguna otra vista compartiendo el scroll.
                        false,
                        // La regla vertical sí puede seguir andando acá:
                        // es puramente cosmética por fila visual, no
                        // afecta al cálculo de scroll que comparten las
                        // dos mitades (a diferencia de `ajuste_linea`
                        // arriba).
                        columna_regla,
                        marcas_git,
                    );
                    vista_markdown::dibujar(
                        frame,
                        columnas[1],
                        &panel_editor.editor.buffer().a_texto(),
                        panel_editor.estado_ui.scroll,
                        panel_editor.editor.buffer().num_lineas(),
                        paleta,
                        resaltador,
                    );
                }
                ModoMarkdown::SoloPreview => {
                    vista_markdown::dibujar(
                        frame,
                        partes[0],
                        &panel_editor.editor.buffer().a_texto(),
                        panel_editor.estado_ui.scroll,
                        panel_editor.editor.buffer().num_lineas(),
                        paleta,
                        resaltador,
                    );
                }
            }
            if let Some(area_statusbar) = area_statusbar {
                statusbar::dibujar(
                    frame,
                    area_statusbar,
                    &panel_editor.editor,
                    &panel_editor.ruta_mostrada,
                    panel_editor.aviso_guardado.as_deref(),
                    paleta,
                    &panel_editor.diagnosticos,
                    interfaz,
                    panel_editor.mensaje_estado.as_deref(),
                );
            }
        }
        Panel::Division { direccion, primero, segundo } => {
            // Ojo: un split "vertical" (PLAN.md §4) reparte el ANCHO —
            // paneles lado a lado — que en `ratatui` es
            // `Direction::Horizontal`, y viceversa.
            let direccion_ratatui = match direccion {
                DireccionSplit::Vertical => Direction::Horizontal,
                DireccionSplit::Horizontal => Direction::Vertical,
            };
            let partes = ratatui::layout::Layout::default()
                .direction(direccion_ratatui)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            dibujar_panel(
                frame, partes[0], primero, activo, indice_actual, paleta, resaltador, estado_busqueda, mostrar_numeros,
                ajuste_linea, columna_regla, indicadores_git, interfaz,
            );
            dibujar_panel(
                frame, partes[1], segundo, activo, indice_actual, paleta, resaltador, estado_busqueda, mostrar_numeros,
                ajuste_linea, columna_regla, indicadores_git, interfaz,
            );
        }
    }
}

/// Divide (por valor, para no pelear con el borrow checker al reconstruir
/// nodos del árbol) la hoja en la posición `indice`.
fn dividir_en_indice(panel: Panel, indice: usize, direccion: DireccionSplit) -> Panel {
    match panel {
        Panel::Hoja(original) if indice == 0 => Panel::Division {
            direccion,
            primero: Box::new(Panel::Hoja(original)),
            segundo: Box::new(Panel::vacio()),
        },
        Panel::Hoja(_) => panel,
        Panel::Division { direccion: d, primero, segundo } => {
            let n = primero.contar_hojas();
            if indice < n {
                Panel::Division {
                    direccion: d,
                    primero: Box::new(dividir_en_indice(*primero, indice, direccion)),
                    segundo,
                }
            } else {
                Panel::Division {
                    direccion: d,
                    primero,
                    segundo: Box::new(dividir_en_indice(*segundo, indice - n, direccion)),
                }
            }
        }
    }
}

/// Cierra (por valor) la hoja en la posición `indice`. Devuelve el árbol
/// resultante y si el nodo actual entero debía colapsarse en su hermano
/// (para que el padre lo haga con `*segundo`/`*primero` directamente).
fn cerrar_en_indice(panel: Panel, indice: usize) -> (Panel, bool) {
    match panel {
        Panel::Hoja(_) => (panel, indice == 0),
        Panel::Division { direccion, primero, segundo } => {
            let n = primero.contar_hojas();
            if indice < n {
                let (nuevo_primero, colapsar) = cerrar_en_indice(*primero, indice);
                if colapsar {
                    (*segundo, false)
                } else {
                    (Panel::Division { direccion, primero: Box::new(nuevo_primero), segundo }, false)
                }
            } else {
                let (nuevo_segundo, colapsar) = cerrar_en_indice(*segundo, indice - n);
                if colapsar {
                    (*primero, false)
                } else {
                    (Panel::Division { direccion, primero, segundo: Box::new(nuevo_segundo) }, false)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout_de_prueba() -> Layout {
        Layout::nuevo(Editor::nuevo(), "a.txt".to_string())
    }

    #[test]
    fn un_archivo_csv_arranca_en_modo_tabla_y_uno_normal_no() {
        let csv = Layout::nuevo(Editor::nuevo(), "datos.csv".to_string());
        assert_eq!(csv.panel_activo().modo_csv, ModoCsv::Tabla);

        let normal = layout_de_prueba();
        assert_eq!(normal.panel_activo().modo_csv, ModoCsv::Fuente);
    }

    #[test]
    fn alternar_vista_tabla_csv_no_hace_nada_en_un_archivo_no_csv() {
        let mut layout = layout_de_prueba();
        layout.alternar_vista_tabla_csv();
        assert_eq!(layout.panel_activo().modo_csv, ModoCsv::Fuente);
    }

    #[test]
    fn alternar_vista_tabla_csv_pasa_a_fuente_y_de_vuelta_a_tabla() {
        let mut layout = Layout::nuevo(Editor::nuevo(), "datos.csv".to_string());
        assert_eq!(layout.panel_activo().modo_csv, ModoCsv::Tabla);

        layout.alternar_vista_tabla_csv();
        assert_eq!(layout.panel_activo().modo_csv, ModoCsv::Fuente);

        layout.alternar_vista_tabla_csv();
        assert_eq!(layout.panel_activo().modo_csv, ModoCsv::Tabla);
    }

    #[test]
    fn abrir_en_activo_reinicia_el_modo_y_la_seleccion_csv() {
        let mut layout = Layout::nuevo(Editor::nuevo(), "datos.csv".to_string());
        layout.alternar_vista_tabla_csv();
        layout.panel_activo_mut().estado_csv.mover_abajo(5);
        assert_eq!(layout.panel_activo().modo_csv, ModoCsv::Fuente);

        layout.abrir_en_activo(Editor::nuevo(), "otro.csv".to_string());
        assert_eq!(layout.panel_activo().modo_csv, ModoCsv::Tabla);
        assert_eq!(layout.panel_activo().estado_csv.fila(), 0);
    }

    #[test]
    fn alternar_preview_markdown_no_hace_nada_en_un_archivo_no_markdown() {
        let mut layout = layout_de_prueba();
        layout.alternar_preview_markdown();
        assert_eq!(layout.panel_activo().modo_markdown, ModoMarkdown::Fuente);
    }

    #[test]
    fn alternar_preview_markdown_pasa_a_dividido_y_de_vuelta_a_fuente() {
        let mut layout = Layout::nuevo(Editor::nuevo(), "notas.md".to_string());
        assert_eq!(layout.panel_activo().modo_markdown, ModoMarkdown::Fuente);

        layout.alternar_preview_markdown();
        assert_eq!(layout.panel_activo().modo_markdown, ModoMarkdown::Dividido);

        layout.alternar_preview_markdown();
        assert_eq!(layout.panel_activo().modo_markdown, ModoMarkdown::Fuente);
    }

    #[test]
    fn alternar_preview_solo_markdown_pasa_a_solo_preview_y_de_vuelta_a_fuente() {
        let mut layout = Layout::nuevo(Editor::nuevo(), "notas.md".to_string());

        layout.alternar_preview_solo_markdown();
        assert_eq!(layout.panel_activo().modo_markdown, ModoMarkdown::SoloPreview);

        layout.alternar_preview_solo_markdown();
        assert_eq!(layout.panel_activo().modo_markdown, ModoMarkdown::Fuente);
    }

    #[test]
    fn alternar_preview_desde_solo_preview_deja_el_panel_dividido() {
        let mut layout = Layout::nuevo(Editor::nuevo(), "notas.md".to_string());
        layout.alternar_preview_solo_markdown();
        assert_eq!(layout.panel_activo().modo_markdown, ModoMarkdown::SoloPreview);

        layout.alternar_preview_markdown();
        assert_eq!(layout.panel_activo().modo_markdown, ModoMarkdown::Dividido);
    }

    #[test]
    fn abrir_en_activo_reinicia_el_modo_markdown() {
        let mut layout = Layout::nuevo(Editor::nuevo(), "notas.md".to_string());
        layout.alternar_preview_markdown();
        assert_eq!(layout.panel_activo().modo_markdown, ModoMarkdown::Dividido);

        layout.abrir_en_activo(Editor::nuevo(), "otro.md".to_string());
        assert_eq!(layout.panel_activo().modo_markdown, ModoMarkdown::Fuente);
    }

    #[test]
    fn arranca_con_un_solo_panel_activo() {
        let layout = layout_de_prueba();
        assert_eq!(layout.num_paneles(), 1);
        assert_eq!(layout.indice_activo(), 0);
        assert_eq!(layout.panel_activo().ruta_mostrada, "a.txt");
    }

    #[test]
    fn dividir_crea_un_segundo_panel_y_lo_activa() {
        let mut layout = layout_de_prueba();
        layout.dividir(DireccionSplit::Vertical);
        assert_eq!(layout.num_paneles(), 2);
        assert_eq!(layout.indice_activo(), 1);
        // El panel nuevo (activo) es un buffer en blanco; el original
        // sigue existiendo en la otra mitad.
        assert_eq!(layout.panel_activo().ruta_mostrada, "");
        layout.ir_a_panel(0);
        assert_eq!(layout.panel_activo().ruta_mostrada, "a.txt");
    }

    #[test]
    fn dividir_dos_veces_y_navegar_a_cada_panel() {
        let mut layout = layout_de_prueba();
        layout.dividir(DireccionSplit::Vertical); // paneles: [a.txt, ""], activo=1
        layout.abrir_en_activo(Editor::nuevo(), "b.txt".to_string());
        layout.dividir(DireccionSplit::Horizontal); // paneles: [a.txt, b.txt, ""], activo=2
        layout.abrir_en_activo(Editor::nuevo(), "c.txt".to_string());

        assert_eq!(layout.num_paneles(), 3);
        let rutas: Vec<String> = (0..3)
            .map(|i| {
                layout.ir_a_panel(i);
                layout.panel_activo().ruta_mostrada.clone()
            })
            .collect();
        assert_eq!(rutas, vec!["a.txt", "b.txt", "c.txt"]);
    }

    #[test]
    fn cerrar_activo_colapsa_al_hermano() {
        let mut layout = layout_de_prueba();
        layout.dividir(DireccionSplit::Vertical);
        layout.abrir_en_activo(Editor::nuevo(), "b.txt".to_string());
        assert_eq!(layout.num_paneles(), 2);

        layout.cerrar_activo(); // cierra "b.txt"
        assert_eq!(layout.num_paneles(), 1);
        assert_eq!(layout.panel_activo().ruta_mostrada, "a.txt");
    }

    #[test]
    fn cerrar_el_ultimo_panel_no_hace_nada() {
        let mut layout = layout_de_prueba();
        layout.cerrar_activo();
        assert_eq!(layout.num_paneles(), 1);
        assert_eq!(layout.panel_activo().ruta_mostrada, "a.txt");
    }

    #[test]
    fn ir_a_panel_fuera_de_rango_no_hace_nada() {
        let mut layout = layout_de_prueba();
        layout.ir_a_panel(5);
        assert_eq!(layout.indice_activo(), 0);
    }

    #[test]
    fn cerrar_con_tres_paneles_deja_los_otros_dos_intactos() {
        let mut layout = layout_de_prueba();
        layout.dividir(DireccionSplit::Vertical);
        layout.abrir_en_activo(Editor::nuevo(), "b.txt".to_string());
        layout.dividir(DireccionSplit::Vertical);
        layout.abrir_en_activo(Editor::nuevo(), "c.txt".to_string());
        // activo = panel de "c.txt" (índice 2)

        layout.cerrar_activo();
        assert_eq!(layout.num_paneles(), 2);
        let rutas: Vec<String> = (0..2)
            .map(|i| {
                layout.ir_a_panel(i);
                layout.panel_activo().ruta_mostrada.clone()
            })
            .collect();
        assert_eq!(rutas, vec!["a.txt", "b.txt"]);
    }

    fn layout_de_tres_paneles() -> Layout {
        let mut layout = layout_de_prueba();
        layout.dividir(DireccionSplit::Vertical);
        layout.abrir_en_activo(Editor::nuevo(), "b.txt".to_string());
        layout.dividir(DireccionSplit::Horizontal);
        layout.abrir_en_activo(Editor::nuevo(), "c.txt".to_string());
        layout
    }

    #[test]
    fn maximizar_con_un_solo_panel_no_hace_nada() {
        let mut layout = layout_de_prueba();
        layout.alternar_maximizado();
        assert!(!layout.maximizado());
    }

    #[test]
    fn maximizar_y_restaurar_deja_el_layout_intacto() {
        let mut layout = layout_de_tres_paneles();
        layout.ir_a_panel(1);
        layout.alternar_maximizado();
        assert!(layout.maximizado());
        assert_eq!(layout.num_paneles(), 3);
        assert_eq!(layout.panel_activo().ruta_mostrada, "b.txt");

        layout.alternar_maximizado();
        assert!(!layout.maximizado());
        assert_eq!(layout.num_paneles(), 3);
        assert_eq!(layout.indice_activo(), 1);
    }

    #[test]
    fn cambiar_de_panel_dividir_o_cerrar_sale_del_maximizado() {
        let mut layout = layout_de_tres_paneles();
        layout.alternar_maximizado();
        layout.ir_a_panel(0);
        assert!(!layout.maximizado());
        assert_eq!(layout.indice_activo(), 0);

        layout.alternar_maximizado();
        layout.dividir(DireccionSplit::Vertical);
        assert!(!layout.maximizado());
        assert_eq!(layout.num_paneles(), 4);

        layout.alternar_maximizado();
        layout.cerrar_activo();
        assert!(!layout.maximizado());
        assert_eq!(layout.num_paneles(), 3);
    }

    #[test]
    fn ir_a_un_panel_fuera_de_rango_no_sale_del_maximizado() {
        let mut layout = layout_de_tres_paneles();
        layout.alternar_maximizado();
        layout.ir_a_panel(7);
        assert!(layout.maximizado());
    }

    #[test]
    fn hoja_en_indice_encuentra_cada_panel() {
        let mut layout = layout_de_tres_paneles();
        for (i, ruta) in ["a.txt", "b.txt", "c.txt"].iter().enumerate() {
            match hoja_en_indice(&mut layout.raiz, i) {
                Panel::Hoja(p) => assert_eq!(p.activo().ruta_mostrada, *ruta),
                Panel::Division { .. } => panic!("no es una hoja"),
            }
        }
    }

    // --- Pestañas (BACKLOG.md P3 #10) ---

    /// Archivos reales en una carpeta temporal única, borrada al final:
    /// reusar una pestaña compara rutas, y `Editor::abrir` las necesita.
    struct DirTemporal(std::path::PathBuf);

    impl DirTemporal {
        fn nuevo(nombre: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("tcode-test-pestanas-{nombre}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        /// Abre (creándolo) el archivo `nombre` y devuelve su editor y su
        /// ruta mostrada.
        fn abrir(&self, nombre: &str) -> (Editor, String) {
            let ruta = self.0.join(nombre);
            if !ruta.exists() {
                std::fs::write(&ruta, nombre).unwrap();
            }
            (Editor::abrir(&ruta).unwrap(), ruta.display().to_string())
        }
    }

    impl Drop for DirTemporal {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    fn nombres_de_pestanas(layout: &Layout) -> Vec<String> {
        layout.pestanas_activas().documentos.iter().map(|d| barra_pestanas::titulos(&[&d.ruta_mostrada])[0].clone()).collect()
    }

    fn layout_con(dir: &DirTemporal, nombres: &[&str]) -> Layout {
        let mut layout = Layout::nuevo(Editor::nuevo(), String::new());
        for nombre in nombres {
            let (editor, ruta) = dir.abrir(nombre);
            layout.abrir_en_activo(editor, ruta);
        }
        layout
    }

    #[test]
    fn abrir_reemplaza_el_sin_nombre_intacto_y_agrega_pestanas_a_la_derecha() {
        let dir = DirTemporal::nuevo("abrir");
        let mut layout = layout_con(&dir, &["a.txt", "b.txt"]);
        assert_eq!(nombres_de_pestanas(&layout), vec!["a.txt", "b.txt"]);
        assert_eq!(layout.indice_pestana_activa(), 1);

        // Lo nuevo va justo a la derecha de la activa, no al final.
        layout.ir_a_pestana(0);
        let (editor, ruta) = dir.abrir("c.txt");
        layout.abrir_en_activo(editor, ruta);
        assert_eq!(nombres_de_pestanas(&layout), vec!["a.txt", "c.txt", "b.txt"]);
        assert_eq!(layout.indice_pestana_activa(), 1);
    }

    #[test]
    fn un_sin_nombre_modificado_no_se_reemplaza() {
        let dir = DirTemporal::nuevo("sin-nombre");
        let mut layout = Layout::nuevo(Editor::nuevo(), String::new());
        layout.editor_activo_mut().insertar_char('x');
        let (editor, ruta) = dir.abrir("a.txt");
        layout.abrir_en_activo(editor, ruta);
        assert_eq!(nombres_de_pestanas(&layout), vec!["[Sin nombre]", "a.txt"]);
    }

    #[test]
    fn abrir_un_archivo_ya_abierto_solo_activa_su_pestana_y_conserva_su_estado() {
        let dir = DirTemporal::nuevo("reusar");
        let mut layout = layout_con(&dir, &["a.txt", "b.txt"]);
        layout.ir_a_pestana(0);
        layout.editor_activo_mut().insertar_char('X');
        layout.ir_a_pestana(1);

        assert!(layout.activar_pestana_de(&dir.0.join("a.txt")));
        assert_eq!(layout.num_pestanas(), 2);
        assert_eq!(layout.indice_pestana_activa(), 0);
        assert_eq!(layout.editor_activo().buffer().a_texto(), "Xa.txt");

        // Por `abrir_en_activo` directo (el editor que llega se descarta).
        layout.ir_a_pestana(1);
        let (editor, ruta) = dir.abrir("a.txt");
        layout.abrir_en_activo(editor, ruta);
        assert_eq!(layout.num_pestanas(), 2);
        assert!(layout.editor_activo().buffer().modificado(), "no se reemplazó por la copia recién leída");
        assert!(!layout.activar_pestana_de(&dir.0.join("z.txt")));
    }

    #[test]
    fn siguiente_y_anterior_dan_la_vuelta() {
        let dir = DirTemporal::nuevo("ciclo");
        let mut layout = layout_con(&dir, &["a.txt", "b.txt", "c.txt"]);
        assert_eq!(layout.indice_pestana_activa(), 2);
        layout.siguiente_pestana();
        assert_eq!(layout.indice_pestana_activa(), 0);
        layout.anterior_pestana();
        assert_eq!(layout.indice_pestana_activa(), 2);
        layout.anterior_pestana();
        assert_eq!(layout.indice_pestana_activa(), 1);
        layout.ir_a_pestana(7);
        assert_eq!(layout.indice_pestana_activa(), 1, "fuera de rango no hace nada");
    }

    #[test]
    fn cerrar_pestana_activa_la_de_la_derecha_o_la_anterior_si_era_la_ultima() {
        let dir = DirTemporal::nuevo("cerrar");
        let mut layout = layout_con(&dir, &["a.txt", "b.txt", "c.txt"]);
        layout.ir_a_pestana(1);
        layout.cerrar_pestana_activa();
        assert_eq!(nombres_de_pestanas(&layout), vec!["a.txt", "c.txt"]);
        assert_eq!(layout.indice_pestana_activa(), 1);

        layout.cerrar_pestana_activa();
        assert_eq!(nombres_de_pestanas(&layout), vec!["a.txt"]);
        assert_eq!(layout.indice_pestana_activa(), 0);
    }

    #[test]
    fn cerrar_la_ultima_pestana_del_unico_panel_deja_un_sin_nombre() {
        let dir = DirTemporal::nuevo("ultima");
        let mut layout = layout_con(&dir, &["a.txt"]);
        layout.cerrar_pestana_activa();
        assert_eq!(layout.num_paneles(), 1);
        assert_eq!(nombres_de_pestanas(&layout), vec!["[Sin nombre]"]);
    }

    #[test]
    fn cerrar_la_ultima_pestana_con_split_cierra_el_panel() {
        let dir = DirTemporal::nuevo("split");
        let mut layout = layout_con(&dir, &["a.txt", "b.txt"]);
        layout.dividir(DireccionSplit::Vertical);
        let (editor, ruta) = dir.abrir("c.txt");
        layout.abrir_en_activo(editor, ruta);
        assert_eq!(nombres_de_pestanas(&layout), vec!["c.txt"], "cada panel tiene sus propias pestañas");

        layout.cerrar_pestana_activa();
        assert_eq!(layout.num_paneles(), 1);
        assert_eq!(nombres_de_pestanas(&layout), vec!["a.txt", "b.txt"]);
    }

    #[test]
    fn modificados_cuenta_todas_las_pestanas_de_todos_los_paneles() {
        let dir = DirTemporal::nuevo("modificados");
        let mut layout = layout_con(&dir, &["a.txt", "b.txt"]);
        layout.ir_a_pestana(0);
        layout.editor_activo_mut().insertar_char('X');
        layout.ir_a_pestana(1);
        assert!(layout.panel_activo_modificado());
        layout.dividir(DireccionSplit::Vertical);
        layout.editor_activo_mut().insertar_char('Y');
        assert_eq!(layout.documentos_modificados(), 2);
        assert_eq!(layout.paneles_mut().len(), 3, "todas las pestañas de todos los paneles");
    }
}
