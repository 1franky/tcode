use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use tcode_config::{CampoEditor, CampoInterfaz, CampoTemas, Config, EstadoPanelAdmin, FocoPanelAdmin, Seccion};
use tcode_keymap::{detectar_conflictos, formatear_atajo, Keymap};

use crate::Paleta;

/// Ancho fijo de la barra lateral de secciones (PLAN.md §5).
const ANCHO_BARRA: u16 = 30;

/// Una fila de la sección "Lenguajes / LSP" (PLAN.md §5.3), ya resuelta a
/// texto — `app` la construye cada frame a partir de `tcode_syntax::
/// Lenguaje`, `tcode_lsp::comando_para` y el estado en vivo del cliente
/// LSP activo (`EstadoLsp`, que vive en `app` y no en ningún crate que
/// `tcode-ui` pueda conocer). Vive acá (no en `tcode-config`, que ya
/// resuelve "Editor"/"Temas" directo) porque es puramente un DTO de
/// render, sin ninguna lógica — no hace falta que ningún otro crate lo
/// conozca.
pub struct FilaLenguajeLsp {
    pub nombre: String,
    pub comando: String,
    pub en_path: bool,
    pub habilitado: bool,
    /// `true` si `comando` viene de un override guardado a mano (PLAN.md
    /// §5.3: "Configurar comando, argumentos...") en vez del que trae
    /// `tcode_lsp::comando_para` por defecto.
    pub personalizado: bool,
    /// "Conectado" / "Iniciando…" / "Inactivo" — ya resuelto a texto por
    /// `app`, que es quien tiene acceso al estado real de la sesión LSP.
    pub estado: String,
}

/// Dibuja el panel de administración (`Ctrl+,`) a pantalla completa: es
/// una vista más del sistema, no un overlay flotante sobre el editor
/// (PLAN.md §5) — quien llama (`tcode_ui::dibujar`) no dibuja nada más
/// del editor mientras este panel está activo.
#[allow(clippy::too_many_arguments)]
pub fn dibujar(
    frame: &mut Frame,
    area_total: Rect,
    panel: &EstadoPanelAdmin,
    config: &Config,
    keymap: &Keymap,
    filas_lenguajes: &[FilaLenguajeLsp],
    paleta: &Paleta,
) {
    frame.render_widget(Clear, area_total);
    frame.render_widget(Paragraph::new("").style(Style::default().bg(paleta.fondo)), area_total);

    let columnas = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(ANCHO_BARRA), Constraint::Min(1)])
        .split(area_total);

    let filas_derecha = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1), Constraint::Length(1)])
        .split(columnas[1]);

    dibujar_barra(frame, columnas[0], panel, paleta);
    match panel.foco() {
        FocoPanelAdmin::Busqueda => dibujar_busqueda(frame, filas_derecha[0], panel, paleta),
        _ => dibujar_central(frame, filas_derecha[0], panel, config, keymap, filas_lenguajes, paleta),
    }
    dibujar_mensaje(frame, filas_derecha[1], panel, paleta);
    dibujar_pie(frame, filas_derecha[2], panel, paleta);
}

/// Barra lateral con las 5 secciones de PLAN.md §5 (todas visibles desde
/// ya, aunque algunas todavía solo muestren un aviso "próximamente" en el
/// área central — ver `Seccion::implementada`). La marca de selección es
/// ASCII (`>`, no `▸`) — ver `panel_archivos` sobre el ancho ambiguo de
/// esos caracteres geométricos en algunas terminales.
fn dibujar_barra(frame: &mut Frame, area: Rect, panel: &EstadoPanelAdmin, paleta: &Paleta) {
    let estilo_base = Style::default().bg(paleta.fondo).fg(paleta.texto);
    let items: Vec<ListItem> = Seccion::TODAS
        .iter()
        .enumerate()
        .map(|(idx, seccion)| {
            let seleccionada = idx == panel.indice_seccion();
            let marca = if seleccionada && panel.foco() == FocoPanelAdmin::Barra { "> " } else { "  " };
            let sufijo = if seccion.implementada() { "" } else { " (próximamente)" };
            let estilo = if seleccionada { estilo_base.bg(paleta.linea_actual) } else { estilo_base };
            ListItem::new(Line::from(Span::styled(format!("{marca}{}{sufijo}", seccion.nombre()), estilo)))
                .style(estilo)
        })
        .collect();
    let lista = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_set(crate::BORDE_ASCII)
                .title(" Administración ")
                .style(estilo_base),
        );
    frame.render_widget(lista, area);
}

/// Área central: filas editables de la sección actual si ya tiene
/// contenido real (solo "Editor" por ahora), o el aviso de qué va a
/// traer si todavía no lo tiene.
#[allow(clippy::too_many_arguments)]
fn dibujar_central(
    frame: &mut Frame,
    area: Rect,
    panel: &EstadoPanelAdmin,
    config: &Config,
    keymap: &Keymap,
    filas_lenguajes: &[FilaLenguajeLsp],
    paleta: &Paleta,
) {
    let estilo_base = Style::default().bg(paleta.fondo).fg(paleta.texto);
    let seccion = panel.seccion_actual();
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(crate::BORDE_ASCII)
        .title(format!(" {} ", seccion.nombre()))
        .style(estilo_base);

    if !seccion.implementada() {
        let parrafo =
            Paragraph::new(seccion.resumen_pendiente()).wrap(Wrap { trim: true }).block(bloque).style(estilo_base);
        frame.render_widget(parrafo, area);
        return;
    }

    let items: Vec<ListItem> = match seccion {
        Seccion::Editor => CampoEditor::TODOS
            .iter()
            .enumerate()
            .map(|(idx, campo)| {
                let seleccionado = idx == panel.campo() && panel.foco() == FocoPanelAdmin::Central;
                let estilo = if seleccionado { estilo_base.bg(paleta.linea_actual) } else { estilo_base };
                let mut texto = format!("{:<34}{}", campo.nombre(), campo.valor_actual(config));
                if let Some(nota) = campo.nota() {
                    texto.push_str(&format!("   ({nota})"));
                }
                ListItem::new(Line::from(Span::styled(texto, estilo))).style(estilo)
            })
            .collect(),
        Seccion::Temas => {
            let activo = tcode_config::TEMAS_EMBEBIDOS.iter().find(|t| t.id == config.interfaz.tema);
            CampoTemas::TODOS
                .iter()
                .enumerate()
                .map(|(idx, campo)| {
                    let seleccionado = idx == panel.campo() && panel.foco() == FocoPanelAdmin::Central;
                    let estilo = if seleccionado { estilo_base.bg(paleta.linea_actual) } else { estilo_base };
                    let mut texto = campo.nombre().to_string();
                    if *campo == CampoTemas::DuplicarActivo {
                        if let Some(tema) = activo {
                            texto = format!("{texto} ({})", tema.nombre);
                        }
                    }
                    ListItem::new(Line::from(Span::styled(texto, estilo))).style(estilo)
                })
                .collect()
        }
        Seccion::Atajos => filas_atajos(panel, keymap, paleta, estilo_base),
        Seccion::Lenguajes => filas_lenguajes_lsp(panel, filas_lenguajes, paleta, estilo_base),
        Seccion::Interfaz => CampoInterfaz::TODOS
            .iter()
            .enumerate()
            .map(|(idx, campo)| {
                let seleccionado = idx == panel.campo() && panel.foco() == FocoPanelAdmin::Central;
                let estilo = if seleccionado { estilo_base.bg(paleta.linea_actual) } else { estilo_base };
                // 41: el nombre más largo de CampoInterfaz mide 38
                // ("Statusbar: resumen de diagnósticos LSP") — deja 3
                // de margen antes del valor.
                let texto = format!("{:<41}{}", campo.nombre(), campo.valor_actual(config));
                ListItem::new(Line::from(Span::styled(texto, estilo))).style(estilo)
            })
            .collect(),
    };
    frame.render_widget(List::new(items).block(bloque), area);
}

/// Filas de la sección "Atajos": una acción especial ("restablecer
/// todos") seguida de una fila por comando de `tcode_commands::
/// comandos_disponibles()`, con su combinación actual (o "(sin atajo)")
/// resaltada en rojo si participa de un conflicto de prefijo
/// (`tcode_keymap::detectar_conflictos`, PLAN.md §5: "detección de
/// conflictos en tiempo real"). Si la fila seleccionada está en modo
/// "esperando la nueva tecla" (`Enter`, ver `EstadoPanelAdmin::
/// capturando`), el valor se reemplaza por un aviso en vez del atajo
/// actual.
fn filas_atajos<'a>(
    panel: &EstadoPanelAdmin,
    keymap: &Keymap,
    paleta: &Paleta,
    estilo_base: Style,
) -> Vec<ListItem<'a>> {
    let conflictos = detectar_conflictos(keymap);
    let en_conflicto = |comando: &str| conflictos.iter().any(|c| c.comando_corto == comando || c.comando_bloqueado == comando);

    let central_activa = panel.foco() == FocoPanelAdmin::Central;
    let mut filas = Vec::new();

    // 3 filas especiales antes de la lista de comandos (PLAN.md §5.1):
    // restablecer todos los atajos, exportar el keymap activo, e
    // importar uno desde el archivo fijo que deja `Keymap::exportar`/
    // `importar_keymap` (ver `app/main.rs`, que interpreta estas mismas
    // posiciones de fila). Sin ícono decorativo (antes ↺/⇩/⇧) — ver
    // `panel_archivos` sobre el ancho ambiguo de esos caracteres en
    // algunas terminales; el texto ya es suficientemente descriptivo
    // solo.
    for (fila, texto) in [
        (0, "Restablecer TODOS los atajos por defecto"),
        (1, "Exportar atajos a archivo"),
        (2, "Importar atajos desde archivo"),
    ] {
        let seleccionada = panel.campo() == fila && central_activa;
        let estilo = if seleccionada { estilo_base.bg(paleta.linea_actual) } else { estilo_base };
        filas.push(ListItem::new(Line::from(Span::styled(texto, estilo))).style(estilo));
    }

    for (idx, comando) in tcode_commands::comandos_disponibles().iter().enumerate() {
        let fila = idx + 3;
        let seleccionada = panel.campo() == fila && central_activa;
        let estilo_fila = if seleccionada { estilo_base.bg(paleta.linea_actual) } else { estilo_base };

        let valor = if seleccionada && panel.capturando() {
            "‹ presioná la nueva combinación… (Esc cancela) ›".to_string()
        } else {
            let atajos = keymap.atajos_para(comando.id);
            if atajos.is_empty() {
                "(sin atajo)".to_string()
            } else {
                atajos.iter().map(|s| formatear_atajo(s)).collect::<Vec<_>>().join(", ")
            }
        };
        let estilo_valor = if seleccionada && panel.capturando() {
            estilo_fila
        } else if en_conflicto(comando.id) {
            estilo_fila.fg(paleta.diagnostico_error)
        } else {
            estilo_fila
        };

        let spans = vec![
            // 47: la descripción más larga de `comandos_disponibles()`
            // hoy mide 45 ("Selección: Seleccionar todas las
            // ocurrencias") — deja 2 de margen antes del atajo.
            Span::styled(format!("{:<47}", comando.descripcion), estilo_fila),
            Span::styled(valor, estilo_valor),
        ];
        filas.push(ListItem::new(Line::from(spans)).style(estilo_fila));
    }
    filas
}

/// Filas de la sección "Lenguajes / LSP" (PLAN.md §5.3): una fila por
/// lenguaje, ya resuelta por `app` a texto (`FilaLenguajeLsp`) — acá solo
/// se decide cómo pintarla (seleccionada, comando en rojo si el binario
/// no está en el `PATH`, atenuada si está deshabilitada, o el buffer de
/// edición en vivo si esta es la fila cuyo comando se está editando —
/// `c`, ver `EstadoPanelAdmin::editando_comando_lsp`).
fn filas_lenguajes_lsp<'a>(
    panel: &EstadoPanelAdmin,
    filas: &[FilaLenguajeLsp],
    paleta: &Paleta,
    estilo_base: Style,
) -> Vec<ListItem<'a>> {
    let central_activa = panel.foco() == FocoPanelAdmin::Central;
    filas
        .iter()
        .enumerate()
        .map(|(idx, fila)| {
            let seleccionada = panel.campo() == idx && central_activa;
            let estilo_fila = if seleccionada { estilo_base.bg(paleta.linea_actual) } else { estilo_base };
            let habilitado = if fila.habilitado { "Sí" } else { "No" };

            if let (true, Some(buffer)) = (seleccionada, panel.editando_comando_lsp()) {
                let spans = vec![
                    Span::styled(format!("{:<14}", fila.nombre), estilo_fila),
                    Span::styled(format!("Habilitado: {habilitado:<5}"), estilo_fila),
                    Span::styled("  ", estilo_fila),
                    Span::styled(format!("{buffer}▏"), estilo_fila),
                    Span::styled(" (Enter guarda · Esc cancela)", estilo_fila),
                ];
                return ListItem::new(Line::from(spans)).style(estilo_fila);
            }

            let estilo_comando = if fila.en_path { estilo_fila } else { estilo_fila.fg(paleta.diagnostico_advertencia) };
            let comando = if fila.comando.is_empty() { "(sin LSP configurado)" } else { &fila.comando };
            let personalizado = if fila.personalizado { " (personalizado)" } else { "" };
            let en_path = if fila.comando.is_empty() {
                String::new()
            } else if fila.en_path {
                " [en el PATH]".to_string()
            } else {
                " [no encontrado en el PATH]".to_string()
            };
            let spans = vec![
                Span::styled(format!("{:<14}", fila.nombre), estilo_fila),
                Span::styled(format!("Habilitado: {habilitado:<5}"), estilo_fila),
                Span::styled(format!("  {comando}"), estilo_comando),
                Span::styled(personalizado, estilo_comando),
                Span::styled(en_path, estilo_comando),
                Span::styled(format!("  — {}", fila.estado), estilo_fila),
            ];
            ListItem::new(Line::from(spans)).style(estilo_fila)
        })
        .collect()
}

/// Mensaje transitorio de la última acción disparada en el área central
/// (por ahora, solo "Temas: Duplicar tema activo" lo deja) — una línea
/// fija entre el área central y el pie, vacía cuando no hay nada que
/// mostrar.
fn dibujar_mensaje(frame: &mut Frame, area: Rect, panel: &EstadoPanelAdmin, paleta: &Paleta) {
    let estilo = Style::default().bg(paleta.fondo).fg(paleta.diagnostico_info);
    let texto = panel.mensaje().unwrap_or("");
    frame.render_widget(Paragraph::new(format!(" {texto}")).style(estilo), area);
}

/// Búsqueda global de opciones (`Ctrl+F` dentro del panel, PLAN.md §5):
/// campo de consulta arriba, resultados abajo con las letras coincidentes
/// en negrita — mismo lenguaje visual que la paleta de comandos y el
/// buscador de archivos (`crate::overlay`), aunque dibujado directo en
/// vez de reusar ese módulo porque aquí no hay que centrar un recuadro
/// flotante: ya vive dentro del área central de este panel.
fn dibujar_busqueda(frame: &mut Frame, area: Rect, panel: &EstadoPanelAdmin, paleta: &Paleta) {
    let estilo_base = Style::default().bg(paleta.fondo).fg(paleta.texto);
    let partes = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let campo = Paragraph::new(format!("> {}", panel.busqueda()))
        .block(Block::default().borders(Borders::ALL).border_set(crate::BORDE_ASCII).title(" Buscar opción "))
        .style(estilo_base);
    frame.render_widget(campo, partes[0]);

    let resultados = panel.resultados_busqueda();
    let items: Vec<ListItem> = resultados
        .iter()
        .enumerate()
        .map(|(idx, r)| {
            let estilo_fila = if idx == panel.campo() { estilo_base.bg(paleta.linea_actual) } else { estilo_base };
            let mut spans: Vec<Span> = r
                .nombre
                .chars()
                .enumerate()
                .map(|(i, c)| {
                    let estilo =
                        if r.posiciones.contains(&i) { estilo_fila.add_modifier(Modifier::BOLD) } else { estilo_fila };
                    Span::styled(c.to_string(), estilo)
                })
                .collect();
            spans.push(Span::styled(format!("  — {}", Seccion::TODAS[r.seccion].nombre()), estilo_fila));
            ListItem::new(Line::from(spans)).style(estilo_fila)
        })
        .collect();
    frame.render_widget(
        List::new(items).block(Block::default().borders(Borders::ALL).border_set(crate::BORDE_ASCII).style(estilo_base)),
        partes[1],
    );
}

/// Barra inferior con el contexto de teclas disponible (PLAN.md §5),
/// distinto según dónde está el foco ahora mismo.
fn dibujar_pie(frame: &mut Frame, area: Rect, panel: &EstadoPanelAdmin, paleta: &Paleta) {
    let texto = match panel.foco() {
        FocoPanelAdmin::Barra => "↑↓ moverse · Enter/→ entrar a la sección · Ctrl+F buscar · Esc cerrar panel",
        FocoPanelAdmin::Central if panel.capturando() => "Presioná la nueva combinación · Esc cancela",
        FocoPanelAdmin::Central if panel.editando_comando_lsp().is_some() => {
            "Escribí el comando y sus argumentos · Enter guarda · Esc cancela"
        }
        FocoPanelAdmin::Central if panel.seccion_actual() == Seccion::Temas => {
            "↑↓ moverse · Enter ejecutar · Tab volver a secciones · Ctrl+F buscar · Esc volver"
        }
        FocoPanelAdmin::Central if panel.seccion_actual() == Seccion::Atajos => {
            "↑↓ moverse · Enter capturar nuevo atajo · Backspace restablecer · Tab secciones · Ctrl+F buscar · Esc volver"
        }
        FocoPanelAdmin::Central if panel.seccion_actual() == Seccion::Lenguajes => {
            "↑↓ moverse · Enter/←→ habilitar · c editar comando · Backspace quitar override · Tab secciones · Esc volver"
        }
        FocoPanelAdmin::Central => {
            "↑↓ moverse · Enter/←→ cambiar valor · Tab volver a secciones · Ctrl+F buscar · Esc volver"
        }
        FocoPanelAdmin::Busqueda => "↑↓ moverse · Enter ir a la opción · Esc cancelar búsqueda",
    };
    let estilo = Style::default().bg(paleta.statusbar_fondo).fg(paleta.statusbar_texto);
    frame.render_widget(Paragraph::new(texto).style(estilo), area);
}
