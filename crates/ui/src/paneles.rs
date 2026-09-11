use ratatui::layout::{Constraint, Direction, Rect};
use ratatui::Frame;

use tcode_core::Editor;
use tcode_syntax::Resaltador;

use crate::{statusbar, vista_codigo, EstadoUi, Paleta};

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

/// Un documento abierto en un panel: su editor, la ruta que se muestra en
/// la statusbar, y su propio desplazamiento vertical — cada panel se
/// desplaza de forma independiente.
pub struct PanelEditor {
    pub editor: Editor,
    pub ruta_mostrada: String,
    pub estado_ui: EstadoUi,
}

impl PanelEditor {
    pub fn nuevo(editor: Editor, ruta_mostrada: String) -> Self {
        Self { editor, ruta_mostrada, estado_ui: EstadoUi::default() }
    }

    fn vacio() -> Self {
        Self::nuevo(Editor::nuevo(), String::new())
    }
}

/// Árbol de paneles: una hoja con un documento, o una división en dos
/// sub-árboles. Privado — quien usa `tcode-ui` solo interactúa con
/// [`Layout`], nunca navega el árbol directamente.
enum Panel {
    Hoja(PanelEditor),
    Division { direccion: DireccionSplit, primero: Box<Panel>, segundo: Box<Panel> },
}

impl Panel {
    fn vacio() -> Self {
        Panel::Hoja(PanelEditor::vacio())
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
}

impl Layout {
    pub fn nuevo(editor: Editor, ruta_mostrada: String) -> Self {
        Self { raiz: Panel::Hoja(PanelEditor::nuevo(editor, ruta_mostrada)), activo: 0 }
    }

    pub fn num_paneles(&self) -> usize {
        self.raiz.contar_hojas()
    }

    pub fn indice_activo(&self) -> usize {
        self.activo
    }

    fn hojas(&self) -> Vec<&PanelEditor> {
        fn recorrer<'a>(panel: &'a Panel, salida: &mut Vec<&'a PanelEditor>) {
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

    fn hojas_mut(&mut self) -> Vec<&mut PanelEditor> {
        fn recorrer<'a>(panel: &'a mut Panel, salida: &mut Vec<&'a mut PanelEditor>) {
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

    pub fn panel_activo(&self) -> &PanelEditor {
        self.hojas()[self.activo]
    }

    pub fn panel_activo_mut(&mut self) -> &mut PanelEditor {
        let activo = self.activo;
        self.hojas_mut().remove(activo)
    }

    pub fn editor_activo(&self) -> &Editor {
        &self.panel_activo().editor
    }

    pub fn editor_activo_mut(&mut self) -> &mut Editor {
        &mut self.panel_activo_mut().editor
    }

    /// Reemplaza el documento del panel activo (abrir un archivo nuevo
    /// desde el explorador o el buscador de archivos).
    pub fn abrir_en_activo(&mut self, editor: Editor, ruta_mostrada: String) {
        let panel = self.panel_activo_mut();
        panel.editor = editor;
        panel.ruta_mostrada = ruta_mostrada;
        panel.estado_ui = EstadoUi::default();
    }

    /// Divide el panel activo en dos: el documento actual se queda en el
    /// primer sub-panel, un buffer nuevo en blanco en el segundo, que
    /// pasa a ser el panel activo (igual que VSCode).
    pub fn dividir(&mut self, direccion: DireccionSplit) {
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
        let raiz = std::mem::replace(&mut self.raiz, Panel::vacio());
        let (nueva_raiz, _) = cerrar_en_indice(raiz, self.activo);
        self.raiz = nueva_raiz;
        self.activo = self.activo.min(self.num_paneles().saturating_sub(1));
    }

    /// Va al panel `indice` (0-based, `Ctrl+1`/`Ctrl+2`/`Ctrl+3`); no hace
    /// nada si está fuera de rango.
    pub fn ir_a_panel(&mut self, indice: usize) {
        if indice < self.num_paneles() {
            self.activo = indice;
        }
    }

    /// Dibuja el árbol de paneles completo dentro de `area`, recursivo:
    /// cada división reparte el espacio 50/50 entre sus dos sub-árboles.
    /// Solo el panel activo recibe el cursor real de la terminal.
    pub fn dibujar(&mut self, frame: &mut Frame, area: Rect, paleta: &Paleta, resaltador: &mut Resaltador) {
        let activo = self.activo;
        let mut indice_actual = 0;
        dibujar_panel(frame, area, &mut self.raiz, activo, &mut indice_actual, paleta, resaltador);
    }
}

fn dibujar_panel(
    frame: &mut Frame,
    area: Rect,
    panel: &mut Panel,
    activo: usize,
    indice_actual: &mut usize,
    paleta: &Paleta,
    resaltador: &mut Resaltador,
) {
    match panel {
        Panel::Hoja(panel_editor) => {
            let es_activo = *indice_actual == activo;
            *indice_actual += 1;

            let partes = ratatui::layout::Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(1)])
                .split(area);

            vista_codigo::dibujar(
                frame,
                partes[0],
                &panel_editor.editor,
                &mut panel_editor.estado_ui,
                paleta,
                resaltador,
                &panel_editor.ruta_mostrada,
                es_activo,
            );
            statusbar::dibujar(frame, partes[1], &panel_editor.editor, &panel_editor.ruta_mostrada, paleta);
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
            dibujar_panel(frame, partes[0], primero, activo, indice_actual, paleta, resaltador);
            dibujar_panel(frame, partes[1], segundo, activo, indice_actual, paleta, resaltador);
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
            segundo: Box::new(Panel::Hoja(PanelEditor::vacio())),
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
}
