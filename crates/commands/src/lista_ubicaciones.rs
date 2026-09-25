use std::path::PathBuf;

/// Un lugar del código en la lista de "Ir a definición" (cuando hay
/// varias) o "Buscar referencias": lo que se muestra (`ruta:línea` más el
/// texto de esa línea) y a dónde saltar — `linea` base 0 y `caracter` en
/// unidades UTF-16, tal como los manda el servidor LSP (`app` los
/// convierte a bytes recién al saltar, contra el texto del archivo ya
/// abierto).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntradaUbicacion {
    pub etiqueta: String,
    pub ruta: PathBuf,
    pub linea: u32,
    pub caracter: u32,
}

/// Estado de la lista de ubicaciones: mismo esquema que el selector de
/// símbolos (`Ctrl+K .`) — escribir filtra con `tcode-fuzzy` dejando el
/// orden original (por archivo y línea), `↑`/`↓` mueven, `Enter` devuelve
/// la entrada elegida.
#[derive(Debug, Default)]
pub struct EstadoListaUbicaciones {
    activo: bool,
    titulo: String,
    consulta: String,
    entradas: Vec<EntradaUbicacion>,
    resultados: Vec<(usize, Vec<usize>)>,
    seleccion: usize,
}

impl EstadoListaUbicaciones {
    pub fn nuevo() -> Self {
        Self::default()
    }

    pub fn activo(&self) -> bool {
        self.activo
    }

    pub fn titulo(&self) -> &str {
        &self.titulo
    }

    pub fn consulta(&self) -> &str {
        &self.consulta
    }

    pub fn seleccion(&self) -> usize {
        self.seleccion
    }

    pub fn abrir(&mut self, titulo: impl Into<String>, entradas: Vec<EntradaUbicacion>) {
        self.activo = true;
        self.titulo = titulo.into();
        self.consulta.clear();
        self.entradas = entradas;
        self.recalcular();
    }

    pub fn cerrar(&mut self) {
        self.activo = false;
        self.entradas.clear();
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

    /// Las entradas que pasan el filtro, con las posiciones (en
    /// caracteres de la etiqueta) que coincidieron.
    pub fn resultados(&self) -> impl Iterator<Item = (&EntradaUbicacion, &[usize])> {
        self.resultados.iter().map(|(i, posiciones)| (&self.entradas[*i], posiciones.as_slice()))
    }

    pub fn mover_abajo(&mut self) {
        if !self.resultados.is_empty() {
            self.seleccion = (self.seleccion + 1).min(self.resultados.len() - 1);
        }
    }

    pub fn mover_arriba(&mut self) {
        self.seleccion = self.seleccion.saturating_sub(1);
    }

    /// Cierra la lista y devuelve la entrada elegida (`None` si nada
    /// pasa el filtro).
    pub fn confirmar(&mut self) -> Option<EntradaUbicacion> {
        let elegida = self.resultados.get(self.seleccion).map(|(i, _)| self.entradas[*i].clone());
        self.cerrar();
        elegida
    }

    fn recalcular(&mut self) {
        self.resultados = self
            .entradas
            .iter()
            .enumerate()
            .filter_map(|(i, e)| tcode_fuzzy::coincidir(&self.consulta, &e.etiqueta).map(|c| (i, c.posiciones)))
            .collect();
        self.seleccion = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entrada(etiqueta: &str, linea: u32) -> EntradaUbicacion {
        EntradaUbicacion { etiqueta: etiqueta.to_string(), ruta: PathBuf::from("a.rs"), linea, caracter: 0 }
    }

    #[test]
    fn filtra_manteniendo_el_orden_y_confirma_la_elegida() {
        let mut lista = EstadoListaUbicaciones::nuevo();
        lista.abrir("Referencias", vec![entrada("a.rs:1 uno", 0), entrada("b.rs:2 dos", 1), entrada("a.rs:9 tres", 8)]);
        assert!(lista.activo());
        lista.escribir('a');
        lista.escribir('.');
        let lineas: Vec<u32> = lista.resultados().map(|(e, _)| e.linea).collect();
        assert_eq!(lineas, [0, 8]);
        lista.mover_abajo();
        lista.mover_abajo();
        assert_eq!(lista.confirmar().unwrap().linea, 8);
        assert!(!lista.activo());
    }

    #[test]
    fn sin_coincidencias_no_confirma_nada() {
        let mut lista = EstadoListaUbicaciones::nuevo();
        lista.abrir("Definiciones", vec![entrada("a.rs:1", 0)]);
        lista.escribir('z');
        assert_eq!(lista.confirmar(), None);
    }
}
