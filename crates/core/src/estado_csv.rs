use crate::csv::{filas_visibles, FiltroCsv, TablaCsv};

/// Estado de selección y edición de la vista CSV/TSV (`Ctrl+K T`, F2/
/// `Enter`, PLAN.md §9): qué celda está seleccionada y, si se está
/// editando una, su texto en construcción. No sabe nada de terminal/UI
/// ni analiza el CSV — eso es `tcode_core::csv`; `app` decide qué tecla
/// llega aquí y aplica el resultado sobre el `Editor` con
/// `Editor::reemplazar_rango_bytes` (reemplazando la fila completa
/// reserializada, ver `csv::serializar_fila`).
///
/// También guarda lo que es solo de VISTA y no toca el archivo
/// (BACKLOG.md P2 #9): el filtro activo (y el prompt de una línea para
/// escribirlo), los anchos de columna fijados a mano y el último orden
/// aplicado (para alternar ascendente/descendente). Con un filtro activo,
/// `fila` es un índice en la lista de filas VISIBLES
/// (`csv::filas_visibles`), no en `TablaCsv::filas` — ver
/// [`EstadoCsv::fila_real`].
#[derive(Debug, Clone, Default)]
pub struct EstadoCsv {
    fila: usize,
    columna: usize,
    edicion: Option<String>,
    filtro: Option<FiltroCsv>,
    prompt_filtro: Option<String>,
    ultimo_orden: Option<(usize, bool)>,
    anchos_manuales: Vec<Option<usize>>,
}

/// Límites del ancho fijado a mano (`Ctrl+K Shift+→`/`Ctrl+K Shift+←`):
/// el máximo automático es `vista_csv::ANCHO_MAX_COLUMNA` (30), así que
/// a mano se permite bastante más para poder leer una celda larga
/// entera, pero no infinito — una columna más ancha que cualquier
/// terminal no tiene sentido.
pub const ANCHO_MANUAL_MIN: usize = 1;
pub const ANCHO_MANUAL_MAX: usize = 200;

impl EstadoCsv {
    pub fn nuevo() -> Self {
        Self::default()
    }

    pub fn fila(&self) -> usize {
        self.fila
    }

    pub fn columna(&self) -> usize {
        self.columna
    }

    pub fn edicion(&self) -> Option<&str> {
        self.edicion.as_deref()
    }

    pub fn editando(&self) -> bool {
        self.edicion.is_some()
    }

    pub fn mover_arriba(&mut self) {
        self.fila = self.fila.saturating_sub(1);
    }

    pub fn mover_abajo(&mut self, num_filas: usize) {
        if num_filas > 0 {
            self.fila = (self.fila + 1).min(num_filas - 1);
        }
    }

    pub fn mover_izquierda(&mut self) {
        self.columna = self.columna.saturating_sub(1);
    }

    pub fn mover_derecha(&mut self, num_columnas: usize) {
        if num_columnas > 0 {
            self.columna = (self.columna + 1).min(num_columnas - 1);
        }
    }

    /// `Tab`: como `mover_derecha`, pero al llegar a la última columna
    /// salta a la primera columna de la fila siguiente — igual que en
    /// una hoja de cálculo, en vez de quedarse pegado al borde.
    pub fn tab(&mut self, num_filas: usize, num_columnas: usize) {
        if num_columnas == 0 {
            return;
        }
        if self.columna + 1 < num_columnas {
            self.columna += 1;
        } else if self.fila + 1 < num_filas {
            self.fila += 1;
            self.columna = 0;
        }
    }

    /// `Shift+Tab`: inverso de `tab`.
    pub fn shift_tab(&mut self, num_columnas: usize) {
        if self.columna > 0 {
            self.columna -= 1;
        } else if self.fila > 0 {
            self.fila -= 1;
            self.columna = num_columnas.saturating_sub(1);
        }
    }

    /// `Enter`/`F2` sobre una celda que no se está editando: abre el modo
    /// edición con `valor_actual` precargado (y el cursor al final, igual
    /// que la barra de búsqueda).
    pub fn iniciar_edicion(&mut self, valor_actual: &str) {
        self.edicion = Some(valor_actual.to_string());
    }

    pub fn escribir(&mut self, c: char) {
        if let Some(texto) = &mut self.edicion {
            texto.push(c);
        }
    }

    pub fn borrar(&mut self) {
        if let Some(texto) = &mut self.edicion {
            texto.pop();
        }
    }

    /// Confirma la edición en curso, devolviendo el texto final (`None`
    /// si no había ninguna edición abierta). Quien llama es responsable
    /// de escribirlo de vuelta en el buffer — este struct no sabe nada
    /// del `Editor`.
    pub fn confirmar_edicion(&mut self) -> Option<String> {
        self.edicion.take()
    }

    pub fn cancelar_edicion(&mut self) {
        self.edicion = None;
    }

    /// Recorta la selección a los límites de una tabla que cambió de
    /// tamaño (p. ej. al abrir un archivo nuevo en el mismo panel) —
    /// evita quedar "fuera" de la tabla.
    pub fn recortar(&mut self, num_filas: usize, num_columnas: usize) {
        self.fila = self.fila.min(num_filas.saturating_sub(1));
        self.columna = self.columna.min(num_columnas.saturating_sub(1));
    }

    /// Índices reales de las filas visibles con el filtro actual (ver
    /// `csv::filas_visibles`).
    pub fn filas_visibles(&self, tabla: &TablaCsv) -> Vec<usize> {
        filas_visibles(tabla, self.filtro.as_ref())
    }

    /// Índice REAL (en `tabla.filas`) de la fila seleccionada — lo que
    /// hay que usar para leer o escribir el archivo. Sin filtro coincide
    /// con `fila()`; con filtro, `fila()` es la posición entre las
    /// visibles. `None` si la selección quedó fuera (tabla vacía).
    pub fn fila_real(&self, tabla: &TablaCsv) -> Option<usize> {
        self.filas_visibles(tabla).get(self.fila).copied()
    }

    pub fn filtro(&self) -> Option<&FiltroCsv> {
        self.filtro.as_ref()
    }

    /// Aplica un filtro sobre `columna` (un `texto` vacío no filtra nada;
    /// para quitar el filtro conservando la fila seleccionada está
    /// [`EstadoCsv::quitar_filtro`], que es lo que usa `app` en ese caso).
    /// La selección vuelve a la primera fila de datos: la que estaba
    /// seleccionada puede no existir entre las visibles nuevas, y ahí es
    /// donde empiezan los resultados (quien llama la recorta si el filtro
    /// no dejó ninguna).
    pub fn filtrar(&mut self, columna: usize, texto: &str) {
        self.filtro = if texto.is_empty() { None } else { Some(FiltroCsv { columna, texto: texto.to_string() }) };
        self.fila = 1;
    }

    /// Quita el filtro. Devuelve si había uno (para que `Esc` solo se
    /// "consuma" cuando realmente había algo que quitar). La selección
    /// se queda sobre la MISMA fila real que tenía seleccionada, no en la
    /// misma posición de pantalla — si no, quitar el filtro la haría
    /// saltar a otra fila del archivo sin que el usuario hiciera nada.
    pub fn quitar_filtro(&mut self, tabla: &TablaCsv) -> bool {
        if self.filtro.is_none() {
            return false;
        }
        let real = self.fila_real(tabla).unwrap_or(0);
        self.filtro = None;
        self.fila = real;
        true
    }

    /// Abre el prompt de filtro (`Ctrl+K /`) con `inicial` precargado
    /// (el texto del filtro vigente si es sobre la misma columna).
    pub fn abrir_prompt_filtro(&mut self, inicial: &str) {
        self.prompt_filtro = Some(inicial.to_string());
    }

    pub fn prompt_filtro(&self) -> Option<&str> {
        self.prompt_filtro.as_deref()
    }

    pub fn escribir_en_prompt_filtro(&mut self, c: char) {
        if let Some(texto) = &mut self.prompt_filtro {
            texto.push(c);
        }
    }

    pub fn borrar_en_prompt_filtro(&mut self) {
        if let Some(texto) = &mut self.prompt_filtro {
            texto.pop();
        }
    }

    /// Cierra el prompt devolviendo lo escrito (`None` si no estaba
    /// abierto) — `Enter` lo aplica con [`EstadoCsv::filtrar`], `Esc` lo
    /// descarta.
    pub fn cerrar_prompt_filtro(&mut self) -> Option<String> {
        self.prompt_filtro.take()
    }

    /// Sentido del próximo orden sobre `columna`: ascendente, salvo que
    /// lo último que se hizo haya sido ordenar ESA columna ascendente —
    /// ahí alterna a descendente (y la siguiente vez, de vuelta a
    /// ascendente). Registra el orden devuelto como el último.
    pub fn siguiente_orden(&mut self, columna: usize) -> bool {
        let ascendente = self.ultimo_orden != Some((columna, true));
        self.ultimo_orden = Some((columna, ascendente));
        ascendente
    }

    pub fn ultimo_orden(&self) -> Option<(usize, bool)> {
        self.ultimo_orden
    }

    /// Ancho fijado a mano para `columna`, si lo hay (si no, la vista
    /// usa el automático según contenido).
    pub fn ancho_manual(&self, columna: usize) -> Option<usize> {
        self.anchos_manuales.get(columna).copied().flatten()
    }

    /// Fija a mano el ancho de `columna`, recortado a
    /// `[ANCHO_MANUAL_MIN, ANCHO_MANUAL_MAX]`.
    pub fn fijar_ancho(&mut self, columna: usize, ancho: usize) {
        if self.anchos_manuales.len() <= columna {
            self.anchos_manuales.resize(columna + 1, None);
        }
        self.anchos_manuales[columna] = Some(ancho.clamp(ANCHO_MANUAL_MIN, ANCHO_MANUAL_MAX));
    }

    /// Vuelve `columna` al ancho automático.
    pub fn restablecer_ancho(&mut self, columna: usize) {
        if let Some(ancho) = self.anchos_manuales.get_mut(columna) {
            *ancho = None;
        }
    }

    /// Se insertó una columna en `indice` en el archivo: todo lo que el
    /// estado recuerda "por columna" (anchos manuales, columna filtrada)
    /// se corre una posición a la derecha desde ahí, para seguir
    /// pegado a la misma columna de datos. El último orden se olvida (la
    /// columna ordenada puede haber cambiado de posición).
    pub fn columna_insertada(&mut self, indice: usize) {
        if indice <= self.anchos_manuales.len() {
            self.anchos_manuales.insert(indice, None);
        }
        if let Some(filtro) = &mut self.filtro {
            if filtro.columna >= indice {
                filtro.columna += 1;
            }
        }
        self.ultimo_orden = None;
    }

    /// Inverso de [`EstadoCsv::columna_insertada`]. Si la columna
    /// eliminada era la filtrada, el filtro se quita (ya no hay sobre qué
    /// filtrar) — devuelve `true` en ese caso.
    pub fn columna_eliminada(&mut self, indice: usize) -> bool {
        if indice < self.anchos_manuales.len() {
            self.anchos_manuales.remove(indice);
        }
        self.ultimo_orden = None;
        let mut filtro_quitado = false;
        if let Some(filtro) = &mut self.filtro {
            if filtro.columna == indice {
                filtro_quitado = true;
            } else if filtro.columna > indice {
                filtro.columna -= 1;
            }
        }
        if filtro_quitado {
            self.filtro = None;
        }
        filtro_quitado
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mover_se_recorta_a_los_limites_de_la_tabla() {
        let mut estado = EstadoCsv::nuevo();
        estado.mover_arriba();
        estado.mover_izquierda();
        assert_eq!((estado.fila(), estado.columna()), (0, 0));

        estado.mover_abajo(3);
        estado.mover_abajo(3);
        estado.mover_abajo(3);
        assert_eq!(estado.fila(), 2); // se recorta a num_filas - 1

        estado.mover_derecha(2);
        estado.mover_derecha(2);
        assert_eq!(estado.columna(), 1); // se recorta a num_columnas - 1
    }

    #[test]
    fn tab_salta_a_la_siguiente_fila_al_llegar_al_final() {
        let mut estado = EstadoCsv::nuevo();
        estado.mover_derecha(2); // (0, 1), última columna de 2
        estado.tab(3, 2);
        assert_eq!((estado.fila(), estado.columna()), (1, 0));

        // En la última celda de la tabla, tab no hace nada (no hay
        // siguiente fila a la que saltar).
        let mut estado = EstadoCsv::nuevo();
        estado.mover_abajo(2);
        estado.mover_derecha(2);
        estado.tab(2, 2);
        assert_eq!((estado.fila(), estado.columna()), (1, 1));
    }

    #[test]
    fn shift_tab_es_el_inverso_de_tab() {
        let mut estado = EstadoCsv::nuevo();
        estado.mover_abajo(3);
        estado.shift_tab(2);
        assert_eq!((estado.fila(), estado.columna()), (0, 1));

        // En la primera celda, shift+tab no hace nada.
        let mut estado = EstadoCsv::nuevo();
        estado.shift_tab(2);
        assert_eq!((estado.fila(), estado.columna()), (0, 0));
    }

    #[test]
    fn editar_una_celda_escribe_borra_y_confirma() {
        let mut estado = EstadoCsv::nuevo();
        assert!(!estado.editando());

        estado.iniciar_edicion("valor");
        assert!(estado.editando());
        assert_eq!(estado.edicion(), Some("valor"));

        for c in " extra".chars() {
            estado.escribir(c);
        }
        estado.borrar();
        assert_eq!(estado.edicion(), Some("valor extr"));

        assert_eq!(estado.confirmar_edicion(), Some("valor extr".to_string()));
        assert!(!estado.editando());
        assert_eq!(estado.confirmar_edicion(), None);
    }

    #[test]
    fn cancelar_edicion_descarta_los_cambios() {
        let mut estado = EstadoCsv::nuevo();
        estado.iniciar_edicion("original");
        estado.escribir('!');
        estado.cancelar_edicion();
        assert!(!estado.editando());
    }

    #[test]
    fn recortar_ajusta_una_seleccion_fuera_de_una_tabla_mas_chica() {
        let mut estado = EstadoCsv::nuevo();
        // `mover_abajo`/`mover_derecha` avanzan de a un paso: hacen falta
        // varias llamadas (con un límite holgado) para simular una
        // selección que quedó más allá de los límites de una tabla que
        // se achicó (p. ej. al abrir un archivo nuevo en el mismo panel).
        for _ in 0..10 {
            estado.mover_abajo(100);
            estado.mover_derecha(100);
        }
        assert_eq!((estado.fila(), estado.columna()), (10, 10));

        estado.recortar(3, 2);
        assert_eq!((estado.fila(), estado.columna()), (2, 1));
    }

    fn tabla(texto: &str) -> TablaCsv {
        crate::csv::analizar(texto, b',').unwrap()
    }

    #[test]
    fn con_filtro_la_fila_visible_se_mapea_a_la_fila_real() {
        let tabla = tabla("ciudad\nLima\nMérida\nBogotá\nMéxico\n");
        let mut estado = EstadoCsv::nuevo();
        estado.filtrar(0, "me");
        // Visibles: encabezado (0), "Mérida" (2), "México" (4).
        assert_eq!(estado.filas_visibles(&tabla), vec![0, 2, 4]);
        assert_eq!(estado.fila(), 1);
        assert_eq!(estado.fila_real(&tabla), Some(2));
        estado.mover_abajo(3);
        assert_eq!(estado.fila_real(&tabla), Some(4));

        // Quitar el filtro deja la selección en la MISMA fila real.
        assert!(estado.quitar_filtro(&tabla));
        assert_eq!(estado.fila(), 4);
        assert_eq!(estado.fila_real(&tabla), Some(4));
        assert!(!estado.quitar_filtro(&tabla)); // ya no había filtro
    }

    #[test]
    fn prompt_de_filtro_escribe_borra_y_cierra() {
        let mut estado = EstadoCsv::nuevo();
        assert_eq!(estado.prompt_filtro(), None);
        estado.abrir_prompt_filtro("ab");
        estado.escribir_en_prompt_filtro('c');
        estado.borrar_en_prompt_filtro();
        estado.escribir_en_prompt_filtro('d');
        assert_eq!(estado.prompt_filtro(), Some("abd"));
        assert_eq!(estado.cerrar_prompt_filtro(), Some("abd".to_string()));
        assert_eq!(estado.prompt_filtro(), None);
    }

    #[test]
    fn ordenar_la_misma_columna_alterna_ascendente_y_descendente() {
        let mut estado = EstadoCsv::nuevo();
        assert!(estado.siguiente_orden(1));
        assert!(!estado.siguiente_orden(1));
        assert!(estado.siguiente_orden(1));
        // Otra columna siempre arranca ascendente.
        assert!(estado.siguiente_orden(0));
    }

    #[test]
    fn anchos_manuales_se_recortan_y_siguen_a_su_columna_al_insertar_o_eliminar() {
        let mut estado = EstadoCsv::nuevo();
        estado.fijar_ancho(1, 500);
        assert_eq!(estado.ancho_manual(1), Some(ANCHO_MANUAL_MAX));
        estado.fijar_ancho(1, 0);
        assert_eq!(estado.ancho_manual(1), Some(ANCHO_MANUAL_MIN));
        estado.fijar_ancho(1, 12);

        estado.columna_insertada(0);
        assert_eq!(estado.ancho_manual(1), None);
        assert_eq!(estado.ancho_manual(2), Some(12));

        estado.columna_eliminada(0);
        assert_eq!(estado.ancho_manual(1), Some(12));
        estado.restablecer_ancho(1);
        assert_eq!(estado.ancho_manual(1), None);
    }

    #[test]
    fn eliminar_la_columna_filtrada_quita_el_filtro() {
        let mut estado = EstadoCsv::nuevo();
        estado.filtrar(2, "x");
        estado.columna_insertada(1);
        assert_eq!(estado.filtro().map(|f| f.columna), Some(3));
        assert!(!estado.columna_eliminada(0));
        assert_eq!(estado.filtro().map(|f| f.columna), Some(2));
        assert!(estado.columna_eliminada(2));
        assert!(estado.filtro().is_none());
    }
}
