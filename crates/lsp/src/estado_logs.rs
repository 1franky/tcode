/// Estado del visor de logs de la sesión LSP activa (`Ctrl+K R`, PLAN.md
/// §5.3: "ver logs en tiempo real"). Al abrir toma las líneas de stderr
/// de ese momento (`Cliente::logs_con_total`/`EstadoLsp::logs_con_total`)
/// y, mientras sigue abierto, `app` le pasa lo más nuevo en cada tick del
/// bucle principal (`actualizar`, BACKLOG.md P1 #2) — sin tocar el filtro
/// que se esté tipeando. Se guardan más nuevas primero (al revés del
/// orden cronológico en que llegaron) porque lo último que pasó es
/// típicamente lo más relevante para diagnosticar un problema.
///
/// Scroll: `seleccion == None` es "siguiendo lo más nuevo" (sin fila
/// resaltada, la lista se dibuja desde arriba, así que cada línea que
/// llega aparece arriba de todo — el equivalente a "estar al final" de
/// un log cronológico). `↓`/`↑`/`RePág`/`AvPág` mueven una fila
/// resaltada por la lista (el overlay hace scroll-follow sobre ella);
/// mientras hay una, las líneas nuevas NO la mueven de la línea que se
/// estaba mirando (se corre el índice tantas filas como entraron
/// arriba). `↑` desde la primera fila vuelve a "siguiendo".
#[derive(Debug, Clone, Default)]
pub struct EstadoLogsLsp {
    activo: bool,
    lineas: Vec<String>,
    filtro: String,
    /// Total de líneas recibidas por la sesión según la última
    /// actualización (ver `RegistroLogs` en `cliente.rs`): la diferencia
    /// con el total nuevo dice cuántas de las primeras filas son nuevas,
    /// aunque el buffer ya esté lleno y su largo no cambie.
    total: u64,
    /// Índice dentro de `lineas_filtradas()`, o `None` = siguiendo lo
    /// más nuevo (ver doc del struct).
    seleccion: Option<usize>,
}

/// Cuántas filas salta `RePág`/`AvPág` en el visor.
const FILAS_POR_PAGINA: isize = 10;

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

    /// Fila resaltada dentro de `lineas_filtradas()`, o `None` si el
    /// visor está siguiendo lo más nuevo.
    pub fn seleccion(&self) -> Option<usize> {
        self.seleccion
    }

    /// Abre el visor con una copia de `lineas` (orden cronológico, más
    /// vieja primero, tal como las devuelve `EstadoLsp::logs_con_total`)
    /// — el propio método la invierte para mostrar la más reciente
    /// arriba. `total` es el contador de líneas recibidas de la sesión.
    pub fn abrir(&mut self, mut lineas: Vec<String>, total: u64) {
        lineas.reverse();
        self.activo = true;
        self.filtro.clear();
        self.lineas = lineas;
        self.total = total;
        self.seleccion = None;
    }

    /// Refresco en vivo mientras el visor está abierto (mismo formato que
    /// `abrir`). Mantiene el filtro, y la fila resaltada sobre la MISMA
    /// línea de log si el usuario se había movido (ver doc del struct).
    /// Devuelve `true` si cambió algo — `app` usa eso para no redibujar
    /// en cada tick mientras el servidor está callado.
    pub fn actualizar(&mut self, mut lineas: Vec<String>, total: u64) -> bool {
        lineas.reverse();
        if total == self.total && lineas == self.lineas {
            return false;
        }
        // Cuántas de las primeras filas (las más nuevas) no estaban antes.
        // Si el total bajó es otra sesión (el LSP se relanzó, p. ej. al
        // cambiar a un archivo de otro lenguaje): nada de lo anterior
        // sirve como referencia, se vuelve a "siguiendo".
        let nuevas = if total >= self.total { Some((total - self.total) as usize) } else { None };
        self.lineas = lineas;
        self.total = total;

        self.seleccion = match (self.seleccion, nuevas) {
            (Some(actual), Some(nuevas)) if nuevas <= self.lineas.len() => {
                let corrimiento = self.lineas[..nuevas].iter().filter(|l| self.coincidencia(l).is_some()).count();
                Some(actual + corrimiento)
            }
            _ => None,
        };
        self.recortar_seleccion();
        true
    }

    pub fn cerrar(&mut self) {
        self.activo = false;
    }

    /// Cambiar el filtro cambia qué fila es cada índice — se vuelve a
    /// "siguiendo" en vez de dejar resaltada una línea cualquiera.
    pub fn escribir(&mut self, c: char) {
        self.filtro.push(c);
        self.seleccion = None;
    }

    pub fn borrar(&mut self) {
        self.filtro.pop();
        self.seleccion = None;
    }

    /// `↓`: hacia líneas más viejas. Desde "siguiendo" resalta la primera.
    pub fn mover_abajo(&mut self) {
        self.desplazar(1);
    }

    /// `↑`: hacia líneas más nuevas; desde la primera fila vuelve a
    /// "siguiendo".
    pub fn mover_arriba(&mut self) {
        self.desplazar(-1);
    }

    pub fn pagina_abajo(&mut self) {
        self.desplazar(FILAS_POR_PAGINA);
    }

    pub fn pagina_arriba(&mut self) {
        self.desplazar(-FILAS_POR_PAGINA);
    }

    fn desplazar(&mut self, filas: isize) {
        self.seleccion = match self.seleccion {
            None if filas > 0 => Some(filas as usize - 1),
            None => None,
            Some(actual) => {
                let nueva = actual as isize + filas;
                if nueva < 0 { None } else { Some(nueva as usize) }
            }
        };
        self.recortar_seleccion();
    }

    fn recortar_seleccion(&mut self) {
        if let Some(actual) = self.seleccion {
            let visibles = self.lineas_filtradas().len();
            self.seleccion = if visibles == 0 { None } else { Some(actual.min(visibles - 1)) };
        }
    }

    /// `true` si el visor se abrió sin ninguna línea — no había sesión
    /// LSP activa, o la tenía pero nunca escribió nada en stderr (lo más
    /// común: la mayoría de los servidores se quedan callados mientras
    /// todo funciona bien).
    pub fn sin_logs(&self) -> bool {
        self.lineas.is_empty()
    }

    /// Si `linea` pasa el filtro actual: `Some` con las posiciones de
    /// carácter que coincidieron (vacío si no hay filtro), `None` si no.
    fn coincidencia(&self, linea: &str) -> Option<Vec<usize>> {
        if self.filtro.is_empty() {
            return Some(Vec::new());
        }
        let filtro = self.filtro.to_lowercase();
        let minuscula = linea.to_lowercase();
        let inicio_byte = minuscula.find(&filtro)?;
        // Posiciones en CARACTERES, no bytes — coherente con el
        // resto de la app (índices de `char`, no de UTF-8 crudo).
        let inicio_char = minuscula[..inicio_byte].chars().count();
        Some((inicio_char..inicio_char + filtro.chars().count()).collect())
    }

    /// Líneas que coinciden con el filtro (subcadena, sin distinguir
    /// mayúsculas/minúsculas) — todas si el filtro está vacío. Cada una
    /// viene con el rango de posiciones de carácter que coincidieron
    /// (para resaltarlas en negrita, mismo criterio visual que la
    /// coincidencia difusa de la paleta de comandos, aunque acá la
    /// búsqueda sea literal, no aproximada — tiene más sentido para
    /// encontrar un mensaje de error exacto).
    pub fn lineas_filtradas(&self) -> Vec<(&str, Vec<usize>)> {
        self.lineas.iter().filter_map(|linea| Some((linea.as_str(), self.coincidencia(linea)?))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abrir_invierte_el_orden_para_mostrar_lo_mas_nuevo_primero() {
        let mut estado = EstadoLogsLsp::nuevo();
        estado.abrir(vec!["uno".to_string(), "dos".to_string(), "tres".to_string()], 0);
        assert!(estado.activo());
        assert_eq!(estado.lineas_filtradas().iter().map(|(l, _)| *l).collect::<Vec<_>>(), vec!["tres", "dos", "uno"]);
    }

    #[test]
    fn abrir_sin_lineas_marca_sin_logs() {
        let mut estado = EstadoLogsLsp::nuevo();
        estado.abrir(Vec::new(), 0);
        assert!(estado.sin_logs());
    }

    #[test]
    fn abrir_con_lineas_no_esta_sin_logs() {
        let mut estado = EstadoLogsLsp::nuevo();
        estado.abrir(vec!["algo".to_string()], 0);
        assert!(!estado.sin_logs());
    }

    #[test]
    fn cerrar_apaga_el_estado_activo() {
        let mut estado = EstadoLogsLsp::nuevo();
        estado.abrir(vec!["algo".to_string()], 0);
        estado.cerrar();
        assert!(!estado.activo());
    }

    #[test]
    fn filtro_recorta_por_subcadena_sin_distinguir_mayusculas() {
        let mut estado = EstadoLogsLsp::nuevo();
        estado.abrir(vec!["Error: archivo no encontrado".to_string(), "info: todo bien".to_string()], 0);

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
        estado.abrir(vec!["abc error xyz".to_string()], 0);
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
        estado.abrir(vec!["a".to_string(), "b".to_string()], 0);
        let filtradas = estado.lineas_filtradas();
        assert_eq!(filtradas.len(), 2);
        assert!(filtradas.iter().all(|(_, pos)| pos.is_empty()));
    }

    #[test]
    fn borrar_quita_el_ultimo_caracter_del_filtro() {
        let mut estado = EstadoLogsLsp::nuevo();
        estado.abrir(vec!["algo".to_string()], 0);
        estado.escribir('a');
        estado.escribir('b');
        estado.borrar();
        assert_eq!(estado.filtro(), "a");
    }

    fn lineas(estado: &EstadoLogsLsp) -> Vec<&str> {
        estado.lineas_filtradas().iter().map(|(l, _)| *l).collect()
    }

    fn cronologicas(desde: usize, hasta: usize) -> Vec<String> {
        (desde..hasta).map(|i| format!("linea {i}")).collect()
    }

    #[test]
    fn actualizar_sin_cambios_no_pide_redibujar() {
        let mut estado = EstadoLogsLsp::nuevo();
        estado.abrir(cronologicas(0, 3), 3);
        assert!(!estado.actualizar(cronologicas(0, 3), 3));
    }

    #[test]
    fn actualizar_agrega_lo_nuevo_arriba_y_mantiene_el_filtro() {
        let mut estado = EstadoLogsLsp::nuevo();
        estado.abrir(vec!["error viejo".to_string(), "info".to_string()], 2);
        estado.escribir('e');
        estado.escribir('r');
        let nuevas = vec!["error viejo".to_string(), "info".to_string(), "error nuevo".to_string()];
        assert!(estado.actualizar(nuevas, 3));
        assert_eq!(estado.filtro(), "er");
        assert_eq!(lineas(&estado), vec!["error nuevo", "error viejo"]);
    }

    #[test]
    fn siguiendo_lo_mas_nuevo_sigue_asi_al_llegar_lineas() {
        let mut estado = EstadoLogsLsp::nuevo();
        estado.abrir(cronologicas(0, 3), 3);
        estado.actualizar(cronologicas(0, 5), 5);
        assert_eq!(estado.seleccion(), None);
        assert_eq!(lineas(&estado)[0], "linea 4");
    }

    #[test]
    fn con_una_fila_resaltada_las_lineas_nuevas_no_la_mueven_de_su_linea() {
        let mut estado = EstadoLogsLsp::nuevo();
        estado.abrir(cronologicas(0, 5), 5);
        estado.mover_abajo();
        estado.mover_abajo(); // resaltada: "linea 3"
        assert_eq!(lineas(&estado)[estado.seleccion().unwrap()], "linea 3");

        estado.actualizar(cronologicas(0, 7), 7);
        assert_eq!(lineas(&estado)[estado.seleccion().unwrap()], "linea 3");
    }

    #[test]
    fn con_el_buffer_lleno_el_total_dice_cuantas_son_nuevas() {
        // El largo no cambia (buffer al tope, las viejas se descartan),
        // pero el total sí — la fila resaltada igual se corre bien.
        let mut estado = EstadoLogsLsp::nuevo();
        estado.abrir(cronologicas(0, 5), 5);
        estado.mover_abajo(); // "linea 4"
        estado.actualizar(cronologicas(2, 7), 7);
        assert_eq!(lineas(&estado)[estado.seleccion().unwrap()], "linea 4");
    }

    #[test]
    fn el_corrimiento_solo_cuenta_las_lineas_nuevas_que_pasan_el_filtro() {
        let mut estado = EstadoLogsLsp::nuevo();
        estado.abrir(vec!["error a".to_string(), "error b".to_string()], 2);
        estado.escribir('e');
        estado.mover_abajo();
        estado.mover_abajo(); // "error a"
        let nuevas = vec!["error a".to_string(), "error b".to_string(), "info".to_string(), "error c".to_string()];
        estado.actualizar(nuevas, 4);
        assert_eq!(lineas(&estado)[estado.seleccion().unwrap()], "error a");
    }

    #[test]
    fn otra_sesion_vuelve_a_siguiendo() {
        let mut estado = EstadoLogsLsp::nuevo();
        estado.abrir(cronologicas(0, 5), 5);
        estado.mover_abajo();
        estado.actualizar(vec!["otra sesión".to_string()], 1);
        assert_eq!(estado.seleccion(), None);
    }

    #[test]
    fn subir_desde_la_primera_fila_vuelve_a_siguiendo_y_bajar_se_recorta() {
        let mut estado = EstadoLogsLsp::nuevo();
        estado.abrir(cronologicas(0, 3), 3);
        estado.mover_abajo();
        assert_eq!(estado.seleccion(), Some(0));
        estado.mover_arriba();
        assert_eq!(estado.seleccion(), None);
        estado.pagina_abajo();
        assert_eq!(estado.seleccion(), Some(2));
        estado.escribir('x');
        assert_eq!(estado.seleccion(), None, "cambiar el filtro vuelve a siguiendo");
    }
}
