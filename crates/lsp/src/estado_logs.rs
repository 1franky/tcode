/// Estado del visor de logs de la sesión LSP activa (`Ctrl+K R`, PLAN.md
/// §5.3: "ver logs" — pendiente desde M4). Snapshot de las líneas de
/// stderr en el momento de abrir (`Cliente::logs`/`EstadoLsp::logs`), NO
/// se actualiza en vivo mientras el visor está abierto — cerrar y volver
/// a abrir muestra lo más nuevo. Se guardan más nuevas primero (al revés
/// del orden cronológico en que llegaron) porque lo último que pasó es
/// típicamente lo más relevante para diagnosticar un problema, y el
/// overlay que las dibuja (`tcode_ui::overlay`, igual que la paleta de
/// comandos) no hace scroll más allá de lo que entra en pantalla — el
/// filtro de texto es la forma de encontrar algo que quedó afuera de esa
/// primera pantalla, no una barra de scroll.
#[derive(Debug, Clone, Default)]
pub struct EstadoLogsLsp {
    activo: bool,
    lineas: Vec<String>,
    filtro: String,
}

impl EstadoLogsLsp {
    pub fn nuevo() -> Self {
        Self::default()
    }

    pub fn activo(&self) -> bool {
        self.activo
    }

    pub fn filtro(&self) -> &str {
        &self.filtro
    }

    /// Abre el visor con una copia de `lineas` (orden cronológico, más
    /// vieja primero, tal como las devuelve `EstadoLsp::logs`) — el
    /// propio método la invierte para mostrar la más reciente arriba.
    pub fn abrir(&mut self, mut lineas: Vec<String>) {
        lineas.reverse();
        self.activo = true;
        self.filtro.clear();
        self.lineas = lineas;
    }

    pub fn cerrar(&mut self) {
        self.activo = false;
    }

    pub fn escribir(&mut self, c: char) {
        self.filtro.push(c);
    }

    pub fn borrar(&mut self) {
        self.filtro.pop();
    }

    /// `true` si el visor se abrió sin ninguna línea — no había sesión
    /// LSP activa, o la tenía pero nunca escribió nada en stderr (lo más
    /// común: la mayoría de los servidores se quedan callados mientras
    /// todo funciona bien).
    pub fn sin_logs(&self) -> bool {
        self.lineas.is_empty()
    }

    /// Líneas que coinciden con el filtro (subcadena, sin distinguir
    /// mayúsculas/minúsculas) — todas si el filtro está vacío. Cada una
    /// viene con el rango de posiciones de carácter que coincidieron
    /// (para resaltarlas en negrita, mismo criterio visual que la
    /// coincidencia difusa de la paleta de comandos, aunque acá la
    /// búsqueda sea literal, no aproximada — tiene más sentido para
    /// encontrar un mensaje de error exacto).
    pub fn lineas_filtradas(&self) -> Vec<(&str, Vec<usize>)> {
        if self.filtro.is_empty() {
            return self.lineas.iter().map(|l| (l.as_str(), Vec::new())).collect();
        }
        let filtro = self.filtro.to_lowercase();
        self.lineas
            .iter()
            .filter_map(|linea| {
                let minuscula = linea.to_lowercase();
                let inicio_byte = minuscula.find(&filtro)?;
                // Posiciones en CARACTERES, no bytes — coherente con el
                // resto de la app (índices de `char`, no de UTF-8 crudo).
                let inicio_char = minuscula[..inicio_byte].chars().count();
                let posiciones = (inicio_char..inicio_char + filtro.chars().count()).collect();
                Some((linea.as_str(), posiciones))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abrir_invierte_el_orden_para_mostrar_lo_mas_nuevo_primero() {
        let mut estado = EstadoLogsLsp::nuevo();
        estado.abrir(vec!["uno".to_string(), "dos".to_string(), "tres".to_string()]);
        assert!(estado.activo());
        assert_eq!(estado.lineas_filtradas().iter().map(|(l, _)| *l).collect::<Vec<_>>(), vec!["tres", "dos", "uno"]);
    }

    #[test]
    fn abrir_sin_lineas_marca_sin_logs() {
        let mut estado = EstadoLogsLsp::nuevo();
        estado.abrir(Vec::new());
        assert!(estado.sin_logs());
    }

    #[test]
    fn abrir_con_lineas_no_esta_sin_logs() {
        let mut estado = EstadoLogsLsp::nuevo();
        estado.abrir(vec!["algo".to_string()]);
        assert!(!estado.sin_logs());
    }

    #[test]
    fn cerrar_apaga_el_estado_activo() {
        let mut estado = EstadoLogsLsp::nuevo();
        estado.abrir(vec!["algo".to_string()]);
        estado.cerrar();
        assert!(!estado.activo());
    }

    #[test]
    fn filtro_recorta_por_subcadena_sin_distinguir_mayusculas() {
        let mut estado = EstadoLogsLsp::nuevo();
        estado.abrir(vec!["Error: archivo no encontrado".to_string(), "info: todo bien".to_string()]);

        for c in "error".chars() {
            estado.escribir(c);
        }
        let filtradas = estado.lineas_filtradas();
        assert_eq!(filtradas.len(), 1);
        assert_eq!(filtradas[0].0, "Error: archivo no encontrado");
    }

    #[test]
    fn filtro_marca_las_posiciones_de_caracteres_que_coincidieron() {
        let mut estado = EstadoLogsLsp::nuevo();
        estado.abrir(vec!["abc error xyz".to_string()]);
        for c in "error".chars() {
            estado.escribir(c);
        }
        let filtradas = estado.lineas_filtradas();
        // "abc error xyz": "error" arranca en el índice 4 (carácter), 5 de largo.
        assert_eq!(filtradas[0].1, vec![4, 5, 6, 7, 8]);
    }

    #[test]
    fn filtro_vacio_muestra_todo_sin_posiciones_marcadas() {
        let mut estado = EstadoLogsLsp::nuevo();
        estado.abrir(vec!["a".to_string(), "b".to_string()]);
        let filtradas = estado.lineas_filtradas();
        assert_eq!(filtradas.len(), 2);
        assert!(filtradas.iter().all(|(_, pos)| pos.is_empty()));
    }

    #[test]
    fn borrar_quita_el_ultimo_caracter_del_filtro() {
        let mut estado = EstadoLogsLsp::nuevo();
        estado.abrir(vec!["algo".to_string()]);
        estado.escribir('a');
        estado.escribir('b');
        estado.borrar();
        assert_eq!(estado.filtro(), "a");
    }
}
