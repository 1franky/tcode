/// Un símbolo del archivo tal como lo muestra el selector de símbolos
/// (`Ctrl+K .`): su etiqueta (`fn insertar_texto`, `class Editor`), cuántos
/// contenedores lo encierran (para indentarlo), su línea (1-based, solo
/// para mostrarla), el byte al que se salta y el byte donde termina (para
/// saber si contiene al cursor). No depende de tree-sitter:
/// `app` lo arma a partir de `tcode_syntax::SimboloEsquema`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntradaSimbolo {
    pub etiqueta: String,
    pub profundidad: usize,
    pub linea: usize,
    pub byte: usize,
    pub fin: usize,
}

/// Un símbolo que pasa el filtro actual: su índice en la lista completa
/// y las posiciones (en caracteres de la etiqueta) que coincidieron.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultadoSimbolo {
    pub indice: usize,
    pub posiciones: Vec<usize>,
}

/// Estado del selector de símbolos del archivo actual (outline, "Ir a
/// símbolo"): mismo esquema que la paleta de comandos — escribir filtra
/// con `tcode-fuzzy`, `↑`/`↓` mueven, `Enter` devuelve a dónde saltar —,
/// salvo que los resultados quedan en el orden del archivo en vez de
/// ordenarse por puntaje: así el anidamiento (la indentación) sigue
/// teniendo sentido aunque haya un filtro escrito.
///
/// Los símbolos se calculan una sola vez al abrir (no mientras se
/// escribe el filtro): el archivo no puede cambiar mientras el selector
/// captura el teclado.
pub struct EstadoSelectorSimbolos {
    activo: bool,
    consulta: String,
    simbolos: Vec<EntradaSimbolo>,
    resultados: Vec<ResultadoSimbolo>,
    seleccion: usize,
}

impl EstadoSelectorSimbolos {
    pub fn nuevo() -> Self {
        Self { activo: false, consulta: String::new(), simbolos: Vec::new(), resultados: Vec::new(), seleccion: 0 }
    }

    pub fn activo(&self) -> bool {
        self.activo
    }

    pub fn consulta(&self) -> &str {
        &self.consulta
    }

    pub fn seleccion(&self) -> usize {
        self.seleccion
    }

    /// Abre el selector con `simbolos` (en orden de aparición en el
    /// archivo) y el filtro en blanco, posicionado en el símbolo más
    /// interno que contiene `byte_cursor` — el último, en preorden, cuyo
    /// rango `byte..=fin` lo incluye —, o en el primero si el cursor no
    /// está adentro de ninguno.
    pub fn abrir(&mut self, simbolos: Vec<EntradaSimbolo>, byte_cursor: usize) {
        self.activo = true;
        self.consulta.clear();
        self.simbolos = simbolos;
        self.recalcular();
        self.seleccion = self.simbolos.iter().rposition(|s| (s.byte..=s.fin).contains(&byte_cursor)).unwrap_or(0);
    }

    pub fn cerrar(&mut self) {
        self.activo = false;
        self.simbolos.clear();
        self.resultados.clear();
    }

    pub fn escribir(&mut self, c: char) {
        self.consulta.push(c);
        self.recalcular();
    }

    pub fn borrar(&mut self) {
        self.consulta.pop();
        self.recalcular();
    }

    /// Símbolos que pasan el filtro, en el orden del archivo.
    pub fn resultados(&self) -> &[ResultadoSimbolo] {
        &self.resultados
    }

    /// El símbolo `indice` de la lista completa (el de un
    /// [`ResultadoSimbolo`]).
    pub fn simbolo(&self, indice: usize) -> &EntradaSimbolo {
        &self.simbolos[indice]
    }

    /// Si el archivo no tiene ningún símbolo (lenguaje sin reglas, o sin
    /// funciones/clases), para que la UI lo diga en vez de mostrar una
    /// lista vacía muda.
    pub fn sin_simbolos(&self) -> bool {
        self.simbolos.is_empty()
    }

    pub fn mover_abajo(&mut self) {
        if !self.resultados.is_empty() {
            self.seleccion = (self.seleccion + 1).min(self.resultados.len() - 1);
        }
    }

    pub fn mover_arriba(&mut self) {
        self.seleccion = self.seleccion.saturating_sub(1);
    }

    /// Cierra el selector y devuelve el byte del símbolo elegido (`None`
    /// si ningún símbolo pasa el filtro).
    pub fn confirmar(&mut self) -> Option<usize> {
        let byte = self.resultados.get(self.seleccion).map(|r| self.simbolos[r.indice].byte);
        self.cerrar();
        byte
    }

    fn recalcular(&mut self) {
        self.resultados = self
            .simbolos
            .iter()
            .enumerate()
            .filter_map(|(indice, s)| {
                tcode_fuzzy::coincidir(&self.consulta, &s.etiqueta)
                    .map(|c| ResultadoSimbolo { indice, posiciones: c.posiciones })
            })
            .collect();
        self.seleccion = 0;
    }
}

impl Default for EstadoSelectorSimbolos {
    fn default() -> Self {
        Self::nuevo()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entrada(etiqueta: &str, profundidad: usize, byte: usize, fin: usize) -> EntradaSimbolo {
        EntradaSimbolo { etiqueta: etiqueta.to_string(), profundidad, linea: byte + 1, byte, fin }
    }

    /// `impl A` (0..50) > `fn uno` (10..20), `fn dos` (30..45); `fn b` (60..70).
    fn abrir_con_cursor(byte_cursor: usize) -> EstadoSelectorSimbolos {
        let mut selector = EstadoSelectorSimbolos::nuevo();
        selector.abrir(
            vec![
                entrada("impl A", 0, 0, 50),
                entrada("fn uno", 1, 10, 20),
                entrada("fn dos", 1, 30, 45),
                entrada("fn b", 0, 60, 70),
            ],
            byte_cursor,
        );
        selector
    }

    #[test]
    fn abre_posicionado_en_el_simbolo_mas_interno_del_cursor() {
        assert_eq!(abrir_con_cursor(35).seleccion(), 2);
        assert_eq!(abrir_con_cursor(25).seleccion(), 0);
        assert_eq!(abrir_con_cursor(65).seleccion(), 3);
        // Fuera de todo: el primero.
        assert_eq!(abrir_con_cursor(55).seleccion(), 0);
    }

    #[test]
    fn el_filtro_mantiene_el_orden_del_archivo() {
        let mut selector = abrir_con_cursor(0);
        selector.escribir('f');
        selector.escribir('n');
        let indices: Vec<usize> = selector.resultados().iter().map(|r| r.indice).collect();
        assert_eq!(indices, [1, 2, 3]);
        selector.escribir('d');
        let indices: Vec<usize> = selector.resultados().iter().map(|r| r.indice).collect();
        assert_eq!(indices, [2]);
        selector.borrar();
        assert_eq!(selector.resultados().len(), 3);
    }

    #[test]
    fn confirmar_devuelve_el_byte_y_cierra() {
        let mut selector = abrir_con_cursor(0);
        selector.mover_abajo();
        selector.mover_abajo();
        assert_eq!(selector.confirmar(), Some(30));
        assert!(!selector.activo());
    }

    #[test]
    fn sin_coincidencias_confirmar_no_salta() {
        let mut selector = abrir_con_cursor(0);
        selector.escribir('z');
        assert!(selector.resultados().is_empty());
        selector.mover_abajo();
        assert_eq!(selector.confirmar(), None);
    }

    #[test]
    fn sin_simbolos_se_abre_vacio() {
        let mut selector = EstadoSelectorSimbolos::nuevo();
        selector.abrir(Vec::new(), 0);
        assert!(selector.activo());
        assert!(selector.sin_simbolos());
        assert_eq!(selector.confirmar(), None);
    }
}
