use crate::registro::{comandos_disponibles, Comando};

/// Un comando que coincidió con la consulta actual, junto con las
/// posiciones (en caracteres de su descripción) que matchearon — para que
/// la UI pueda resaltarlas.
#[derive(Debug, Clone)]
pub struct ResultadoPaleta {
    pub comando: Comando,
    pub posiciones: Vec<usize>,
}

/// Estado de la paleta de comandos (`Ctrl+Shift+P`, PLAN.md §4): si está
/// abierta, qué se escribió, y qué resultado está seleccionado. No sabe
/// nada de terminal/`ratatui` — el crate `ui` la dibuja; `app` decide qué
/// tecla llega aquí y qué hacer con el id de comando que devuelve
/// `confirmar`.
pub struct EstadoPaleta {
    activa: bool,
    consulta: String,
    seleccion: usize,
}

impl EstadoPaleta {
    pub fn nueva() -> Self {
        Self { activa: false, consulta: String::new(), seleccion: 0 }
    }

    pub fn activa(&self) -> bool {
        self.activa
    }

    pub fn consulta(&self) -> &str {
        &self.consulta
    }

    pub fn seleccion(&self) -> usize {
        self.seleccion
    }

    /// Abre la paleta con la consulta en blanco.
    pub fn abrir(&mut self) {
        self.activa = true;
        self.consulta.clear();
        self.seleccion = 0;
    }

    pub fn cerrar(&mut self) {
        self.activa = false;
    }

    pub fn escribir(&mut self, c: char) {
        self.consulta.push(c);
        self.seleccion = 0;
    }

    pub fn borrar(&mut self) {
        self.consulta.pop();
        self.seleccion = 0;
    }

    /// Comandos que coinciden con la consulta actual, de mejor a peor
    /// coincidencia (consulta vacía = todos, en su orden de registro).
    pub fn resultados(&self) -> Vec<ResultadoPaleta> {
        tcode_fuzzy::filtrar_y_ordenar(&self.consulta, comandos_disponibles(), |c| c.descripcion)
            .into_iter()
            .map(|(comando, coincidencia)| ResultadoPaleta { comando: *comando, posiciones: coincidencia.posiciones })
            .collect()
    }

    pub fn mover_abajo(&mut self) {
        let total = self.resultados().len();
        if total == 0 {
            return;
        }
        self.seleccion = (self.seleccion + 1).min(total - 1);
    }

    pub fn mover_arriba(&mut self) {
        self.seleccion = self.seleccion.saturating_sub(1);
    }

    /// Confirma el resultado seleccionado: cierra la paleta y devuelve el
    /// id del comando a ejecutar (`None` si no hay resultados que
    /// coincidan con la consulta actual).
    pub fn confirmar(&mut self) -> Option<&'static str> {
        let id = self.resultados().get(self.seleccion).map(|r| r.comando.id);
        self.cerrar();
        id
    }
}

impl Default for EstadoPaleta {
    fn default() -> Self {
        Self::nueva()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abrir_resetea_consulta_y_seleccion() {
        let mut paleta = EstadoPaleta::nueva();
        paleta.escribir('x');
        paleta.abrir();
        assert!(paleta.activa());
        assert_eq!(paleta.consulta(), "");
        assert_eq!(paleta.seleccion(), 0);
    }

    #[test]
    fn escribir_filtra_los_resultados() {
        let mut paleta = EstadoPaleta::nueva();
        paleta.abrir();
        let total_sin_filtro = paleta.resultados().len();

        for c in "guardar".chars() {
            paleta.escribir(c);
        }
        let filtrados = paleta.resultados();
        assert!(filtrados.len() < total_sin_filtro);
        assert!(filtrados.iter().any(|r| r.comando.id == "archivo.guardar"));
    }

    #[test]
    fn borrar_deshace_caracteres_de_la_consulta() {
        let mut paleta = EstadoPaleta::nueva();
        paleta.abrir();
        paleta.escribir('a');
        paleta.escribir('b');
        paleta.borrar();
        assert_eq!(paleta.consulta(), "a");
    }

    #[test]
    fn confirmar_devuelve_el_id_seleccionado_y_cierra_la_paleta() {
        let mut paleta = EstadoPaleta::nueva();
        paleta.abrir();
        for c in "guardar".chars() {
            paleta.escribir(c);
        }
        let id = paleta.confirmar();
        assert_eq!(id, Some("archivo.guardar"));
        assert!(!paleta.activa());
    }

    #[test]
    fn confirmar_sin_resultados_devuelve_none() {
        let mut paleta = EstadoPaleta::nueva();
        paleta.abrir();
        for c in "esto-no-existe-como-comando".chars() {
            paleta.escribir(c);
        }
        assert_eq!(paleta.confirmar(), None);
    }

    #[test]
    fn mover_arriba_y_abajo_se_recorta_a_los_limites() {
        let mut paleta = EstadoPaleta::nueva();
        paleta.abrir();
        paleta.mover_arriba();
        assert_eq!(paleta.seleccion(), 0);

        let total = paleta.resultados().len();
        for _ in 0..(total + 5) {
            paleta.mover_abajo();
        }
        assert_eq!(paleta.seleccion(), total - 1);
    }
}
