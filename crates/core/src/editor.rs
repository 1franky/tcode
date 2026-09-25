use std::ops::Range;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::buffer::Buffer;
use crate::busqueda::{buscar_coincidencias, OpcionesBusqueda};
use crate::comentarios::EstiloComentario;
use crate::cursor::{Cursor, CursorMultiple};
use crate::history::Historia;
use crate::plegado::{tramo_que_oculta, Plegado, Pliegue};

/// Modo de edición actual. `tcode` es no-modal por defecto (PLAN.md §4):
/// hasta M4 solo existía `Insertar` — la paleta de comandos (M2) y la
/// selección/multi-cursor (M3) no necesitaron un modo propio acá (la
/// paleta es un overlay aparte, `tcode_commands::EstadoPaleta`; el
/// multi-cursor es simplemente más de un [`CursorMultiple`] en
/// `Editor::cursores`). `Normal` llega en M5 con el modo VIM opcional
/// (`config.editor.modo_vim`, ver `tcode_core::vim`): `Editor` no sabe
/// nada de esa config, solo expone `entrar_modo_normal`/
/// `entrar_modo_insertar` para que quien sí la conoce (`app`) decida
/// cuándo alcanzar este modo — si el modo VIM está apagado, `Normal`
/// simplemente nunca se alcanza.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modo {
    Insertar,
    Normal,
    /// Modo Visual de VIM (`v`, por caracteres): la selección va del
    /// ancla (en `tcode_core::EstadoVim`) al cursor, ambos incluidos.
    Visual,
    /// Modo Visual por líneas (`V`): la selección son las líneas enteras
    /// entre el ancla y el cursor.
    VisualLinea,
}

/// Texto que copian `Ctrl+C`/`Ctrl+X` (ver `Editor::texto_para_copiar`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextoCopiado {
    pub texto: String,
    /// Líneas completas (se copió sin selección): pegarlo inserta arriba
    /// de la línea del cursor en vez de en el medio.
    pub lineal: bool,
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
    /// Bloques plegados (BACKLOG.md P2 #7) — acá y no en la UI porque
    /// cada edición y cada movimiento del cursor tienen que respetarlos
    /// (desplazarlos, desplegarlos, saltar las líneas ocultas).
    plegado: Plegado,
}

impl Editor {
    pub fn nuevo() -> Self {
        Self {
            buffer: Buffer::nuevo(),
            cursores: vec![CursorMultiple::sin_seleccion(Cursor::nuevo())],
            historia: Historia::nueva(),
            modo: Modo::Insertar,
            plegado: Plegado::default(),
        }
    }

    pub fn abrir(ruta: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            buffer: Buffer::desde_archivo(ruta)?,
            cursores: vec![CursorMultiple::sin_seleccion(Cursor::nuevo())],
            historia: Historia::nueva(),
            modo: Modo::Insertar,
            plegado: Plegado::default(),
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

    /// `i`/`a`/`o` en modo Normal, o volver de "Guardar como"/etc. sin
    /// haber cancelado la edición — entra a `Modo::Insertar`.
    pub fn entrar_modo_insertar(&mut self) {
        self.modo = Modo::Insertar;
    }

    /// `Esc` en modo Insertar (VIM, `config.editor.modo_vim`) — entra a
    /// `Modo::Normal`. No hace nada por sí sola si el modo VIM está
    /// apagado: `app` es quien decide si llamarla o no según esa config.
    ///
    /// También recorta el cursor si hacía falta (ver
    /// `recortar_cursor_para_normal`) — `crates/app/src/vim.rs` la llama
    /// de nuevo (siendo un no-op sobre `modo`, ya se está en `Normal`)
    /// después de cada movimiento en modo Normal, precisamente para
    /// reaplicar ese recorte en cada tecla, no solo al cambiar de modo.
    pub fn entrar_modo_normal(&mut self) {
        self.modo = Modo::Normal;
        // Cualquier grupo de deshacer abierto por el modo VIM (`cw` +
        // texto, `o` + texto...) termina al volver a Normal — así nunca
        // queda uno abierto de más, salga como se salga de Insertar.
        self.historia.cerrar_grupo();
        self.recortar_cursor_para_normal();
    }

    /// `v`/`V` del modo VIM: solo cambia el modo — el ancla de la
    /// selección la lleva `tcode_core::EstadoVim` y la selección visible
    /// la fija `fijar_seleccion` en cada tecla.
    pub fn entrar_modo_visual(&mut self, lineas: bool) {
        self.modo = if lineas { Modo::VisualLinea } else { Modo::Visual };
    }

    /// Deja un único cursor en `cursor` con la selección desde `ancla`
    /// (la selección visible del modo Visual de VIM) — colapsa cualquier
    /// otro cursor, igual que `mover_cursor_a_byte`.
    pub fn fijar_seleccion(&mut self, ancla: Cursor, cursor: Cursor) {
        let mut c = CursorMultiple { ancla, cursor };
        c.recortar(&self.buffer);
        self.cursores = vec![c];
        self.revelar_cursores();
    }

    /// Abre un grupo de deshacer (ver `Historia::abrir_grupo`): todas las
    /// ediciones hasta `cerrar_grupo_deshacer` (o volver a Normal, o
    /// deshacer) se deshacen en un solo paso. Lo usa el modo VIM para que
    /// `cw` + lo tipeado hasta `Esc` sea un único cambio, como en VIM.
    pub fn abrir_grupo_deshacer(&mut self) {
        self.historia.abrir_grupo();
    }

    pub fn cerrar_grupo_deshacer(&mut self) {
        self.historia.cerrar_grupo();
    }

    /// VIM real nunca deja el cursor "después" del último carácter de una
    /// línea no vacía en modo Normal (a diferencia de Insertar, donde esa
    /// posición es la normal para escribir al final de la línea). Los
    /// movimientos que reutiliza el modo Normal (`mover_izquierda`,
    /// `fin_linea`, etc. — los mismos que usa el resto del editor, que sí
    /// permiten esa posición) no conocen esta regla, así que hay que
    /// recortar la columna después del hecho en vez de cambiarles el
    /// comportamiento compartido.
    fn recortar_cursor_para_normal(&mut self) {
        let cursor = self.cursor();
        let longitud = self.buffer.longitud_visible_linea(cursor.linea);
        if longitud == 0 {
            return;
        }
        let maximo = longitud - 1;
        if cursor.columna > maximo {
            let offset = self.buffer.offset_byte(cursor.linea, maximo);
            self.mover_cursor_a_byte(offset);
        }
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
            self.reemplazar_y_ajustar_pliegues(rango.clone(), &reemplazo);

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
        self.revelar_cursores();
    }

    /// Reemplaza `rango` (bytes) por `reemplazo` en el buffer y ajusta
    /// los pliegues a las líneas que cambiaron (ver
    /// `Plegado::ajustar_por_edicion`). Todas las ediciones de texto
    /// pasan por acá (salvo deshacer/rehacer, que reemplazan el rope
    /// entero — ver `ajustar_pliegues_tras_reemplazo_de_rope`).
    fn reemplazar_y_ajustar_pliegues(&mut self, rango: Range<usize>, reemplazo: &str) {
        if self.plegado.esta_vacio() {
            self.buffer.reemplazar_rango_bytes(rango.start, rango.end, reemplazo);
            return;
        }
        let (inicio, columna_inicio) = self.buffer.linea_columna_desde_byte(rango.start);
        let (fin_viejo, columna_fin) = self.buffer.linea_columna_desde_byte(rango.end);
        let fin_nuevo = inicio + reemplazo.matches('\n').count();
        // Termina al principio de `fin_viejo` y lo que queda antes de esa
        // línea termina en salto de línea: su contenido no cambia, solo
        // se corre (ver `Plegado::ajustar_por_edicion`).
        let toca_fin =
            !(columna_fin == 0 && (reemplazo.ends_with('\n') || (reemplazo.is_empty() && columna_inicio == 0)));
        self.buffer.reemplazar_rango_bytes(rango.start, rango.end, reemplazo);
        self.plegado.ajustar_por_edicion(inicio, fin_viejo, fin_nuevo, toca_fin);
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

    /// Inserta un texto completo (típicamente lo pegado desde el
    /// portapapeles de la terminal, vía bracketed paste) en la posición de
    /// CADA cursor, como UNA sola edición: un solo paso de deshacer y sin
    /// pasar por `editor.nueva_linea` por cada salto de línea (que además
    /// de lento, re-indentaría cada línea pegada). Los finales de línea
    /// `\r\n`/`\r` se normalizan a `\n`, igual que al cargar un archivo
    /// (ver `Buffer`): el buffer en memoria siempre usa `\n`.
    pub fn insertar_texto(&mut self, texto: &str) {
        if texto.is_empty() {
            return;
        }
        let texto = texto.replace("\r\n", "\n").replace('\r', "\n");
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

    /// Hermana de `mover_cada_cursor` para `Shift`+movimiento (PLAN.md §4
    /// "Navegación", `Shift+flechas`/`Shift+Home`/`Shift+End`): mueve el
    /// extremo activo (`cursor`) de cada `CursorMultiple` SIN tocar
    /// `ancla`, extendiendo la selección en vez de colapsarla. `ancla`
    /// queda fija en la posición de donde arrancó el primer
    /// `Shift`+movimiento — exactamente el mismo mecanismo que ya usa
    /// multi-cursor (`Ctrl+D`, PLAN.md §11 M3) para su propia selección,
    /// reutilizado acá para selección "de toda la vida".
    fn extender_cada_cursor(&mut self, f: impl Fn(&mut Cursor, &Buffer)) {
        for c in &mut self.cursores {
            f(&mut c.cursor, &self.buffer);
        }
        // Dedup por el PAR completo (ancla, cursor), no solo por
        // `cursor` como `fusionar_cursores_duplicados` — acá dos
        // selecciones distintas pueden converger a la misma posición
        // activa sin ser la misma selección (mismo extremo, ancla
        // distinta); fusionarlas por el cursor solo perdería una de las
        // dos sin querer.
        self.cursores.sort_by_key(|c| (c.ancla.linea, c.ancla.columna, c.cursor.linea, c.cursor.columna));
        self.cursores.dedup();
    }

    pub fn mover_izquierda(&mut self) {
        self.mover_cada_cursor(Cursor::mover_izquierda);
        self.saltar_pliegues(Salto::Izquierda, false);
    }

    pub fn mover_derecha(&mut self) {
        self.mover_cada_cursor(Cursor::mover_derecha);
        self.saltar_pliegues(Salto::Derecha, false);
    }

    pub fn mover_arriba(&mut self) {
        self.mover_cada_cursor(Cursor::mover_arriba);
        self.saltar_pliegues(Salto::Arriba, false);
    }

    pub fn mover_abajo(&mut self) {
        self.mover_cada_cursor(Cursor::mover_abajo);
        self.saltar_pliegues(Salto::Abajo, false);
    }

    pub fn inicio_linea(&mut self) {
        self.mover_cada_cursor(|c, _| c.inicio_linea());
    }

    pub fn fin_linea(&mut self) {
        self.mover_cada_cursor(Cursor::fin_linea);
    }

    /// `Shift+Left`: extiende la selección un carácter a la izquierda.
    pub fn seleccionar_izquierda(&mut self) {
        self.extender_cada_cursor(Cursor::mover_izquierda);
        self.saltar_pliegues(Salto::Izquierda, true);
    }

    /// `Shift+Right`: extiende la selección un carácter a la derecha.
    pub fn seleccionar_derecha(&mut self) {
        self.extender_cada_cursor(Cursor::mover_derecha);
        self.saltar_pliegues(Salto::Derecha, true);
    }

    /// `Shift+Up`: extiende la selección una línea hacia arriba.
    pub fn seleccionar_arriba(&mut self) {
        self.extender_cada_cursor(Cursor::mover_arriba);
        self.saltar_pliegues(Salto::Arriba, true);
    }

    /// `Shift+Down`: extiende la selección una línea hacia abajo.
    pub fn seleccionar_abajo(&mut self) {
        self.extender_cada_cursor(Cursor::mover_abajo);
        self.saltar_pliegues(Salto::Abajo, true);
    }

    /// `Shift+Home`: extiende la selección hasta el inicio de la línea.
    pub fn seleccionar_inicio_linea(&mut self) {
        self.extender_cada_cursor(|c, _| c.inicio_linea());
    }

    /// `Shift+End`: extiende la selección hasta el fin de la línea.
    pub fn seleccionar_fin_linea(&mut self) {
        self.extender_cada_cursor(Cursor::fin_linea);
    }

    pub fn inicio_archivo(&mut self) {
        self.mover_cada_cursor(|c, _| c.inicio_archivo());
    }

    pub fn fin_archivo(&mut self) {
        self.mover_cada_cursor(Cursor::fin_archivo);
        self.saltar_pliegues(Salto::Arriba, false);
    }

    /// Mueve el cursor principal a la posición del offset de bytes
    /// `offset_byte` (usado para saltar a una coincidencia de búsqueda,
    /// PLAN.md §4), colapsando cualquier cursor/selección adicional —
    /// abrir la búsqueda con varios cursores activos vuelve a uno solo,
    /// igual que en cualquier editor.
    pub fn mover_cursor_a_byte(&mut self, offset_byte: usize) {
        let (linea, columna) = self.buffer.linea_columna_desde_byte(offset_byte);
        self.cursores = vec![CursorMultiple::sin_seleccion(Cursor { linea, columna })];
        // Saltar a una coincidencia de búsqueda (o a cualquier posición)
        // que cae dentro de un bloque plegado lo despliega.
        self.revelar_cursores();
    }

    /// Reemplaza el texto en el rango de bytes `[inicio, fin)` por
    /// `reemplazo` (PLAN.md §4, "buscar.reemplazar") y deja el cursor
    /// principal justo después del texto insertado, colapsando cualquier
    /// cursor/selección adicional (ver `mover_cursor_a_byte`).
    pub fn reemplazar_rango_bytes(&mut self, inicio_byte: usize, fin_byte: usize, reemplazo: &str) {
        self.registrar_snapshot();
        self.reemplazar_y_ajustar_pliegues(inicio_byte..fin_byte, reemplazo);
        self.mover_cursor_a_byte(inicio_byte + reemplazo.len());
    }

    /// Aplica varias ediciones `(rango de bytes, texto nuevo)` de una
    /// sola vez, como UN solo paso de deshacer (un `Ctrl+Z` vuelve al
    /// texto de antes de todas ellas) — pensado para el "formatear al
    /// guardar" vía LSP (BACKLOG.md P2 #5), cuyo `TextEdit[]` puede traer
    /// decenas de cambios chicos repartidos por el archivo.
    ///
    /// Todos los rangos se refieren al texto ANTERIOR a cualquiera de
    /// ellas (la misma convención que LSP) y no pueden solaparse: se
    /// aplican de atrás hacia adelante para que ninguna invalide los
    /// offsets de las que faltan. Si dos se solapan (un servidor que no
    /// respeta la spec) no se aplica ninguna y devuelve `false` — mejor
    /// no formatear que dejar el archivo a medio romper. También `false`
    /// (sin registrar nada en el historial) si la lista viene vacía o
    /// ninguna edición cambia el texto de verdad, para que un guardado
    /// sin cambios de formato no deje un paso de deshacer vacío.
    ///
    /// Los cursores (y sus selecciones) se conservan "lo mejor posible":
    /// cada extremo se corre según lo que se insertó/borró ANTES de él;
    /// uno que caía DENTRO de un rango reemplazado se queda a la misma
    /// distancia del inicio de ese rango, recortado al largo del texto
    /// nuevo. Con ediciones mínimas (lo que devuelven rust-analyzer y la
    /// mayoría de los servidores: solo los espacios que cambian) el
    /// cursor queda sobre el mismo código que antes de formatear.
    pub fn aplicar_ediciones(&mut self, ediciones: &[(Range<usize>, String)]) -> bool {
        let texto = self.buffer.a_texto();
        let mut ordenadas: Vec<&(Range<usize>, String)> = ediciones.iter().collect();
        ordenadas.sort_by_key(|(rango, _)| (rango.start, rango.end));
        let invalida = ordenadas.iter().any(|(r, _)| texto.get(r.clone()).is_none())
            || ordenadas.windows(2).any(|par| par[0].0.end > par[1].0.start);
        if invalida {
            return false;
        }
        if ordenadas.iter().all(|(r, nuevo)| texto[r.clone()] == **nuevo) {
            return false;
        }

        let mapear = |offset: usize| -> usize {
            let mut delta: isize = 0;
            for (rango, nuevo) in &ordenadas {
                if rango.end <= offset {
                    delta += nuevo.len() as isize - rango.len() as isize;
                } else if rango.start < offset {
                    // Dentro del rango reemplazado: misma distancia al
                    // inicio, sin pasarse del texto nuevo.
                    let dentro = (offset - rango.start).min(nuevo.len());
                    return (rango.start as isize + delta) as usize + dentro;
                } else {
                    break;
                }
            }
            (offset as isize + delta) as usize
        };
        let cursores_mapeados: Vec<(usize, usize)> = self
            .cursores
            .iter()
            .map(|c| {
                let ancla = self.buffer.offset_byte(c.ancla.linea, c.ancla.columna);
                let cursor = self.buffer.offset_byte(c.cursor.linea, c.cursor.columna);
                (mapear(ancla), mapear(cursor))
            })
            .collect();

        self.registrar_snapshot();
        for (rango, nuevo) in ordenadas.iter().rev() {
            self.reemplazar_y_ajustar_pliegues(rango.clone(), nuevo);
        }
        self.cursores = cursores_mapeados
            .into_iter()
            .map(|(ancla, cursor)| {
                let ancla = self.buffer.linea_columna_desde_byte(ancla);
                let cursor = self.buffer.linea_columna_desde_byte(cursor);
                let mut c = CursorMultiple {
                    ancla: Cursor { linea: ancla.0, columna: ancla.1 },
                    cursor: Cursor { linea: cursor.0, columna: cursor.1 },
                };
                c.recortar(&self.buffer);
                c
            })
            .collect();
        self.revelar_cursores();
        true
    }

    /// Lo que copia `Ctrl+C` (BACKLOG.md P0 #15), sin tocar el buffer.
    /// Con alguna selección: el texto de cada selección, en orden de
    /// aparición en el documento, unidas por `\n` (los cursores sin
    /// selección de un multi-cursor mixto no aportan nada). Sin ninguna
    /// selección: la línea completa de cada cursor (una sola vez por
    /// línea), cada una con su salto de línea — como VSCode, y marcado
    /// como `lineal` para que pegarlo inserte la línea arriba en vez de
    /// partir la del cursor (ver `pegar_lineas`).
    pub fn texto_para_copiar(&self) -> TextoCopiado {
        let mut rangos: Vec<Range<usize>> = (0..self.cursores.len()).map(|i| self.rango_bytes(i)).collect();
        rangos.sort_by_key(|r| (r.start, r.end));
        if rangos.iter().any(|r| !r.is_empty()) {
            let rope = self.buffer.rope();
            let partes: Vec<String> =
                rangos.iter().filter(|r| !r.is_empty()).map(|r| rope.byte_slice(r.clone()).to_string()).collect();
            return TextoCopiado { texto: partes.join("\n"), lineal: false };
        }
        let mut texto = String::new();
        for linea in self.lineas_de_los_cursores() {
            texto.push_str(&self.buffer.linea_con_salto(linea));
            if !texto.ends_with('\n') {
                texto.push('\n');
            }
        }
        TextoCopiado { texto, lineal: true }
    }

    /// `Ctrl+X`: lo mismo que copia `texto_para_copiar`, y además lo
    /// borra como UNA sola edición (un `Ctrl+Z` lo devuelve entero).
    /// Sin selección borra las líneas completas de los cursores; la
    /// última línea del archivo, si no termina en salto de línea, se
    /// lleva el salto de la anterior (si no, quedaría una línea vacía
    /// colgando). Cada cursor queda al principio de lo que ocupó el
    /// lugar de lo borrado.
    pub fn cortar(&mut self) -> TextoCopiado {
        let copiado = self.texto_para_copiar();
        if !copiado.lineal {
            self.editar_cada_cursor(|_, seleccion| (seleccion, String::new()));
            return copiado;
        }
        let total = self.buffer.len_bytes();
        let num_lineas = self.buffer.num_lineas();
        let mut rangos: Vec<Range<usize>> = Vec::new();
        for linea in self.lineas_de_los_cursores() {
            let inicio = self.buffer.inicio_byte_linea(linea);
            let fin = if linea + 1 < num_lineas { self.buffer.inicio_byte_linea(linea + 1) } else { total };
            // Líneas contiguas se funden en un solo rango:
            // `aplicar_ediciones` no acepta rangos solapados.
            match rangos.last_mut() {
                Some(anterior) if anterior.end >= inicio => anterior.end = fin,
                _ => rangos.push(inicio..fin),
            }
        }
        if let Some(ultimo) = rangos.last_mut() {
            if ultimo.end == total && ultimo.start > 0 && !self.buffer.termina_en_salto_de_linea() {
                ultimo.start -= 1;
            }
        }
        let ediciones: Vec<(Range<usize>, String)> = rangos.into_iter().map(|r| (r, String::new())).collect();
        if self.aplicar_ediciones(&ediciones) {
            for c in &mut self.cursores {
                c.ancla = c.cursor;
            }
            self.fusionar_cursores_duplicados();
        }
        copiado
    }

    /// Pega `texto` (líneas completas, terminado en `\n`) al principio
    /// de la línea de cada cursor, como UNA sola edición: es lo que hace
    /// VSCode al pegar algo copiado con `Ctrl+C` sin selección — la línea
    /// aparece arriba y el cursor sigue sobre la suya, en la misma
    /// columna.
    pub fn pegar_lineas(&mut self, texto: &str) {
        if texto.is_empty() {
            return;
        }
        let texto = texto.replace("\r\n", "\n").replace('\r', "\n");
        let ediciones: Vec<(Range<usize>, String)> = self
            .lineas_de_los_cursores()
            .into_iter()
            .map(|linea| {
                let inicio = self.buffer.inicio_byte_linea(linea);
                (inicio..inicio, texto.clone())
            })
            .collect();
        self.aplicar_ediciones(&ediciones);
    }

    /// Líneas de los cursores, ordenadas y sin repetir.
    fn lineas_de_los_cursores(&self) -> Vec<usize> {
        let mut lineas: Vec<usize> = self.cursores.iter().map(|c| c.cursor.linea).collect();
        lineas.sort_unstable();
        lineas.dedup();
        lineas
    }

    /// Tramos de líneas enteras sobre los que actúan comentar, mover y
    /// duplicar líneas (BACKLOG.md P0 #19): el de cada cursor (su línea,
    /// o todas las que toca su selección), ordenados y fusionados cuando
    /// se solapan o se tocan — dos cursores en líneas vecinas se mueven
    /// juntos, como un solo bloque. Con `extender_pliegues`, una última
    /// línea que es la cabecera de un bloque plegado arrastra todo lo que
    /// oculta: lo que en pantalla se ve como una línea se mueve o duplica
    /// entero.
    fn bloques_de_lineas(&self, extender_pliegues: bool) -> Vec<Range<usize>> {
        let mut bloques: Vec<Range<usize>> = self.cursores.iter().map(lineas_de_cursor).collect();
        if extender_pliegues && !self.plegado.esta_vacio() {
            let tramos = self.plegado.tramos_ocultos();
            for bloque in &mut bloques {
                if let Some(tramo) = tramos.iter().find(|t| t.start == bloque.end) {
                    bloque.end = tramo.end;
                }
            }
        }
        bloques.sort_by_key(|b| (b.start, b.end));
        let mut fusionados: Vec<Range<usize>> = Vec::with_capacity(bloques.len());
        for bloque in bloques {
            match fusionados.last_mut() {
                Some(anterior) if bloque.start <= anterior.end => anterior.end = anterior.end.max(bloque.end),
                _ => fusionados.push(bloque),
            }
        }
        fusionados
    }

    /// `Ctrl+/` (BACKLOG.md P0 #19): comenta o descomenta las líneas de
    /// cada cursor/selección, como UNA sola edición (un `Ctrl+Z`). Si
    /// TODAS las líneas no vacías de todos los cursores ya están
    /// comentadas, las descomenta; si no, las comenta todas (así una
    /// mezcla queda uniforme en vez de invertirse línea por línea, igual
    /// que VSCode). Las líneas en blanco no cuentan ni se tocan.
    ///
    /// Con comentario de línea, el prefijo va alineado a la sangría
    /// mínima de cada bloque (`// ` con un espacio), así el bloque
    /// comentado sigue viéndose indentado; al descomentar se quita el
    /// prefijo y un espacio después, si lo hay. Con comentario de bloque
    /// (HTML, CSS) se envuelve cada línea por separado (ver
    /// [`EstiloComentario::Bloque`]). Devuelve si cambió algo (`false`
    /// si solo había líneas en blanco).
    pub fn alternar_comentario(&mut self, estilo: EstiloComentario) -> bool {
        let bloques: Vec<Vec<(usize, String)>> = self
            .bloques_de_lineas(false)
            .into_iter()
            .map(|bloque| {
                bloque
                    .filter_map(|linea| {
                        let texto = self.buffer.linea_texto(linea);
                        (!texto.trim().is_empty()).then_some((linea, texto))
                    })
                    .collect()
            })
            .collect();
        if bloques.iter().all(Vec::is_empty) {
            return false;
        }
        let descomentar = bloques.iter().flatten().all(|(_, texto)| esta_comentada(texto, estilo));

        let mut ediciones: Vec<(Range<usize>, String)> = Vec::new();
        for bloque in &bloques {
            let sangria = bloque.iter().map(|(_, t)| t.chars().take_while(|c| c.is_whitespace()).count()).min();
            for (linea, texto) in bloque {
                let base = self.buffer.inicio_byte_linea(*linea);
                // Principio y fin (bytes) del texto sin los espacios de
                // los costados.
                let inicio = texto.len() - texto.trim_start().len();
                let fin = texto.trim_end().len();
                match (estilo, descomentar) {
                    (EstiloComentario::Linea(prefijo), true) => {
                        let mut hasta = inicio + prefijo.len();
                        if texto[hasta..].starts_with(' ') {
                            hasta += 1;
                        }
                        ediciones.push((base + inicio..base + hasta, String::new()));
                    }
                    (EstiloComentario::Bloque(apertura, cierre), true) => {
                        let mut hasta = inicio + apertura.len();
                        let mut desde = fin - cierre.len();
                        if hasta < desde && texto[hasta..].starts_with(' ') {
                            hasta += 1;
                        }
                        if desde > hasta && texto[..desde].ends_with(' ') {
                            desde -= 1;
                        }
                        ediciones.push((base + inicio..base + hasta, String::new()));
                        ediciones.push((base + desde..base + fin, String::new()));
                    }
                    (_, false) => {
                        let columna = sangria.unwrap_or(0);
                        let en = texto.char_indices().nth(columna).map_or(texto.len(), |(i, _)| i);
                        let (apertura, cierre) = match estilo {
                            EstiloComentario::Linea(prefijo) => (prefijo, None),
                            EstiloComentario::Bloque(apertura, cierre) => (apertura, Some(cierre)),
                        };
                        ediciones.push((base + en..base + en, format!("{apertura} ")));
                        if let Some(cierre) = cierre {
                            ediciones.push((base + fin..base + fin, format!(" {cierre}")));
                        }
                    }
                }
            }
        }
        self.aplicar_ediciones(&ediciones)
    }

    /// `Alt+↑` (BACKLOG.md P0 #19): sube una línea la línea de cada
    /// cursor (o las de su selección), con el cursor y la selección
    /// acompañándola. Ver `mover_lineas`.
    pub fn mover_lineas_arriba(&mut self) -> bool {
        self.mover_lineas(false)
    }

    /// `Alt+↓`: igual que `mover_lineas_arriba`, hacia abajo.
    pub fn mover_lineas_abajo(&mut self) -> bool {
        self.mover_lineas(true)
    }

    /// Intercambia cada bloque de líneas (`bloques_de_lineas`, con los
    /// pliegues extendidos) con la línea VISIBLE de al lado: si esa línea
    /// es la cabecera de un bloque plegado, el bloque movido salta el
    /// bloque plegado entero en vez de meterse adentro; y un bloque
    /// plegado que se mueve sigue plegado (`Plegado::intercambiar`). Es
    /// UNA edición (un `Ctrl+Z`). Si algún bloque ya está en el borde (la
    /// primera línea hacia arriba, la última hacia abajo) no se mueve
    /// ninguno — mover solo algunos desarmaría la forma de un
    /// multi-cursor. Devuelve si movió algo.
    ///
    /// No pasa por `reemplazar_y_ajustar_pliegues`: esa despliega todo
    /// pliegue que la edición toca, y acá justamente el contenido de las
    /// líneas no cambia, solo se reordena — los pliegues se ajustan con
    /// `Plegado::intercambiar`.
    fn mover_lineas(&mut self, abajo: bool) -> bool {
        let bloques = self.bloques_de_lineas(true);
        let num_lineas = self.buffer.num_lineas();
        let tramos = self.plegado.tramos_ocultos();
        // Por bloque, los dos tramos contiguos a intercambiar (el de
        // arriba primero).
        let mut pares: Vec<(Range<usize>, Range<usize>)> = Vec::with_capacity(bloques.len());
        for bloque in &bloques {
            if abajo {
                if bloque.end >= num_lineas {
                    return false;
                }
                let fin = tramos.iter().find(|t| t.start == bloque.end + 1).map_or(bloque.end + 1, |t| t.end);
                pares.push((bloque.clone(), bloque.end..fin));
            } else {
                if bloque.start == 0 {
                    return false;
                }
                let inicio = tramo_que_oculta(&tramos, bloque.start - 1).map_or(bloque.start - 1, |t| t.start - 1);
                pares.push((inicio..bloque.start, bloque.clone()));
            }
        }
        if pares.windows(2).any(|par| par[0].1.end > par[1].0.start) {
            return false;
        }

        self.registrar_snapshot();
        // De abajo hacia arriba, aunque la cantidad de líneas no cambia:
        // así los offsets de bytes de los pares que faltan siguen valiendo.
        for (primero, segundo) in pares.iter().rev() {
            let inicio = self.buffer.inicio_byte_linea(primero.start);
            let hay_mas = segundo.end < num_lineas;
            let fin = if hay_mas { self.buffer.inicio_byte_linea(segundo.end) } else { self.buffer.len_bytes() };
            // Unidas por `\n` y con salto final solo si había más líneas
            // después: la última línea de un archivo sin `\n` final sigue
            // sin tenerlo, aunque ahora sea otra.
            let lineas: Vec<String> = segundo.clone().chain(primero.clone()).map(|l| self.buffer.linea_texto(l)).collect();
            let mut nuevo = lineas.join("\n");
            if hay_mas {
                nuevo.push('\n');
            }
            self.buffer.reemplazar_rango_bytes(inicio, fin, &nuevo);
            self.plegado.intercambiar(primero.clone(), segundo.clone());
        }

        for c in &mut self.cursores {
            let linea = lineas_de_cursor(c).start;
            let Some(i) = bloques.iter().position(|b| b.contains(&linea)) else { continue };
            let (primero, segundo) = &pares[i];
            let delta = if abajo { segundo.len() as isize } else { -(primero.len() as isize) };
            // Los dos extremos, incluido uno que queda en la columna 0 de
            // la línea siguiente al bloque (fuera de él): sigue marcando
            // "hasta el final del bloque" en su nueva posición.
            c.ancla.linea = c.ancla.linea.saturating_add_signed(delta);
            c.cursor.linea = c.cursor.linea.saturating_add_signed(delta);
            c.recortar(&self.buffer);
        }
        self.revelar_cursores();
        true
    }

    /// `Shift+Alt+↓` (BACKLOG.md P0 #19): duplica la línea de cada
    /// cursor (o las líneas enteras que toca su selección, con los
    /// bloques plegados completos) justo debajo, como UNA edición. El
    /// cursor y la selección pasan a la copia de abajo, como en VSCode —
    /// repetirlo sigue duplicando hacia abajo.
    ///
    /// La copia se inserta en realidad ARRIBA del original (idéntico en
    /// el texto): así el original, con sus pliegues, solo se corre hacia
    /// abajo (`reemplazar_y_ajustar_pliegues` con una inserción en la
    /// columna 0), y la copia nueva queda desplegada.
    pub fn duplicar_lineas(&mut self) {
        let bloques = self.bloques_de_lineas(true);
        self.registrar_snapshot();
        for bloque in bloques.iter().rev() {
            let mut copia = bloque.clone().map(|l| self.buffer.linea_texto(l)).collect::<Vec<_>>().join("\n");
            copia.push('\n');
            let inicio = self.buffer.inicio_byte_linea(bloque.start);
            self.reemplazar_y_ajustar_pliegues(inicio..inicio, &copia);
        }
        for c in &mut self.cursores {
            let linea = lineas_de_cursor(c).start;
            let delta: usize = bloques.iter().filter(|b| b.start <= linea).map(|b| b.len()).sum();
            c.ancla.linea += delta;
            c.cursor.linea += delta;
        }
        self.revelar_cursores();
    }

    /// `Ctrl+A` (BACKLOG.md P0 #19): un solo cursor al final del
    /// documento con la selección desde el principio. Si el final cae
    /// dentro de un bloque plegado, ese bloque se despliega (igual que
    /// cualquier otro salto del cursor).
    pub fn seleccionar_todo(&mut self) {
        let mut fin = Cursor::nuevo();
        fin.fin_archivo(&self.buffer);
        self.cursores = vec![CursorMultiple { ancla: Cursor::nuevo(), cursor: fin }];
        self.revelar_cursores();
    }

    /// `Ctrl+G` (BACKLOG.md P0 #19): lleva el cursor (uno solo, sin
    /// selección) a `linea`/`columna`, en base cero, recortadas al
    /// documento y al largo de esa línea. Despliega lo que la oculte
    /// (`mover_cursor_a_byte`).
    pub fn ir_a_linea(&mut self, linea: usize, columna: usize) {
        let linea = linea.min(self.buffer.num_lineas().saturating_sub(1));
        let offset = self.buffer.offset_byte(linea, columna);
        self.mover_cursor_a_byte(offset);
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
        self.revelar_cursores();
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
        self.revelar_cursores();
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
        self.revelar_cursores();
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
            let anterior = self.buffer.rope().clone();
            self.buffer.reemplazar_rope(rope);
            self.ajustar_pliegues_tras_reemplazo_de_rope(&anterior);
            for c in &mut cursores {
                c.recortar(&self.buffer);
            }
            self.cursores = cursores;
            self.revelar_cursores();
        }
    }

    pub fn rehacer(&mut self) {
        if let Some((rope, mut cursores)) = self.historia.rehacer(self.buffer.rope(), &self.cursores) {
            let anterior = self.buffer.rope().clone();
            self.buffer.reemplazar_rope(rope);
            self.ajustar_pliegues_tras_reemplazo_de_rope(&anterior);
            for c in &mut cursores {
                c.recortar(&self.buffer);
            }
            self.cursores = cursores;
            self.revelar_cursores();
        }
    }

    /// Deshacer/rehacer reemplazan el rope entero (sin decir qué
    /// cambió): la región cambiada se reconstruye comparando líneas
    /// desde arriba y desde abajo (prefijo y sufijo comunes), y se ajustan
    /// los pliegues como con cualquier otra edición. Recorre el archivo,
    /// pero solo al deshacer/rehacer y solo si hay algo plegado.
    fn ajustar_pliegues_tras_reemplazo_de_rope(&mut self, anterior: &ropey::Rope) {
        if self.plegado.esta_vacio() {
            return;
        }
        let actual = self.buffer.rope();
        let (lineas_viejo, lineas_nuevo) = (anterior.len_lines(), actual.len_lines());
        let minimo = lineas_viejo.min(lineas_nuevo);
        let prefijo = (0..minimo).take_while(|&i| anterior.line(i) == actual.line(i)).count();
        if prefijo == lineas_viejo && prefijo == lineas_nuevo {
            return;
        }
        let sufijo = (0..minimo - prefijo)
            .take_while(|&i| anterior.line(lineas_viejo - 1 - i) == actual.line(lineas_nuevo - 1 - i))
            .count();
        // Se reemplazaron las líneas enteras `prefijo..lineas_viejo -
        // sufijo` (extremo exclusivo): la edición "termina" en la columna
        // 0 de la primera línea del sufijo, que no cambió (`toca_fin`
        // en `false`, ver `Plegado::ajustar_por_edicion`).
        self.plegado.ajustar_por_edicion(prefijo, lineas_viejo - sufijo, lineas_nuevo - sufijo, false);
        self.plegado.recortar(self.buffer.num_lineas());
    }

    /// Bloques plegados de este documento (para dibujarlos).
    pub fn plegado(&self) -> &Plegado {
        &self.plegado
    }

    /// `Ctrl+Shift+[` (PLAN.md §4 "Plegado"): pliega el bloque más
    /// interno que contiene la línea del cursor principal, de entre
    /// `candidatos` (los rangos plegables del documento, calculados por
    /// `tcode-syntax`) — sin contar los que ya están plegados, así que
    /// repetirlo va plegando hacia afuera, igual que en VSCode. Devuelve
    /// si plegó algo.
    pub fn plegar_en_cursor(&mut self, candidatos: &[Pliegue]) -> bool {
        let linea = self.cursor().linea;
        let elegido = candidatos
            .iter()
            .filter(|p| p.inicio <= linea && linea <= p.fin && p.fin > p.inicio)
            .filter(|p| !self.plegado.pliegues().contains(p))
            .min_by_key(|p| p.fin - p.inicio);
        let Some(&elegido) = elegido else { return false };
        self.plegado.plegar(elegido);
        self.sacar_cursores_de_pliegues();
        true
    }

    /// `Ctrl+K Ctrl+0`: pliega todos los `candidatos` (anidados incluidos:
    /// al desplegar uno de afuera, los de adentro siguen plegados).
    pub fn plegar_todo(&mut self, candidatos: &[Pliegue]) {
        for p in candidatos {
            self.plegado.plegar(*p);
        }
        self.plegado.recortar(self.buffer.num_lineas());
        self.sacar_cursores_de_pliegues();
    }

    /// `Ctrl+Shift+]`: despliega el bloque del cursor principal (el que
    /// tiene su cabecera en esa línea o, si no, el más interno que la
    /// contiene). Devuelve si desplegó algo.
    pub fn desplegar_en_cursor(&mut self) -> bool {
        self.plegado.desplegar_en(self.cursor().linea)
    }

    /// `Ctrl+K Ctrl+J`: despliega todo.
    pub fn desplegar_todo(&mut self) {
        self.plegado.desplegar_todo();
    }

    /// Después de plegar, un cursor que quedó en una línea oculta pasa a
    /// la cabecera del bloque (sin selección: la selección podría quedar
    /// con un extremo invisible).
    fn sacar_cursores_de_pliegues(&mut self) {
        let tramos = self.plegado.tramos_ocultos();
        for c in &mut self.cursores {
            if let Some(tramo) = tramo_que_oculta(&tramos, c.cursor.linea) {
                let linea = tramo.start - 1;
                let columna = c.cursor.columna.min(self.buffer.longitud_visible_linea(linea));
                *c = CursorMultiple::sin_seleccion(Cursor { linea, columna });
            }
        }
        self.fusionar_cursores_duplicados();
    }

    /// Después de un salto (búsqueda, deshacer, `Ctrl+D`...), despliega
    /// lo que oculte la línea de algún cursor.
    fn revelar_cursores(&mut self) {
        if self.plegado.esta_vacio() {
            return;
        }
        for i in 0..self.cursores.len() {
            let linea = self.cursores[i].cursor.linea;
            self.plegado.revelar(linea);
        }
    }

    /// Después de mover con las flechas: un cursor que cayó en una línea
    /// oculta salta el bloque plegado entero en la dirección del
    /// movimiento — hacia arriba/izquierda a la cabecera, hacia
    /// abajo/derecha a la primera línea después del bloque (o a la
    /// cabecera si el bloque llega hasta el final del archivo). Con
    /// `extender` (`Shift`+flecha) solo se mueve el extremo activo: la
    /// selección abarca el bloque oculto completo, igual que en VSCode.
    fn saltar_pliegues(&mut self, salto: Salto, extender: bool) {
        if self.plegado.esta_vacio() {
            return;
        }
        let tramos = self.plegado.tramos_ocultos();
        let num_lineas = self.buffer.num_lineas();
        for c in &mut self.cursores {
            let Some(tramo) = tramo_que_oculta(&tramos, c.cursor.linea) else { continue };
            let cabecera = tramo.start - 1;
            let adelante = matches!(salto, Salto::Abajo | Salto::Derecha) && tramo.end < num_lineas;
            let linea = if adelante { tramo.end } else { cabecera };
            let columna = match salto {
                Salto::Arriba | Salto::Abajo => c.cursor.columna.min(self.buffer.longitud_visible_linea(linea)),
                Salto::Derecha if adelante => 0,
                Salto::Izquierda | Salto::Derecha => self.buffer.longitud_visible_linea(linea),
            };
            c.cursor = Cursor { linea, columna };
            if !extender {
                c.ancla = c.cursor;
            }
        }
        if !extender {
            self.fusionar_cursores_duplicados();
        }
    }
}

/// Dirección del movimiento que dejó un cursor dentro de un bloque
/// plegado (ver `Editor::saltar_pliegues`).
#[derive(Clone, Copy)]
enum Salto {
    Arriba,
    Abajo,
    Izquierda,
    Derecha,
}

impl Default for Editor {
    fn default() -> Self {
        Self::nuevo()
    }
}

/// Líneas enteras `[inicio, fin)` que toca un cursor: la suya sin
/// selección, o todas las de la selección — salvo la última si la
/// selección termina en su columna 0 (seleccionar líneas enteras con
/// `Shift+↓` deja el cursor ahí, y esa línea no se ve seleccionada),
/// igual que VSCode.
fn lineas_de_cursor(c: &CursorMultiple) -> Range<usize> {
    let (a, b) = if (c.ancla.linea, c.ancla.columna) <= (c.cursor.linea, c.cursor.columna) {
        (c.ancla, c.cursor)
    } else {
        (c.cursor, c.ancla)
    };
    let fin = if b.linea > a.linea && b.columna == 0 { b.linea } else { b.linea + 1 };
    a.linea..fin
}

/// Si `texto` (una línea no vacía) ya está comentada con `estilo`,
/// ignorando la sangría y los espacios del final.
fn esta_comentada(texto: &str, estilo: EstiloComentario) -> bool {
    let texto = texto.trim();
    match estilo {
        EstiloComentario::Linea(prefijo) => texto.starts_with(prefijo),
        EstiloComentario::Bloque(apertura, cierre) => {
            texto.len() >= apertura.len() + cierre.len() && texto.starts_with(apertura) && texto.ends_with(cierre)
        }
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
    use crate::plegado::Pliegue;

    fn escribir(editor: &mut Editor, texto: &str) {
        for c in texto.chars() {
            editor.insertar_char(c);
        }
    }

    #[test]
    fn copiar_la_seleccion_o_la_linea_actual() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "gato perro\nsegunda");
        editor.inicio_archivo();
        let copiado = editor.texto_para_copiar();
        assert_eq!(copiado, TextoCopiado { texto: "gato perro\n".to_string(), lineal: true });
        // Última línea sin salto: se le agrega uno, igual es "una línea".
        editor.fin_archivo();
        assert_eq!(editor.texto_para_copiar().texto, "segunda\n");
        editor.inicio_archivo();
        editor.seleccionar_siguiente_ocurrencia(); // "gato"
        assert_eq!(editor.texto_para_copiar(), TextoCopiado { texto: "gato".to_string(), lineal: false });
        assert_eq!(editor.buffer().a_texto(), "gato perro\nsegunda");
    }

    #[test]
    fn copiar_con_multicursor_une_las_selecciones_en_orden() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "uno x\ndos x\ntres x");
        editor.inicio_archivo();
        editor.seleccionar_todas_ocurrencias(); // "uno"
        assert_eq!(editor.texto_para_copiar().texto, "uno");
        editor.colapsar_cursores();
        editor.fin_archivo();
        editor.agregar_cursor_arriba();
        editor.agregar_cursor_arriba();
        assert_eq!(editor.texto_para_copiar().texto, "uno x\ndos x\ntres x\n");
        // Selecciones de largo distinto, sin importar el orden de creación.
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "ab ab ab");
        editor.inicio_archivo();
        editor.seleccionar_siguiente_ocurrencia();
        editor.seleccionar_siguiente_ocurrencia();
        editor.seleccionar_siguiente_ocurrencia();
        assert_eq!(editor.texto_para_copiar().texto, "ab\nab\nab");
    }

    #[test]
    fn cortar_la_seleccion_es_un_solo_paso_de_deshacer() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "gato perro gato");
        editor.inicio_archivo();
        editor.seleccionar_todas_ocurrencias();
        let copiado = editor.cortar();
        assert_eq!(copiado.texto, "gato\ngato");
        assert_eq!(editor.buffer().a_texto(), " perro ");
        editor.deshacer();
        assert_eq!(editor.buffer().a_texto(), "gato perro gato");
    }

    #[test]
    fn cortar_sin_seleccion_se_lleva_la_linea_entera() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "uno\ndos\ntres");
        editor.inicio_archivo();
        editor.mover_abajo();
        assert_eq!(editor.cortar().texto, "dos\n");
        assert_eq!(editor.buffer().a_texto(), "uno\ntres");
        assert_eq!((editor.cursor().linea, editor.cursor().columna), (1, 0));
        // La última línea (sin salto) se lleva el salto de la anterior.
        assert_eq!(editor.cortar().texto, "tres\n");
        assert_eq!(editor.buffer().a_texto(), "uno");
        editor.deshacer();
        editor.deshacer();
        assert_eq!(editor.buffer().a_texto(), "uno\ndos\ntres");
    }

    #[test]
    fn cortar_lineas_contiguas_con_multicursor() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "a\nb\nc");
        editor.agregar_cursor_arriba(); // cursores en "b" y "c"
        assert_eq!(editor.cortar().texto, "b\nc\n");
        assert_eq!(editor.buffer().a_texto(), "a");
        assert_eq!(editor.cursores().len(), 1);
    }

    #[test]
    fn pegar_lineas_inserta_arriba_y_conserva_el_cursor() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "uno\ndos");
        editor.pegar_lineas("nueva\n");
        assert_eq!(editor.buffer().a_texto(), "uno\nnueva\ndos");
        assert_eq!((editor.cursor().linea, editor.cursor().columna), (2, 3));
        editor.deshacer();
        assert_eq!(editor.buffer().a_texto(), "uno\ndos");
    }

    #[test]
    fn un_editor_nuevo_tiene_un_solo_cursor_sin_seleccion() {
        let editor = Editor::nuevo();
        assert_eq!(editor.cursores().len(), 1);
        assert!(!editor.tiene_multiples_cursores());
        assert!(!editor.cursores()[0].tiene_seleccion());
    }

    #[test]
    fn insertar_texto_pega_todo_en_un_solo_paso_de_deshacer() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "a");
        editor.insertar_texto("uno\r\ndos\rtres\n");
        assert_eq!(editor.buffer().a_texto(), "auno\ndos\ntres\n");
        assert_eq!((editor.cursor().linea, editor.cursor().columna), (3, 0));

        editor.deshacer();
        assert_eq!(editor.buffer().a_texto(), "a");
    }

    #[test]
    fn insertar_texto_reemplaza_la_seleccion() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "gato perro");
        editor.inicio_archivo();
        editor.seleccionar_siguiente_ocurrencia(); // selecciona "gato"
        editor.insertar_texto("lobo");
        assert_eq!(editor.buffer().a_texto(), "lobo perro");
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

    #[test]
    fn entrar_modo_normal_recorta_el_cursor_al_ultimo_caracter_si_hacia_falta() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "hola");
        // Al escribir, el cursor queda al final ("después" de la 'a',
        // columna 4) — la posición normal en Insertar.
        assert_eq!(editor.cursor().columna, 4);

        editor.entrar_modo_normal();
        assert_eq!(editor.modo(), Modo::Normal);
        // VIM real nunca deja el cursor ahí en modo Normal: se recorta al
        // último carácter real (columna 3, la 'a').
        assert_eq!(editor.cursor().columna, 3);
    }

    #[test]
    fn entrar_modo_normal_no_recorta_si_el_cursor_ya_esta_dentro_de_rango() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "hola");
        editor.inicio_linea();
        editor.entrar_modo_normal();
        assert_eq!(editor.cursor().columna, 0);
    }

    #[test]
    fn entrar_modo_normal_en_una_linea_vacia_deja_la_columna_en_cero() {
        let editor = &mut Editor::nuevo();
        editor.entrar_modo_normal();
        assert_eq!(editor.cursor().columna, 0);
    }

    #[test]
    fn entrar_modo_insertar_vuelve_a_insertar() {
        let mut editor = Editor::nuevo();
        editor.entrar_modo_normal();
        assert_eq!(editor.modo(), Modo::Normal);
        editor.entrar_modo_insertar();
        assert_eq!(editor.modo(), Modo::Insertar);
    }

    #[test]
    fn seleccionar_derecha_extiende_sin_mover_el_ancla() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "hola mundo");
        editor.inicio_archivo();

        editor.seleccionar_derecha();
        editor.seleccionar_derecha();
        editor.seleccionar_derecha();

        let c = editor.cursores()[0];
        assert_eq!(c.ancla, Cursor { linea: 0, columna: 0 });
        assert_eq!(c.cursor, Cursor { linea: 0, columna: 3 });
        assert!(c.tiene_seleccion());
    }

    #[test]
    fn seleccionar_izquierda_desde_el_medio_extiende_hacia_atras() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "hola mundo");
        // El cursor queda al final tras escribir; ir a columna 5.
        editor.inicio_linea();
        for _ in 0..5 {
            editor.mover_derecha();
        }

        editor.seleccionar_izquierda();
        editor.seleccionar_izquierda();

        let c = editor.cursores()[0];
        assert_eq!(c.ancla, Cursor { linea: 0, columna: 5 });
        assert_eq!(c.cursor, Cursor { linea: 0, columna: 3 });
    }

    #[test]
    fn seleccionar_abajo_y_arriba_extiende_por_linea() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "uno\ndos\ntres");
        editor.inicio_archivo();

        editor.seleccionar_abajo();
        editor.seleccionar_abajo();
        let c = editor.cursores()[0];
        assert_eq!(c.ancla, Cursor { linea: 0, columna: 0 });
        assert_eq!(c.cursor.linea, 2);

        editor.seleccionar_arriba();
        let c = editor.cursores()[0];
        assert_eq!(c.ancla, Cursor { linea: 0, columna: 0 }, "el ancla no se mueve nunca");
        assert_eq!(c.cursor.linea, 1);
    }

    #[test]
    fn seleccionar_inicio_y_fin_de_linea() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "hola mundo");
        editor.inicio_linea();
        for _ in 0..4 {
            editor.mover_derecha();
        }

        editor.seleccionar_fin_linea();
        assert_eq!(editor.cursores()[0].cursor.columna, 10);
        assert_eq!(editor.cursores()[0].ancla.columna, 4);

        editor.seleccionar_inicio_linea();
        assert_eq!(editor.cursores()[0].cursor.columna, 0);
        assert_eq!(editor.cursores()[0].ancla.columna, 4, "el ancla sigue siendo la misma de siempre");
    }

    #[test]
    fn mover_sin_shift_colapsa_la_seleccion_hecha_con_shift() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "hola mundo");
        editor.inicio_archivo();
        editor.seleccionar_derecha();
        editor.seleccionar_derecha();
        assert!(editor.cursores()[0].tiene_seleccion());

        editor.mover_derecha();

        let c = editor.cursores()[0];
        assert!(!c.tiene_seleccion(), "un movimiento sin Shift debe colapsar la selección");
        assert_eq!(c.ancla, c.cursor);
    }

    #[test]
    fn seleccionar_funciona_de_forma_independiente_con_varios_cursores() {
        let mut editor = Editor::nuevo();
        escribir(&mut editor, "gato perro gato lobo");
        editor.inicio_archivo();

        // Selecciona la primera ocurrencia de "gato" y agrega un
        // segundo cursor en la siguiente — mismo mecanismo de Ctrl+D.
        editor.seleccionar_siguiente_ocurrencia();
        editor.seleccionar_siguiente_ocurrencia();
        assert_eq!(editor.cursores().len(), 2);

        // Extender selección con Shift+Right debe mover el extremo
        // activo de AMBOS cursores, cada uno desde su propia posición.
        let anclas_antes: Vec<Cursor> = editor.cursores().iter().map(|c| c.ancla).collect();
        let cursores_antes: Vec<Cursor> = editor.cursores().iter().map(|c| c.cursor).collect();

        editor.seleccionar_derecha();

        for (i, c) in editor.cursores().iter().enumerate() {
            assert_eq!(c.ancla, anclas_antes[i], "el ancla de cada cursor no debe moverse");
            assert_ne!(c.cursor, cursores_antes[i], "el extremo activo de cada cursor sí debe avanzar");
        }
    }

    /// Editor con `texto` ya cargado y el cursor en `(linea, columna)`.
    fn editor_con_cursor(texto: &str, linea: usize, columna: usize) -> Editor {
        let mut editor = Editor::nuevo();
        editor.insertar_texto(texto);
        let offset = editor.buffer().offset_byte(linea, columna);
        editor.mover_cursor_a_byte(offset);
        editor
    }

    #[test]
    fn aplicar_ediciones_de_atras_hacia_adelante_con_rangos_del_texto_original() {
        // Rangos del texto ORIGINAL, en cualquier orden: la segunda
        // edición no se corre por lo que insertó la primera.
        let mut editor = editor_con_cursor("fn main(){\nlet x=1;\n}\n", 0, 0);
        let ediciones = vec![(11..11, "    ".to_string()), (9..9, " ".to_string()), (16..17, " = ".to_string())];
        assert!(editor.aplicar_ediciones(&ediciones));
        assert_eq!(editor.buffer().a_texto(), "fn main() {\n    let x = 1;\n}\n");
    }

    #[test]
    fn aplicar_ediciones_se_deshace_en_un_solo_paso() {
        let mut editor = editor_con_cursor("a=1\nb=2\n", 0, 0);
        editor.aplicar_ediciones(&[(1..2, " = ".to_string()), (5..6, " = ".to_string())]);
        assert_eq!(editor.buffer().a_texto(), "a = 1\nb = 2\n");

        editor.deshacer();
        assert_eq!(editor.buffer().a_texto(), "a=1\nb=2\n");
        editor.rehacer();
        assert_eq!(editor.buffer().a_texto(), "a = 1\nb = 2\n");
    }

    #[test]
    fn aplicar_ediciones_conserva_el_cursor_sobre_el_mismo_codigo() {
        // Cursor sobre la "b" de la línea 1; se inserta indentación antes
        // de ella y espacios en la línea 0: tiene que seguir sobre la "b".
        let mut editor = editor_con_cursor("a=1\nb=2\n", 1, 0);
        editor.aplicar_ediciones(&[(1..2, " = ".to_string()), (4..4, "    ".to_string())]);
        assert_eq!((editor.cursor().linea, editor.cursor().columna), (1, 4));
    }

    #[test]
    fn aplicar_ediciones_con_cursor_dentro_de_un_rango_reemplazado_lo_recorta() {
        // Reemplazo de todo el archivo por algo más corto: el cursor no
        // puede quedar más allá del texto nuevo.
        let mut editor = editor_con_cursor("abcdefgh", 0, 6);
        editor.aplicar_ediciones(&[(0..8, "xy".to_string())]);
        assert_eq!(editor.buffer().a_texto(), "xy");
        assert_eq!((editor.cursor().linea, editor.cursor().columna), (0, 2));
    }

    #[test]
    fn aplicar_ediciones_con_acentos_y_emoji() {
        let texto = "let s=\"ñandú 😀\";\n";
        let mut editor = editor_con_cursor(texto, 0, 8); // sobre la "a" de "ñandú"
        let igual = texto.find('=').unwrap();
        editor.aplicar_ediciones(&[(igual..igual + 1, " = ".to_string())]);
        assert_eq!(editor.buffer().a_texto(), "let s = \"ñandú 😀\";\n");
        assert_eq!(editor.cursor().columna, 10);
    }

    #[test]
    fn aplicar_ediciones_corre_los_pliegues_si_cambia_la_cantidad_de_lineas() {
        // El formateo agrega una línea ANTES del bloque plegado: el pliegue
        // tiene que seguir cubriendo el mismo código, una línea más abajo.
        let mut editor = editor_con_cursor("use a;\nfn a() {\n    x\n}\n", 1, 0);
        editor.plegar_en_cursor(&[Pliegue { inicio: 1, fin: 2 }]);
        editor.aplicar_ediciones(&[(6..6, "\n".to_string())]);
        assert_eq!(editor.plegado().pliegues(), &[Pliegue { inicio: 2, fin: 3 }]);
    }

    #[test]
    fn aplicar_ediciones_solapadas_no_aplica_nada() {
        let mut editor = editor_con_cursor("abcdef", 0, 0);
        assert!(!editor.aplicar_ediciones(&[(0..3, "x".to_string()), (2..4, "y".to_string())]));
        assert_eq!(editor.buffer().a_texto(), "abcdef");
    }

    #[test]
    fn aplicar_ediciones_fuera_de_rango_o_a_mitad_de_caracter_no_aplica_nada() {
        let mut editor = editor_con_cursor("ñ", 0, 0);
        assert!(!editor.aplicar_ediciones(&[(0..10, "x".to_string())]));
        assert!(!editor.aplicar_ediciones(&[(1..2, "x".to_string())]));
        assert_eq!(editor.buffer().a_texto(), "ñ");
    }

    #[test]
    fn aplicar_ediciones_sin_cambios_reales_no_deja_paso_de_deshacer() {
        let mut editor = editor_con_cursor("abc", 0, 0);
        assert!(!editor.aplicar_ediciones(&[]));
        assert!(!editor.aplicar_ediciones(&[(0..1, "a".to_string())]));
        // El único paso de deshacer sigue siendo el `insertar_texto`.
        editor.deshacer();
        assert_eq!(editor.buffer().a_texto(), "");
    }

    fn editor_con(texto: &str) -> Editor {
        let mut editor = Editor::nuevo();
        editor.insertar_texto(texto);
        editor.inicio_archivo();
        editor
    }

    /// 0: fn a() {   1: x   2: y   3: }   4: fin
    const BLOQUE: &str = "fn a() {\n    x\n    y\n}\nfin";

    #[test]
    fn plegar_en_cursor_elige_el_bloque_mas_interno_y_saca_el_cursor() {
        let mut editor = editor_con(BLOQUE);
        editor.mover_abajo();
        editor.mover_abajo();
        let candidatos = [Pliegue { inicio: 0, fin: 2 }, Pliegue { inicio: 0, fin: 3 }];
        assert!(editor.plegar_en_cursor(&candidatos));
        assert_eq!(editor.plegado().pliegues(), &[Pliegue { inicio: 0, fin: 2 }]);
        // El cursor estaba en la línea 2 (oculta): pasa a la cabecera.
        assert_eq!(editor.cursor().linea, 0);
        // Repetirlo pliega el siguiente hacia afuera.
        assert!(editor.plegar_en_cursor(&candidatos));
        assert_eq!(editor.plegado().pliegues().len(), 2);
        assert!(!editor.plegar_en_cursor(&candidatos));
    }

    #[test]
    fn las_flechas_saltan_las_lineas_plegadas() {
        let mut editor = editor_con(BLOQUE);
        editor.plegar_en_cursor(&[Pliegue { inicio: 0, fin: 2 }]);
        editor.mover_abajo();
        assert_eq!(editor.cursor().linea, 3);
        editor.mover_arriba();
        assert_eq!(editor.cursor().linea, 0);
        editor.fin_linea();
        editor.mover_derecha();
        assert_eq!((editor.cursor().linea, editor.cursor().columna), (3, 0));
        editor.mover_izquierda();
        assert_eq!((editor.cursor().linea, editor.cursor().columna), (0, 8));
    }

    #[test]
    fn editar_antes_desplaza_el_pliegue_y_deshacer_lo_vuelve_a_su_lugar() {
        let mut editor = editor_con(BLOQUE);
        editor.plegar_en_cursor(&[Pliegue { inicio: 0, fin: 2 }]);
        editor.insertar_char('\n');
        assert_eq!(editor.plegado().pliegues(), &[Pliegue { inicio: 1, fin: 3 }]);
        editor.deshacer();
        assert_eq!(editor.plegado().pliegues(), &[Pliegue { inicio: 0, fin: 2 }]);
    }

    #[test]
    fn saltar_a_una_linea_plegada_la_despliega() {
        let mut editor = editor_con(BLOQUE);
        editor.plegar_en_cursor(&[Pliegue { inicio: 0, fin: 2 }]);
        let offset = editor.buffer().offset_byte(1, 4);
        editor.mover_cursor_a_byte(offset);
        assert!(editor.plegado().esta_vacio());
        assert_eq!(editor.cursor().linea, 1);
    }

    #[test]
    fn borrar_dentro_de_una_seleccion_que_cubre_el_pliegue_lo_despliega() {
        let mut editor = editor_con(BLOQUE);
        editor.plegar_en_cursor(&[Pliegue { inicio: 0, fin: 2 }]);
        editor.seleccionar_abajo();
        editor.borrar_atras();
        assert!(editor.plegado().esta_vacio());
    }

    // --- Edición básica (BACKLOG.md P0 #19) ---

    const RUST: EstiloComentario = EstiloComentario::Linea("//");

    #[test]
    fn comentar_alinea_a_la_sangria_minima_y_descomentar_lo_revierte() {
        let mut editor = editor_con("fn a() {\n    x\n\n        y\n}");
        editor.fijar_seleccion(Cursor { linea: 1, columna: 2 }, Cursor { linea: 3, columna: 3 });
        assert!(editor.alternar_comentario(RUST));
        // La línea en blanco no se toca.
        assert_eq!(editor.buffer().a_texto(), "fn a() {\n    // x\n\n    //     y\n}");
        assert!(editor.alternar_comentario(RUST));
        assert_eq!(editor.buffer().a_texto(), "fn a() {\n    x\n\n        y\n}");
    }

    #[test]
    fn comentar_una_mezcla_comenta_todas_y_se_deshace_en_un_paso() {
        let mut editor = editor_con("# ya\nno\n");
        editor.fijar_seleccion(Cursor { linea: 0, columna: 0 }, Cursor { linea: 1, columna: 2 });
        assert!(editor.alternar_comentario(EstiloComentario::Linea("#")));
        assert_eq!(editor.buffer().a_texto(), "# # ya\n# no\n");
        editor.deshacer();
        assert_eq!(editor.buffer().a_texto(), "# ya\nno\n");
    }

    #[test]
    fn comentar_sin_seleccion_mueve_el_cursor_con_el_texto() {
        let mut editor = editor_con_cursor("  select 1;", 0, 4);
        editor.alternar_comentario(EstiloComentario::Linea("--"));
        assert_eq!(editor.buffer().a_texto(), "  -- select 1;");
        assert_eq!(editor.cursor(), Cursor { linea: 0, columna: 7 });
        // Descomentar sin espacio después del prefijo también anda.
        let mut editor = editor_con(";sin espacio");
        editor.alternar_comentario(EstiloComentario::Linea(";"));
        assert_eq!(editor.buffer().a_texto(), "sin espacio");
    }

    #[test]
    fn comentar_con_bloque_envuelve_cada_linea() {
        let html = EstiloComentario::Bloque("<!--", "-->");
        let mut editor = editor_con("<p>\n  <b>x</b>\n</p>");
        editor.seleccionar_todo();
        editor.alternar_comentario(html);
        assert_eq!(editor.buffer().a_texto(), "<!-- <p> -->\n<!--   <b>x</b> -->\n<!-- </p> -->");
        editor.seleccionar_todo();
        editor.alternar_comentario(html);
        assert_eq!(editor.buffer().a_texto(), "<p>\n  <b>x</b>\n</p>");
        let mut editor = editor_con("/*a*/");
        editor.alternar_comentario(EstiloComentario::Bloque("/*", "*/"));
        assert_eq!(editor.buffer().a_texto(), "a");
    }

    #[test]
    fn comentar_con_multicursor_y_seleccion_que_termina_en_columna_cero() {
        let mut editor = editor_con("a\nb\nc\nd");
        // Seleccionar "a" y "b" enteras con Shift+↓ deja el cursor en la
        // columna 0 de "c": "c" no se comenta.
        editor.seleccionar_abajo();
        editor.seleccionar_abajo();
        editor.alternar_comentario(RUST);
        assert_eq!(editor.buffer().a_texto(), "// a\n// b\nc\nd");
        let mut editor = editor_con("a\nb\nc\nd");
        editor.agregar_cursor_abajo();
        editor.agregar_cursor_abajo();
        editor.agregar_cursor_abajo();
        editor.alternar_comentario(RUST);
        assert_eq!(editor.buffer().a_texto(), "// a\n// b\n// c\n// d");
    }

    #[test]
    fn comentar_solo_lineas_en_blanco_no_hace_nada() {
        let mut editor = editor_con("   \n");
        assert!(!editor.alternar_comentario(RUST));
        editor.deshacer();
        // Nada que deshacer de más: solo quedaba el insertar_texto inicial.
        assert_eq!(editor.buffer().a_texto(), "");
    }

    #[test]
    fn mover_linea_abajo_y_arriba_con_el_cursor() {
        let mut editor = editor_con_cursor("uno\ndos\ntres\n", 0, 2);
        assert!(editor.mover_lineas_abajo());
        assert_eq!(editor.buffer().a_texto(), "dos\nuno\ntres\n");
        assert_eq!(editor.cursor(), Cursor { linea: 1, columna: 2 });
        assert!(editor.mover_lineas_arriba());
        assert_eq!(editor.buffer().a_texto(), "uno\ndos\ntres\n");
        // Primera línea hacia arriba: no hace nada ni deja un paso vacío.
        assert!(!editor.mover_lineas_arriba());
        editor.deshacer();
        assert_eq!(editor.buffer().a_texto(), "dos\nuno\ntres\n");
    }

    #[test]
    fn mover_la_ultima_linea_de_un_archivo_sin_salto_final() {
        let mut editor = editor_con_cursor("a\nb", 1, 1);
        assert!(!editor.mover_lineas_abajo());
        assert!(editor.mover_lineas_arriba());
        assert_eq!(editor.buffer().a_texto(), "b\na");
        assert_eq!(editor.cursor(), Cursor { linea: 0, columna: 1 });
        assert!(editor.mover_lineas_abajo());
        assert_eq!(editor.buffer().a_texto(), "a\nb");
    }

    #[test]
    fn mover_una_seleccion_de_varias_lineas_la_conserva() {
        let mut editor = editor_con("a\nb\nc\nd\n");
        editor.fijar_seleccion(Cursor { linea: 0, columna: 0 }, Cursor { linea: 2, columna: 0 });
        assert!(editor.mover_lineas_abajo());
        assert_eq!(editor.buffer().a_texto(), "c\na\nb\nd\n");
        assert_eq!(editor.cursores()[0].ancla, Cursor { linea: 1, columna: 0 });
        assert_eq!(editor.cursores()[0].cursor, Cursor { linea: 3, columna: 0 });
        editor.deshacer();
        assert_eq!(editor.buffer().a_texto(), "a\nb\nc\nd\n");
    }

    #[test]
    fn mover_con_multicursor_mueve_cada_bloque() {
        let mut editor = editor_con("1\n2\n3\n4\n5");
        let offset = editor.buffer().offset_byte(1, 0);
        editor.mover_cursor_a_byte(offset);
        editor.agregar_cursor_abajo(); // líneas 1 y 2: un solo bloque
        assert!(editor.mover_lineas_abajo());
        assert_eq!(editor.buffer().a_texto(), "1\n4\n2\n3\n5");
        let lineas: Vec<usize> = editor.cursores().iter().map(|c| c.cursor.linea).collect();
        assert_eq!(lineas, vec![2, 3]);
        // Dos cursores separados: cada uno sube su línea.
        let mut editor = editor_con("1\n2\n3\n4");
        let offset = editor.buffer().offset_byte(1, 0);
        editor.mover_cursor_a_byte(offset);
        editor.agregar_cursor_abajo();
        editor.agregar_cursor_abajo();
        editor.cursores.remove(1); // quedan las líneas 1 y 3
        assert!(editor.mover_lineas_arriba());
        assert_eq!(editor.buffer().a_texto(), "2\n1\n4\n3");
    }

    #[test]
    fn mover_sobre_un_bloque_plegado_lo_salta_entero_y_lo_deja_plegado() {
        // 0: antes   1..=4: BLOQUE, con 1..=3 plegado   5: fin
        let mut editor = editor_con(&format!("antes\n{BLOQUE}"));
        let offset = editor.buffer().offset_byte(1, 0);
        editor.mover_cursor_a_byte(offset);
        editor.plegar_en_cursor(&[Pliegue { inicio: 1, fin: 3 }]);
        editor.inicio_archivo();
        assert!(editor.mover_lineas_abajo());
        assert_eq!(editor.buffer().a_texto(), "fn a() {\n    x\n    y\nantes\n}\nfin");
        assert_eq!(editor.plegado().pliegues(), &[Pliegue { inicio: 0, fin: 2 }]);
        assert_eq!(editor.cursor().linea, 3);
        // Mover la cabecera plegada mueve el bloque entero, plegado.
        editor.inicio_archivo();
        assert!(editor.mover_lineas_abajo());
        assert_eq!(editor.buffer().a_texto(), "antes\nfn a() {\n    x\n    y\n}\nfin");
        assert_eq!(editor.plegado().pliegues(), &[Pliegue { inicio: 1, fin: 3 }]);
        assert_eq!(editor.cursor().linea, 1);
    }

    #[test]
    fn duplicar_la_linea_deja_el_cursor_en_la_copia_de_abajo() {
        let mut editor = editor_con_cursor("uno\ndos", 1, 2);
        editor.duplicar_lineas();
        assert_eq!(editor.buffer().a_texto(), "uno\ndos\ndos");
        assert_eq!(editor.cursor(), Cursor { linea: 2, columna: 2 });
        editor.deshacer();
        assert_eq!(editor.buffer().a_texto(), "uno\ndos");
    }

    #[test]
    fn duplicar_una_seleccion_y_con_multicursor() {
        let mut editor = editor_con("a\nb\nc\n");
        editor.fijar_seleccion(Cursor { linea: 0, columna: 0 }, Cursor { linea: 2, columna: 0 });
        editor.duplicar_lineas();
        assert_eq!(editor.buffer().a_texto(), "a\nb\na\nb\nc\n");
        assert_eq!(editor.cursores()[0].ancla, Cursor { linea: 2, columna: 0 });
        assert_eq!(editor.cursores()[0].cursor, Cursor { linea: 4, columna: 0 });
        let mut editor = editor_con("a\nb\nc");
        editor.agregar_cursor_abajo();
        editor.agregar_cursor_abajo();
        editor.cursores.remove(1); // líneas 0 y 2
        editor.duplicar_lineas();
        assert_eq!(editor.buffer().a_texto(), "a\na\nb\nc\nc");
        let lineas: Vec<usize> = editor.cursores().iter().map(|c| c.cursor.linea).collect();
        assert_eq!(lineas, vec![1, 4]);
        editor.deshacer();
        assert_eq!(editor.buffer().a_texto(), "a\nb\nc");
    }

    #[test]
    fn duplicar_una_cabecera_plegada_duplica_el_bloque_y_conserva_el_pliegue() {
        let mut editor = editor_con(BLOQUE);
        editor.plegar_en_cursor(&[Pliegue { inicio: 0, fin: 3 }]);
        editor.duplicar_lineas();
        assert_eq!(editor.buffer().a_texto(), "fn a() {\n    x\n    y\n}\nfn a() {\n    x\n    y\n}\nfin");
        assert_eq!(editor.plegado().pliegues(), &[Pliegue { inicio: 4, fin: 7 }]);
        assert_eq!(editor.cursor().linea, 4);
    }

    #[test]
    fn seleccionar_todo_colapsa_los_cursores() {
        let mut editor = editor_con_cursor("uno\ndos", 1, 1);
        editor.agregar_cursor_arriba();
        editor.seleccionar_todo();
        assert_eq!(editor.cursores().len(), 1);
        assert_eq!(editor.texto_para_copiar().texto, "uno\ndos");
        let mut vacio = Editor::nuevo();
        vacio.seleccionar_todo();
        assert!(!vacio.cursores()[0].tiene_seleccion());
    }

    #[test]
    fn ir_a_linea_recorta_la_columna_y_despliega() {
        let mut editor = editor_con(BLOQUE);
        editor.plegar_en_cursor(&[Pliegue { inicio: 0, fin: 2 }]);
        editor.ir_a_linea(1, 99);
        assert_eq!(editor.cursor(), Cursor { linea: 1, columna: 5 });
        assert!(editor.plegado().esta_vacio());
        editor.ir_a_linea(500, 0);
        assert_eq!(editor.cursor(), Cursor { linea: 4, columna: 0 });
    }
}
