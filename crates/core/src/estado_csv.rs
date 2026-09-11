/// Estado de selección y edición de la vista CSV/TSV (`Ctrl+K T`, F2/
/// `Enter`, PLAN.md §9): qué celda está seleccionada y, si se está
/// editando una, su texto en construcción. No sabe nada de terminal/UI
/// ni analiza el CSV — eso es `tcode_core::csv`; `app` decide qué tecla
/// llega aquí y aplica el resultado sobre el `Editor` con
/// `Editor::reemplazar_rango_bytes` (reemplazando la fila completa
/// reserializada, ver `csv::serializar_fila`).
#[derive(Debug, Clone, Default)]
pub struct EstadoCsv {
    fila: usize,
    columna: usize,
    edicion: Option<String>,
}

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
}
