//! Binario principal de `tcode`: carga de configuración y de atajos,
//! arranque/apagado de la terminal, y el bucle de eventos (PLAN.md §3,
//! crate `app`).
//!
//! Los eventos de teclado pasan por el [`Resolvedor`] de `tcode-keymap`
//! para convertirse en nombres de comando en español (`"archivo.guardar"`);
//! `ejecutar_comando` es el dispatcher que los traduce a llamadas sobre el
//! [`tcode_ui::Layout`] activo (que puede tener varios paneles divididos,
//! `Ctrl+\`) o el [`Explorador`]. La paleta de comandos (`Ctrl+Shift+P`/
//! `F1`) y el buscador de archivos (`Ctrl+P`) producen esos mismos ids por
//! otra vía (buscar por nombre en vez de memorizar un atajo) y terminan en
//! el mismo dispatcher.
//!
//! Desde M2 el bucle es asíncrono (`tokio`): además del teclado, hay que
//! escuchar en paralelo los mensajes que llegan del servidor LSP activo
//! (`lsp.rs`) sin bloquear ninguno de los dos.

mod lsp;

use std::io::{self, Stdout};

use anyhow::Result;
use crossterm::event::{
    Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio_stream::StreamExt;

use tcode_commands::EstadoPaleta;
use tcode_config::{
    CampoTemas, Config, EstadoPanelAdmin, EstadoSelectorTema, FocoPanelAdmin, ResultadoDuplicarTema, Seccion,
};
use tcode_core::{analizar_csv, delimitador_por_extension, serializar_fila_csv, CampoBusqueda, Editor, EstadoBusqueda};
use tcode_fs::{BuscadorArchivos, Explorador};
use tcode_keymap::{Keymap, Resolucion, Resolvedor};
use tcode_syntax::{Lenguaje, Resaltador};
use tcode_ui::{DireccionSplit, FilaLenguajeLsp, Layout as PanelLayout, ModoCsv, Paleta};

type Backend = CrosstermBackend<Stdout>;

#[tokio::main]
async fn main() -> Result<()> {
    let ruta_arg = std::env::args().nth(1);

    let editor = match &ruta_arg {
        Some(ruta) => Editor::abrir(ruta)?,
        None => Editor::nuevo(),
    };
    let mut layout = PanelLayout::nuevo(editor, ruta_arg.clone().unwrap_or_else(|| "[Sin nombre]".to_string()));

    // La config y el keymap nunca hacen fallar el arranque: si el archivo
    // del usuario está corrupto, se sigue con los valores por defecto en
    // vez de negarse a abrir el editor.
    let config = tcode_config::cargar().unwrap_or_default();
    let keymap = tcode_keymap::cargar().unwrap_or_else(|_| tcode_keymap::keymap_por_defecto());
    let explorador = crear_explorador(ruta_arg.as_deref());

    let (mut terminal, protocolo_kitty) = iniciar_terminal()?;
    let resultado = ejecutar(&mut terminal, &mut layout, config, keymap, explorador, ruta_arg.as_deref()).await;
    finalizar_terminal(&mut terminal, protocolo_kitty)?;

    resultado
}

fn cargar_paleta(nombre_tema: &str) -> Paleta {
    let tema = tcode_config::cargar_tema(nombre_tema).unwrap_or_else(|_| tcode_config::tema_por_defecto());
    Paleta::desde_tema(&tema).unwrap_or_else(|_| Paleta::basica())
}

/// El árbol del explorador nunca hace fallar el arranque: si la carpeta no
/// se puede leer, queda vacío (se ve un panel sin filas al abrirlo con
/// `Ctrl+B`) en vez de negarse a abrir el editor.
fn crear_explorador(ruta_arg: Option<&str>) -> Explorador {
    let raiz = tcode_fs::raiz_por_defecto(ruta_arg);
    Explorador::nuevo(raiz).unwrap_or_else(|_| Explorador::vacio())
}

/// La consola clásica de Windows (`conhost.exe`: `cmd.exe` y
/// `powershell.exe` sin Windows Terminal) no usa UTF-8 por defecto —
/// interpreta cada byte de los caracteres especiales de tcode (`│`, `▾`,
/// `▸`, `●`) como un glifo separado del codepage regional del sistema,
/// descuadrando el ancho de columna que `ratatui` calculó. Al redibujar
/// (p. ej. al mover el cursor) eso se ve como texto "faltante" o con
/// artefactos — reportado y confirmado en Windows (CMD y PowerShell
/// clásico) el 2026-09-11: el archivo en disco quedaba intacto, solo la
/// pantalla se veía mal. Forzar el codepage de salida/entrada a UTF-8
/// (65001) antes de dibujar nada lo soluciona.
#[cfg(windows)]
fn configurar_consola_utf8() {
    unsafe {
        windows_sys::Win32::System::Console::SetConsoleOutputCP(65001);
        windows_sys::Win32::System::Console::SetConsoleCP(65001);
    }
}

#[cfg(not(windows))]
fn configurar_consola_utf8() {}

/// `ratatui` normalmente solo redibuja las celdas que cambiaron entre un
/// frame y el siguiente (diffing). Un reporte real en Windows (Windows
/// Terminal, no la consola clásica — se creyó eso al principio, pero
/// `$env:WT_SESSION` confirmó lo contrario) mostró que, tras abrir un
/// archivo desde el explorador (`Ctrl+B`), el contenido queda mal
/// dibujado de forma PERMANENTE (no se autocorrige en frames
/// posteriores) — consistente con que ese frame de transición grande
/// (todo el panel de código cambia de golpe) desincroniza el buffer
/// interno de "último frame" de `ratatui` contra lo que el terminal
/// realmente tiene en pantalla.
///
/// Forzar `Terminal::clear()` en TODOS los frames (intentado en v0.1.3)
/// empeoró el problema — probablemente por saturar la conexión ConPTY
/// con mucho más volumen de datos del necesario en cada tecla. Este fix
/// es quirúrgico: solo se limpia cuando la "forma" de lo que hay en
/// pantalla cambió de verdad (otro archivo activo, otro número de
/// paneles, el explorador se mostró/ocultó) — no en cada tecla.
#[cfg(windows)]
fn forzar_redibujado_completo(terminal: &mut Terminal<Backend>) -> Result<()> {
    terminal.clear()?;
    Ok(())
}

#[cfg(not(windows))]
fn forzar_redibujado_completo(_terminal: &mut Terminal<Backend>) -> Result<()> {
    Ok(())
}

/// Resumen barato de "qué tan distinta se ve la pantalla en términos
/// estructurales" — no del contenido línea a línea (eso cambia
/// constantemente al escribir, no amerita un redibujado completo), sino
/// de la forma general: qué archivo está activo, cuántos paneles hay,
/// si el explorador está visible. Comparar esto antes/después de
/// procesar una tecla es lo que decide si hace falta forzar limpieza.
fn firma_estructural(layout: &PanelLayout, explorador: &Explorador) -> (String, usize, bool) {
    (layout.panel_activo().ruta_mostrada.clone(), layout.num_paneles(), explorador.visible())
}

/// Además de inicializar la terminal, intenta activar el protocolo de
/// teclado extendido de Kitty (best-effort: si el terminal no lo soporta
/// no pasa nada, `desde_evento` sigue funcionando igual). Sin esto,
/// `Ctrl+Shift+<letra>` es indistinguible de `Ctrl+<letra>` en terminales
/// clásicas (confirmado con un diagnóstico directo contra `crossterm` en
/// tmux) — por eso `paleta.comandos` también tiene `F1` como atajo
/// alternativo universal en `runtime/keymaps/default.toml`.
fn iniciar_terminal() -> Result<(Terminal<Backend>, bool)> {
    configurar_consola_utf8();
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let protocolo_kitty = supports_keyboard_enhancement().unwrap_or(false);
    if protocolo_kitty {
        execute!(stdout, PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES))?;
    }

    Ok((Terminal::new(CrosstermBackend::new(stdout))?, protocolo_kitty))
}

fn finalizar_terminal(terminal: &mut Terminal<Backend>, protocolo_kitty: bool) -> Result<()> {
    if protocolo_kitty {
        execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags)?;
    }
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

/// Qué panel recibe las teclas de navegación/edición genéricas
/// (`cursor.*`, `Enter`...). Los comandos globales (guardar, deshacer,
/// salir, recargar config, dividir/cerrar/ir a un panel) funcionan sin
/// importar el foco. Mientras la paleta de comandos o el buscador de
/// archivos están abiertos, ningún foco importa: capturan el teclado por
/// completo (ver `ejecutar`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Foco {
    Editor,
    Explorador,
}

/// Resultado de ejecutar un comando: si el bucle principal debe seguir o
/// terminar.
enum Accion {
    Continuar,
    Salir,
}

/// Todo el estado mutable que un comando puede necesitar tocar, agrupado
/// para no ir sumando parámetros sueltos a cada función del dispatcher
/// según crece la lista de comandos. `layout` (el árbol de paneles de
/// edición, cada uno con su propio documento) se mantiene aparte, fuera
/// de este struct: es "el documento activo", conceptualmente distinto de
/// "todo lo demás".
struct EstadoApp {
    config: Config,
    paleta: Paleta,
    explorador: Explorador,
    foco: Foco,
    confirmar_salida: bool,
    paleta_comandos: EstadoPaleta,
    buscador_archivos: BuscadorArchivos,
    estado_busqueda: EstadoBusqueda,
    selector_tema: EstadoSelectorTema,
    panel_admin: EstadoPanelAdmin,
    /// Keymap activo — fuente de verdad para la sección "Atajos" del
    /// panel de administración (`Ctrl+,`, PLAN.md §5). El `Resolvedor`
    /// que de verdad resuelve teclas tiene su PROPIA copia (`resolvedor`
    /// es una variable aparte, no un campo de este struct): cada vez que
    /// este campo cambia hay que llamar `resolvedor.reemplazar_keymap`
    /// con un clon para que el cambio surta efecto en el editor real, no
    /// solo en lo que se ve en el panel.
    keymap: Keymap,
    lsp: lsp::EstadoLsp,
}

fn sin_modificadores(key: KeyEvent) -> bool {
    !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT)
}

async fn ejecutar(
    terminal: &mut Terminal<Backend>,
    layout: &mut PanelLayout,
    config: Config,
    keymap: Keymap,
    explorador: Explorador,
    ruta_arg: Option<&str>,
) -> Result<()> {
    let mut resaltador = Resaltador::nuevo();
    let mut resolvedor = Resolvedor::nuevo(keymap.clone());
    let mut eventos = EventStream::new();

    // Info fija de las secciones "Atajos" y "Lenguajes / LSP" del panel
    // de administración (`Ctrl+,`, PLAN.md §5): ni la lista de comandos
    // ni la de lenguajes cambia durante la sesión, así que se calculan
    // una sola vez al arrancar en vez de en cada tecla (ver doc de
    // `EstadoPanelAdmin::fijar_opciones_externas`).
    let mut panel_admin = EstadoPanelAdmin::nueva();
    let indice_atajos = tcode_config::indice_de(Seccion::Atajos);
    let indice_lenguajes = tcode_config::indice_de(Seccion::Lenguajes);
    panel_admin.fijar_num_filas_atajos(1 + tcode_commands::comandos_disponibles().len());
    panel_admin.fijar_num_filas_lenguajes(Lenguaje::TODOS.len());
    panel_admin.fijar_opciones_externas(
        tcode_commands::comandos_disponibles()
            .iter()
            .enumerate()
            .map(|(i, c)| tcode_config::OpcionExterna { seccion: indice_atajos, campo: i + 1, nombre: c.descripcion })
            .chain(Lenguaje::TODOS.iter().enumerate().map(|(i, l)| tcode_config::OpcionExterna {
                seccion: indice_lenguajes,
                campo: i,
                nombre: l.nombre_mostrado(),
            }))
            .collect(),
    );

    let mut estado = EstadoApp {
        paleta: cargar_paleta(&config.interfaz.tema),
        config,
        explorador,
        foco: Foco::Editor,
        // Ctrl+Q con cambios sin guardar pide una segunda confirmación en
        // vez de perder trabajo en silencio (nano-style). Cualquier otra
        // resolución la cancela.
        confirmar_salida: false,
        paleta_comandos: EstadoPaleta::nueva(),
        buscador_archivos: BuscadorArchivos::nuevo(tcode_fs::raiz_por_defecto(ruta_arg)),
        estado_busqueda: EstadoBusqueda::nueva(),
        selector_tema: EstadoSelectorTema::nueva(),
        panel_admin,
        keymap,
        lsp: lsp::EstadoLsp::nuevo(),
    };

    // El archivo abierto al arrancar también dispara el LSP si su
    // lenguaje tiene uno configurado.
    sincronizar_lsp(layout, &mut estado.lsp, &estado.config).await;

    // Ver `forzar_redibujado_completo`: en Windows, si la "forma" de la
    // pantalla cambió (otro archivo activo, otro número de paneles, el
    // explorador se mostró/ocultó), se limpia antes del próximo draw.
    let mut necesita_redibujado = false;

    loop {
        if necesita_redibujado {
            forzar_redibujado_completo(terminal)?;
            necesita_redibujado = false;
        }

        // Barato (5 lenguajes): se recalcula cada frame en vez de
        // cachearlo, para que el estado en vivo del cliente LSP
        // (conectado/iniciando) se vea siempre actualizado en la
        // sección "Lenguajes / LSP" mientras el panel está abierto ahí.
        let filas_lenguajes = filas_lenguajes_lsp(&estado);

        terminal.draw(|frame| {
            tcode_ui::dibujar(
                frame,
                layout,
                &estado.paleta,
                &mut resaltador,
                &estado.explorador,
                &estado.paleta_comandos,
                &estado.buscador_archivos,
                &estado.estado_busqueda,
                &estado.selector_tema,
                &estado.panel_admin,
                &estado.config,
                &estado.keymap,
                &filas_lenguajes,
            )
        })?;

        let firma_antes = firma_estructural(layout, &estado.explorador);

        let key = tokio::select! {
            evento = eventos.next() => {
                match evento {
                    Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => key,
                    _ => continue,
                }
            }
            mensaje = estado.lsp.siguiente_mensaje() => {
                if let Some(mensaje) = mensaje {
                    estado.lsp.procesar_mensaje(mensaje, layout).await;
                }
                continue;
            }
        };

        // El panel de administración (`Ctrl+,`, PLAN.md §5) es una vista
        // aparte que también captura el teclado por completo mientras
        // está abierta, con su propia navegación de tres niveles (barra
        // lateral de secciones, área central de la sección actual,
        // búsqueda global de opciones) en vez de encajar en ningún otro
        // bloque modal de acá abajo.
        if estado.panel_admin.activo() {
            estado.confirmar_salida = false;
            // Mientras se espera la tecla que va a convertirse en el
            // nuevo atajo (`Enter` sobre un comando en "Atajos"), CUALQUIER
            // tecla (salvo `Esc`, que cancela) se consume acá — ni
            // siquiera `↑`/`↓`/`Tab` navegan mientras tanto, tiene que
            // resolverse esta captura antes de cualquier otra cosa.
            if estado.panel_admin.capturando() {
                manejar_captura_atajo(&mut estado, &mut resolvedor, key);
            } else {
                match estado.panel_admin.foco() {
                    FocoPanelAdmin::Barra => match key.code {
                        KeyCode::Esc => {
                            estado.panel_admin.escape();
                        }
                        KeyCode::Up => estado.panel_admin.mover_seccion_arriba(),
                        KeyCode::Down => estado.panel_admin.mover_seccion_abajo(),
                        KeyCode::Enter | KeyCode::Right => estado.panel_admin.entrar(),
                        KeyCode::Tab => estado.panel_admin.alternar_foco(),
                        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            estado.panel_admin.abrir_busqueda()
                        }
                        _ => {}
                    },
                    FocoPanelAdmin::Central => match key.code {
                        KeyCode::Esc => {
                            estado.panel_admin.escape();
                        }
                        KeyCode::Up => estado.panel_admin.mover_campo_arriba(),
                        KeyCode::Down => estado.panel_admin.mover_campo_abajo(),
                        KeyCode::Tab => estado.panel_admin.alternar_foco(),
                        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            estado.panel_admin.abrir_busqueda()
                        }
                        // `Ctrl+S` dentro del panel (PLAN.md §5): cada
                        // cambio ya se aplica y persiste al instante (ver
                        // los brazos de `Enter`/`←`/`→`/`Backspace` de
                        // abajo) — este atajo es redundante a propósito,
                        // para que exista igual el gesto de "guardar" que
                        // describe el plan, sin arriesgar perder cambios
                        // si alguien lo espera.
                        KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            let _ = tcode_config::guardar(&estado.config);
                        }
                        KeyCode::Enter if estado.panel_admin.seccion_actual() == Seccion::Temas => {
                            ejecutar_accion_temas_admin(&mut estado);
                        }
                        KeyCode::Enter if estado.panel_admin.seccion_actual() == Seccion::Atajos => {
                            iniciar_o_restablecer_todos_los_atajos(&mut estado, &mut resolvedor);
                        }
                        KeyCode::Backspace if estado.panel_admin.seccion_actual() == Seccion::Atajos => {
                            restablecer_atajo_seleccionado(&mut estado, &mut resolvedor);
                        }
                        KeyCode::Enter | KeyCode::Left | KeyCode::Right
                            if estado.panel_admin.seccion_actual() == Seccion::Lenguajes =>
                        {
                            alternar_lsp_lenguaje_seleccionado(&mut estado);
                        }
                        KeyCode::Enter | KeyCode::Left | KeyCode::Right => {
                            if let Some(campo) = estado.panel_admin.campo_editor_actual() {
                                let delta = if key.code == KeyCode::Left { -1 } else { 1 };
                                campo.aplicar(&mut estado.config, delta);
                                let _ = tcode_config::guardar(&estado.config);
                            }
                        }
                        _ => {}
                    },
                    FocoPanelAdmin::Busqueda => match key.code {
                        KeyCode::Esc => {
                            estado.panel_admin.escape();
                        }
                        KeyCode::Up => estado.panel_admin.mover_campo_arriba(),
                        KeyCode::Down => estado.panel_admin.mover_campo_abajo(),
                        KeyCode::Backspace => estado.panel_admin.borrar_busqueda(),
                        KeyCode::Enter => estado.panel_admin.confirmar_busqueda(),
                        KeyCode::Char(c) if sin_modificadores(key) => estado.panel_admin.escribir_busqueda(c),
                        _ => {}
                    },
                }
            }
            sincronizar_lsp(layout, &mut estado.lsp, &estado.config).await;
            necesita_redibujado |= firma_estructural(layout, &estado.explorador) != firma_antes;
            continue;
        }

        // Paleta de comandos y buscador de archivos son modales
        // mutuamente excluyentes que capturan el teclado por completo
        // mientras están abiertos: escribir busca, no pasa por el sistema
        // de atajos ni llega al editor.
        if estado.paleta_comandos.activa() {
            estado.confirmar_salida = false;
            match key.code {
                KeyCode::Esc => estado.paleta_comandos.cerrar(),
                KeyCode::Up => estado.paleta_comandos.mover_arriba(),
                KeyCode::Down => estado.paleta_comandos.mover_abajo(),
                KeyCode::Backspace => estado.paleta_comandos.borrar(),
                KeyCode::Enter => {
                    if let Some(id) = estado.paleta_comandos.confirmar() {
                        if let Accion::Salir = procesar_comando(id, layout, &mut estado, &mut resolvedor) {
                            break;
                        }
                    }
                }
                KeyCode::Char(c) if sin_modificadores(key) => estado.paleta_comandos.escribir(c),
                _ => {}
            }
            sincronizar_lsp(layout, &mut estado.lsp, &estado.config).await;
            necesita_redibujado |= firma_estructural(layout, &estado.explorador) != firma_antes;
            continue;
        }

        if estado.buscador_archivos.activo() {
            estado.confirmar_salida = false;
            match key.code {
                KeyCode::Esc => estado.buscador_archivos.cerrar(),
                KeyCode::Up => estado.buscador_archivos.mover_arriba(),
                KeyCode::Down => estado.buscador_archivos.mover_abajo(),
                KeyCode::Backspace => estado.buscador_archivos.borrar(),
                KeyCode::Enter => {
                    if let Some(ruta) = estado.buscador_archivos.confirmar() {
                        if let Ok(nuevo_editor) = Editor::abrir(&ruta) {
                            layout.abrir_en_activo(nuevo_editor, ruta.display().to_string());
                        }
                    }
                }
                KeyCode::Char(c) if sin_modificadores(key) => estado.buscador_archivos.escribir(c),
                _ => {}
            }
            sincronizar_lsp(layout, &mut estado.lsp, &estado.config).await;
            necesita_redibujado |= firma_estructural(layout, &estado.explorador) != firma_antes;
            continue;
        }

        // El selector de temas (`Ctrl+K Ctrl+T`, PLAN.md §7) también
        // captura el teclado por completo mientras está abierto: `↑`/`↓`
        // recorren la lista aplicando cada tema de inmediato a `estado.
        // paleta` (preview en vivo, sin tocar `config.toml` todavía),
        // `Tab` cambia el filtro Todos/Oscuro/Claro, `Enter` confirma
        // (persiste el cambio) y `Esc` cancela volviendo al tema que
        // estaba activo antes de abrir el selector.
        if estado.selector_tema.activa() {
            estado.confirmar_salida = false;
            match key.code {
                KeyCode::Esc => {
                    let original = estado.selector_tema.tema_original().to_string();
                    estado.selector_tema.cerrar();
                    estado.paleta = cargar_paleta(&original);
                }
                KeyCode::Up => {
                    estado.selector_tema.mover_arriba();
                    aplicar_preview_tema(&mut estado);
                }
                KeyCode::Down => {
                    estado.selector_tema.mover_abajo();
                    aplicar_preview_tema(&mut estado);
                }
                KeyCode::Tab => {
                    estado.selector_tema.alternar_filtro();
                    aplicar_preview_tema(&mut estado);
                }
                KeyCode::Enter => {
                    if let Some(id) = estado.selector_tema.confirmar() {
                        confirmar_tema_seleccionado(&mut estado, id);
                    }
                }
                _ => {}
            }
            sincronizar_lsp(layout, &mut estado.lsp, &estado.config).await;
            necesita_redibujado |= firma_estructural(layout, &estado.explorador) != firma_antes;
            continue;
        }

        // La barra de búsqueda/reemplazo (`Ctrl+F`/`Ctrl+H`) también
        // captura el teclado por completo mientras está abierta, pero a
        // diferencia de la paleta/buscador de archivos algunos de sus
        // atajos (`Alt+R/C/W`, `F3`) coinciden con combinaciones que el
        // keymap también define como comandos globales — se manejan aquí
        // directamente en vez de pasar por el `Resolvedor` para no
        // duplicar esa lógica dos veces.
        if estado.estado_busqueda.activa() {
            estado.confirmar_salida = false;
            let editor = layout.editor_activo_mut();
            let texto = editor.buffer().a_texto();
            match key.code {
                KeyCode::Esc => estado.estado_busqueda.cerrar(),
                KeyCode::Tab => estado.estado_busqueda.alternar_campo(),
                KeyCode::Backspace => estado.estado_busqueda.borrar(&texto),
                KeyCode::F(3) if key.modifiers.contains(KeyModifiers::SHIFT) => estado.estado_busqueda.anterior(),
                KeyCode::F(3) => estado.estado_busqueda.siguiente(),
                KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::ALT) => {
                    estado.estado_busqueda.alternar_regex(&texto)
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::ALT) => {
                    estado.estado_busqueda.alternar_mayusculas(&texto)
                }
                KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::ALT) => {
                    estado.estado_busqueda.alternar_palabra(&texto)
                }
                // `Ctrl+Alt+Enter` es el atajo "canónico", pero terminales
                // clásicas sin el protocolo extendido de Kitty a veces no
                // transmiten ambos modificadores a la vez sobre `Enter`
                // (confirmado en tmux); `Alt+Enter` a secas sí se
                // distingue siempre, así que también vale. Solo en modo
                // reemplazar: en modo "solo buscar" no hay texto de
                // reemplazo que usar (sería reemplazar todo por "").
                KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) && estado.estado_busqueda.modo_reemplazar() => {
                    reemplazar_todas_las_coincidencias(editor, &mut estado.estado_busqueda);
                }
                KeyCode::Enter
                    if estado.estado_busqueda.modo_reemplazar()
                        && estado.estado_busqueda.campo_activo() == CampoBusqueda::Reemplazo =>
                {
                    reemplazar_coincidencia_actual(editor, &mut estado.estado_busqueda);
                }
                KeyCode::Enter => estado.estado_busqueda.siguiente(),
                KeyCode::Char(c) if sin_modificadores(key) => estado.estado_busqueda.escribir(c, &texto),
                _ => {}
            }
            sincronizar_lsp(layout, &mut estado.lsp, &estado.config).await;
            necesita_redibujado |= firma_estructural(layout, &estado.explorador) != firma_antes;
            continue;
        }

        // Edición de una celda de la vista CSV/TSV (`Enter`/`F2` sobre
        // una celda, PLAN.md §9): captura el teclado por completo igual
        // que los bloques anteriores, mientras dura la edición de esa
        // celda puntual (la navegación entre celdas, cuando NO se está
        // editando ninguna, pasa por el `Resolvedor` normal más abajo,
        // reinterpretada en `ejecutar_comando_csv`).
        if layout.panel_activo().estado_csv.editando() {
            estado.confirmar_salida = false;
            match key.code {
                KeyCode::Esc => layout.panel_activo_mut().estado_csv.cancelar_edicion(),
                KeyCode::Backspace => layout.panel_activo_mut().estado_csv.borrar(),
                KeyCode::Enter => confirmar_edicion_celda_csv(layout, true),
                KeyCode::Char(c) if sin_modificadores(key) => layout.panel_activo_mut().estado_csv.escribir(c),
                _ => {}
            }
            sincronizar_lsp(layout, &mut estado.lsp, &estado.config).await;
            necesita_redibujado |= firma_estructural(layout, &estado.explorador) != firma_antes;
            continue;
        }

        let resolucion = resolvedor.procesar(tcode_keymap::desde_evento(key));

        let reintentando_salida = matches!(&resolucion, Resolucion::Comando(n) if n == "app.salir");
        if !reintentando_salida {
            estado.confirmar_salida = false;
        }

        match resolucion {
            Resolucion::Comando(nombre) => {
                if let Accion::Salir = procesar_comando(&nombre, layout, &mut estado, &mut resolvedor) {
                    break;
                }
            }
            Resolucion::Pendiente | Resolucion::Cancelado => {}
            Resolucion::SinCoincidencia => {
                // Ninguna tecla/chord configurado coincide: si es un
                // carácter imprimible sin Ctrl/Alt Y el foco está en el
                // editor, se inserta como texto normal (escribir no pasa
                // por el sistema de atajos; el explorador no recibe
                // texto). La vista de tabla CSV/TSV tampoco: no hay
                // cursor de texto visible ahí — insertarlo a ciegas en el
                // buffer crudo corrompería el archivo sin que se vea en
                // pantalla. Escribir en una celda pasa por el bloque
                // modal de edición (`Enter`/`F2`), no por aquí.
                if estado.foco == Foco::Editor && layout.panel_activo().modo_csv != ModoCsv::Tabla {
                    if let KeyCode::Char(c) = key.code {
                        if sin_modificadores(key) {
                            layout.editor_activo_mut().insertar_char(c);
                        }
                    }
                }
            }
        }

        sincronizar_lsp(layout, &mut estado.lsp, &estado.config).await;
        necesita_redibujado |= firma_estructural(layout, &estado.explorador) != firma_antes;
    }

    estado.lsp.cerrar().await;
    Ok(())
}

/// Le avisa al `EstadoLsp` cuál es el archivo/contenido activos ahora
/// mismo: relanza el cliente si cambió el lenguaje (o si se
/// habilitó/deshabilitó desde el panel de administración, sección
/// "Lenguajes / LSP", PLAN.md §5.3 — `config` es lo que decide eso), y
/// notifica `didChange` si el texto cambió desde el último envío.
async fn sincronizar_lsp(layout: &PanelLayout, lsp: &mut lsp::EstadoLsp, config: &Config) {
    let panel = layout.panel_activo();
    let contenido = panel.editor.buffer().a_texto();
    lsp.actualizar_para_archivo(&panel.ruta_mostrada, &contenido, config).await;
    lsp.sincronizar_contenido(&contenido).await;
}

/// Punto de entrada único para ejecutar un id de comando, venga de un
/// atajo de teclado o de confirmar un resultado en la paleta de comandos.
/// `config.recargar`, `paleta.comandos` y `buscar.archivos` necesitan
/// estado que no le corresponde a `ejecutar_comando` (la paleta de
/// colores, los propios overlays), así que se interceptan aquí antes de
/// delegar.
fn procesar_comando(id: &str, layout: &mut PanelLayout, estado: &mut EstadoApp, resolvedor: &mut Resolvedor) -> Accion {
    match id {
        "config.recargar" => {
            recargar_config_tema_y_keymap(estado, resolvedor);
            Accion::Continuar
        }
        "paleta.comandos" => {
            estado.paleta_comandos.abrir();
            Accion::Continuar
        }
        "buscar.archivos" => {
            estado.buscador_archivos.abrir();
            Accion::Continuar
        }
        "tema.seleccionar" => {
            estado.selector_tema.abrir(&estado.config.interfaz.tema);
            Accion::Continuar
        }
        "admin.abrir_panel" => {
            estado.panel_admin.abrir();
            Accion::Continuar
        }
        "buscar.en_archivo" | "buscar.reemplazar" => {
            let texto = layout.editor_activo().buffer().a_texto();
            estado.estado_busqueda.abrir(id == "buscar.reemplazar", &texto);
            saltar_a_coincidencia_actual(layout, &estado.estado_busqueda);
            Accion::Continuar
        }
        // Repiten la última búsqueda aunque la barra esté cerrada (estilo
        // VSCode): útil para seguir saltando entre coincidencias sin
        // volver a abrir `Ctrl+F` cada vez.
        "buscar.siguiente" => {
            estado.estado_busqueda.siguiente();
            saltar_a_coincidencia_actual(layout, &estado.estado_busqueda);
            Accion::Continuar
        }
        "buscar.anterior" => {
            estado.estado_busqueda.anterior();
            saltar_a_coincidencia_actual(layout, &estado.estado_busqueda);
            Accion::Continuar
        }
        "buscar.alternar_regex" => {
            let texto = layout.editor_activo().buffer().a_texto();
            estado.estado_busqueda.alternar_regex(&texto);
            Accion::Continuar
        }
        "buscar.alternar_mayusculas" => {
            let texto = layout.editor_activo().buffer().a_texto();
            estado.estado_busqueda.alternar_mayusculas(&texto);
            Accion::Continuar
        }
        "buscar.alternar_palabra" => {
            let texto = layout.editor_activo().buffer().a_texto();
            estado.estado_busqueda.alternar_palabra(&texto);
            Accion::Continuar
        }
        _ => ejecutar_comando(
            id,
            layout,
            &estado.config,
            &mut estado.explorador,
            &mut estado.foco,
            &mut estado.confirmar_salida,
        ),
    }
}

/// Dispatcher comando -> acción. Los nombres coinciden con los de
/// `runtime/keymaps/default.toml` y con PLAN.md §4.
fn ejecutar_comando(
    comando: &str,
    layout: &mut PanelLayout,
    config: &Config,
    explorador: &mut Explorador,
    foco: &mut Foco,
    confirmar_salida: &mut bool,
) -> Accion {
    // Comandos globales: funcionan sin importar qué panel tiene el foco.
    match comando {
        "app.salir" => {
            if layout.editor_activo().buffer().modificado() && !*confirmar_salida {
                *confirmar_salida = true;
            } else {
                return Accion::Salir;
            }
            return Accion::Continuar;
        }
        // M1 no tiene "guardar como" todavía (llega con la paleta de
        // comandos en M2): si el buffer no tiene ruta, Ctrl+S no hace nada
        // en vez de hacer fallar el editor entero.
        "archivo.guardar" => {
            let _ = layout.editor_activo_mut().guardar();
            return Accion::Continuar;
        }
        "editor.deshacer" => {
            layout.editor_activo_mut().deshacer();
            return Accion::Continuar;
        }
        "editor.rehacer" => {
            layout.editor_activo_mut().rehacer();
            return Accion::Continuar;
        }
        "panel.alternar_lateral" => {
            explorador.alternar_visibilidad();
            *foco = if explorador.visible() { Foco::Explorador } else { Foco::Editor };
            return Accion::Continuar;
        }
        "explorador.enfocar_editor" => {
            // `Esc` siempre significa "volver a un solo cursor" también
            // (PLAN.md §11 M3, `cursor.una_seleccion`) — no hace falta un
            // atajo aparte: si ya había uno solo, esto no hace nada.
            layout.editor_activo_mut().colapsar_cursores();
            *foco = Foco::Editor;
            return Accion::Continuar;
        }
        "panel.dividir_vertical" => {
            layout.dividir(DireccionSplit::Vertical);
            return Accion::Continuar;
        }
        "panel.dividir_horizontal" => {
            layout.dividir(DireccionSplit::Horizontal);
            return Accion::Continuar;
        }
        "panel.cerrar" => {
            layout.cerrar_activo();
            return Accion::Continuar;
        }
        "panel.ir_a_1" => {
            layout.ir_a_panel(0);
            return Accion::Continuar;
        }
        "panel.ir_a_2" => {
            layout.ir_a_panel(1);
            return Accion::Continuar;
        }
        "panel.ir_a_3" => {
            layout.ir_a_panel(2);
            return Accion::Continuar;
        }
        "markdown.alternar_preview" => {
            layout.alternar_preview_markdown();
            return Accion::Continuar;
        }
        "markdown.preview_solo" => {
            layout.alternar_preview_solo_markdown();
            return Accion::Continuar;
        }
        "csv.alternar_vista_tabla" => {
            layout.alternar_vista_tabla_csv();
            return Accion::Continuar;
        }
        "cursor.seleccionar_siguiente_ocurrencia" => {
            layout.editor_activo_mut().seleccionar_siguiente_ocurrencia();
            return Accion::Continuar;
        }
        "cursor.seleccionar_todas_ocurrencias" => {
            layout.editor_activo_mut().seleccionar_todas_ocurrencias();
            return Accion::Continuar;
        }
        "cursor.agregar_arriba" => {
            layout.editor_activo_mut().agregar_cursor_arriba();
            return Accion::Continuar;
        }
        "cursor.agregar_abajo" => {
            layout.editor_activo_mut().agregar_cursor_abajo();
            return Accion::Continuar;
        }
        _ => {}
    }

    if *foco == Foco::Explorador {
        return ejecutar_comando_explorador(comando, layout, explorador, foco);
    }

    // La vista de tabla CSV/TSV reinterpreta la navegación genérica igual
    // que el explorador (arriba): mover celda en vez de mover el cursor
    // de texto, `Tab`/`Shift+Tab` para saltar de celda, `Enter`/`F2` para
    // empezar a editar la celda seleccionada (PLAN.md §9).
    if layout.panel_activo().modo_csv == ModoCsv::Tabla {
        return ejecutar_comando_csv(comando, layout);
    }

    let editor = layout.editor_activo_mut();
    match comando {
        "cursor.arriba" => editor.mover_arriba(),
        "cursor.abajo" => editor.mover_abajo(),
        "cursor.izquierda" => editor.mover_izquierda(),
        "cursor.derecha" => editor.mover_derecha(),
        "cursor.inicio_linea" => editor.inicio_linea(),
        "cursor.fin_linea" => editor.fin_linea(),
        "cursor.inicio_archivo" => editor.inicio_archivo(),
        "cursor.fin_archivo" => editor.fin_archivo(),
        "editor.borrar_atras" => editor.borrar_atras(),
        "editor.borrar_adelante" => editor.borrar_adelante(),
        "editor.nueva_linea" => editor.insertar_char('\n'),
        "editor.indentar_o_autocompletar" => insertar_tabulacion(editor, config),
        _ => {}
    }
    Accion::Continuar
}

/// Comandos genéricos de navegación reinterpretados para el explorador:
/// las mismas teclas mueven la selección del árbol o abren/expanden la
/// fila seleccionada, en vez de mover el cursor del editor.
fn ejecutar_comando_explorador(comando: &str, layout: &mut PanelLayout, explorador: &mut Explorador, foco: &mut Foco) -> Accion {
    match comando {
        "cursor.arriba" => explorador.mover_arriba(),
        "cursor.abajo" => explorador.mover_abajo(),
        "editor.nueva_linea" => {
            if let Ok(Some(ruta)) = explorador.activar_seleccion() {
                if let Ok(nuevo_editor) = Editor::abrir(&ruta) {
                    layout.abrir_en_activo(nuevo_editor, ruta.display().to_string());
                    // Abrir un archivo devuelve el foco al editor: el
                    // usuario ya eligió qué quería, tiene sentido poder
                    // escribir de inmediato en vez de seguir en el árbol.
                    *foco = Foco::Editor;
                }
            }
        }
        _ => {}
    }
    Accion::Continuar
}

/// Comandos genéricos de navegación reinterpretados para la vista de
/// tabla CSV/TSV (PLAN.md §9): mover la celda seleccionada en vez del
/// cursor de texto, `Tab`/`Shift+Tab` para saltar de celda (como en una
/// hoja de cálculo) y `Enter`/`F2` para empezar a editar la celda actual
/// — la edición en sí (escribir/confirmar/cancelar) la captura un bloque
/// modal aparte en el bucle principal, igual que la barra de búsqueda.
fn ejecutar_comando_csv(comando: &str, layout: &mut PanelLayout) -> Accion {
    let delimitador = delimitador_por_extension(&layout.panel_activo().ruta_mostrada);
    let texto = layout.panel_activo().editor.buffer().a_texto();
    let tabla = analizar_csv(&texto, delimitador).unwrap_or_default();
    let (num_filas, num_columnas) = (tabla.num_filas(), tabla.num_columnas());

    let panel = layout.panel_activo_mut();
    match comando {
        "cursor.arriba" => panel.estado_csv.mover_arriba(),
        "cursor.abajo" => panel.estado_csv.mover_abajo(num_filas),
        "cursor.izquierda" => panel.estado_csv.mover_izquierda(),
        "cursor.derecha" => panel.estado_csv.mover_derecha(num_columnas),
        "editor.indentar_o_autocompletar" => panel.estado_csv.tab(num_filas, num_columnas),
        "editor.desindentar" => panel.estado_csv.shift_tab(num_columnas),
        "editor.nueva_linea" | "csv.editar_celda" => {
            let valor_actual =
                tabla.filas.get(panel.estado_csv.fila()).and_then(|f| f.celdas.get(panel.estado_csv.columna()));
            panel.estado_csv.iniciar_edicion(valor_actual.map(String::as_str).unwrap_or(""));
        }
        _ => {}
    }
    Accion::Continuar
}

/// `Enter` con una celda de la vista CSV/TSV en edición: reemplaza la
/// fila completa reserializada (ver `tcode_core::csv::serializar_fila`)
/// en el buffer, y si `avanzar` es `true` mueve la selección a la fila
/// siguiente (como confirmar una celda en una hoja de cálculo).
fn confirmar_edicion_celda_csv(layout: &mut PanelLayout, avanzar: bool) {
    let panel = layout.panel_activo_mut();
    let Some(nuevo_valor) = panel.estado_csv.confirmar_edicion() else { return };

    let delimitador = delimitador_por_extension(&panel.ruta_mostrada);
    let texto = panel.editor.buffer().a_texto();
    let Ok(tabla) = analizar_csv(&texto, delimitador) else { return };
    let Some(fila) = tabla.filas.get(panel.estado_csv.fila()) else { return };

    let mut celdas = fila.celdas.clone();
    if panel.estado_csv.columna() >= celdas.len() {
        celdas.resize(panel.estado_csv.columna() + 1, String::new());
    }
    celdas[panel.estado_csv.columna()] = nuevo_valor;

    let Ok(nueva_fila_texto) = serializar_fila_csv(&celdas, delimitador) else { return };
    panel.editor.reemplazar_rango_bytes(fila.inicio_byte, fila.fin_byte, &nueva_fila_texto);

    if avanzar {
        let num_filas =
            analizar_csv(&panel.editor.buffer().a_texto(), delimitador).map(|t| t.num_filas()).unwrap_or(0);
        panel.estado_csv.mover_abajo(num_filas);
    }
}

/// `config.recargar` (`Ctrl+K Ctrl+L`): recarga `config.toml` y
/// `keymap.toml` desde disco sin reiniciar el editor (PLAN.md §4),
/// reconstruye la paleta de colores si el tema activo cambió, y
/// refresca el `Resolvedor` con el keymap recién leído — sin este último
/// paso, un atajo editado a mano en `keymap.toml` (o restablecido desde
/// el panel de administración en otra instancia) no tomaría efecto hasta
/// reiniciar, aunque el panel ya mostrara el valor nuevo.
fn recargar_config_tema_y_keymap(estado: &mut EstadoApp, resolvedor: &mut Resolvedor) {
    if let Ok(nueva) = tcode_config::cargar() {
        let tema_cambio = nueva.interfaz.tema != estado.config.interfaz.tema;
        estado.config = nueva;
        if tema_cambio {
            estado.paleta = cargar_paleta(&estado.config.interfaz.tema);
        }
    }
    if let Ok(nuevo_keymap) = tcode_keymap::cargar() {
        estado.keymap = nuevo_keymap;
        resolvedor.reemplazar_keymap(estado.keymap.clone());
    }
}

/// Aplica a `estado.paleta` el tema bajo la fila seleccionada del selector
/// (`Ctrl+K Ctrl+T`) — el preview en vivo que se ve mientras se navega la
/// lista con `↑`/`↓`, sin persistir nada todavía en `config.toml`. Si el
/// filtro actual no deja ninguna fila visible, no hace nada (la paleta se
/// queda como estaba).
fn aplicar_preview_tema(estado: &mut EstadoApp) {
    if let Some(id) = estado.selector_tema.tema_seleccionado() {
        estado.paleta = cargar_paleta(id);
    }
}

/// `Enter` sobre una fila del selector de temas: además del preview que ya
/// se venía aplicando, persiste el cambio en `config.toml` para que
/// sobreviva a reiniciar el editor (best-effort — si no se puede escribir
/// a disco, el tema queda igual aplicado en memoria para esta sesión).
fn confirmar_tema_seleccionado(estado: &mut EstadoApp, id: &str) {
    estado.config.interfaz.tema = id.to_string();
    estado.paleta = cargar_paleta(id);
    let _ = tcode_config::guardar(&estado.config);
}

/// `Enter` sobre una fila de la sección "Temas" del panel de
/// administración (PLAN.md §5): a diferencia de "Editor", estas filas no
/// alternan un valor en el sitio — disparan una acción que cruza a
/// estado que `tcode-config` no conoce (abrir el selector de temas ya
/// activo, o escribir un archivo).
fn ejecutar_accion_temas_admin(estado: &mut EstadoApp) {
    match estado.panel_admin.campo_temas_actual() {
        Some(CampoTemas::ElegirTema) => {
            // Cierra el panel y abre el selector de temas ya existente
            // (`Ctrl+K Ctrl+T`) en vez de reimplementar la misma lista y
            // el mismo preview en vivo acá adentro.
            estado.panel_admin.cerrar();
            estado.selector_tema.abrir(&estado.config.interfaz.tema);
        }
        Some(CampoTemas::DuplicarActivo) => {
            let mensaje = match tcode_config::duplicar_tema_para_editar(&estado.config.interfaz.tema) {
                Ok(ResultadoDuplicarTema::Creado(ruta)) => format!("Copia creada en {}", ruta.display()),
                Ok(ResultadoDuplicarTema::YaExistia(ruta)) => {
                    format!("Ya existía: {} (editalo directamente)", ruta.display())
                }
                Err(e) => format!("No se pudo duplicar: {e}"),
            };
            estado.panel_admin.establecer_mensaje(mensaje);
        }
        None => {}
    }
}

/// Construye las filas de la sección "Lenguajes / LSP" del panel de
/// administración (PLAN.md §5.3), una por cada lenguaje de `tcode_syntax::
/// Lenguaje::TODOS`: qué LSP tiene configurado (si alguno), si ese
/// binario está en el `PATH`, si está habilitado, y el estado en vivo de
/// la sesión activa si es justo el lenguaje del archivo abierto ahora.
fn filas_lenguajes_lsp(estado: &EstadoApp) -> Vec<FilaLenguajeLsp> {
    let lenguaje_activo = estado.lsp.lenguaje_activo();
    Lenguaje::TODOS
        .iter()
        .map(|&lenguaje| {
            let (comando, en_path) = match tcode_lsp::comando_para(lenguaje) {
                Some((comando, args)) => {
                    let texto = if args.is_empty() { comando.to_string() } else { format!("{comando} {}", args.join(" ")) };
                    (texto, ruta_en_path(comando))
                }
                None => (String::new(), false),
            };
            let estado_texto = if lenguaje_activo == Some(lenguaje) {
                estado.lsp.estado_texto().unwrap_or("Inactivo").to_string()
            } else {
                "Inactivo".to_string()
            };
            FilaLenguajeLsp {
                nombre: lenguaje.nombre_mostrado().to_string(),
                comando,
                en_path,
                habilitado: estado.config.lenguajes.lsp_habilitado(lenguaje.id()),
                estado: estado_texto,
            }
        })
        .collect()
}

/// Búsqueda simple del ejecutable `comando` en el `PATH` — suficiente
/// para el indicador "¿está instalado?" de PLAN.md §5.3 (`which`/`where`
/// completo, con `PATHEXT` en Windows, queda para cuando haga falta más
/// precisión). No confirma que el binario funcione, solo que existe un
/// archivo con ese nombre en algún directorio del `PATH`.
fn ruta_en_path(comando: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else { return false };
    std::env::split_paths(&path).any(|dir| dir.join(comando).is_file())
}

/// `Enter`/`←`/`→` sobre una fila de "Lenguajes / LSP": alterna si el
/// LSP de ese lenguaje está habilitado. A diferencia de "Editor", no
/// hace falta un `delta` — es un toggle simple, sin valores numéricos.
fn alternar_lsp_lenguaje_seleccionado(estado: &mut EstadoApp) {
    if estado.panel_admin.seccion_actual() != Seccion::Lenguajes {
        return;
    }
    if let Some(&lenguaje) = Lenguaje::TODOS.get(estado.panel_admin.campo()) {
        estado.config.lenguajes.alternar_lsp(lenguaje.id());
        let _ = tcode_config::guardar(&estado.config);
    }
}

/// El comando de `tcode_commands::comandos_disponibles()` que corresponde
/// a la fila seleccionada de la sección "Atajos" del panel de
/// administración — `None` si la fila 0 (la acción especial "restablecer
/// todos") está seleccionada, o si la sección actual no es "Atajos".
fn comando_seleccionado_en_atajos(panel: &tcode_config::EstadoPanelAdmin) -> Option<&'static str> {
    if panel.seccion_actual() != Seccion::Atajos || panel.campo() == 0 {
        return None;
    }
    tcode_commands::comandos_disponibles().get(panel.campo() - 1).map(|c| c.id)
}

/// `Enter` sobre una fila de "Atajos" (PLAN.md §5): la fila 0 es la
/// acción especial "restablecer TODOS los atajos por defecto" (borra el
/// `keymap.toml` de usuario); cualquier otra fila entra en modo captura
/// para reasignar ESE comando en particular.
fn iniciar_o_restablecer_todos_los_atajos(estado: &mut EstadoApp, resolvedor: &mut Resolvedor) {
    if estado.panel_admin.campo() == 0 {
        let _ = tcode_keymap::eliminar_override_usuario();
        estado.keymap = tcode_keymap::keymap_por_defecto();
        resolvedor.reemplazar_keymap(estado.keymap.clone());
        estado.panel_admin.establecer_mensaje("Todos los atajos vuelven a su valor por defecto".to_string());
    } else {
        estado.panel_admin.iniciar_captura();
    }
}

/// `Backspace` sobre un comando de "Atajos": lo restablece a lo que ese
/// comando tiene en el keymap por defecto (PLAN.md §5: "Botón
/// 'Restablecer valor por defecto' por atajo"), sin tocar el resto de
/// las personalizaciones. Sobre la fila 0 ("restablecer todos") no hace
/// nada — ya tiene su propio gesto con `Enter`.
fn restablecer_atajo_seleccionado(estado: &mut EstadoApp, resolvedor: &mut Resolvedor) {
    let Some(comando) = comando_seleccionado_en_atajos(&estado.panel_admin) else { return };
    estado.keymap = estado.keymap.restablecer_comando(comando);
    resolvedor.reemplazar_keymap(estado.keymap.clone());
    let _ = estado.keymap.guardar();
    estado.panel_admin.establecer_mensaje("Restablecido a su atajo por defecto".to_string());
}

/// La tecla que llega justo después de `Enter` sobre un comando en
/// "Atajos" (`EstadoPanelAdmin::capturando`, PLAN.md §5: "presionás la
/// nueva combinación y se guarda"). `Esc` cancela sin cambiar nada;
/// cualquier otra tecla —incluida una tecla sola sin modificadores,
/// como la plantea el plan— se convierte en la nueva combinación de ese
/// comando. Si esa combinación ya estaba en uso por otro comando
/// distinto, no se aplica — se avisa en vez de robarle el atajo en
/// silencio.
fn manejar_captura_atajo(estado: &mut EstadoApp, resolvedor: &mut Resolvedor, key: KeyEvent) {
    estado.panel_admin.terminar_captura();
    if key.code == KeyCode::Esc {
        return;
    }
    let Some(comando) = comando_seleccionado_en_atajos(&estado.panel_admin) else { return };
    let combinacion = tcode_keymap::desde_evento(key);
    match estado.keymap.rebindear(comando, combinacion) {
        Ok(nuevo) => {
            estado.keymap = nuevo;
            resolvedor.reemplazar_keymap(estado.keymap.clone());
            let _ = estado.keymap.guardar();
            let texto = tcode_keymap::formatear_combinacion(&combinacion);
            estado.panel_admin.establecer_mensaje(format!("Nuevo atajo: {texto}"));
        }
        Err(otro_comando) => {
            let descripcion: String = tcode_commands::comandos_disponibles()
                .iter()
                .find(|c| c.id == otro_comando)
                .map(|c| c.descripcion.to_string())
                .unwrap_or_else(|| otro_comando.clone());
            estado.panel_admin.establecer_mensaje(format!("Ya usado por: {descripcion} — no se cambió nada"));
        }
    }
}

/// `editor.tamano_tabulacion` / `editor.usar_espacios` de `config.toml`
/// (panel "Editor" de PLAN.md §5) ya afectan el comportamiento real del
/// editor, no solo el tema visual.
fn insertar_tabulacion(editor: &mut Editor, config: &Config) {
    if config.editor.usar_espacios {
        for _ in 0..config.editor.tamano_tabulacion {
            editor.insertar_char(' ');
        }
    } else {
        editor.insertar_char('\t');
    }
}

/// Mueve el cursor del editor activo a la coincidencia de búsqueda
/// seleccionada, si hay alguna — usado tanto al abrir la barra (`Ctrl+F`)
/// como al saltar con `F3`/`Shift+F3`, con la barra abierta o cerrada.
fn saltar_a_coincidencia_actual(layout: &mut PanelLayout, estado_busqueda: &EstadoBusqueda) {
    if let Some(coincidencia) = estado_busqueda.coincidencia_actual() {
        layout.editor_activo_mut().mover_cursor_a_byte(coincidencia.inicio);
    }
}

/// `Ctrl+H` con el foco en el campo de reemplazo, `Enter`: reemplaza solo
/// la coincidencia actual y deja seleccionada la siguiente (o vuelve a la
/// primera si no queda ninguna después), sin tocar el resto del archivo.
fn reemplazar_coincidencia_actual(editor: &mut Editor, estado_busqueda: &mut EstadoBusqueda) {
    let Some(coincidencia) = estado_busqueda.coincidencia_actual() else {
        return;
    };
    let reemplazo = estado_busqueda.reemplazo().to_string();
    editor.reemplazar_rango_bytes(coincidencia.inicio, coincidencia.fin, &reemplazo);
    let punto_edicion = coincidencia.inicio + reemplazo.len();
    estado_busqueda.recalcular_y_posicionar(&editor.buffer().a_texto(), punto_edicion);
}

/// `Ctrl+Alt+Enter`: reemplaza todas las coincidencias de una vez. Se
/// recorren de atrás hacia adelante para que reemplazar una no invalide
/// los offsets de bytes de las que todavía faltan (una más corta o más
/// larga que el patrón desplaza todo lo que viene después, pero nunca lo
/// que viene antes).
fn reemplazar_todas_las_coincidencias(editor: &mut Editor, estado_busqueda: &mut EstadoBusqueda) {
    let reemplazo = estado_busqueda.reemplazo().to_string();
    for coincidencia in estado_busqueda.coincidencias().iter().rev() {
        editor.reemplazar_rango_bytes(coincidencia.inicio, coincidencia.fin, &reemplazo);
    }
    estado_busqueda.recalcular(&editor.buffer().a_texto());
}
