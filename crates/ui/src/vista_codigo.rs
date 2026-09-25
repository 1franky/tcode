use std::ops::Range;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use tcode_core::{linea_de_ordinal, ordinal_visible, tramo_que_oculta, Coincidencia, Editor};
use tcode_fs::MarcaGit;
use tcode_lsp::{DiagnosticoSimple, Severidad};
use tcode_syntax::{Lenguaje, Resaltador, Token};

use crate::{EstadoUi, Paleta};

/// El tramo de una línea lógica que ocupa una fila de pantalla. Sin
/// ajuste de línea hay una fila visual por línea lógica (`inicio: 0,
/// fin: linea.len()` siempre — el comportamiento de toda la vida);
/// con el ajuste activo (`Ctrl+,` → Editor → "Ajuste de línea") una
/// línea más larga que el ancho visible se parte en varias filas
/// consecutivas. `inicio`/`fin` son offsets de BYTE relativos al
/// inicio de la línea (no de carácter: hace falta para recortar el
/// `&str` sin partir un carácter UTF-8 a la mitad).
#[derive(Debug, Clone, Copy)]
struct FilaVisual {
    idx_linea: usize,
    inicio: usize,
    fin: usize,
}

impl FilaVisual {
    /// La primera fila de su línea (donde va el número en el gutter) —
    /// las siguientes son "continuación" y van sin número, igual que en
    /// VSCode y el resto de los editores con ajuste de línea.
    fn primera(&self) -> bool {
        self.inicio == 0
    }
}

/// Parte `linea` en filas de a lo sumo `ancho` CARACTERES (no bytes)
/// cada una — al menos una fila siempre, incluso para una línea vacía.
/// Ajuste "por carácter", no por palabra (no busca el espacio más
/// cercano): más simple de implementar y alcanza para el caso de uso
/// principal (líneas largas de código) — ajuste por palabra queda como
/// mejora posible a futuro si hace falta para prosa (Markdown, por
/// ejemplo).
fn filas_visuales_de(idx_linea: usize, linea: &str, ancho: usize) -> Vec<FilaVisual> {
    if linea.is_empty() {
        return vec![FilaVisual { idx_linea, inicio: 0, fin: 0 }];
    }
    let mut filas = Vec::new();
    let mut inicio = 0usize;
    let mut contador = 0usize;
    for (byte_idx, _) in linea.char_indices() {
        if contador == ancho {
            filas.push(FilaVisual { idx_linea, inicio, fin: byte_idx });
            inicio = byte_idx;
            contador = 0;
        }
        contador += 1;
    }
    filas.push(FilaVisual { idx_linea, inicio, fin: linea.len() });
    filas
}

/// Cuántas filas visuales ocupa una línea de `num_chars` caracteres con
/// ajuste de línea a `ancho` — la misma cuenta que hace
/// [`filas_visuales_de`] (al menos una fila, incluso vacía), sin
/// necesitar el texto de la línea: alcanza con su largo, que el `Buffer`
/// da sin copiar nada (`longitud_visible_linea`).
fn filas_de_linea(num_chars: usize, ancho: usize) -> usize {
    if num_chars == 0 { 1 } else { num_chars.div_ceil(ancho) }
}

/// Una posición en filas visuales relativa a una línea lógica: `(línea,
/// sub-fila dentro de esa línea)`. Así se guarda el scroll con ajuste de
/// línea (ver [`EstadoUi`]), y así se ubica el cursor.
type PosicionVisual = (usize, usize);

/// La línea visible que sigue a `linea`, salteando las ocultas por
/// bloques plegados (`ocultos`, BACKLOG.md P2 #7) — puede ser
/// `num_lineas` o más si no queda ninguna.
fn siguiente_visible(ocultos: &[Range<usize>], linea: usize) -> usize {
    tramo_que_oculta(ocultos, linea + 1).map_or(linea + 1, |t| t.end)
}

/// La línea visible anterior a `linea`, salteando las plegadas; `None`
/// en la primera. Una línea oculta siempre tiene su cabecera visible
/// justo antes del tramo (los tramos nunca empiezan en la línea 0 ni se
/// tocan entre sí), así que es `tramo.start - 1`.
fn anterior_visible(ocultos: &[Range<usize>], linea: usize) -> Option<usize> {
    let anterior = linea.checked_sub(1)?;
    Some(tramo_que_oculta(ocultos, anterior).map_or(anterior, |t| t.start - 1))
}

/// `linea` si es visible; si está dentro de un bloque plegado, la
/// cabecera del bloque (lo que se ve en su lugar).
fn visible_o_cabecera(ocultos: &[Range<usize>], linea: usize) -> usize {
    tramo_que_oculta(ocultos, linea).map_or(linea, |t| t.start - 1)
}

/// Scroll automático con ajuste de línea: el equivalente de
/// [`ajustar_scroll`] cuando una línea puede ocupar varias filas. Recibe
/// el scroll actual (`ancla`, la primera fila en pantalla) y la fila del
/// cursor, y devuelve el scroll nuevo más a cuántas filas debajo de él
/// queda el cursor (su fila en pantalla).
///
/// Antes el scroll era un índice de fila visual sobre el archivo ENTERO,
/// y ubicarlo obligaba a partir en filas todas las líneas en cada frame —
/// O(archivo) aunque solo se vieran 40 filas (BACKLOG.md P1 #14). Ahora
/// el ancla es relativa a una línea lógica y solo se recorren las líneas
/// visibles entre el ancla y el cursor, cortando en cuanto la distancia
/// pasa de `alto` (y, si el cursor quedó abajo, `alto` filas hacia atrás
/// desde él): O(alto de pantalla) sin importar el tamaño del archivo ni
/// cuánto haya saltado el cursor (`Ctrl+End`). Las líneas plegadas
/// (`ocultos`) no ocupan filas: se saltan de un tramo entero. El
/// resultado en pantalla es el mismo que antes — mismo criterio de
/// "mover lo mínimo para que el cursor se vea" — salvo cuando cambia
/// cuántas filas ocupan líneas que quedan ARRIBA de lo visible
/// (redimensionar la ventana, editar con un multi-cursor fuera de
/// pantalla, plegar algo más arriba): antes se conservaba el número de
/// fila global y lo visible se corría; ahora se conserva la línea de
/// arriba, que es lo que se espera.
///
/// `filas(l)` da cuántas filas visuales ocupa la línea visible `l` (ver
/// [`filas_de_linea`]); parámetro en vez de leer del `Buffer` acá adentro
/// para poder probarla sin armar un editor.
fn ajustar_scroll_con_ajuste(
    ancla: PosicionVisual,
    cursor: PosicionVisual,
    alto_visible: usize,
    num_lineas: usize,
    ocultos: &[Range<usize>],
    filas: impl Fn(usize) -> usize,
) -> (PosicionVisual, usize) {
    let num_lineas = num_lineas.max(1);
    // Una columna justo al final de una línea cuyo largo es múltiplo del
    // ancho cae en la sub-fila "siguiente" (`columna / ancho`); con el
    // índice global de antes eso era la primera fila de la línea visible
    // de abajo — se normaliza igual para que el cursor se dibuje donde
    // siempre.
    let (mut linea_cursor, mut subfila_cursor) = (visible_o_cabecera(ocultos, cursor.0), cursor.1);
    while siguiente_visible(ocultos, linea_cursor) < num_lineas && subfila_cursor >= filas(linea_cursor) {
        subfila_cursor -= filas(linea_cursor);
        linea_cursor = siguiente_visible(ocultos, linea_cursor);
    }
    // El texto o los pliegues pudieron cambiar desde el frame anterior
    // (líneas borradas, una línea que ahora ocupa menos filas, el ancla
    // quedó adentro de un bloque recién plegado): el ancla se lleva a una
    // fila que exista y se vea.
    let linea_ancla = visible_o_cabecera(ocultos, ancla.0.min(num_lineas - 1));
    let ancla = (linea_ancla, ancla.1.min(filas(linea_ancla) - 1));
    if alto_visible == 0 {
        return (ancla, 0);
    }
    let cursor = (linea_cursor, subfila_cursor);
    if cursor < ancla {
        return (cursor, 0);
    }

    // Distancia en filas del ancla al cursor, sin pasar de `alto_visible`.
    let distancia = if linea_cursor == ancla.0 {
        subfila_cursor - ancla.1
    } else {
        let mut distancia = filas(ancla.0) - ancla.1;
        let mut linea = siguiente_visible(ocultos, ancla.0);
        while linea < linea_cursor && distancia < alto_visible {
            distancia += filas(linea);
            linea = siguiente_visible(ocultos, linea);
        }
        distancia + subfila_cursor
    };
    if distancia < alto_visible {
        return (ancla, distancia);
    }

    // El cursor quedó debajo de lo visible: el ancla nueva es la fila que
    // deja al cursor en la última fila de la pantalla.
    let (mut linea, mut subfila) = cursor;
    let mut faltan = alto_visible - 1;
    while faltan > 0 {
        if subfila >= faltan {
            subfila -= faltan;
            break;
        }
        faltan -= subfila + 1;
        let Some(anterior) = anterior_visible(ocultos, linea) else {
            subfila = 0;
            break;
        };
        linea = anterior;
        subfila = filas(linea) - 1;
    }
    ((linea, subfila), alto_visible - 1)
}

/// Dibuja el contenido del archivo (coloreado por tree-sitter si la
/// extensión corresponde a uno de los lenguajes de M1, PLAN.md §11),
/// resalta la(s) línea(s) con cursor, subraya las líneas con diagnósticos
/// LSP (M2, PLAN.md §2: "diagnósticos inline"), resalta el fondo de las
/// coincidencias de búsqueda activas (`Ctrl+F`/`Ctrl+H`, PLAN.md §4) y
/// las selecciones de multi-cursor (`Ctrl+D`/`Ctrl+Shift+L`, PLAN.md §11
/// M3) — cada cursor adicional (más allá del principal) se marca con un
/// carácter en video invertido, porque solo puede haber un cursor REAL de
/// la terminal a la vez. `mostrar_cursor` posiciona ese cursor real en el
/// principal — solo debe ser `true` para el panel activo cuando hay
/// varios (`Ctrl+\`, PLAN.md §4). `mostrar_numeros` es
/// `config.editor.numeros_de_linea` (panel de administración, PLAN.md §5
/// M4): reserva un gutter angosto a la izquierda con el número de cada
/// línea visible, la actual resaltada con un color distinto.
/// `ajuste_linea` es `config.editor.ajuste_linea`: con el toggle
/// apagado, el comportamiento es exactamente el de siempre (líneas
/// largas se recortan al ancho visible en vez de partirse en varias
/// filas). Con el toggle prendido, una línea lógica puede ocupar varias
/// filas de pantalla consecutivas (ver [`FilaVisual`]) — `Up`/`Down`
/// siguen moviendo por línea LÓGICA, no por fila visual, igual que
/// antes de esta pieza: reinterpretarlos como movimiento "visual" queda
/// fuera de alcance por ahora (tocaría el cursor del `core`, compartido
/// con multi-cursor y demás, no solo el renderizado).
/// `columna_regla` es `config.editor.columna_regla` (BACKLOG.md P1 #5,
/// `None` = apagada): marca esa columna de CADA fila visual con un fondo
/// distinto (`Paleta::regla_vertical`) — relativa a la fila de pantalla,
/// no a la línea lógica, así que con ajuste de línea activo se ve en la
/// misma columna de pantalla en todas las filas de una línea partida,
/// consistente con cómo se ve en cualquier otro editor.
/// `marcas_git` son los indicadores de git (BACKLOG.md P2 #6, ver
/// `tcode_fs::DiffGit`): `None` = sin columna de git (toggle apagado, o
/// archivo sin base en `HEAD`); `Some` reserva una columna de 1 carácter
/// entre los números y el código — ver [`dibujar_gutter`].
#[allow(clippy::too_many_arguments)]
pub fn dibujar(
    frame: &mut Frame,
    area: Rect,
    editor: &Editor,
    estado: &mut EstadoUi,
    paleta: &Paleta,
    resaltador: &mut Resaltador,
    ruta: &str,
    mostrar_cursor: bool,
    diagnosticos: &[DiagnosticoSimple],
    coincidencias_busqueda: &[Coincidencia],
    indice_coincidencia_actual: Option<usize>,
    mostrar_numeros: bool,
    ajuste_linea: bool,
    columna_regla: Option<usize>,
    marcas_git: Option<&[Option<MarcaGit>]>,
) {
    estado.posicion_cursor = None;
    let buffer = editor.buffer();
    let (area_gutter, area) = dividir_gutter(area, buffer.num_lineas(), mostrar_numeros, marcas_git.is_some());

    let alto_visible = area.height as usize;
    let ancho_visible = area.width as usize;
    let ancho = ancho_visible.max(1);
    let cursor = editor.cursor();
    let cursores = editor.cursores();
    // Líneas ocultas por bloques plegados (BACKLOG.md P2 #7): no se
    // dibujan, y el scroll cuenta solo filas de líneas visibles.
    let ocultos = editor.plegado().tramos_ocultos();

    // Solo las filas que entran en pantalla, leyendo del buffer
    // únicamente las líneas visibles — copiar todas las líneas en cada
    // frame costaba ~3 ms con 10.000 líneas (BACKLOG.md P1 #14). Sin
    // ajuste de línea hay una fila por línea visible (`filas[0]` es la
    // línea visible número `estado.scroll`); con ajuste, el scroll es la
    // posición `(estado.scroll, estado.subfila_scroll)` — línea lógica +
    // sub-fila, ver `ajustar_scroll_con_ajuste` — y se parten en filas
    // solo las líneas desde ahí hasta llenar la pantalla. En los dos
    // casos las líneas plegadas se saltan de un tramo entero.
    let (filas, fila_cursor_en_pantalla) = if ajuste_linea {
        let filas_de = |l: usize| filas_de_linea(buffer.longitud_visible_linea(l), ancho);
        let ancla = (estado.scroll, estado.subfila_scroll);
        let cursor_visual = (cursor.linea, cursor.columna / ancho);
        let (ancla, fila_cursor) =
            ajustar_scroll_con_ajuste(ancla, cursor_visual, alto_visible, buffer.num_lineas(), &ocultos, filas_de);
        (estado.scroll, estado.subfila_scroll) = ancla;
        let mut filas: Vec<FilaVisual> = Vec::with_capacity(alto_visible);
        let mut linea = ancla.0;
        let mut saltear = ancla.1;
        while filas.len() < alto_visible && linea < buffer.num_lineas() {
            let texto = buffer.linea_texto(linea);
            let restantes = alto_visible - filas.len();
            filas.extend(filas_visuales_de(linea, &texto, ancho).into_iter().skip(saltear).take(restantes));
            saltear = 0;
            linea = siguiente_visible(&ocultos, linea);
        }
        (filas, fila_cursor)
    } else {
        estado.subfila_scroll = 0;
        // Sin pliegues, `linea_de_ordinal`/`ordinal_visible` son la
        // identidad y esto es exactamente lo de antes: las líneas
        // `scroll..scroll + alto`.
        let fila_cursor = ordinal_visible(&ocultos, cursor.linea);
        ajustar_scroll(estado, fila_cursor, alto_visible);
        let mut filas = Vec::with_capacity(alto_visible);
        let mut idx = linea_de_ordinal(&ocultos, estado.scroll);
        while filas.len() < alto_visible && idx < buffer.num_lineas() {
            if let Some(tramo) = tramo_que_oculta(&ocultos, idx) {
                idx = tramo.end;
                continue;
            }
            filas.push(FilaVisual { idx_linea: idx, inicio: 0, fin: buffer.linea_texto(idx).len() });
            idx += 1;
        }
        (filas, fila_cursor.saturating_sub(estado.scroll))
    };
    // Texto de las líneas que tocan las filas visibles, indexado desde la
    // primera de ellas. Las ocultas por un pliegue en el medio no se
    // leen del buffer (quedan vacías): con todo plegado, lo visible puede
    // abarcar el archivo entero.
    let primera_linea = filas.first().map_or(0, |f| f.idx_linea);
    let ultima_linea = filas.last().map_or(0, |f| f.idx_linea);
    let lineas: Vec<String> = if filas.is_empty() {
        Vec::new()
    } else {
        (primera_linea..=ultima_linea)
            .map(|i| if tramo_que_oculta(&ocultos, i).is_some() { String::new() } else { buffer.linea_texto(i) })
            .collect()
    };

    let tramos_visibles = tramos_bytes_visibles(editor, &filas);
    let tokens = calcular_tokens(editor, resaltador, ruta, &tramos_visibles);
    let lineas_con_cursor: Vec<usize> = cursores.iter().map(|c| c.cursor.linea).collect();

    let visibles: Vec<Line> = filas
        .iter()
        .map(|fila| {
            let linea = &lineas[fila.idx_linea - primera_linea];
            let inicio_byte_linea = editor.buffer().inicio_byte_linea(fila.idx_linea);
            let inicio_byte = inicio_byte_linea + fila.inicio;
            let fin_byte = inicio_byte_linea + fila.fin;
            let texto_fila = &linea[fila.inicio..fila.fin];
            let mut spans = spans_de_linea(
                texto_fila,
                inicio_byte,
                &tokens,
                paleta,
                coincidencias_busqueda,
                indice_coincidencia_actual,
            );

            for c in cursores {
                if !c.tiene_seleccion() {
                    continue;
                }
                let a = editor.buffer().offset_byte(c.ancla.linea, c.ancla.columna);
                let b = editor.buffer().offset_byte(c.cursor.linea, c.cursor.columna);
                let (inicio_sel, fin_sel) = (a.min(b), a.max(b));
                if fin_sel <= inicio_byte || inicio_sel >= fin_byte {
                    continue;
                }
                let inicio_local = inicio_sel.max(inicio_byte) - inicio_byte;
                let fin_local = fin_sel.min(fin_byte) - inicio_byte;
                spans = transformar_rango(spans, inicio_local, fin_local, |estilo| {
                    if estilo.bg.is_none() {
                        estilo.bg(paleta.seleccion)
                    } else {
                        estilo
                    }
                });
            }

            // Marcador de bloque plegado (BACKLOG.md P2 #7) al final de la
            // cabecera — en su última fila si la línea está partida por el
            // ajuste de línea. Solo ASCII: los símbolos Unicode de ancho
            // ambiguo (el de puntos suspensivos, los triángulos) desalinean
            // el render en Windows Terminal (ver el fix de v0.5.1).
            if fila.fin == linea.len() && ocultos.iter().any(|t| t.start == fila.idx_linea + 1) {
                spans.push(Span::styled(" ... ", Style::default().fg(paleta.numero_linea).bg(paleta.seleccion)));
            }

            // Regla vertical (BACKLOG.md P1 #5): entre selección/búsqueda
            // (que ya corrieron arriba y ganan si coinciden en la misma
            // columna — `transformar_rango` de acá abajo no pisa un
            // fondo que ya esté puesto) y el resaltado de "línea actual"
            // de más abajo (que rellena todo lo que siga sin fondo,
            // incluida esta columna si la regla no llegó a pintarla
            // antes) — por eso va ACÁ, no después: si fuera después de
            // "línea actual" nunca se vería en la línea con el cursor,
            // que es precisamente donde más sirve verla mientras se
            // escribe.
            if let Some(columna_regla) = columna_regla {
                if columna_regla >= 1 && columna_regla <= ancho_visible {
                    let indice_char = columna_regla - 1;
                    let ancho_actual: usize = spans.iter().map(|s| s.content.chars().count()).sum();
                    if ancho_actual <= indice_char {
                        spans.push(Span::raw(" ".repeat(indice_char + 1 - ancho_actual)));
                    }
                    if let Some((inicio, fin)) = rango_char_en_spans(&spans, indice_char) {
                        spans = transformar_rango(spans, inicio, fin, |estilo| {
                            if estilo.bg.is_none() { estilo.bg(paleta.regla_vertical) } else { estilo }
                        });
                    }
                }
            }

            // Marcador de los cursores adicionales (todo menos el
            // principal, índice 0, que usa el cursor real de la terminal
            // — ver doc de esta función). Con ajuste de línea, una
            // línea puede tener varias filas: el marcador va solo en la
            // que de verdad contiene el cursor, no en todas las de esa
            // línea.
            for c in cursores.iter().skip(1) {
                if c.cursor.linea != fila.idx_linea {
                    continue;
                }
                let offset_abs = editor.buffer().offset_byte(c.cursor.linea, c.cursor.columna);
                let en_esta_fila =
                    offset_abs >= inicio_byte && (offset_abs < fin_byte || (offset_abs == fin_byte && fila.fin == linea.len()));
                if !en_esta_fila {
                    continue;
                }
                let offset_local = offset_abs - inicio_byte;
                if offset_local >= texto_fila.len() {
                    // Al final de la fila no hay carácter que invertir:
                    // se agrega un espacio de relleno marcado en su lugar.
                    spans.push(Span::styled(" ", Style::default().add_modifier(Modifier::REVERSED)));
                    continue;
                }
                let ancho_char = texto_fila[offset_local..].chars().next().map(char::len_utf8).unwrap_or(1);
                spans = transformar_rango(spans, offset_local, offset_local + ancho_char, |estilo| {
                    estilo.add_modifier(Modifier::REVERSED)
                });
            }

            if lineas_con_cursor.contains(&fila.idx_linea) {
                // Se añade un span final de relleno para que el resaltado
                // de la línea actual cubra todo el ancho, no solo el
                // texto. `ocupado` se calcula sobre `spans`, no sobre
                // `texto_fila`: la regla vertical de más arriba puede
                // haber agregado ya su propio padding si la línea era
                // más corta que su columna — contar desde `texto_fila`
                // acá subestimaría cuánto falta y duplicaría relleno.
                let ocupado: usize = spans.iter().map(|s| s.content.chars().count()).sum();
                if ancho_visible > ocupado {
                    spans.push(Span::raw(" ".repeat(ancho_visible - ocupado)));
                }
                for span in &mut spans {
                    // No pisa el fondo si el span ya tiene uno propio (una
                    // coincidencia de búsqueda o una selección): esa señal
                    // debe seguir siendo visible aunque el cursor esté en
                    // esa línea.
                    if span.style.bg.is_none() {
                        span.style = span.style.bg(paleta.linea_actual);
                    }
                }
            }
            if let Some(severidad) = severidad_mas_grave_en_linea(diagnosticos, fila.idx_linea) {
                let color = color_severidad(paleta, severidad);
                for span in &mut spans {
                    // Subraya sin tocar el color del texto (preserva el
                    // resaltado de sintaxis): `underline_color` separa el
                    // color del subrayado del color del texto.
                    span.style = span.style.add_modifier(Modifier::UNDERLINED).underline_color(color);
                }
            }
            Line::from(spans)
        })
        .collect();

    frame.render_widget(
        Paragraph::new(visibles).style(Style::default().bg(paleta.fondo).fg(paleta.texto)),
        area,
    );

    if let Some(area_gutter) = area_gutter {
        dibujar_gutter(frame, area_gutter, &filas, alto_visible, cursor.linea, paleta, mostrar_numeros, marcas_git);
    }

    if mostrar_cursor {
        let columna_local = if ajuste_linea { cursor.columna % ancho } else { cursor.columna };
        let columna = area.x + columna_local as u16;
        let fila_pantalla = area.y + fila_cursor_en_pantalla as u16;
        frame.set_cursor_position((columna, fila_pantalla));
        estado.posicion_cursor = Some((columna, fila_pantalla));
    }
}

/// El diagnóstico más grave (error > advertencia > información >
/// sugerencia) que cubre la línea `idx_linea`, si hay alguno.
fn severidad_mas_grave_en_linea(diagnosticos: &[DiagnosticoSimple], idx_linea: usize) -> Option<Severidad> {
    diagnosticos
        .iter()
        .filter(|d| (d.linea_inicio as usize) <= idx_linea && idx_linea <= (d.linea_fin as usize))
        .map(|d| d.severidad)
        .min_by_key(|severidad| match severidad {
            Severidad::Error => 0,
            Severidad::Advertencia => 1,
            Severidad::Informacion => 2,
            Severidad::Sugerencia => 3,
        })
}

fn color_severidad(paleta: &Paleta, severidad: Severidad) -> ratatui::style::Color {
    match severidad {
        Severidad::Error => paleta.diagnostico_error,
        Severidad::Advertencia => paleta.diagnostico_advertencia,
        Severidad::Informacion => paleta.diagnostico_info,
        Severidad::Sugerencia => paleta.diagnostico_sugerencia,
    }
}

/// Reparte `area` entre el gutter de números de línea (ancho fijo, según
/// la cantidad de dígitos de la última línea del archivo) y el área de
/// código en sí. Si `mostrar_numeros` es `false`, o la ventana es
/// demasiado angosta para reservarle aunque sea 3 columnas al gutter (2
/// dígitos + 1 espacio de separación), no hay gutter: se devuelve `(None,
/// area)` sin recortar nada, priorizando el código sobre los números.
///
/// `con_git` suma la columna de los indicadores de git (BACKLOG.md P2 #6)
/// entre los números y el espacio de separación. Con los números
/// apagados, el gutter aparece igual si hay columna de git (marca +
/// espacio, 2 columnas): el usuario apagó los números, no las marcas —
/// para no verlas está su propio toggle. Como `con_git` solo es `true`
/// para archivos con base en `HEAD`, un archivo fuera de un repo sigue
/// sin gutter con los números apagados, igual que antes de esta pieza.
fn dividir_gutter(area: Rect, total_lineas: usize, mostrar_numeros: bool, con_git: bool) -> (Option<Rect>, Rect) {
    if !mostrar_numeros && !con_git {
        return (None, area);
    }
    let ancho_numero = if mostrar_numeros { total_lineas.max(1).to_string().len().max(2) as u16 } else { 0 };
    let ancho_gutter = ancho_numero + u16::from(con_git) + 1;
    if area.width <= ancho_gutter {
        return (None, area);
    }
    let partes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(ancho_gutter), Constraint::Min(1)])
        .split(area);
    (Some(partes[0]), partes[1])
}

/// Dibuja los números de línea de las filas visibles (`filas` son las
/// mismas que se ven en el código, ya recortadas al scroll, en términos
/// de FILA VISUAL — con ajuste de línea
/// activo, varias filas seguidas pueden compartir línea lógica), alineados
/// a la derecha con un espacio de separación antes del código. Solo la
/// primera fila de cada línea (`FilaVisual::primera`) muestra el número
/// — las de continuación van en blanco, igual que en VSCode y el resto
/// de los editores con ajuste de línea. Las filas que quedan más allá
/// del final del archivo (ventana más alta que el contenido) también van
/// en blanco en vez de mostrar números inexistentes.
///
/// Con `marcas_git` (BACKLOG.md P2 #6), una columna más entre el número y
/// el espacio de separación: `+` agregada, `~` modificada, `-` hay líneas
/// borradas justo antes de esta, cada una con su color de la sección
/// `[git]` del tema. ASCII a propósito, no `▎`/`▔` como en otros
/// editores: los símbolos de ancho "ambiguo" desalineaban Windows
/// Terminal (ver `BORDE_ASCII` en `lib.rs`). Agregada/modificada se
/// repiten en las filas de continuación de una línea partida por el
/// ajuste de línea (la línea entera cambió); borrada va solo en la
/// primera, que es donde está el hueco.
#[allow(clippy::too_many_arguments)]
fn dibujar_gutter(
    frame: &mut Frame,
    area: Rect,
    filas: &[FilaVisual],
    alto_visible: usize,
    linea_cursor: usize,
    paleta: &Paleta,
    mostrar_numeros: bool,
    marcas_git: Option<&[Option<MarcaGit>]>,
) {
    let ancho_git = usize::from(marcas_git.is_some());
    let ancho_numero = if mostrar_numeros { (area.width as usize).saturating_sub(1 + ancho_git) } else { 0 };
    let fondo = Style::default().bg(paleta.fondo);
    let en_blanco = || Line::from(Span::styled(" ".repeat(area.width as usize), fondo));
    let filas_pantalla: Vec<Line> = (0..alto_visible)
        .map(|offset| {
            let Some(fila) = filas.get(offset) else { return en_blanco() };
            let mut spans = Vec::with_capacity(3);
            if ancho_numero > 0 {
                let texto = if fila.primera() {
                    format!("{:>ancho$}", fila.idx_linea + 1, ancho = ancho_numero)
                } else {
                    " ".repeat(ancho_numero)
                };
                let color = if fila.idx_linea == linea_cursor { paleta.numero_linea_activo } else { paleta.numero_linea };
                spans.push(Span::styled(texto, fondo.fg(color)));
            }
            if let Some(marcas) = marcas_git {
                let marca = match marcas.get(fila.idx_linea).copied().flatten() {
                    Some(MarcaGit::Agregada) => Some(("+", paleta.git_agregada)),
                    Some(MarcaGit::Modificada) => Some(("~", paleta.git_modificada)),
                    Some(MarcaGit::Borrada) if fila.primera() => Some(("-", paleta.git_borrada)),
                    _ => None,
                };
                spans.push(match marca {
                    Some((simbolo, color)) => Span::styled(simbolo, fondo.fg(color)),
                    None => Span::styled(" ", fondo),
                });
            }
            spans.push(Span::styled(" ", fondo));
            Line::from(spans)
        })
        .collect();
    frame.render_widget(Paragraph::new(filas_pantalla), area);
}

/// Rangos de bytes del texto que ocupan las filas visibles — lo único
/// que hace falta resaltar. Uno solo (de la primera fila en pantalla a la
/// última) salvo que haya bloques plegados en el medio: ahí se corta en
/// un tramo por cada grupo de líneas consecutivas, para no resaltar
/// también todo lo oculto entre ellos.
fn tramos_bytes_visibles(editor: &Editor, filas: &[FilaVisual]) -> Vec<Range<usize>> {
    let buffer = editor.buffer();
    let mut tramos: Vec<Range<usize>> = Vec::new();
    let mut linea_anterior: Option<usize> = None;
    for fila in filas {
        let inicio_linea = buffer.inicio_byte_linea(fila.idx_linea);
        let (inicio, fin) = (inicio_linea + fila.inicio, inicio_linea + fila.fin);
        match (tramos.last_mut(), linea_anterior) {
            (Some(ultimo), Some(anterior)) if fila.idx_linea <= anterior + 1 => ultimo.end = fin,
            _ => tramos.push(inicio..fin),
        }
        linea_anterior = Some(fila.idx_linea);
    }
    tramos
}

/// Resalta lo visible del archivo si su extensión corresponde a un
/// lenguaje soportado; si no, o si el parseo falla, se sigue mostrando el
/// texto sin colorear (nunca rompe el render). La ruta identifica al
/// documento para que el resaltador re-parsee de forma incremental (ver
/// `Resaltador::resaltar_documento_tramos_versionado`).
fn calcular_tokens(editor: &Editor, resaltador: &mut Resaltador, ruta: &str, tramos: &[Range<usize>]) -> Vec<Token> {
    let Some(lenguaje) = Lenguaje::detectar_por_extension(ruta) else {
        return Vec::new();
    };
    // Con la revisión del buffer el resaltador se saltea `a_texto()` (una
    // copia del archivo entero) y la comparación contra el texto anterior
    // en todos los frames sin edición (BACKLOG.md P1 #14).
    let buffer = editor.buffer();
    resaltador
        .resaltar_documento_tramos_versionado(ruta, lenguaje, Some(buffer.revision()), || buffer.a_texto(), tramos)
        .unwrap_or_default()
}

/// Construye los spans coloreados de una línea a partir de los tokens del
/// archivo completo (en offsets de byte) y de las coincidencias de
/// búsqueda activas (mismo tipo de offset, ver `tcode_core::busqueda`),
/// recortados ambos al rango de esta línea. Como un rango de búsqueda
/// puede caer a mitad de un token de sintaxis (o viceversa), se calculan
/// los puntos de corte combinados de ambas fuentes y se resuelve el
/// estilo (color de texto + fondo de coincidencia) por cada segmento
/// resultante.
fn spans_de_linea<'a>(
    texto_linea: &'a str,
    inicio_byte_linea: usize,
    tokens: &[Token],
    paleta: &Paleta,
    coincidencias: &[Coincidencia],
    indice_coincidencia_actual: Option<usize>,
) -> Vec<Span<'a>> {
    let fin_byte_linea = inicio_byte_linea + texto_linea.len();

    // Los tokens vienen en orden de aparición y sin solaparse (ver
    // `tcode_syntax::Token`): búsqueda binaria del primero que llega a
    // esta línea en vez de recorrer los del archivo completo por cada
    // fila visible.
    let primero = tokens.partition_point(|t| t.fin <= inicio_byte_linea);
    let tokens_en_linea: Vec<&Token> =
        tokens[primero..].iter().take_while(|t| t.inicio < fin_byte_linea).collect();

    // Rangos de coincidencia recortados a offsets LOCALES (relativos al
    // inicio de esta línea), con si cada una es la coincidencia actual.
    let coincidencias_en_linea: Vec<(usize, usize, bool)> = coincidencias
        .iter()
        .enumerate()
        .filter(|(_, c)| c.fin > inicio_byte_linea && c.inicio < fin_byte_linea)
        .map(|(i, c)| {
            let inicio = c.inicio.max(inicio_byte_linea) - inicio_byte_linea;
            let fin = c.fin.min(fin_byte_linea) - inicio_byte_linea;
            (inicio, fin, Some(i) == indice_coincidencia_actual)
        })
        .collect();

    if tokens_en_linea.is_empty() && coincidencias_en_linea.is_empty() {
        return vec![Span::raw(texto_linea)];
    }

    let mut puntos: Vec<usize> = vec![0, texto_linea.len()];
    for t in &tokens_en_linea {
        puntos.push(t.inicio.max(inicio_byte_linea) - inicio_byte_linea);
        puntos.push(t.fin.min(fin_byte_linea) - inicio_byte_linea);
    }
    for (inicio, fin, _) in &coincidencias_en_linea {
        puntos.push(*inicio);
        puntos.push(*fin);
    }
    puntos.sort_unstable();
    puntos.dedup();

    let mut spans = Vec::new();
    for ventana in puntos.windows(2) {
        let (inicio, fin) = (ventana[0], ventana[1]);
        if inicio >= fin {
            continue;
        }

        let offset_abs = inicio_byte_linea + inicio;
        let mut estilo = tokens_en_linea
            .iter()
            .find(|t| t.inicio <= offset_abs && offset_abs < t.fin)
            .map(|t| paleta.estilo_sintaxis(t.nombre))
            .unwrap_or_default();

        if let Some((_, _, actual)) =
            coincidencias_en_linea.iter().find(|(ci, cf, _)| inicio >= *ci && fin <= *cf)
        {
            let color = if *actual { paleta.busqueda_actual } else { paleta.busqueda_otras };
            estilo = estilo.bg(color);
        }

        spans.push(Span::styled(&texto_linea[inicio..fin], estilo));
    }

    spans
}

/// Post-procesa `spans` (ya construidos por `spans_de_linea`) aplicando
/// `transformar` al estilo del tramo `[inicio_local, fin_local)`
/// (offsets de bytes relativos al inicio de la línea) — partiendo los
/// spans que caen a mitad del rango. Usado para pintar selecciones y
/// marcar cursores secundarios (multi-cursor, PLAN.md §11 M3) sin
/// duplicar la lógica de partir spans en cada caso.
/// Rango de bytes, dentro del contenido concatenado de `spans` (el mismo
/// espacio de coordenadas que usa [`transformar_rango`] — sus posiciones
/// son acumuladas a través de todos los spans, no relativas a uno solo),
/// que ocupa el carácter en el índice `indice_char` (0-indexado). `None`
/// si `spans` tiene menos caracteres que `indice_char + 1` — quien llama
/// (`dibujar`) ya se asegura de que no pase agregando el padding que
/// haga falta antes de pedir este rango, así que en la práctica esto no
/// debería devolver `None`, pero no hay motivo para entrar en pánico si
/// pasara.
fn rango_char_en_spans(spans: &[Span], indice_char: usize) -> Option<(usize, usize)> {
    let mut char_actual = 0usize;
    let mut byte_actual = 0usize;
    for span in spans {
        for c in span.content.chars() {
            if char_actual == indice_char {
                return Some((byte_actual, byte_actual + c.len_utf8()));
            }
            char_actual += 1;
            byte_actual += c.len_utf8();
        }
    }
    None
}

fn transformar_rango<'a>(
    spans: Vec<Span<'a>>,
    inicio_local: usize,
    fin_local: usize,
    transformar: impl Fn(Style) -> Style,
) -> Vec<Span<'a>> {
    if inicio_local >= fin_local {
        return spans;
    }

    let mut resultado = Vec::with_capacity(spans.len() + 2);
    let mut cursor = 0usize;
    for span in spans {
        let len = span.content.len();
        let (inicio_span, fin_span) = (cursor, cursor + len);
        cursor = fin_span;

        if len == 0 || fin_span <= inicio_local || inicio_span >= fin_local {
            resultado.push(span);
            continue;
        }

        let corte_izq = inicio_local.saturating_sub(inicio_span).min(len);
        let corte_der = fin_local.saturating_sub(inicio_span).min(len);
        let estilo_original = span.style;
        let contenido = span.content.into_owned();

        if corte_izq > 0 {
            resultado.push(Span::styled(contenido[..corte_izq].to_string(), estilo_original));
        }
        resultado.push(Span::styled(contenido[corte_izq..corte_der].to_string(), transformar(estilo_original)));
        if corte_der < len {
            resultado.push(Span::styled(contenido[corte_der..].to_string(), estilo_original));
        }
    }
    resultado
}

/// Desplazamiento vertical automático: mantiene la línea del cursor siempre
/// visible dentro del área disponible. Es un concern puro de renderizado,
/// no vive en el `core`.
fn ajustar_scroll(estado: &mut EstadoUi, linea_cursor: usize, alto_visible: usize) {
    if alto_visible == 0 {
        return;
    }
    if linea_cursor < estado.scroll {
        estado.scroll = linea_cursor;
    } else if linea_cursor >= estado.scroll + alto_visible {
        estado.scroll = linea_cursor - alto_visible + 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texto(spans: &[Span]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn transformar_rango_parte_un_span_a_la_mitad() {
        let spans = vec![Span::raw("hola mundo")];
        let resultado = transformar_rango(spans, 5, 10, |e| e.bg(ratatui::style::Color::Red));
        assert_eq!(texto(&resultado), "hola mundo");
        // "hola " (sin tocar), "mundo" (con el fondo nuevo).
        assert_eq!(resultado.len(), 2);
        assert_eq!(resultado[0].content.as_ref(), "hola ");
        assert!(resultado[0].style.bg.is_none());
        assert_eq!(resultado[1].content.as_ref(), "mundo");
        assert_eq!(resultado[1].style.bg, Some(ratatui::style::Color::Red));
    }

    #[test]
    fn transformar_rango_no_pisa_un_fondo_que_ya_estaba_si_la_transformacion_lo_respeta() {
        let spans = vec![Span::styled("hola", Style::default().bg(ratatui::style::Color::Blue))];
        let resultado =
            transformar_rango(spans, 0, 4, |e| if e.bg.is_none() { e.bg(ratatui::style::Color::Red) } else { e });
        assert_eq!(resultado[0].style.bg, Some(ratatui::style::Color::Blue));
    }

    #[test]
    fn transformar_rango_atraviesa_varios_spans() {
        let spans = vec![Span::raw("ab"), Span::raw("cd"), Span::raw("ef")];
        // Rango [1,5): mitad de "ab", todo "cd", mitad de "ef" no — de
        // hecho [1,5) cubre 'b','c','d','e' (offsets 1..5 sobre "abcdef").
        let resultado = transformar_rango(spans, 1, 5, |e| e.add_modifier(Modifier::BOLD));
        assert_eq!(texto(&resultado), "abcdef");
        let en_negrita: String = resultado
            .iter()
            .filter(|s| s.style.add_modifier.contains(Modifier::BOLD))
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(en_negrita, "bcde");
    }

    #[test]
    fn transformar_rango_vacio_no_cambia_nada() {
        let spans = vec![Span::raw("hola")];
        let resultado = transformar_rango(spans.clone(), 2, 2, |e| e.bg(ratatui::style::Color::Red));
        assert_eq!(texto(&resultado), texto(&spans));
        assert!(resultado[0].style.bg.is_none());
    }

    #[test]
    fn rango_char_en_spans_encuentra_el_byte_del_indice_pedido() {
        let spans = vec![Span::raw("hola "), Span::raw("mundo")];
        // Índice de carácter 6 = la 'u' de "mundo" (0-indexado: h-o-l-a-
        // espacio-m-u...).
        assert_eq!(rango_char_en_spans(&spans, 6), Some((6, 7)));
    }

    #[test]
    fn rango_char_en_spans_respeta_caracteres_multibyte() {
        // "á" ocupa 2 bytes en UTF-8: el índice de carácter 1 ('é') debe
        // caer en el byte 2, no en el 1.
        let spans = vec![Span::raw("áé")];
        assert_eq!(rango_char_en_spans(&spans, 0), Some((0, 2)));
        assert_eq!(rango_char_en_spans(&spans, 1), Some((2, 4)));
    }

    #[test]
    fn rango_char_en_spans_mas_alla_del_contenido_da_none() {
        let spans = vec![Span::raw("hi")];
        assert_eq!(rango_char_en_spans(&spans, 5), None);
    }

    #[test]
    fn rango_char_en_spans_con_spans_vacio_da_none() {
        assert_eq!(rango_char_en_spans(&[], 0), None);
    }

    fn textos_de_filas<'a>(linea: &'a str, filas: &[FilaVisual]) -> Vec<&'a str> {
        filas.iter().map(|f| &linea[f.inicio..f.fin]).collect()
    }

    #[test]
    fn filas_visuales_de_linea_vacia_da_una_sola_fila_vacia() {
        let filas = filas_visuales_de(0, "", 10);
        assert_eq!(filas.len(), 1);
        assert_eq!(filas[0].inicio, 0);
        assert_eq!(filas[0].fin, 0);
        assert!(filas[0].primera());
    }

    #[test]
    fn filas_visuales_de_linea_mas_corta_que_el_ancho_no_se_parte() {
        let filas = filas_visuales_de(0, "hola", 10);
        assert_eq!(textos_de_filas("hola", &filas), vec!["hola"]);
    }

    #[test]
    fn filas_visuales_de_multiplo_exacto_del_ancho() {
        // 6 caracteres, ancho 3: exactamente 2 filas, ninguna a medias.
        let filas = filas_visuales_de(0, "abcdef", 3);
        assert_eq!(textos_de_filas("abcdef", &filas), vec!["abc", "def"]);
    }

    #[test]
    fn filas_visuales_de_deja_la_ultima_fila_mas_corta() {
        // 8 caracteres, ancho 3: dos filas completas más un resto de 2.
        let filas = filas_visuales_de(0, "abcdefgh", 3);
        assert_eq!(textos_de_filas("abcdefgh", &filas), vec!["abc", "def", "gh"]);
        assert!(filas[0].primera());
        assert!(!filas[1].primera());
        assert!(!filas[2].primera());
    }

    #[test]
    fn filas_visuales_de_no_parte_un_caracter_utf8_a_la_mitad() {
        // "áéí" son 3 caracteres pero 6 bytes (2 cada uno) — con ancho 2
        // (en CARACTERES) la primera fila debe ser "áé" completo, no
        // "á" + medio byte de "é".
        let filas = filas_visuales_de(0, "áéíóú", 2);
        assert_eq!(textos_de_filas("áéíóú", &filas), vec!["áé", "íó", "ú"]);
    }

    /// Filas por línea de un archivo de prueba: la línea `i` ocupa
    /// `largos[i]` filas.
    fn ajustar(largos: &[usize], ancla: PosicionVisual, cursor: PosicionVisual, alto: usize) -> (PosicionVisual, usize) {
        ajustar_plegado(largos, &[], ancla, cursor, alto)
    }

    fn ajustar_plegado(
        largos: &[usize],
        ocultos: &[Range<usize>],
        ancla: PosicionVisual,
        cursor: PosicionVisual,
        alto: usize,
    ) -> (PosicionVisual, usize) {
        ajustar_scroll_con_ajuste(ancla, cursor, alto, largos.len(), ocultos, |l| {
            assert!(tramo_que_oculta(ocultos, l).is_none(), "nunca pide las filas de una línea plegada ({l})");
            largos[l]
        })
    }

    /// Referencia: el algoritmo de antes (scroll = índice de fila visual
    /// global, contando las filas de todo el archivo), para comparar.
    fn ajustar_como_antes(largos: &[usize], scroll: usize, cursor: PosicionVisual, alto: usize) -> (usize, usize) {
        let fila_cursor = a_global(largos, cursor);
        let mut scroll = scroll;
        if fila_cursor < scroll {
            scroll = fila_cursor;
        } else if fila_cursor >= scroll + alto {
            scroll = fila_cursor - alto + 1;
        }
        (scroll, fila_cursor - scroll)
    }

    fn a_global(largos: &[usize], posicion: PosicionVisual) -> usize {
        a_global_plegado(largos, &[], posicion)
    }

    /// Índice de fila visual global contando solo las líneas visibles
    /// (las plegadas no ocupan filas) — el espacio de coordenadas del
    /// scroll con ajuste de línea antes de BACKLOG.md P1 #14.
    fn a_global_plegado(largos: &[usize], ocultos: &[Range<usize>], posicion: PosicionVisual) -> usize {
        let antes: usize =
            (0..posicion.0).filter(|&l| tramo_que_oculta(ocultos, l).is_none()).map(|l| largos[l]).sum();
        antes + posicion.1
    }

    #[test]
    fn ajustar_con_ajuste_saltea_las_lineas_plegadas() {
        // Línea 1 (3 filas) oculta bajo la 0: el cursor en la 2 queda en
        // la fila 1, igual que si la 1 no existiera.
        let largos = [1, 3, 1, 1];
        let ocultos = vec![Range { start: 1, end: 2 }];
        assert_eq!(ajustar_plegado(&largos, &ocultos, (0, 0), (2, 0), 5), ((0, 0), 1));
        // Bajar con alto 2: el ancla nueva es la cabecera, no la oculta.
        assert_eq!(ajustar_plegado(&largos, &ocultos, (0, 0), (3, 0), 2), ((2, 0), 1));
        assert_eq!(ajustar_plegado(&largos, &ocultos, (3, 0), (2, 0), 2), ((2, 0), 0));
        // Un ancla que quedó adentro de un bloque recién plegado pasa a
        // la cabecera.
        assert_eq!(ajustar_plegado(&largos, &ocultos, (1, 2), (3, 0), 5), ((0, 0), 2));
    }

    /// Igual que el test de abajo, con bloques plegados al azar.
    #[test]
    fn ajustar_con_ajuste_y_pliegues_coincide_con_el_algoritmo_global_de_antes() {
        let mut semilla: u64 = 0x1234_5678_9abc_def1;
        let mut azar = |n: usize| {
            semilla ^= semilla << 13;
            semilla ^= semilla >> 7;
            semilla ^= semilla << 17;
            (semilla % n as u64) as usize
        };
        for _ in 0..5000 {
            let largos: Vec<usize> = (0..2 + azar(30)).map(|_| 1 + azar(4)).collect();
            // Tramos ordenados, que no empiezan en la línea 0 y no se
            // tocan (como los arma `Plegado::tramos_ocultos`).
            let mut ocultos: Vec<Range<usize>> = Vec::new();
            let mut linea = 1 + azar(3);
            while linea < largos.len() {
                let fin = (linea + 1 + azar(4)).min(largos.len());
                ocultos.push(linea..fin);
                linea = fin + 1 + azar(4);
            }
            let visibles: Vec<usize> = (0..largos.len()).filter(|&l| tramo_que_oculta(&ocultos, l).is_none()).collect();
            let alto = 1 + azar(8);
            let linea_ancla = visibles[azar(visibles.len())];
            let ancla = (linea_ancla, azar(largos[linea_ancla]));
            let linea_cursor = visibles[azar(visibles.len())];
            let cursor = (linea_cursor, azar(largos[linea_cursor]));
            let (nueva, fila) = ajustar_plegado(&largos, &ocultos, ancla, cursor, alto);
            assert!(tramo_que_oculta(&ocultos, nueva.0).is_none());
            let esperado = ajustar_como_antes_plegado(&largos, &ocultos, ancla, cursor, alto);
            assert_eq!(
                (a_global_plegado(&largos, &ocultos, nueva), fila),
                esperado,
                "{largos:?} {ocultos:?} ancla {ancla:?} cursor {cursor:?} alto {alto}"
            );
        }
    }

    fn ajustar_como_antes_plegado(
        largos: &[usize],
        ocultos: &[Range<usize>],
        ancla: PosicionVisual,
        cursor: PosicionVisual,
        alto: usize,
    ) -> (usize, usize) {
        let fila_cursor = a_global_plegado(largos, ocultos, cursor);
        let mut scroll = a_global_plegado(largos, ocultos, ancla);
        if fila_cursor < scroll {
            scroll = fila_cursor;
        } else if fila_cursor >= scroll + alto {
            scroll = fila_cursor - alto + 1;
        }
        (scroll, fila_cursor - scroll)
    }

    #[test]
    fn filas_de_linea_cuenta_igual_que_filas_visuales_de() {
        for linea in ["", "a", "abc", "abcd", "áéíóúñ", "abcdefghi"] {
            let esperado = filas_visuales_de(0, linea, 3).len();
            assert_eq!(filas_de_linea(linea.chars().count(), 3), esperado, "{linea:?}");
        }
    }

    #[test]
    fn ajustar_con_ajuste_no_mueve_si_el_cursor_ya_se_ve() {
        let largos = [1, 3, 1, 2, 1];
        assert_eq!(ajustar(&largos, (1, 1), (3, 0), 5), ((1, 1), 3));
    }

    #[test]
    fn ajustar_con_ajuste_sube_hasta_el_cursor() {
        let largos = [1, 3, 1, 2, 1];
        assert_eq!(ajustar(&largos, (3, 0), (1, 2), 5), ((1, 2), 0));
    }

    #[test]
    fn ajustar_con_ajuste_baja_dejando_el_cursor_en_la_ultima_fila() {
        // Filas globales: l0=0, l1=1..3, l2=4, l3=5..6, l4=7. Cursor en
        // (4,0) = fila 7, alto 3 -> la primera fila visible es la 5 = (3,0).
        let largos = [1, 3, 1, 2, 1];
        assert_eq!(ajustar(&largos, (0, 0), (4, 0), 3), ((3, 0), 2));
        // A mitad de una línea partida: cursor (3,1) = fila 6 -> desde la 4.
        assert_eq!(ajustar(&largos, (0, 0), (3, 1), 3), ((2, 0), 2));
    }

    #[test]
    fn ajustar_con_ajuste_normaliza_la_columna_al_final_de_una_linea_justa() {
        // Línea 0 de 3 filas: la sub-fila 3 (columna == largo, múltiplo
        // del ancho) es la primera fila de la línea 1, igual que antes.
        let largos = [3, 1];
        assert_eq!(ajustar(&largos, (0, 0), (0, 3), 5), ((0, 0), 3));
    }

    #[test]
    fn ajustar_con_ajuste_recorta_un_ancla_que_ya_no_existe() {
        let largos = [2, 1];
        assert_eq!(ajustar(&largos, (9, 4), (0, 0), 5), ((0, 0), 0));
        assert_eq!(ajustar(&largos, (0, 7), (1, 0), 5), ((0, 1), 1));
    }

    /// Mismo resultado que el algoritmo de antes (sobre el archivo
    /// entero) para muchas combinaciones pseudoaleatorias de largos de
    /// línea, scroll y cursor.
    #[test]
    fn ajustar_con_ajuste_coincide_con_el_algoritmo_global_de_antes() {
        let mut semilla: u64 = 0x2545_f491_4f6c_dd1d;
        let mut azar = |n: usize| {
            semilla ^= semilla << 13;
            semilla ^= semilla >> 7;
            semilla ^= semilla << 17;
            (semilla % n as u64) as usize
        };
        for _ in 0..5000 {
            let largos: Vec<usize> = (0..1 + azar(30)).map(|_| 1 + azar(4)).collect();
            let alto = 1 + azar(8);
            let linea_ancla = azar(largos.len());
            let ancla = (linea_ancla, azar(largos[linea_ancla]));
            let linea_cursor = azar(largos.len());
            let cursor = (linea_cursor, azar(largos[linea_cursor]));
            let (nueva, fila) = ajustar(&largos, ancla, cursor, alto);
            let esperado = ajustar_como_antes(&largos, a_global(&largos, ancla), cursor, alto);
            assert_eq!(
                (a_global(&largos, nueva), fila),
                esperado,
                "{largos:?} ancla {ancla:?} cursor {cursor:?} alto {alto}"
            );
        }
    }
}
