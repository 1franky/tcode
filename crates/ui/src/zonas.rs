//! Dónde quedó dibujada cada cosa en el último frame (BACKLOG.md P0 #18,
//! soporte de mouse): la terminal solo informa columna y fila de un
//! clic, y para saber qué hay ahí hace falta recordar cómo se repartió
//! la pantalla. Cada `dibujar` anota su parte en [`ZonasMouse`] mientras
//! dibuja — lo que ya tenía calculado (rectángulos, desplazamiento de la
//! lista, pestañas que entraron), nunca nada O(archivo) — y `app` lo
//! consulta al llegar un evento de mouse. Se rehace entera en cada frame
//! (ver [`crate::dibujar`]), así nunca describe algo que ya no se ve.

use ratatui::layout::Rect;

/// Si `(x, y)` cae adentro de `area`.
pub fn contiene(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height
}

/// Una lista dibujada (explorador, overlays, resultados): qué ítem se ve
/// en cada fila de `area`, de arriba hacia abajo — `None` en una fila que
/// no corresponde a ningún ítem elegible (el encabezado de un archivo en
/// la búsqueda en el proyecto, o el hueco debajo del último). Una entrada
/// por fila visible: O(alto de pantalla).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ZonaLista {
    pub area: Rect,
    pub filas: Vec<Option<usize>>,
}

impl ZonaLista {
    /// La forma común: ítems consecutivos desde `desde` (el desplazamiento
    /// con el que se dibujó la lista), hasta llenar `area` o agotar
    /// `total`.
    pub fn continua(area: Rect, desde: usize, total: usize) -> Self {
        let filas = (desde..total).take(area.height as usize).map(Some).collect();
        Self { area, filas }
    }

    /// El ítem en `(x, y)`, si hay uno ahí.
    pub fn item_en(&self, x: u16, y: u16) -> Option<usize> {
        if !contiene(self.area, x, y) {
            return None;
        }
        self.filas.get((y - self.area.y) as usize).copied().flatten()
    }
}

/// Un overlay o popup abierto: su recuadro entero (un clic afuera lo
/// cierra) y, si tiene, su lista de ítems elegibles con el mouse.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ZonaOverlay {
    pub area: Rect,
    pub lista: Option<ZonaLista>,
}

/// Una pestaña de la barra: columnas `x..x + ancho` de su fila.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZonaPestana {
    pub x: u16,
    pub ancho: u16,
    pub indice: usize,
}

/// La vista de código de un panel: el área del texto y la del gutter (si
/// hay). Traducir un punto a línea/columna necesita además el scroll y
/// los pliegues del documento — eso lo hace `Layout::posicion_en_codigo`
/// con el estado del documento, ver `vista_codigo::posicion_en`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZonaCodigo {
    pub texto: Rect,
    pub gutter: Option<Rect>,
    /// Si se dibujó con ajuste de línea (el panel Markdown dividido nunca
    /// lo usa, aunque la config lo tenga prendido).
    pub ajuste: bool,
}

/// La vista de tabla CSV/TSV de un panel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ZonaTabla {
    /// Donde se dibujó la tabla: la primera fila es el encabezado.
    pub area: Rect,
    /// Índice (en filas visibles, sin contar el encabezado) de la primera
    /// fila del cuerpo que se ve — el `offset` del `TableState`.
    pub desde: usize,
    /// Filas visibles de la tabla, encabezado incluido.
    pub num_filas: usize,
    /// Columnas de la tabla (todas, no solo las que se ven).
    pub num_columnas: usize,
    /// Columnas que se ven: `(x, ancho, índice de columna)`.
    pub columnas: Vec<(u16, u16, usize)>,
}

impl ZonaTabla {
    /// La celda `(fila visible, columna)` en `(x, y)`, si hay una ahí —
    /// la fila en el mismo espacio que `EstadoCsv::fila` (0 = encabezado).
    pub fn celda_en(&self, x: u16, y: u16) -> Option<(usize, usize)> {
        if !contiene(self.area, x, y) {
            return None;
        }
        let fila = match y - self.area.y {
            0 => 0,
            n => self.desde + n as usize,
        };
        if fila >= self.num_filas {
            return None;
        }
        let &(_, _, columna) = self.columnas.iter().find(|(cx, ancho, _)| x >= *cx && x < cx + ancho)?;
        Some((fila, columna))
    }
}

/// Un panel de edición (hoja del árbol de splits) tal como se dibujó.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ZonaPanel {
    /// Índice del panel (el de `Layout::ir_a_panel`), también maximizado.
    pub indice: usize,
    /// Todo el panel: pestañas, breadcrumbs, contenido y statusbar.
    pub area: Rect,
    /// La fila de la barra de pestañas y las pestañas que entraron.
    pub barra_pestanas: Option<Rect>,
    pub pestanas: Vec<ZonaPestana>,
    /// El contenido (código, tabla o preview), sin barras.
    pub contenido: Rect,
    pub codigo: Option<ZonaCodigo>,
    pub tabla: Option<ZonaTabla>,
}

impl ZonaPanel {
    /// La pestaña en `(x, y)`, si hay una ahí.
    pub fn pestana_en(&self, x: u16, y: u16) -> Option<usize> {
        let barra = self.barra_pestanas?;
        if !contiene(barra, x, y) {
            return None;
        }
        self.pestanas.iter().find(|p| x >= p.x && x < p.x + p.ancho).map(|p| p.indice)
    }
}

/// Todo lo que el mouse puede tocar en el último frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ZonasMouse {
    pub explorador: Option<ZonaLista>,
    pub paneles: Vec<ZonaPanel>,
    /// El overlay de más arriba (hay uno a la vez, ver [`crate::dibujar`]),
    /// o la barra de `Ctrl+F`.
    pub overlay: Option<ZonaOverlay>,
    /// Popup de completado o de hover del LSP, pegado al cursor: aparte
    /// de `overlay` porque puede estar abierto con la barra de búsqueda.
    pub popup: Option<ZonaOverlay>,
    /// La pantalla de la terminal integrada (BACKLOG.md P3 #26, sin el
    /// título), si se ve: su tamaño es el que tiene que tener la terminal.
    pub terminal: Option<Rect>,
}

impl ZonasMouse {
    /// El panel en `(x, y)`, si hay uno ahí.
    pub fn panel_en(&self, x: u16, y: u16) -> Option<&ZonaPanel> {
        self.paneles.iter().find(|p| contiene(p.area, x, y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lista_continua_mapea_filas_a_items_desde_el_desplazamiento() {
        let lista = ZonaLista::continua(Rect { x: 2, y: 5, width: 10, height: 3 }, 7, 9);
        assert_eq!(lista.filas, vec![Some(7), Some(8)]);
        assert_eq!(lista.item_en(2, 5), Some(7));
        assert_eq!(lista.item_en(11, 6), Some(8));
        // Debajo del último ítem, o afuera del área: nada.
        assert_eq!(lista.item_en(3, 7), None);
        assert_eq!(lista.item_en(12, 5), None);
        assert_eq!(lista.item_en(2, 4), None);
    }

    #[test]
    fn celda_de_tabla_respeta_encabezado_desplazamiento_y_columnas() {
        let tabla = ZonaTabla {
            area: Rect { x: 0, y: 1, width: 20, height: 5 },
            desde: 3,
            num_filas: 6,
            num_columnas: 4,
            columnas: vec![(0, 4, 2), (5, 6, 3)],
        };
        assert_eq!(tabla.celda_en(1, 1), Some((0, 2)));
        assert_eq!(tabla.celda_en(6, 2), Some((4, 3)));
        assert_eq!(tabla.celda_en(6, 3), Some((5, 3)));
        // Más allá de la última fila visible, en el espacio entre
        // columnas, o a la derecha de la última.
        assert_eq!(tabla.celda_en(6, 4), None);
        assert_eq!(tabla.celda_en(4, 2), None);
        assert_eq!(tabla.celda_en(15, 2), None);
    }

    #[test]
    fn pestana_en_solo_dentro_de_la_barra() {
        let panel = ZonaPanel {
            barra_pestanas: Some(Rect { x: 10, y: 0, width: 30, height: 1 }),
            pestanas: vec![ZonaPestana { x: 10, ancho: 5, indice: 0 }, ZonaPestana { x: 15, ancho: 8, indice: 1 }],
            ..Default::default()
        };
        assert_eq!(panel.pestana_en(12, 0), Some(0));
        assert_eq!(panel.pestana_en(22, 0), Some(1));
        assert_eq!(panel.pestana_en(30, 0), None);
        assert_eq!(panel.pestana_en(12, 1), None);
    }
}
