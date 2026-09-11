use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use tcode_syntax::{Lenguaje, Resaltador};

use crate::Paleta;

/// Opciones de `pulldown-cmark` habilitadas para la vista de preview
/// (PLAN.md §8): tablas, tachado y listas de tareas además del
/// CommonMark base.
fn opciones_markdown() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS
}

/// Dibuja el panel derecho de la vista Markdown doble (PLAN.md §8):
/// `texto_fuente` es el Markdown crudo del buffer, `scroll_fuente`/
/// `total_lineas_fuente` el desplazamiento y tamaño del panel de código a
/// la izquierda — se usan para la sincronización de scroll (proporcional
/// al total de líneas de cada lado, ya que el preview casi nunca tiene el
/// mismo número de líneas que la fuente).
pub fn dibujar(
    frame: &mut Frame,
    area: Rect,
    texto_fuente: &str,
    scroll_fuente: usize,
    total_lineas_fuente: usize,
    paleta: &Paleta,
    resaltador: &mut Resaltador,
) {
    let lineas = renderizar(texto_fuente, area.width as usize, paleta, resaltador);

    let alto_visible = area.height as usize;
    let max_scroll = lineas.len().saturating_sub(alto_visible);
    let scroll_preview =
        (scroll_fuente * lineas.len()).checked_div(total_lineas_fuente).map(|v| v.min(max_scroll)).unwrap_or(0);

    let widget = Paragraph::new(lineas)
        .style(Style::default().bg(paleta.fondo).fg(paleta.texto))
        .scroll((scroll_preview as u16, 0));
    frame.render_widget(widget, area);
}

/// Estado de una lista abierta (`Start(List(_))..End(List(_))`): si es
/// numerada y, si lo es, el número del siguiente ítem.
struct EstadoLista {
    ordenada: bool,
    siguiente: u64,
}

/// Bloque de código en construcción (`Start(CodeBlock(_))..End(CodeBlock)`):
/// se acumula el texto completo antes de resaltarlo, porque tree-sitter
/// necesita el bloque entero, no línea por línea.
struct BufferCodigo {
    lenguaje: Option<Lenguaje>,
    texto: String,
}

/// Tabla en construcción (`Start(Table(_))..End(Table)`): el formato
/// final (con anchos de columna alineados) solo se conoce al cerrar la
/// tabla, así que se acumulan las celdas como texto plano — PLAN.md §8
/// solo pide "widget `Table`"; para no mezclar un widget de área fija con
/// el resto del flujo de líneas del preview, se renderiza como texto
/// alineado en columnas en su lugar (mismo resultado visual en una TUI
/// monoespaciada, más simple de intercalar con el resto del Markdown).
struct EstadoTabla {
    filas: Vec<Vec<String>>,
    fila_actual: Vec<String>,
    celda_actual: String,
    en_encabezado: bool,
    num_encabezado: usize,
}

/// Convierte Markdown crudo en líneas de `ratatui` ya resaltadas —
/// función pura, sin dependencias de terminal, para poder testearla
/// directamente. `ancho` solo se usa para el largo de los separadores
/// horizontales (`---`); el resto del texto no se envuelve (igual que
/// `vista_codigo`, que tampoco envuelve líneas largas).
fn renderizar(texto: &str, ancho: usize, paleta: &Paleta, resaltador: &mut Resaltador) -> Vec<Line<'static>> {
    let mut r = Renderizador {
        paleta,
        resaltador,
        ancho: ancho.max(10),
        lineas: Vec::new(),
        actual: Vec::new(),
        negrita: 0,
        cursiva: 0,
        tachado: 0,
        en_link: 0,
        heading: None,
        nivel_blockquote: 0,
        en_item: 0,
        listas: Vec::new(),
        codigo: None,
        en_imagen: false,
        imagen_alt: String::new(),
        tabla: None,
    };
    for evento in Parser::new_ext(texto, opciones_markdown()) {
        r.procesar(evento);
    }
    r.flush_linea();
    r.lineas
}

struct Renderizador<'p> {
    paleta: &'p Paleta,
    resaltador: &'p mut Resaltador,
    ancho: usize,
    lineas: Vec<Line<'static>>,
    actual: Vec<Span<'static>>,
    negrita: u32,
    cursiva: u32,
    tachado: u32,
    en_link: u32,
    heading: Option<HeadingLevel>,
    nivel_blockquote: u32,
    en_item: u32,
    listas: Vec<EstadoLista>,
    codigo: Option<BufferCodigo>,
    en_imagen: bool,
    imagen_alt: String,
    tabla: Option<EstadoTabla>,
}

impl Renderizador<'_> {
    fn procesar(&mut self, evento: Event) {
        match evento {
            Event::Start(tag) => self.iniciar(tag),
            Event::End(fin) => self.terminar(fin),
            Event::Text(texto) => self.texto(&texto),
            Event::Code(texto) => self.codigo_en_linea(&texto),
            Event::SoftBreak => self.texto(" "),
            Event::HardBreak => self.flush_linea(),
            Event::Rule => {
                self.flush_linea();
                self.lineas.push(Line::from(Span::styled(
                    "─".repeat(self.ancho),
                    Style::default().fg(self.paleta.diagnostico_sugerencia),
                )));
            }
            Event::TaskListMarker(marcado) => {
                let texto = if marcado { "[x] " } else { "[ ] " };
                self.actual.push(Span::styled(texto, Style::default().fg(self.paleta.texto)));
            }
            // HTML embebido, notas al pie, math: fuera de alcance de M3 —
            // se ignoran en vez de romper el render (PLAN.md §8 no los
            // menciona entre los elementos soportados).
            Event::Html(_)
            | Event::InlineHtml(_)
            | Event::FootnoteReference(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_) => {}
        }
    }

    fn iniciar(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph => {}
            Tag::Heading { level, .. } => self.heading = Some(level),
            Tag::BlockQuote(_) => self.nivel_blockquote += 1,
            Tag::CodeBlock(tipo) => {
                let lenguaje = match &tipo {
                    pulldown_cmark::CodeBlockKind::Fenced(etiqueta) => Lenguaje::detectar_por_etiqueta(etiqueta),
                    pulldown_cmark::CodeBlockKind::Indented => None,
                };
                self.codigo = Some(BufferCodigo { lenguaje, texto: String::new() });
            }
            Tag::List(numero) => self.listas.push(EstadoLista { ordenada: numero.is_some(), siguiente: numero.unwrap_or(1) }),
            Tag::Item => {
                self.en_item += 1;
                let profundidad = self.listas.len().max(1);
                let marcador = match self.listas.last_mut() {
                    Some(lista) if lista.ordenada => {
                        let n = lista.siguiente;
                        lista.siguiente += 1;
                        format!("{n}. ")
                    }
                    _ => "• ".to_string(),
                };
                let indent = "  ".repeat(profundidad - 1);
                self.actual.push(Span::styled(format!("{indent}{marcador}"), Style::default().fg(self.paleta.texto)));
            }
            Tag::Table(_) => {
                self.tabla = Some(EstadoTabla {
                    filas: Vec::new(),
                    fila_actual: Vec::new(),
                    celda_actual: String::new(),
                    en_encabezado: false,
                    num_encabezado: 0,
                });
            }
            Tag::TableHead => {
                if let Some(tabla) = &mut self.tabla {
                    tabla.en_encabezado = true;
                }
            }
            Tag::TableRow | Tag::TableCell => {}
            Tag::Emphasis => self.cursiva += 1,
            Tag::Strong => self.negrita += 1,
            Tag::Strikethrough => self.tachado += 1,
            Tag::Link { .. } => self.en_link += 1,
            Tag::Image { .. } => {
                self.en_imagen = true;
                self.imagen_alt.clear();
            }
            // Notas al pie, listas de definición, bloques de metadatos y
            // HTML de bloque: fuera de alcance de M3.
            Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::HtmlBlock
            | Tag::MetadataBlock(_) => {}
        }
    }

    fn terminar(&mut self, fin: TagEnd) {
        match fin {
            TagEnd::Paragraph => {
                self.flush_linea();
                if self.en_item == 0 {
                    self.linea_vacia();
                }
            }
            TagEnd::Heading(_) => {
                self.flush_linea();
                self.linea_vacia();
                self.heading = None;
            }
            TagEnd::BlockQuote(_) => {
                self.nivel_blockquote = self.nivel_blockquote.saturating_sub(1);
                if self.nivel_blockquote == 0 {
                    self.linea_vacia();
                }
            }
            TagEnd::CodeBlock => self.cerrar_bloque_codigo(),
            TagEnd::List(_) => {
                self.listas.pop();
                if self.listas.is_empty() {
                    self.linea_vacia();
                }
            }
            TagEnd::Item => {
                self.flush_linea();
                self.en_item = self.en_item.saturating_sub(1);
            }
            TagEnd::Table => self.cerrar_tabla(),
            // A diferencia de una fila de cuerpo, las celdas del
            // encabezado NO vienen envueltas en su propio `TableRow`
            // (confirmado contra el stream de eventos real de
            // `pulldown-cmark` 0.12: van directo bajo `TableHead`) — hay
            // que cerrar esa fila aquí, no solo en `TagEnd::TableRow`.
            TagEnd::TableHead => {
                if let Some(tabla) = &mut self.tabla {
                    let fila = std::mem::take(&mut tabla.fila_actual);
                    tabla.filas.push(fila);
                    tabla.en_encabezado = false;
                    tabla.num_encabezado = tabla.filas.len();
                }
            }
            TagEnd::TableRow => {
                if let Some(tabla) = &mut self.tabla {
                    let fila = std::mem::take(&mut tabla.fila_actual);
                    tabla.filas.push(fila);
                }
            }
            TagEnd::TableCell => {
                if let Some(tabla) = &mut self.tabla {
                    let celda = std::mem::take(&mut tabla.celda_actual);
                    tabla.fila_actual.push(celda);
                }
            }
            TagEnd::Emphasis => self.cursiva = self.cursiva.saturating_sub(1),
            TagEnd::Strong => self.negrita = self.negrita.saturating_sub(1),
            TagEnd::Strikethrough => self.tachado = self.tachado.saturating_sub(1),
            TagEnd::Link => self.en_link = self.en_link.saturating_sub(1),
            TagEnd::Image => {
                self.en_imagen = false;
                let alt = std::mem::take(&mut self.imagen_alt);
                self.actual.push(Span::styled(
                    format!("[img: {alt}]"),
                    Style::default().fg(self.paleta.diagnostico_sugerencia).add_modifier(Modifier::ITALIC),
                ));
            }
            TagEnd::FootnoteDefinition
            | TagEnd::DefinitionList
            | TagEnd::DefinitionListTitle
            | TagEnd::DefinitionListDefinition
            | TagEnd::HtmlBlock
            | TagEnd::MetadataBlock(_) => {}
        }
    }

    fn texto(&mut self, texto: &str) {
        if texto.is_empty() {
            return;
        }
        if let Some(codigo) = &mut self.codigo {
            codigo.texto.push_str(texto);
        } else if self.en_imagen {
            self.imagen_alt.push_str(texto);
        } else if let Some(tabla) = &mut self.tabla {
            tabla.celda_actual.push_str(texto);
        } else {
            self.actual.push(Span::styled(texto.to_string(), self.estilo_actual()));
        }
    }

    fn codigo_en_linea(&mut self, texto: &str) {
        if let Some(tabla) = &mut self.tabla {
            tabla.celda_actual.push_str(texto);
            return;
        }
        self.actual.push(Span::styled(
            texto.to_string(),
            Style::default().fg(self.paleta.estilo_sintaxis("string").fg.unwrap_or(self.paleta.texto)).bg(self.paleta.linea_actual),
        ));
    }

    /// Combina negrita/cursiva/tachado/link/heading en un único `Style`.
    /// Los encabezados usan un color propio por nivel en vez de heredar
    /// negrita/cursiva ambiente (un encabezado siempre se ve igual, no
    /// importa si viniera de `**## título**`, caso raro pero posible).
    fn estilo_actual(&self) -> Style {
        if let Some(nivel) = self.heading {
            return estilo_encabezado(self.paleta, nivel);
        }
        let mut estilo = if self.en_link > 0 {
            Style::default().fg(self.paleta.diagnostico_info).add_modifier(Modifier::UNDERLINED)
        } else {
            Style::default().fg(self.paleta.texto)
        };
        if self.negrita > 0 {
            estilo = estilo.add_modifier(Modifier::BOLD);
        }
        if self.cursiva > 0 {
            estilo = estilo.add_modifier(Modifier::ITALIC);
        }
        if self.tachado > 0 {
            estilo = estilo.add_modifier(Modifier::CROSSED_OUT);
        }
        estilo
    }

    fn flush_linea(&mut self) {
        if self.actual.is_empty() {
            return;
        }
        let spans = std::mem::take(&mut self.actual);
        if self.nivel_blockquote > 0 {
            let mut con_prefijo = vec![Span::styled(
                "▏ ".repeat(self.nivel_blockquote as usize),
                Style::default().fg(self.paleta.diagnostico_sugerencia),
            )];
            con_prefijo.extend(spans);
            self.lineas.push(Line::from(con_prefijo));
        } else {
            self.lineas.push(Line::from(spans));
        }
    }

    fn linea_vacia(&mut self) {
        if !matches!(self.lineas.last(), Some(l) if l.spans.is_empty()) {
            self.lineas.push(Line::from(""));
        }
    }

    /// Resalta el bloque de código completo (si el lenguaje declarado se
    /// reconoce) y lo agrega como líneas propias, indentadas y con el
    /// fondo de "línea actual" del tema para diferenciarlo visualmente
    /// del resto del preview.
    fn cerrar_bloque_codigo(&mut self) {
        let Some(codigo) = self.codigo.take() else { return };
        let fuente = codigo.texto.strip_suffix('\n').unwrap_or(&codigo.texto);
        let tokens = codigo.lenguaje.and_then(|l| self.resaltador.resaltar(l, fuente).ok()).unwrap_or_default();

        let mut inicio_linea = 0usize;
        for linea in fuente.split('\n') {
            let fin_linea = inicio_linea + linea.len();
            let mut spans = vec![Span::styled("  ", Style::default().bg(self.paleta.linea_actual))];
            spans.extend(spans_resaltados(linea, inicio_linea, &tokens, self.paleta));
            // Relleno para que el fondo cubra todo el ancho, no solo el texto.
            let ocupado = 2 + linea.chars().count();
            if self.ancho > ocupado {
                spans.push(Span::styled(" ".repeat(self.ancho - ocupado), Style::default().bg(self.paleta.linea_actual)));
            }
            self.lineas.push(Line::from(spans));
            inicio_linea = fin_linea + 1;
        }
        self.linea_vacia();
    }

    /// Formatea la tabla acumulada como texto alineado en columnas (ver
    /// doc de [`EstadoTabla`]): encabezado en negrita, separador de
    /// guiones y filas de cuerpo con el ancho de la columna más larga.
    fn cerrar_tabla(&mut self) {
        let Some(tabla) = self.tabla.take() else { return };
        if tabla.filas.is_empty() {
            return;
        }
        let num_columnas = tabla.filas.iter().map(|f| f.len()).max().unwrap_or(0);
        let mut anchos = vec![0usize; num_columnas];
        for fila in &tabla.filas {
            for (i, celda) in fila.iter().enumerate() {
                anchos[i] = anchos[i].max(celda.chars().count());
            }
        }

        for (i, fila) in tabla.filas.iter().enumerate() {
            let es_encabezado = i < tabla.num_encabezado;
            let mut spans = Vec::new();
            for (col, ancho_col) in anchos.iter().enumerate() {
                let celda = fila.get(col).map(String::as_str).unwrap_or("");
                let relleno = " ".repeat(ancho_col.saturating_sub(celda.chars().count()));
                let mut estilo = Style::default().fg(self.paleta.texto);
                if es_encabezado {
                    estilo = estilo.add_modifier(Modifier::BOLD);
                }
                spans.push(Span::styled(format!("{celda}{relleno}"), estilo));
                spans.push(Span::raw(" │ "));
            }
            self.lineas.push(Line::from(spans));

            if es_encabezado && i + 1 == tabla.num_encabezado {
                let separador: String = anchos
                    .iter()
                    .map(|ancho| "─".repeat(*ancho))
                    .collect::<Vec<_>>()
                    .join("─┼─");
                self.lineas.push(Line::from(Span::styled(
                    separador,
                    Style::default().fg(self.paleta.diagnostico_sugerencia),
                )));
            }
        }
        self.linea_vacia();
    }
}

/// Color + negrita por nivel de encabezado (PLAN.md §8: "texto con estilo
/// bold + color por nivel"). Reutiliza los colores ya resueltos de
/// sintaxis en vez de sumar campos de tema nuevos solo para esto — da
/// variedad visual coherente con el tema activo sin plumbing adicional.
fn estilo_encabezado(paleta: &Paleta, nivel: HeadingLevel) -> Style {
    let nombre_token = match nivel {
        HeadingLevel::H1 => "keyword",
        HeadingLevel::H2 => "function",
        HeadingLevel::H3 => "type",
        HeadingLevel::H4 => "constant",
        HeadingLevel::H5 => "variable",
        HeadingLevel::H6 => "comment",
    };
    let color = paleta.estilo_sintaxis(nombre_token).fg.unwrap_or(paleta.texto);
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

/// Igual que el `spans_de_linea` de `vista_codigo`, pero sin coincidencias
/// de búsqueda (el preview no participa de `Ctrl+F` en M3) y devolviendo
/// `Span`s con texto propio (`'static`) en vez de tomados prestados del
/// texto fuente: el bloque de código de un preview vive en un `String`
/// local a `cerrar_bloque_codigo` que no sobrevive a la función, así que
/// no hay un `&str` externo del que tomar prestado. Se duplica el barrido
/// en vez de compartir código con `vista_codigo` porque ese módulo
/// también combina resaltado de búsqueda, y generalizarlo ahí complica
/// más de lo que ahorra para esta única reutilización.
fn spans_resaltados(texto_linea: &str, inicio_byte_linea: usize, tokens: &[tcode_syntax::Token], paleta: &Paleta) -> Vec<Span<'static>> {
    // Todo el span lleva el fondo de "línea actual" del tema (el bloque
    // de código entero se ve como una franja diferenciada del resto del
    // preview), tenga o no token de sintaxis encima.
    let fondo = Style::default().bg(paleta.linea_actual);

    if tokens.is_empty() {
        return vec![Span::styled(texto_linea.to_string(), fondo)];
    }

    let fin_byte_linea = inicio_byte_linea + texto_linea.len();
    let mut spans = Vec::new();
    let mut cursor = inicio_byte_linea;

    for token in tokens {
        if token.fin <= inicio_byte_linea || token.inicio >= fin_byte_linea {
            continue;
        }
        let inicio = token.inicio.max(inicio_byte_linea);
        let fin = token.fin.min(fin_byte_linea);

        if inicio > cursor {
            let fragmento = &texto_linea[(cursor - inicio_byte_linea)..(inicio - inicio_byte_linea)];
            spans.push(Span::styled(fragmento.to_string(), fondo));
        }
        let estilo = paleta.estilo_sintaxis(token.nombre).bg(paleta.linea_actual);
        let fragmento = &texto_linea[(inicio - inicio_byte_linea)..(fin - inicio_byte_linea)];
        spans.push(Span::styled(fragmento.to_string(), estilo));
        cursor = fin;
    }

    if cursor < fin_byte_linea {
        let fragmento = &texto_linea[(cursor - inicio_byte_linea)..];
        spans.push(Span::styled(fragmento.to_string(), fondo));
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lineas_de_texto(lineas: &[Line]) -> Vec<String> {
        lineas.iter().map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>()).collect()
    }

    #[test]
    fn parrafo_simple_se_renderiza_como_una_linea() {
        let mut resaltador = Resaltador::nuevo();
        let paleta = Paleta::basica();
        let lineas = renderizar("hola mundo", 80, &paleta, &mut resaltador);
        // Después de cada párrafo se agrega una línea en blanco de
        // separación (ver `TagEnd::Paragraph`).
        assert_eq!(lineas_de_texto(&lineas), vec!["hola mundo", ""]);
    }

    #[test]
    fn encabezado_pierde_los_simbolos_numeral() {
        let mut resaltador = Resaltador::nuevo();
        let paleta = Paleta::basica();
        let lineas = renderizar("## Título", 80, &paleta, &mut resaltador);
        assert_eq!(lineas_de_texto(&lineas)[0], "Título");
        assert!(lineas[0].spans[0].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn negrita_y_cursiva_se_combinan() {
        let mut resaltador = Resaltador::nuevo();
        let paleta = Paleta::basica();
        let lineas = renderizar("normal ***negrita cursiva*** normal", 80, &paleta, &mut resaltador);
        let texto = lineas_de_texto(&lineas);
        assert_eq!(texto[0], "normal negrita cursiva normal");
        let span_enfasis = &lineas[0].spans[1];
        assert!(span_enfasis.style.add_modifier.contains(Modifier::BOLD));
        assert!(span_enfasis.style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn lista_no_ordenada_usa_vinetas_y_ordenada_numeros() {
        let mut resaltador = Resaltador::nuevo();
        let paleta = Paleta::basica();
        let lineas = renderizar("- a\n- b\n", 80, &paleta, &mut resaltador);
        let texto = lineas_de_texto(&lineas);
        assert_eq!(texto[0], "• a");
        assert_eq!(texto[1], "• b");

        let lineas = renderizar("1. a\n2. b\n", 80, &paleta, &mut resaltador);
        let texto = lineas_de_texto(&lineas);
        assert_eq!(texto[0], "1. a");
        assert_eq!(texto[1], "2. b");
    }

    #[test]
    fn lista_ordenada_reindexa_desde_el_numero_inicial() {
        let mut resaltador = Resaltador::nuevo();
        let paleta = Paleta::basica();
        let lineas = renderizar("5. a\n6. b\n", 80, &paleta, &mut resaltador);
        let texto = lineas_de_texto(&lineas);
        assert_eq!(texto[0], "5. a");
        assert_eq!(texto[1], "6. b");
    }

    #[test]
    fn bloque_de_codigo_se_indenta_y_resalta() {
        let mut resaltador = Resaltador::nuevo();
        let paleta = Paleta::basica();
        let lineas = renderizar("```rust\nlet x = 1;\n```\n", 80, &paleta, &mut resaltador);
        assert!(lineas_de_texto(&lineas)[0].contains("let x = 1;"));
    }

    #[test]
    fn tabla_alinea_columnas_con_encabezado_en_negrita() {
        let mut resaltador = Resaltador::nuevo();
        let paleta = Paleta::basica();
        let md = "| Nombre | Edad |\n|---|---|\n| Ana | 3 |\n| Beto | 10 |\n";
        let lineas = renderizar(md, 80, &paleta, &mut resaltador);
        let texto = lineas_de_texto(&lineas);
        // Encabezado, separador, dos filas: "Edad" se ensancha a "10".
        assert!(texto[0].starts_with("Nombre"));
        assert!(texto[0].contains("Edad"));
        assert!(texto[1].chars().all(|c| c == '─' || c == '┼'));
        assert!(texto[2].starts_with("Ana  "));
        assert!(lineas[0].spans[0].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn enlace_tiene_subrayado_y_color_distinto() {
        let mut resaltador = Resaltador::nuevo();
        let paleta = Paleta::basica();
        let lineas = renderizar("[texto](https://ejemplo.com)", 80, &paleta, &mut resaltador);
        assert_eq!(lineas_de_texto(&lineas)[0], "texto");
        assert!(lineas[0].spans[0].style.add_modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn imagen_se_muestra_como_placeholder() {
        let mut resaltador = Resaltador::nuevo();
        let paleta = Paleta::basica();
        let lineas = renderizar("![un gato](gato.png)", 80, &paleta, &mut resaltador);
        assert_eq!(lineas_de_texto(&lineas)[0], "[img: un gato]");
    }

    #[test]
    fn cita_tiene_prefijo_en_cada_linea() {
        let mut resaltador = Resaltador::nuevo();
        let paleta = Paleta::basica();
        // Dos líneas de cita consecutivas SIN línea en blanco entre ellas
        // son, en CommonMark, el mismo párrafo con un salto de línea suave
        // (se unen con un espacio) — hace falta una línea de cita vacía
        // ("> ") para forzar dos párrafos separados dentro de la cita.
        let lineas = renderizar("> línea uno\n>\n> línea dos\n", 80, &paleta, &mut resaltador);
        // Cada párrafo (dentro o fuera de una cita) deja una línea en
        // blanco de separación después — de ahí filtrar antes de
        // comparar, el interés del test es el contenido, no el espaciado.
        let texto: Vec<String> = lineas_de_texto(&lineas).into_iter().filter(|l| !l.is_empty()).collect();
        assert_eq!(texto, vec!["▏ línea uno", "▏ línea dos"]);
    }

    #[test]
    fn separador_horizontal_llena_el_ancho_pedido() {
        let mut resaltador = Resaltador::nuevo();
        let paleta = Paleta::basica();
        let lineas = renderizar("texto\n\n---\n", 20, &paleta, &mut resaltador);
        let texto = lineas_de_texto(&lineas);
        assert!(texto.iter().any(|l| l == &"─".repeat(20)));
    }

    #[test]
    fn lista_de_tareas_marca_completadas_y_pendientes() {
        let mut resaltador = Resaltador::nuevo();
        let paleta = Paleta::basica();
        let lineas = renderizar("- [x] hecho\n- [ ] pendiente\n", 80, &paleta, &mut resaltador);
        let texto = lineas_de_texto(&lineas);
        assert_eq!(texto[0], "• [x] hecho");
        assert_eq!(texto[1], "• [ ] pendiente");
    }
}
