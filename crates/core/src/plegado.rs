//! Estado de plegado de bloques de un documento (BACKLOG.md P2 #7,
//! PLAN.md §4 "Plegado"): qué rangos de líneas están plegados. No sabe
//! nada de tree-sitter — qué rangos SE PUEDEN plegar lo calcula
//! `tcode-syntax` y `app` se los pasa a [`Editor`](crate::Editor), que es
//! dueño de este estado (uno por panel, igual que el cursor) porque es el
//! único que ve cada edición y cada movimiento del cursor.

use std::ops::Range;

/// Un bloque plegado: la línea `inicio` (la "cabecera", p. ej. `fn f() {`)
/// sigue visible con un marcador de plegado; las líneas `inicio + 1 ..=
/// fin` no se dibujan. Siempre `fin > inicio`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pliegue {
    pub inicio: usize,
    pub fin: usize,
}

impl Pliegue {
    /// Las líneas que oculta este pliegue (sin la cabecera).
    pub fn ocultas(&self) -> Range<usize> {
        self.inicio + 1..self.fin + 1
    }

    fn contiene(&self, linea: usize) -> bool {
        self.inicio <= linea && linea <= self.fin
    }
}

/// Los pliegues activos de un documento, ordenados por `inicio` (y, a
/// igual `inicio`, del más grande al más chico). Pueden anidarse (plegar
/// una función y después la clase que la contiene): al desplegar la de
/// afuera, la de adentro sigue plegada, igual que en VSCode.
#[derive(Debug, Clone, Default)]
pub struct Plegado {
    pliegues: Vec<Pliegue>,
}

impl Plegado {
    pub fn pliegues(&self) -> &[Pliegue] {
        &self.pliegues
    }

    pub fn esta_vacio(&self) -> bool {
        self.pliegues.is_empty()
    }

    /// Agrega `pliegue` (no hace nada si ya estaba o si no oculta ninguna
    /// línea). Devuelve si cambió algo.
    pub fn plegar(&mut self, pliegue: Pliegue) -> bool {
        if pliegue.fin <= pliegue.inicio || self.pliegues.contains(&pliegue) {
            return false;
        }
        let pos = self
            .pliegues
            .partition_point(|p| (p.inicio, std::cmp::Reverse(p.fin)) < (pliegue.inicio, std::cmp::Reverse(pliegue.fin)));
        self.pliegues.insert(pos, pliegue);
        true
    }

    pub fn desplegar_todo(&mut self) {
        self.pliegues.clear();
    }

    /// Quita el pliegue "del cursor": los que tienen su cabecera en
    /// `linea` o, si no hay ninguno, el más interno que la contiene.
    /// Devuelve si cambió algo.
    pub fn desplegar_en(&mut self, linea: usize) -> bool {
        let antes = self.pliegues.len();
        self.pliegues.retain(|p| p.inicio != linea);
        if self.pliegues.len() != antes {
            return true;
        }
        let interno = self.pliegues.iter().enumerate().filter(|(_, p)| p.contiene(linea)).max_by_key(|(_, p)| p.inicio);
        if let Some((i, _)) = interno {
            self.pliegues.remove(i);
            return true;
        }
        false
    }

    /// Quita todos los pliegues que ocultan `linea` (una búsqueda o un
    /// salto que cae adentro de un bloque plegado lo despliega, igual que
    /// en VSCode). Devuelve si cambió algo.
    pub fn revelar(&mut self, linea: usize) -> bool {
        let antes = self.pliegues.len();
        self.pliegues.retain(|p| !p.ocultas().contains(&linea));
        self.pliegues.len() != antes
    }

    /// Unión de las líneas ocultas por todos los pliegues, como tramos
    /// ordenados, sin solaparse y sin tocarse. Lineal en la cantidad de
    /// pliegues (ya vienen ordenados por `inicio`) — barato de recalcular
    /// en cada frame.
    pub fn tramos_ocultos(&self) -> Vec<Range<usize>> {
        let mut tramos: Vec<Range<usize>> = Vec::new();
        for p in &self.pliegues {
            let ocultas = p.ocultas();
            match tramos.last_mut() {
                Some(ultimo) if ocultas.start <= ultimo.end => ultimo.end = ultimo.end.max(ocultas.end),
                _ => tramos.push(ocultas),
            }
        }
        tramos
    }

    pub fn linea_oculta(&self, linea: usize) -> bool {
        self.pliegues.iter().any(|p| p.ocultas().contains(&linea))
    }

    /// Ajusta los pliegues a una edición que reemplazó el texto entre las
    /// líneas `inicio..=fin_viejo` por otro que ahora ocupa
    /// `inicio..=fin_nuevo`. Criterio simple y seguro (nunca plegar
    /// líneas equivocadas): un pliegue que la edición toca se despliega;
    /// uno que queda entero después se desplaza con las líneas agregadas
    /// o quitadas; uno que queda entero antes no cambia. Única excepción:
    /// editar DENTRO de la cabecera sin agregar ni quitar líneas (p. ej.
    /// renombrar la función plegada) no despliega nada.
    ///
    /// `toca_fin` es `false` cuando la edición termina en la columna 0 de
    /// `fin_viejo` sin cambiar su contenido (un `Enter` al principio de
    /// la línea, pegar o borrar líneas enteras justo antes): esa línea
    /// solo se corre, así que un pliegue con cabecera ahí se desplaza en
    /// vez de desplegarse.
    pub fn ajustar_por_edicion(&mut self, inicio: usize, fin_viejo: usize, fin_nuevo: usize, toca_fin: bool) {
        let delta = fin_nuevo as isize - fin_viejo as isize;
        self.pliegues.retain_mut(|p| {
            if inicio > p.fin {
                return true;
            }
            if fin_viejo < p.inicio || (!toca_fin && fin_viejo == p.inicio) {
                p.inicio = (p.inicio as isize + delta) as usize;
                p.fin = (p.fin as isize + delta) as usize;
                return true;
            }
            inicio == p.inicio && fin_viejo == p.inicio && delta == 0
        });
        self.pliegues.dedup();
    }

    /// Ajusta los pliegues a un intercambio de dos tramos de líneas
    /// contiguos (`primero.end == segundo.start`): mover líneas arriba o
    /// abajo (`Alt+↑`/`Alt+↓`, BACKLOG.md P0 #19). A diferencia de
    /// `ajustar_por_edicion`, que despliega todo lo que la edición toca,
    /// acá el contenido de cada tramo no cambia, solo se corre: un pliegue
    /// que cae entero dentro de uno de los dos tramos viaja con él (mover
    /// una función plegada la deja plegada). Uno que cruza el borde entre
    /// los tramos, o uno de sus extremos, se despliega; uno que queda
    /// entero afuera, o que los contiene a los dos, no cambia.
    pub fn intercambiar(&mut self, primero: Range<usize>, segundo: Range<usize>) {
        debug_assert_eq!(primero.end, segundo.start);
        let (largo_primero, largo_segundo) = (primero.len(), segundo.len());
        self.pliegues.retain_mut(|p| {
            let adentro = |tramo: &Range<usize>| tramo.start <= p.inicio && p.fin < tramo.end;
            if p.fin < primero.start || p.inicio >= segundo.end || (p.inicio < primero.start && p.fin + 1 >= segundo.end)
            {
                return true;
            }
            if adentro(&primero) {
                p.inicio += largo_segundo;
                p.fin += largo_segundo;
                return true;
            }
            if adentro(&segundo) {
                p.inicio -= largo_primero;
                p.fin -= largo_primero;
                return true;
            }
            false
        });
        self.pliegues.sort_by_key(|p| (p.inicio, std::cmp::Reverse(p.fin)));
    }

    /// Descarta los pliegues que se salen de un documento de
    /// `num_lineas` líneas (por las dudas, tras deshacer/rehacer).
    pub fn recortar(&mut self, num_lineas: usize) {
        self.pliegues.retain(|p| p.fin < num_lineas && p.fin > p.inicio);
    }
}

/// Si `linea` está dentro de uno de `tramos` (ver
/// [`Plegado::tramos_ocultos`]), ese tramo.
pub fn tramo_que_oculta(tramos: &[Range<usize>], linea: usize) -> Option<Range<usize>> {
    let i = tramos.partition_point(|t| t.end <= linea);
    tramos.get(i).filter(|t| t.start <= linea).cloned()
}

/// Posición de `linea` entre las líneas VISIBLES (sin contar las ocultas
/// por `tramos` que vienen antes) — el espacio de coordenadas del scroll
/// de la vista de código. Sin pliegues coincide con `linea`. Si `linea`
/// está oculta, da la posición de la línea visible anterior + 1.
pub fn ordinal_visible(tramos: &[Range<usize>], linea: usize) -> usize {
    let ocultas_antes: usize = tramos.iter().take_while(|t| t.start < linea).map(|t| t.end.min(linea) - t.start).sum();
    linea - ocultas_antes
}

/// Inversa de [`ordinal_visible`]: la línea visible número `ordinal`.
pub fn linea_de_ordinal(tramos: &[Range<usize>], ordinal: usize) -> usize {
    let mut linea = ordinal;
    for t in tramos {
        if t.start > linea {
            break;
        }
        linea += t.len();
    }
    linea
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(inicio: usize, fin: usize) -> Pliegue {
        Pliegue { inicio, fin }
    }

    #[test]
    fn tramos_ocultos_une_pliegues_anidados_y_contiguos() {
        let mut plegado = Plegado::default();
        plegado.plegar(p(2, 10));
        plegado.plegar(p(4, 6));
        plegado.plegar(p(10, 12));
        plegado.plegar(p(20, 22));
        assert_eq!(plegado.tramos_ocultos(), vec![3..13, 21..23]);
    }

    #[test]
    fn no_pliega_rangos_vacios_ni_repetidos() {
        let mut plegado = Plegado::default();
        assert!(!plegado.plegar(p(3, 3)));
        assert!(plegado.plegar(p(3, 5)));
        assert!(!plegado.plegar(p(3, 5)));
        assert_eq!(plegado.pliegues().len(), 1);
    }

    #[test]
    fn ordinal_y_linea_de_ordinal_son_inversas_en_las_lineas_visibles() {
        let tramos = vec![3..6, 10..12];
        let visibles: Vec<usize> = (0..15).filter(|l| tramo_que_oculta(&tramos, *l).is_none()).collect();
        for (ordinal, linea) in visibles.iter().enumerate() {
            assert_eq!(ordinal_visible(&tramos, *linea), ordinal);
            assert_eq!(linea_de_ordinal(&tramos, ordinal), *linea);
        }
        assert_eq!(ordinal_visible(&[], 7), 7);
        assert_eq!(linea_de_ordinal(&[], 7), 7);
    }

    #[test]
    fn desplegar_en_prefiere_la_cabecera_y_si_no_el_mas_interno() {
        let mut plegado = Plegado::default();
        plegado.plegar(p(0, 20));
        plegado.plegar(p(5, 8));
        assert!(plegado.desplegar_en(5));
        assert_eq!(plegado.pliegues(), &[p(0, 20)]);
        plegado.plegar(p(5, 8));
        // Línea 7: adentro de los dos, se va el más interno.
        assert!(plegado.desplegar_en(7));
        assert_eq!(plegado.pliegues(), &[p(0, 20)]);
        assert!(!plegado.desplegar_en(30));
    }

    #[test]
    fn revelar_quita_solo_los_pliegues_que_ocultan_la_linea() {
        let mut plegado = Plegado::default();
        plegado.plegar(p(0, 20));
        plegado.plegar(p(5, 8));
        plegado.plegar(p(30, 40));
        assert!(plegado.revelar(6));
        assert_eq!(plegado.pliegues(), &[p(30, 40)]);
        // La cabecera no está oculta: no despliega nada.
        assert!(!plegado.revelar(30));
    }

    #[test]
    fn una_edicion_antes_desplaza_y_una_adentro_despliega() {
        let mut plegado = Plegado::default();
        plegado.plegar(p(10, 15));
        plegado.plegar(p(20, 25));
        // Enter en la línea 2: una línea más antes de ambos.
        plegado.ajustar_por_edicion(2, 2, 3, true);
        assert_eq!(plegado.pliegues(), &[p(11, 16), p(21, 26)]);
        // Borrar una línea dentro del primero: se despliega, el segundo
        // se desplaza.
        plegado.ajustar_por_edicion(12, 13, 12, true);
        assert_eq!(plegado.pliegues(), &[p(20, 25)]);
        // Escribir en la cabecera sin cambiar de líneas: sigue plegado.
        plegado.ajustar_por_edicion(20, 20, 20, true);
        assert_eq!(plegado.pliegues(), &[p(20, 25)]);
        // Enter al principio de la cabecera: solo la corre.
        plegado.ajustar_por_edicion(20, 20, 21, false);
        assert_eq!(plegado.pliegues(), &[p(21, 26)]);
        // Enter en medio de la cabecera: se despliega.
        plegado.ajustar_por_edicion(21, 21, 22, true);
        assert!(plegado.esta_vacio());
    }

    #[test]
    fn una_edicion_despues_no_cambia_nada() {
        let mut plegado = Plegado::default();
        plegado.plegar(p(10, 15));
        plegado.ajustar_por_edicion(16, 18, 30, true);
        assert_eq!(plegado.pliegues(), &[p(10, 15)]);
    }

    #[test]
    fn intercambiar_mueve_los_de_adentro_y_despliega_los_que_cruzan() {
        let mut plegado = Plegado::default();
        plegado.plegar(p(0, 20)); // contiene a los dos tramos: queda
        plegado.plegar(p(2, 3)); // dentro del primero (2..5)
        plegado.plegar(p(6, 8)); // dentro del segundo (5..9)
        plegado.plegar(p(4, 6)); // cruza el borde: se despliega
        plegado.plegar(p(12, 14)); // afuera: queda igual
        plegado.intercambiar(2..5, 5..9);
        assert_eq!(plegado.pliegues(), &[p(0, 20), p(3, 5), p(6, 7), p(12, 14)]);
    }
}
