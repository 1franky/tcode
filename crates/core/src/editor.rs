use std::ops::Range;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::buffer::Buffer;
use crate::busqueda::{buscar_coincidencias, OpcionesBusqueda};
use crate::cursor::{Cursor, CursorMultiple};
use crate::history::Historia;

/// Modo de edición actual. `tcode` es no-modal por defecto (PLAN.md §4): en
/// M0 solo existe `Insertar`. `Comando` se activa con la paleta de comandos
/// (M2); la selección/multi-cursor (M3) no necesitó un modo propio — es
/// simplemente más de un [`CursorMultiple`] en `Editor::cursores`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modo {
    Insertar,
}

/// Estado de edición de un archivo: buffer + cursor(es) + historial de
/// deshacer/rehacer, combinados en las operaciones que un editor expone
/// (insertar, borrar, mover el cursor, guardar...).
///
/// No sabe nada de terminal ni de ratatui — eso vive en el crate `ui`, que
/// observa este estado para dibujarlo (principio arquitectónico de
/// PLAN.md §3: "el core no conoce nada de UI").
pub struct Editor {
    buffer: Buffer,
    /// Invariante: nunca vacío. `cursores[0]` es el "principal" — el que
    /// se muestra en la statusbar y sobrevive a `colapsar_cursores`
    /// (`Esc`, PLAN.md §11 M3). El resto son cursores adicionales, cada
    /// uno con su propia selección opcional.
    cursores: Vec<CursorMultiple>,
    historia: Historia,
    modo: Modo,
}

impl Editor {
    pub fn nuevo() -> Self {
        Self {
            buffer: Buffer::nuevo(),
            cursores: vec![CursorMultiple::sin_seleccion(Cursor::nuevo())],
            historia: Historia::nueva(),
            modo: Modo::Insertar,
        }
    }

    pub fn abrir(ruta: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            buffer: Buffer::desde_archivo(ruta)?,
            cursores: vec![CursorMultiple::sin_seleccion(Cursor::nuevo())],
            historia: Historia::nueva(),
            modo: Modo::Insertar,
        })
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    /// Posición del cursor principal (`cursores[0]`) — lo que muestra la
    /// statusbar (`Ln`/`Col`) y lo que usan las funciones de una sola
    /// posición (búsqueda, ir a una línea...).
    pub fn cursor(&self) -> Cursor {
        self.cursores[0].cursor
    }

    /// Todos los cursores activos, cada uno con su selección opcional
    /// (`Ctrl+D`/`Ctrl+Shift+L`/`Ctrl+Alt+↑↓`, PLAN.md §11 M3) — lo que
    /// necesita `tcode-ui` para dibujar selecciones y los cursores
    /// secundarios.
    pub fn cursores(&self) -> &[CursorMultiple] {
        &self.cursores
    }

    pub fn tiene_multiples_cursores(&self) -> bool {
        self.cursores.len() > 1
    }

    pub fn modo(&self) -> Modo {
        self.modo
    }

    pub fn guardar(&mut self) -> Result<()> {
        self.buffer.guardar()
    }

    pub fn guardar_como(&mut self, ruta: impl Into<PathBuf>) -> Result<()> {
        self.buffer.guardar_como(ruta)
    }

    fn registrar_snapshot(&mut self) {
        self.historia.registrar(self.buffer.rope(), &self.cursores);
    }

    /// Rango de bytes `[inicio, fin)` (con `inicio <= fin`) que ocupa la
    /// selección del cursor en `indice` — vacío (`inicio == fin`) si ese
    /// cursor no tiene selección.
    fn rango_bytes(&self, indice: usize) -> Range<usize> {
        let c = self.cursores[indice];
        let a = self.buffer.offset_byte(c.ancla.linea, c.ancla.columna);
        let b = self.buffer.offset_byte(c.cursor.linea, c.cursor.columna);
        a.min(b)..a.max(b)
    }

    /// Texto exactamente cubierto por la selección de `cursor` (cadena
    /// vacía si no tiene selección).
    fn texto_seleccionado(&self, cursor: CursorMultiple) -> String {
        let a = self.buffer.offset_byte(cursor.ancla.linea, cursor.ancla.columna);
        let b = self.buffer.offset_byte(cursor.cursor.linea, cursor.cursor.columna);
        let (inicio, fin) = (a.min(b), a.max(b));
        let texto = self.buffer.a_texto();
        texto.get(inicio..fin).unwrap_or_default().to_string()
    }

    /// Aplica una edición de texto en la posición (o selección) de CADA
    /// cursor a la vez — la base de `insertar_char`/`borrar_atras`/
    /// `borrar_adelante` con multi-cursor (PLAN.md §11 M3).
    ///
    /// Se procesa de atrás hacia adelante en el texto (el cursor con
    /// mayor offset de bytes primero) para que editar uno no invalide los
    /// offsets de bytes de los que faltan por procesar — mismo patrón que
    /// usa la búsqueda/reemplazo para "reemplazar todo". Pero eso solo
    /// resuelve la mitad del problema: una vez fijada la posición final
    /// de un cursor ya procesado, una edición *anterior* en el texto
    /// (procesada después, por estar más atrás) puede desplazarla igual
    /// si cae en la misma línea — por eso se llevan los offsets ya
    /// resueltos como absolutos y se corrigen con el delta de cada
    /// edición siguiente, convirtiendo a línea/columna recién al final.
    ///
    /// `calcular` recibe el rango de bytes seleccionado por ese cursor
    /// (vacío si no tiene selección) y devuelve el rango real a
    /// reemplazar (puede ampliarlo, p. ej. `borrar_atras` sin selección
    /// necesita un carácter más atrás) junto con el texto de reemplazo;
    /// ese cursor queda posicionado justo después del texto insertado,
    /// sin selección.
    fn editar_cada_cursor(&mut self, calcular: impl Fn(&Buffer, Range<usize>) -> (Range<usize>, String)) {
        self.registrar_snapshot();

        let mut indices: Vec<usize> = (0..self.cursores.len()).collect();
        indices.sort_by_key(|&i| std::cmp::Reverse(self.rango_bytes(i).start));

        let mut offsets_finales: Vec<(usize, usize)> = Vec::with_capacity(indices.len());

        for i in indices {
            let seleccion = self.rango_bytes(i);
            let (rango, reemplazo) = calcular(&self.buffer, seleccion);
            self.buffer.reemplazar_rango_bytes(rango.start, rango.end, &reemplazo);

            // Toda edición ya aplicada quedó, por construcción (se
            // procesa de mayor a menor offset original y las selecciones
            // nunca se superponen), estrictamente DESPUÉS de esta en el
            // texto — hay que desplazar sus offsets ya calculados.
            let delta = reemplazo.len() as isize - (rango.end - rango.start) as isize;
            for (_, offset) in &mut offsets_finales {
                *offset = (*offset as isize + delta) as usize;
            }
            offsets_finales.push((i, rango.start + reemplazo.len()));
        }

        for (i, offset) in offsets_finales {
            let (linea, columna) = self.buffer.linea_columna_desde_byte(offset);
            self.cursores[i] = CursorMultiple::sin_seleccion(Cursor { linea, columna });
        }

        self.fusionar_cursores_duplicados();
    }

    /// Quita cursores que terminaron en la misma posición tras una
    /// edición o un movimiento (p. ej. dos cursores que se movían hacia
    /// la misma línea llegaron a la misma columna) — sin esto, seguir
    /// editando duplicaría cada carácter una vez por cursor superpuesto.
    fn fusionar_cursores_duplicados(&mut self) {
        self.cursores.sort_by_key(|c| (c.cursor.linea, c.cursor.columna));
        self.cursores.dedup_by_key(|c| c.cursor);
    }

    /// Inserta un carácter en la posición de CADA cursor (reemplazando su
    /// selección si tenía una) y avanza cada uno. `'\n'` inserta un salto
    /// de línea real (usado también por `Enter`).
    pub fn insertar_char(&mut self, c: char) {
        let texto = c.to_string();
        self.editar_cada_cursor(|_, seleccion| (seleccion, texto.clone()));
    }

    /// Backspace: si el cursor tiene selección la borra; si no, borra
    /// hacia atrás un carácter (fusionando con la línea anterior si
    /// estaba al inicio de línea) — para cada cursor a la vez.
    pub fn borrar_atras(&mut self) {
        self.editar_cada_cursor(|buffer, seleccion| {
            if !seleccion.is_empty() || seleccion.start == 0 {
                return (seleccion, String::new());
            }
            // `saturating_sub(1)` puede caer a mitad de un carácter
            // multibyte; se recalcula el offset real desde línea/columna
            // resuelta para no partir un carácter UTF-8.
            let (linea, columna) = buffer.linea_columna_desde_byte(seleccion.start.saturating_sub(1));
            let inicio_real = buffer.offset_byte(linea, columna);
            (inicio_real..seleccion.end, String::new())
        });
    }

    /// Delete: si el cursor tiene selección la borra; si no, borra el
    /// carácter bajo/después del cursor sin moverlo — para cada cursor a
    /// la vez.
    pub fn borrar_adelante(&mut self) {
        self.editar_cada_cursor(|buffer, seleccion| {
            if !seleccion.is_empty() {
                return (seleccion, String::new());
            }
            // Avanza una posición de carácter con el propio
            // `Cursor::mover_derecha` (respeta fin de línea/archivo, y
            // con multibyte UTF-8 avanza un carácter completo, no un
            // byte) para encontrar el final del carácter a borrar.
            let inicio = buffer.linea_columna_desde_byte(seleccion.start);
            let mut fin = Cursor { linea: inicio.0, columna: inicio.1 };
            fin.mover_derecha(buffer);
            let fin_byte = buffer.offset_byte(fin.linea, fin.columna);
            (seleccion.start..fin_byte, String::new())
        });
    }

    /// Mueve TODOS los cursores en la dirección dada, cada uno de forma
    /// independiente, y colapsa cualquier selección que tuvieran (mover
    /// sin `Shift` siempre deja de seleccionar, igual que en cualquier
    /// editor).
    fn mover_cada_cursor(&mut self, f: impl Fn(&mut Cursor, &Buffer)) {
        for c in &mut self.cursores {
            f(&mut c.cursor, &self.buffer);
            c.ancla = c.cursor;
        }
        self.fusionar_cursores_duplicados();
    }

    pub fn mover_izquierda(&mut self) {
        self.mover_cada_cursor(Cursor::mover_izquierda);
    }

    pub fn mover_derecha(&mut self) {
        self.mover_cada_cursor(Cursor::mover_derecha);
    }

    pub fn mover_arriba(&mut self) {
        self.mover_cada_cursor(Cursor::mover_arriba);
    }

    pub fn mover_abajo(&mut self) {
        self.mover_cada_cursor(Cursor::mover_abajo);
    }

    pub fn inicio_linea(&mut self) {
        self.mover_cada_cursor(|c, _| c.inicio_linea());
    }

    pub fn fin_linea(&mut self) {
        self.mover_cada_cursor(Cursor::fin_linea);
    }

    pub fn inicio_archivo(&mut self) {
        self.mover_cada_cursor(|c, _| c.inicio_archivo());
    }

    pub fn fin_archivo(&mut self) {
        self.mover_cada_cursor(Cursor::fin_archivo);
    }

    /// Mueve el cursor principal a la posición del offset de bytes
    /// `offset_byte` (usado para saltar a una coincidencia de búsqueda,
    /// PLAN.md §4), colapsando cualquier cursor/selección adicional —
    /// abrir la búsqueda con varios cursores activos vuelve a uno solo,
    /// igual que en cualquier editor.
    pub fn mover_cursor_a_byte(&mut self, offset_byte: usize) {
        let (linea, columna) = self.buffer.linea_columna_desde_byte(offset_byte);
        self.cursores = vec![CursorMultiple::sin_seleccion(Cursor { linea, columna })];
    }

    /// Reemplaza el texto en el rango de bytes `[inicio, fin)` por
    /// `reemplazo` (PLAN.md §4, "buscar.reemplazar") y deja el cursor
    /// principal justo después del texto insertado, colapsando cualquier
    /// cursor/selección adicional (ver `mover_cursor_a_byte`).
    pub fn reemplazar_rango_bytes(&mut self, inicio_byte: usize, fin_byte: usize, reemplazo: &str) {
        self.registrar_snapshot();
        self.buffer.reemplazar_rango_bytes(inicio_byte, fin_byte, reemplazo);
        self.mover_cursor_a_byte(inicio_byte + reemplazo.len());
    }

    /// `Ctrl+D` (PLAN.md §11 M3): si el cursor principal no tiene
    /// selección, selecciona la palabra bajo ese cursor (sin agregar
    /// ninguno nuevo todavía — la primera pulsación solo marca la
    /// referencia). Si ya tiene una selección, agrega un cursor nuevo en
    /// la siguiente ocurrencia de ese mismo texto después del último
    /// cursor existente (dando la vuelta al principio del archivo si
    /// hace falta), sin repetir una ocurrencia ya seleccionada.
    pub fn seleccionar_siguiente_ocurrencia(&mut self) {
        let principal = self.cursores[0];
        if !principal.tiene_seleccion() {
            if let Some(seleccion) = self.seleccion_de_palabra(principal.cursor) {
                self.cursores[0] = seleccion;
            }
            return;
        }

        let texto = self.texto_seleccionado(principal);
        if texto.is_empty() {
            return;
        }

        let Some(nueva) = self.siguiente_ocurrencia_no_seleccionada(&texto) else { return };
        self.cursores.push(nueva);
    }

    /// `Ctrl+Shift+L` (PLAN.md §11 M3): selecciona TODAS las ocurrencias
    /// del texto de referencia (la selección del cursor principal, o la
    /// palabra bajo ese cursor si no había ninguna) de una sola vez —
    /// reemplaza el conjunto de cursores actual en vez de ir agregando de
    /// a una.
    pub fn seleccionar_todas_ocurrencias(&mut self) {
        let principal = self.cursores[0];
        let referencia = if principal.tiene_seleccion() {
            principal
        } else if let Some(seleccion) = self.seleccion_de_palabra(principal.cursor) {
            seleccion
        } else {
            return;
        };

        let texto = self.texto_seleccionado(referencia);
        if texto.is_empty() {
            return;
        }

        let coincidencias = self.buscar_texto_literal(&texto);
        if coincidencias.is_empty() {
            return;
        }

        self.cursores = coincidencias
            .into_iter()
            .map(|(inicio, fin)| {
                let ancla = self.buffer.linea_columna_desde_byte(inicio);
                let cursor = self.buffer.linea_columna_desde_byte(fin);
                CursorMultiple {
                    ancla: Cursor { linea: ancla.0, columna: ancla.1 },
                    cursor: Cursor { linea: cursor.0, columna: cursor.1 },
                }
            })
            .collect();
    }

    /// `Ctrl+Alt+↑` (PLAN.md §11 M3): agrega, por cada cursor existente,
    /// uno nuevo (sin selección) una línea arriba, en la misma columna
    /// (recortada a esa línea) — no hace nada para los cursores que ya
    /// están en la primera línea.
    pub fn agregar_cursor_arriba(&mut self) {
        self.agregar_cursor_vertical(-1);
    }

    /// `Ctrl+Alt+↓`: igual que `agregar_cursor_arriba` pero una línea
    /// abajo.
    pub fn agregar_cursor_abajo(&mut self) {
        self.agregar_cursor_vertical(1);
    }

    fn agregar_cursor_vertical(&mut self, delta: isize) {
        let mut nuevos = Vec::new();
        for c in &self.cursores {
            let Some(nueva_linea) = c.cursor.linea.checked_add_signed(delta) else { continue };
            if nueva_linea >= self.buffer.num_lineas() {
                continue;
            }
            let columna = c.cursor.columna.min(self.buffer.longitud_visible_linea(nueva_linea));
            let candidato = CursorMultiple::sin_seleccion(Cursor { linea: nueva_linea, columna });
            if !self.cursores.contains(&candidato) && !nuevos.contains(&candidato) {
                nuevos.push(candidato);
            }
        }
        self.cursores.extend(nuevos);
        self.fusionar_cursores_duplicados();
    }

    /// `Esc` (PLAN.md §11 M3, `cursor.una_seleccion`): vuelve a un solo
    /// cursor, el principal, sin selección. No hace nada si ya había uno
    /// solo (para que sea seguro llamarlo siempre desde `Esc`, tenga o no
    /// varios cursores activos).
    pub fn colapsar_cursores(&mut self) {
        self.cursores.truncate(1);
        self.cursores[0].ancla = self.cursores[0].cursor;
    }

    /// La palabra (secuencia de alfanuméricos/`_`) que cubre `cursor`,
    /// como una selección — `None` si esa posición no cae ni dentro ni
    /// justo después de ninguna palabra (está sobre un espacio, símbolo,
    /// o línea vacía).
    fn seleccion_de_palabra(&self, cursor: Cursor) -> Option<CursorMultiple> {
        let lineas = self.buffer.lineas_texto();
        let caracteres: Vec<char> = lineas.get(cursor.linea)?.chars().collect();
        let (inicio, fin) = limites_palabra(&caracteres, cursor.columna)?;
        Some(CursorMultiple {
            ancla: Cursor { linea: cursor.linea, columna: inicio },
            cursor: Cursor { linea: cursor.linea, columna: fin },
        })
    }

    /// Todas las coincidencias literales (sensibles a mayúsculas) de
    /// `texto` en el documento, como rangos de bytes `(inicio, fin)`.
    fn buscar_texto_literal(&self, texto: &str) -> Vec<(usize, usize)> {
        let opciones = OpcionesBusqueda { sensible_mayusculas: true, ..Default::default() };
        buscar_coincidencias(&self.buffer.a_texto(), texto, opciones)
            .map(|cs| cs.into_iter().map(|c| (c.inicio, c.fin)).collect())
            .unwrap_or_default()
    }

    /// La primera coincidencia de `texto` que no coincide ya con ninguno
    /// de los cursores actuales, buscando después del final del último
    /// cursor (por orden de aparición en el documento) y dando la vuelta
    /// al principio si hace falta.
    fn siguiente_ocurrencia_no_seleccionada(&self, texto: &str) -> Option<CursorMultiple> {
        let coincidencias = self.buscar_texto_literal(texto);
        if coincidencias.is_empty() {
            return None;
        }

        let ya_seleccionadas: Vec<(usize, usize)> = (0..self.cursores.len())
            .map(|i| {
                let r = self.rango_bytes(i);
                (r.start, r.end)
            })
            .collect();
        let fin_ultimo = ya_seleccionadas.iter().map(|(_, fin)| *fin).max().unwrap_or(0);

        let candidata = coincidencias
            .iter()
            .find(|c| c.0 >= fin_ultimo && !ya_seleccionadas.contains(c))
            .or_else(|| coincidencias.iter().find(|c| !ya_seleccionadas.contains(c)))?;

        let ancla = self.buffer.linea_columna_desde_byte(candidata.0);
        let cursor = self.buffer.linea_columna_desde_byte(candidata.1);
        Some(CursorMultiple {
            ancla: Cursor { linea: ancla.0, columna: ancla.1 },
            cursor: Cursor { linea: cursor.0, columna: cursor.1 },
        })
    }

    pub fn deshacer(&mut self) {
        if let Some((rope, mut cursores)) = self.historia.deshacer(self.buffer.rope(), &self.cursores) {
            self.buffer.reemplazar_rope(rope);
            for c in &mut cursores {
                c.recortar(&self.buffer);
            }
            self.cursores = cursores;
        }
    }

    pub fn rehacer(&mut self) {
        if let Some((rope, mut cursores)) = self.historia.rehacer(self.buffer.rope(), &self.cursores) {
            self.buffer.reemplazar_rope(rope);
            for c in &mut cursores {
                c.recortar(&self.buffer);
            }
            self.cursores = cursores;
        }
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::nuevo()
    }
}

/// `true` si `c` cuenta como parte de una "palabra" para
/// `seleccion_de_palabra`/`Ctrl+D` — alfanumérico o guión bajo, la misma
/// convención que la mayoría de los editores.
fn es_caracter_palabra(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Límites `[inicio, fin)` (en columnas/caracteres) de la palabra que
/// cubre `columna` dentro de `caracteres` — o la palabra justo antes, si
/// `columna` cayó exactamente después de ella (el caso típico tras
/// moverse con las flechas). `None` si ninguna de las dos aplica.
fn limites_palabra(caracteres: &[char], columna: usize) -> Option<(usize, usize)> {
    let punto = if columna < caracteres.len() && es_caracter_palabra(caracteres[columna]) {
        columna
    } else if columna > 0 && es_caracter_palabra(caracteres[columna - 1]) {
        columna - 1
    } else {
        return None;
    };

    let mut inicio = punto;
    while inicio > 0 && es_caracter_palabra(caracteres[inicio - 1]) {
        inicio -= 1;
    }
    let mut fin = punto + 1;
    while fin < caracteres.len() && es_caracter_palabra(caracteres[fin]) {
        fin += 1;
    }
    Some((inicio, fin))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn escribir(editor: &mut Editor, texto: &str) {
        for c in texto.chars() {
            editor.insertar_char(c);
        }
    }

    #[test]
    fn un_editor_nuevo_tiene_un_solo_cursor_sin_seleccion() {
        let editor = Editor::nuevo();
        assert_eq!(editor.cursores().len(), 1);
        assert!(!editor.tiene_multiples_cursores());
        assert!(!editor.cursores()[0].tiene_seleccion());
    }

    #[test]
    fn ctrl_d_selecciona_la_palabra_bajo_el_cursor_primero() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "gato perro gato");
        editor.inicio_archivo();

        editor.seleccionar_siguiente_ocurrencia();
        assert_eq!(editor.cursores().len(), 1);
        assert!(editor.cursores()[0].tiene_seleccion());
        assert_eq!(editor.buffer().a_texto()[..4].to_string(), "gato");
    }

    #[test]
    fn ctrl_d_repetido_va_agregando_ocurrencias_en_orden() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "gato perro gato lobo gato");
        editor.inicio_archivo();

        editor.seleccionar_siguiente_ocurrencia(); // selecciona el primer "gato"
        editor.seleccionar_siguiente_ocurrencia(); // agrega el segundo
        assert_eq!(editor.cursores().len(), 2);

        editor.seleccionar_siguiente_ocurrencia(); // agrega el tercero
        assert_eq!(editor.cursores().len(), 3);

        // Ya no quedan más "gato": una pulsación más no agrega un cuarto
        // cursor duplicando alguno existente.
        editor.seleccionar_siguiente_ocurrencia();
        assert_eq!(editor.cursores().len(), 3);
    }

    #[test]
    fn ctrl_shift_l_selecciona_todas_las_ocurrencias_de_una_vez() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "a b a c a");
        editor.inicio_archivo();

        editor.seleccionar_siguiente_ocurrencia(); // referencia: "a"
        editor.seleccionar_todas_ocurrencias();
        assert_eq!(editor.cursores().len(), 3);
        assert!(editor.cursores().iter().all(|c| c.tiene_seleccion()));
    }

    #[test]
    fn escribir_con_varios_cursores_inserta_en_todas_las_posiciones() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "a b a c a");
        editor.inicio_archivo();
        editor.seleccionar_siguiente_ocurrencia();
        editor.seleccionar_todas_ocurrencias();
        assert_eq!(editor.cursores().len(), 3);

        // Escribir con selección activa reemplaza cada "a" por "X" a la
        // vez, sin desincronizar los offsets de los cursores que faltan.
        editor.insertar_char('X');
        assert_eq!(editor.buffer().a_texto(), "X b X c X");
        assert_eq!(editor.cursores().len(), 3);
        assert!(editor.cursores().iter().all(|c| !c.tiene_seleccion()));
    }

    #[test]
    fn borrar_atras_con_varios_cursores_sin_seleccion() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "aa bb");
        // Cursor principal al final ("aa bb", columna 5); agrega uno más
        // en medio (columna 2, justo tras "aa").
        editor.cursores.push(CursorMultiple::sin_seleccion(Cursor { linea: 0, columna: 2 }));

        editor.borrar_atras();
        assert_eq!(editor.buffer().a_texto(), "a b");
    }

    #[test]
    fn colapsar_cursores_deja_solo_el_principal_sin_seleccion() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "a b a");
        editor.inicio_archivo();
        editor.seleccionar_siguiente_ocurrencia();
        editor.seleccionar_todas_ocurrencias();
        assert_eq!(editor.cursores().len(), 2);

        editor.colapsar_cursores();
        assert_eq!(editor.cursores().len(), 1);
        assert!(!editor.cursores()[0].tiene_seleccion());
    }

    #[test]
    fn agregar_cursor_abajo_respeta_el_fin_del_archivo() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "uno\ndos\n");
        editor.inicio_archivo();

        editor.agregar_cursor_abajo();
        assert_eq!(editor.cursores().len(), 2);

        // La última línea ("", tras el \n final) no tiene una línea
        // debajo: agregar de nuevo no debe duplicar ni crecer más allá
        // del número de líneas real.
        editor.agregar_cursor_abajo();
        assert_eq!(editor.cursores().len(), 3);
        editor.agregar_cursor_abajo();
        assert_eq!(editor.cursores().len(), 3);
    }

    #[test]
    fn agregar_cursor_arriba_no_hace_nada_en_la_primera_linea() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "uno\ndos");
        editor.inicio_archivo();

        editor.agregar_cursor_arriba();
        assert_eq!(editor.cursores().len(), 1);
    }

    #[test]
    fn deshacer_restaura_todos_los_cursores() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "a b a");
        editor.inicio_archivo();
        editor.seleccionar_siguiente_ocurrencia();
        editor.seleccionar_todas_ocurrencias();
        assert_eq!(editor.cursores().len(), 2);

        editor.insertar_char('X');
        assert_eq!(editor.buffer().a_texto(), "X b X");
        assert_eq!(editor.cursores().len(), 2);

        editor.deshacer();
        assert_eq!(editor.buffer().a_texto(), "a b a");
        // Los dos cursores (con su selección) vuelven a donde estaban.
        assert_eq!(editor.cursores().len(), 2);
        assert!(editor.cursores().iter().all(|c| c.tiene_seleccion()));
    }

    #[test]
    fn limites_palabra_encuentra_la_palabra_bajo_o_justo_despues_del_cursor() {
        let caracteres: Vec<char> = "hola mundo".chars().collect();
        assert_eq!(limites_palabra(&caracteres, 0), Some((0, 4))); // "hola"
        assert_eq!(limites_palabra(&caracteres, 2), Some((0, 4))); // dentro
        assert_eq!(limites_palabra(&caracteres, 4), Some((0, 4))); // justo después
        assert_eq!(limites_palabra(&caracteres, 5), Some((5, 10))); // "mundo"
        assert_eq!(limites_palabra(&caracteres, 4).unwrap(), (0, 4));
    }

    /// Regresión: escribir MÁS DE UN carácter seguido con varios cursores
    /// en la misma línea desincronizaba los que ya se habían procesado —
    /// cada edición ya aplicada necesita desplazarse cuando una edición
    /// *anterior* en el texto (procesada después, por ir de atrás hacia
    /// adelante) cambia de longitud. Reproducido primero manualmente en
    /// `tmux` con "gato perro gato lobo gato" + `Ctrl+D` x2 + escribir
    /// "GATO": daba "GATO perro G loOTAbo gato" en vez de
    /// "GATO perro GATO lobo gato".
    #[test]
    fn escribir_varios_caracteres_seguidos_con_varios_cursores_en_la_misma_linea() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "gato perro gato lobo gato");
        editor.inicio_archivo();
        editor.seleccionar_siguiente_ocurrencia();
        editor.seleccionar_siguiente_ocurrencia();
        assert_eq!(editor.cursores().len(), 2);

        for c in "GATO".chars() {
            editor.insertar_char(c);
        }
        assert_eq!(editor.buffer().a_texto(), "GATO perro GATO lobo gato");
    }

    #[test]
    fn borrar_atras_varios_caracteres_seguidos_con_varios_cursores() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "aaXX bbXX ccXX");
        // Cursores justo después de cada "XX": columnas 4, 9, 14.
        editor.cursores = vec![
            CursorMultiple::sin_seleccion(Cursor { linea: 0, columna: 4 }),
            CursorMultiple::sin_seleccion(Cursor { linea: 0, columna: 9 }),
            CursorMultiple::sin_seleccion(Cursor { linea: 0, columna: 14 }),
        ];

        editor.borrar_atras();
        editor.borrar_atras();
        assert_eq!(editor.buffer().a_texto(), "aa bb cc");
    }
}
