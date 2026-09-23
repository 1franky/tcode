use std::ops::Range;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use tcode_core::{Coincidencia, Editor};
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

/// Índice de la fila visual (posición dentro de la secuencia completa de
/// filas de todo el archivo, el mismo espacio de coordenadas que
/// `EstadoUi::scroll`) que contiene la posición `(idx_linea,
/// columna)` — `columna` es un índice de CARÁCTER dentro de la línea
/// (`tcode_core::Cursor::columna`), no de byte. Sin ajuste de línea
/// coincide siempre con `idx_linea` (una fila por línea); con el ajuste
/// activo, suma las filas de todas las líneas anteriores más la
/// sub-fila de `columna` dentro de la suya — sin necesitar buscar en la
/// lista de filas ya construida.
fn fila_de_cursor(lineas: &[String], idx_linea: usize, columna: usize, ajuste_linea: bool, ancho: usize) -> usize {
    if !ajuste_linea {
        return idx_linea;
    }
    let filas_antes: usize = lineas[..idx_linea]
        .iter()
        .map(|l| {
            let num_chars = l.chars().count();
            if num_chars == 0 { 1 } else { num_chars.div_ceil(ancho) }
        })
        .sum();
    filas_antes + columna / ancho
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
) {
    let buffer = editor.buffer();
    let (area_gutter, area) = dividir_gutter(area, buffer.num_lineas(), mostrar_numeros);

    let alto_visible = area.height as usize;
    let ancho_visible = area.width as usize;
    let ancho = ancho_visible.max(1);
    let cursor = editor.cursor();
    let cursores = editor.cursores();

    // Solo las filas que entran en pantalla (`filas[0]` es la fila visual
    // número `estado.scroll`). Sin ajuste de línea hay una fila por línea
    // lógica, así que alcanza con leer del buffer las líneas visibles —
    // copiar todas las líneas en cada frame costaba ~3 ms con 10.000
    // líneas (BACKLOG.md P1 #14). Con ajuste de línea sí hace falta
    // recorrer el archivo entero: una línea larga que se parte en varias
    // filas corre el índice de fila de todas las que vienen después, y
    // sin eso no se puede ubicar el scroll ni el cursor en filas visuales.
    let (filas, fila_cursor) = if ajuste_linea {
        let lineas = buffer.lineas_texto();
        let todas: Vec<FilaVisual> =
            lineas.iter().enumerate().flat_map(|(idx, l)| filas_visuales_de(idx, l, ancho)).collect();
        let fila_cursor = fila_de_cursor(&lineas, cursor.linea, cursor.columna, ajuste_linea, ancho);
        ajustar_scroll(estado, fila_cursor, alto_visible);
        let fin = (estado.scroll + alto_visible).min(todas.len());
        (todas.get(estado.scroll..fin).unwrap_or_default().to_vec(), fila_cursor)
    } else {
        ajustar_scroll(estado, cursor.linea, alto_visible);
        let fin = (estado.scroll + alto_visible).min(buffer.num_lineas());
        let filas = (estado.scroll..fin)
            .map(|idx| FilaVisual { idx_linea: idx, inicio: 0, fin: buffer.linea_texto(idx).len() })
            .collect();
        (filas, cursor.linea)
    };
    // Texto de las líneas que tocan las filas visibles, indexado desde la
    // primera de ellas.
    let primera_linea = filas.first().map_or(0, |f| f.idx_linea);
    let ultima_linea = filas.last().map_or(0, |f| f.idx_linea);
    let lineas: Vec<String> =
        if filas.is_empty() { Vec::new() } else { (primera_linea..=ultima_linea).map(|i| buffer.linea_texto(i)).collect() };

    let rango_visible = rango_bytes_visible(editor, &filas);
    let tokens = calcular_tokens(editor, resaltador, ruta, rango_visible);
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
        dibujar_gutter(frame, area_gutter, &filas, alto_visible, cursor.linea, paleta);
    }

    if mostrar_cursor {
        let columna_local = if ajuste_linea { cursor.columna % ancho } else { cursor.columna };
        let columna = area.x + columna_local as u16;
        let fila_pantalla = area.y + (fila_cursor - estado.scroll) as u16;
        frame.set_cursor_position((columna, fila_pantalla));
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
fn dividir_gutter(area: Rect, total_lineas: usize, mostrar_numeros: bool) -> (Option<Rect>, Rect) {
    if !mostrar_numeros {
        return (None, area);
    }
    let ancho_numero = total_lineas.max(1).to_string().len().max(2) as u16;
    let ancho_gutter = ancho_numero + 1;
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
fn dibujar_gutter(
    frame: &mut Frame,
    area: Rect,
    filas: &[FilaVisual],
    alto_visible: usize,
    linea_cursor: usize,
    paleta: &Paleta,
) {
    let ancho_numero = area.width.saturating_sub(1) as usize;
    let en_blanco = || Line::from(Span::styled(" ".repeat(area.width as usize), Style::default().bg(paleta.fondo)));
    let filas_pantalla: Vec<Line> = (0..alto_visible)
        .map(|offset| {
            let Some(fila) = filas.get(offset) else { return en_blanco() };
            if !fila.primera() {
                return en_blanco();
            }
            let color = if fila.idx_linea == linea_cursor { paleta.numero_linea_activo } else { paleta.numero_linea };
            let texto = format!("{:>ancho$} ", fila.idx_linea + 1, ancho = ancho_numero);
            Line::from(Span::styled(texto, Style::default().fg(color).bg(paleta.fondo)))
        })
        .collect();
    frame.render_widget(Paragraph::new(filas_pantalla), area);
}

/// Rango de bytes del texto que ocupan las filas visibles (de la primera
/// fila en pantalla a la última) — lo único que hace falta resaltar.
fn rango_bytes_visible(editor: &Editor, filas: &[FilaVisual]) -> Range<usize> {
    let buffer = editor.buffer();
    let (Some(primera), Some(ultima)) = (filas.first(), filas.last()) else {
        return 0..0;
    };
    buffer.inicio_byte_linea(primera.idx_linea) + primera.inicio..buffer.inicio_byte_linea(ultima.idx_linea) + ultima.fin
}

/// Resalta lo visible del archivo si su extensión corresponde a un
/// lenguaje soportado; si no, o si el parseo falla, se sigue mostrando el
/// texto sin colorear (nunca rompe el render). La ruta identifica al
/// documento para que el resaltador re-parsee de forma incremental (ver
/// `Resaltador::resaltar_documento`).
fn calcular_tokens(editor: &Editor, resaltador: &mut Resaltador, ruta: &str, rango: Range<usize>) -> Vec<Token> {
    let Some(lenguaje) = Lenguaje::detectar_por_extension(ruta) else {
        return Vec::new();
    };
    let fuente = editor.buffer().a_texto();
    resaltador.resaltar_documento(ruta, lenguaje, &fuente, rango).unwrap_or_default()
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

    #[test]
    fn fila_de_cursor_sin_ajuste_es_siempre_el_indice_de_linea() {
        let lineas = vec!["una línea bastante larga de verdad".to_string(), "corta".to_string()];
        assert_eq!(fila_de_cursor(&lineas, 0, 20, false, 10), 0);
        assert_eq!(fila_de_cursor(&lineas, 1, 3, false, 10), 1);
    }

    #[test]
    fn fila_de_cursor_con_ajuste_suma_las_filas_de_las_lineas_anteriores() {
        // Línea 0: 8 caracteres, ancho 3 -> 3 filas visuales (0,1,2).
        // Línea 1 arranca en la fila visual global 3.
        let lineas = vec!["abcdefgh".to_string(), "xy".to_string()];
        assert_eq!(fila_de_cursor(&lineas, 1, 0, true, 3), 3);
        assert_eq!(fila_de_cursor(&lineas, 1, 1, true, 3), 3);
    }

    #[test]
    fn fila_de_cursor_con_ajuste_encuentra_la_sub_fila_dentro_de_su_propia_linea() {
        let lineas = vec!["abcdefgh".to_string()];
        // Columna 0..2 -> fila 0 ("abc"); 3..5 -> fila 1 ("def"); 6..7 ->
        // fila 2 ("gh").
        assert_eq!(fila_de_cursor(&lineas, 0, 0, true, 3), 0);
        assert_eq!(fila_de_cursor(&lineas, 0, 2, true, 3), 0);
        assert_eq!(fila_de_cursor(&lineas, 0, 3, true, 3), 1);
        assert_eq!(fila_de_cursor(&lineas, 0, 6, true, 3), 2);
        // Columna 8 (justo después de la última 'h', fin de línea):
        // sigue siendo la fila 2, igual que el resto de "gh".
        assert_eq!(fila_de_cursor(&lineas, 0, 8, true, 3), 2);
    }

    #[test]
    fn fila_de_cursor_con_ajuste_y_linea_vacia_cuenta_como_una_fila() {
        let lineas = vec!["".to_string(), "resto".to_string()];
        assert_eq!(fila_de_cursor(&lineas, 1, 0, true, 3), 1);
    }
}
