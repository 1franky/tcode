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
mod vim;

use std::collections::VecDeque;
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, EnableBracketedPaste, EnableFocusChange, Event, EventStream, KeyCode,
    KeyEvent, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement, EnterAlternateScreen, LeaveAlternateScreen,
};
use lsp_types::FormattingOptions;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::time::MissedTickBehavior;
use tokio_stream::StreamExt;

use tcode_commands::EstadoPaleta;
use tcode_config::{
    CampoEditor, CampoTemas, ComandoLsp, Config, ConfigEditor, ConfigProyecto, EstadoEditorTema, EstadoPanelAdmin,
    EstadoSelectorTema, FocoPanelAdmin, GuardadoAutomatico, ModoEdicion, ResultadoDuplicarTema, Seccion,
};
use tcode_core::{
    analizar_csv, delimitador_por_extension, serializar_fila_csv, CampoBusqueda, Editor, EstadoBusqueda, EstadoGuardarComo,
    EstadoVim, Modo, Pliegue,
};
use tcode_fs::{BuscadorArchivos, EstadoConfirmarBorrado, EstadoPromptExplorador, Explorador, ModoPromptExplorador};
use tcode_keymap::{Keymap, Resolucion, Resolvedor};
use tcode_lsp::EstadoLogsLsp;
use tcode_syntax::{Lenguaje, Resaltador};
use tcode_ui::{ancho_columna_csv, DireccionSplit, FilaLenguajeLsp, Layout as PanelLayout, ModoCsv, Paleta, PanelEditor};

type Backend = CrosstermBackend<Stdout>;

/// Versión mostrada por `tcode --version`/`-v` — el tag de la release
/// (`v0.4.1`, ej.) si el binario se compiló en el workflow de release
/// (que fija `TCODE_VERSION` al `github.ref_name` del tag disparador,
/// ver `.github/workflows/release.yml`), o un valor obviamente "no es
/// una release" en un build local de desarrollo (`cargo build` no fija
/// esa variable). Sirve para que alguien que instaló tcode desde
/// `install/linux.sh`/`install/windows.ps1` pueda confirmar qué versión
/// quedó instalada sin tener que abrir el editor.
const VERSION: &str = match option_env!("TCODE_VERSION") {
    Some(v) => v,
    None => concat!("v", env!("CARGO_PKG_VERSION"), "-dev"),
};

#[tokio::main]
async fn main() -> Result<()> {
    let ruta_arg = std::env::args().nth(1);

    if matches!(ruta_arg.as_deref(), Some("--version") | Some("-v")) {
        println!("tcode {VERSION}");
        return Ok(());
    }

    let editor = match &ruta_arg {
        Some(ruta) => Editor::abrir(ruta)?,
        None => Editor::nuevo(),
    };
    let mut layout = PanelLayout::nuevo(editor, ruta_arg.clone().unwrap_or_else(|| "[Sin nombre]".to_string()));

    // La config y el keymap nunca hacen fallar el arranque: si el archivo
    // del usuario está corrupto, se sigue con los valores por defecto en
    // vez de negarse a abrir el editor. Lo mismo con la config de
    // proyecto (`.tcode/config.toml`, BACKLOG.md P2 #8): si es inválida
    // se sigue solo con la global, y el motivo se ve en la cabecera del
    // panel de administración (`Ctrl+,`).
    let capas_config = CapasConfig::cargar(tcode_config::directorio_inicio_proyecto(ruta_arg.as_deref()));
    let config = capas_config.efectiva();
    let keymap = tcode_keymap::cargar().unwrap_or_else(|_| tcode_keymap::keymap_por_defecto());
    let explorador = crear_explorador(ruta_arg.as_deref());

    // Modo VIM (M5, `config.editor.modo_vim`, apagado por defecto): el
    // `Editor` arranca siempre en `Modo::Insertar` sin saber nada de esta
    // config — acá es donde `app` decide si corresponde pasarlo a
    // `Normal` antes de la primera tecla. Nota: esto solo cubre el
    // arranque y abrir un archivo (`abrir_ruta_desde_explorador`, el
    // buscador de archivos); un panel nuevo por `Ctrl+\` siempre arranca
    // en Insertar (limitación conocida, ver PRUEBAS.md) porque
    // `tcode_ui::Layout::dividir` no conoce la config.
    if config.editor.modo_vim {
        layout.editor_activo_mut().entrar_modo_normal();
    }

    let (mut terminal, protocolo_kitty) = iniciar_terminal()?;
    let resultado = ejecutar(&mut terminal, &mut layout, capas_config, keymap, explorador, ruta_arg.as_deref()).await;
    finalizar_terminal(&mut terminal, protocolo_kitty)?;

    resultado
}

/// Las dos capas de configuración (BACKLOG.md P2 #8, PLAN.md §12
/// decisión abierta #5): la global del usuario — la ÚNICA que edita y
/// guarda el panel de administración — y la de proyecto
/// (`.tcode/config.toml`, buscada subiendo desde `dir_inicio`), que solo
/// se lee. `EstadoApp::config` es siempre `efectiva()`: todo lo que
/// consulta la config (vista de código, statusbar, LSP...) la ve ya
/// mezclada, sin saber que hay dos capas.
struct CapasConfig {
    global: Config,
    proyecto: Option<ConfigProyecto>,
    /// Directorio del archivo abierto al arrancar (o el cwd) — fijo toda
    /// la sesión: `config.recargar` vuelve a buscar desde acá, no desde
    /// el archivo activo en ese momento (más predecible: abrir otro
    /// archivo no cambia la config en silencio).
    dir_inicio: PathBuf,
}

impl CapasConfig {
    fn cargar(dir_inicio: PathBuf) -> Self {
        let global = tcode_config::cargar().unwrap_or_default();
        let proyecto = tcode_config::cargar_config_proyecto(&dir_inicio);
        Self { global, proyecto, dir_inicio }
    }

    fn efectiva(&self) -> Config {
        match &self.proyecto {
            Some(proyecto) => proyecto.aplicar_sobre(&self.global),
            None => self.global.clone(),
        }
    }
}

/// Guarda la config GLOBAL (nunca la efectiva: los valores del proyecto
/// no deben terminar en la config de todos los demás proyectos del
/// usuario) y recalcula `estado.config` con el cambio. Todo lo que edita
/// config desde la UI tiene que mutar `estado.capas_config.global` y
/// después llamar a esto — mutar `estado.config` directamente se
/// perdería en la próxima recomposición.
fn guardar_config_global(estado: &mut EstadoApp) {
    let _ = tcode_config::guardar(&estado.capas_config.global);
    estado.config = estado.capas_config.efectiva();
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
/// interpreta cada byte de los caracteres especiales de tcode como un
/// glifo separado del codepage regional del sistema, descuadrando el
/// ancho de columna que `ratatui` calculó. Al redibujar (p. ej. al mover
/// el cursor) eso se ve como texto "faltante" o con artefactos —
/// reportado y confirmado en Windows (CMD y PowerShell clásico) el
/// 2026-09-11: el archivo en disco quedaba intacto, solo la pantalla se
/// veía mal. Forzar el codepage de salida/entrada a UTF-8 (65001) antes
/// de dibujar nada lo soluciona.
///
/// Nota aparte, para **Windows Terminal** (que sí usa UTF-8 sin este
/// fix): un bug de desalineación distinto, reportado después y sin
/// resolver por 3 intentos previos, resultó tener la misma raíz —
/// caracteres decorativos (`▾`/`▸`/`●`/`│`/`─` y similares) con ancho
/// "ambiguo" en Unicode que esa terminal en particular podía renderizar
/// distinto a como `ratatui` lo calculaba internamente, desalineando su
/// buffer de diffing de forma permanente. Se reemplazaron por ASCII en
/// los widgets donde se repiten estructuralmente (panel del explorador,
/// separador de la statusbar, vista Markdown) — ver el historial de
/// commits de esa pieza para el detalle completo.
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
    // Bracketed paste: lo pegado desde el portapapeles de la terminal
    // llega como UN evento `Event::Paste` con todo el texto, en vez de
    // una tecla por carácter (ver `pegar_texto`).
    execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)?;

    // Eventos de foco de la terminal (`Event::FocusLost`), para el
    // guardado automático "al perder foco" (BACKLOG.md P2 #4) —
    // best-effort: una terminal que no los soporta ignora la secuencia
    // y ese modo sigue andando igual con los cambios de panel/archivo.
    // En tmux hace falta `set -g focus-events on` para que los reenvíe.
    let _ = execute!(stdout, EnableFocusChange);

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
    let _ = execute!(terminal.backend_mut(), DisableFocusChange);
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), DisableBracketedPaste, LeaveAlternateScreen)?;
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
    /// Config EFECTIVA (global + proyecto) — de solo lectura en la
    /// práctica: para cambiar algo, ver `guardar_config_global`.
    config: Config,
    capas_config: CapasConfig,
    paleta: Paleta,
    explorador: Explorador,
    foco: Foco,
    confirmar_salida: bool,
    paleta_comandos: EstadoPaleta,
    buscador_archivos: BuscadorArchivos,
    estado_busqueda: EstadoBusqueda,
    guardar_como: EstadoGuardarComo,
    selector_tema: EstadoSelectorTema,
    panel_admin: EstadoPanelAdmin,
    editor_tema: EstadoEditorTema,
    /// Keymap activo — fuente de verdad para la sección "Atajos" del
    /// panel de administración (`Ctrl+,`, PLAN.md §5). El `Resolvedor`
    /// que de verdad resuelve teclas tiene su PROPIA copia (`resolvedor`
    /// es una variable aparte, no un campo de este struct): cada vez que
    /// este campo cambia hay que llamar `resolvedor.reemplazar_keymap`
    /// con un clon para que el cambio surta efecto en el editor real, no
    /// solo en lo que se ve en el panel.
    keymap: Keymap,
    lsp: lsp::EstadoLsp,
    /// Registro sin nombre + comando de dos teclas pendiente del modo VIM
    /// (`config.editor.modo_vim`, M5) — uno solo para toda la app, no por
    /// panel (ver `tcode_core::EstadoVim`). Sin efecto mientras ningún
    /// `Editor` llegue a `Modo::Normal`.
    vim: EstadoVim,
    /// Visor de logs de stderr de la sesión LSP activa (`Ctrl+K R`,
    /// PLAN.md §5.3).
    logs_lsp: EstadoLogsLsp,
    /// Prompt de texto del explorador (`Ctrl+K N`/`Ctrl+K C`/`Ctrl+K M`
    /// — nuevo archivo/carpeta/renombrar, BACKLOG.md P0 "explorador de
    /// solo lectura").
    prompt_explorador: EstadoPromptExplorador,
    /// Confirmación de borrado del explorador (`Delete` con el
    /// explorador enfocado) — separada de `prompt_explorador` porque no
    /// tiene ningún campo de texto, solo `y`/cualquier otra tecla.
    confirmar_borrado: EstadoConfirmarBorrado,
    /// Cuándo corrió por última vez el guardado automático "cada N
    /// segundos" (BACKLOG.md P2 #4) — se reinicia también al cambiar ese
    /// modo o el número de segundos desde el panel de administración, para
    /// que el primer guardado llegue N segundos DESPUÉS de prenderlo y no
    /// de golpe en el próximo tick.
    ultimo_autoguardado: Instant,
    /// `archivo.guardar` pedido sobre un archivo con ruta, pendiente de
    /// ejecutarse al principio de la próxima vuelta del bucle de
    /// `ejecutar` (`guardar_archivo_activo`). No se guarda ahí mismo
    /// porque `procesar_comando` es síncrona y guardar puede tener que
    /// esperar la respuesta del LSP a `textDocument/formatting`
    /// ("formatear al guardar", BACKLOG.md P2 #5); se ejecuta antes de
    /// procesar cualquier otra tecla, así que el orden de los eventos no
    /// cambia (`Ctrl+S` seguido de `Ctrl+Q` en la misma ráfaga guarda
    /// primero).
    guardado_pendiente: bool,
    /// Resaltador de sintaxis (árbol de tree-sitter incremental por
    /// documento). Vive acá, no suelto en `ejecutar`, porque además de
    /// dibujar lo usan los comandos de plegado (BACKLOG.md P2 #7) para
    /// sacar los rangos plegables del mismo árbol, sin volver a parsear.
    resaltador: Resaltador,
}

fn sin_modificadores(key: KeyEvent) -> bool {
    !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT)
}

async fn ejecutar(
    terminal: &mut Terminal<Backend>,
    layout: &mut PanelLayout,
    capas_config: CapasConfig,
    keymap: Keymap,
    explorador: Explorador,
    ruta_arg: Option<&str>,
) -> Result<()> {
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
    panel_admin.fijar_num_filas_atajos(FILAS_ESPECIALES_ATAJOS + tcode_commands::comandos_disponibles().len());
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

    let config = capas_config.efectiva();
    let mut estado = EstadoApp {
        paleta: cargar_paleta(&config.interfaz.tema),
        config,
        capas_config,
        explorador,
        foco: Foco::Editor,
        // Ctrl+Q con cambios sin guardar pide una segunda confirmación en
        // vez de perder trabajo en silencio (nano-style). Cualquier otra
        // resolución la cancela.
        confirmar_salida: false,
        paleta_comandos: EstadoPaleta::nueva(),
        buscador_archivos: BuscadorArchivos::nuevo(tcode_fs::raiz_por_defecto(ruta_arg)),
        estado_busqueda: EstadoBusqueda::nueva(),
        guardar_como: EstadoGuardarComo::nueva(),
        selector_tema: EstadoSelectorTema::nueva(),
        panel_admin,
        editor_tema: EstadoEditorTema::nueva(),
        keymap,
        lsp: lsp::EstadoLsp::nuevo(),
        vim: EstadoVim::nuevo(),
        logs_lsp: EstadoLogsLsp::nuevo(),
        prompt_explorador: EstadoPromptExplorador::nuevo(),
        confirmar_borrado: EstadoConfirmarBorrado::nuevo(),
        ultimo_autoguardado: Instant::now(),
        guardado_pendiente: false,
        resaltador: Resaltador::nuevo(),
    };

    // Ver `forzar_redibujado_completo`: en Windows, si la "forma" de la
    // pantalla cambió (otro archivo activo, otro número de paneles, el
    // explorador se mostró/ocultó), se limpia antes del próximo draw.
    let mut necesita_redibujado = false;

    // Teclas generadas por la app en vez de por la terminal: lo pegado
    // mientras hay un prompt de una línea abierto se reparte en teclas
    // sueltas (ver `pegar_texto`). Se procesan antes que cualquier
    // evento nuevo de la terminal.
    let mut teclas_sinteticas: VecDeque<KeyEvent> = VecDeque::new();
    let mut ultimo_dibujo = Instant::now();

    // Tick periódico (BACKLOG.md P2 #4 y P1 #2): el bucle era puramente
    // reactivo (teclado o LSP), pero el guardado automático "cada N
    // segundos" y el visor de logs del LSP en vivo necesitan despertarse
    // solos. Solo se escucha mientras alguno de los dos lo necesita
    // (`necesita_tick`) — en reposo, con la config por defecto, el bucle
    // sigue sin despertarse para nada. `Skip`: si el bucle estuvo ocupado
    // (o el tick apagado un rato) no se disparan de golpe los ticks
    // atrasados.
    let mut tick = tokio::time::interval(INTERVALO_TICK);
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    // Un tick que no cambió nada (sin líneas nuevas de log, sin nada que
    // guardar) no redibuja: sin esto el visor de logs abierto repintaba
    // la pantalla entera 4 veces por segundo aunque el LSP estuviera
    // callado.
    let mut omitir_dibujo = false;

    // "Al perder foco" (BACKLOG.md P2 #4): qué panel/archivo/zona tenía
    // el foco en la vuelta anterior del bucle — si cambió (`Ctrl+1/2/3`,
    // cerrar/dividir un panel, abrir otro archivo, pasar al explorador),
    // se guarda. Comparar acá arriba, una vez por vuelta, cubre todos los
    // caminos de más abajo sin tocar cada `continue`.
    let mut ultimo_foco = firma_foco(layout, &estado);

    loop {
        if std::mem::take(&mut estado.guardado_pendiente) {
            let _ = guardar_archivo_activo(layout, &mut estado).await;
        }
        let foco_actual = firma_foco(layout, &estado);
        if foco_actual != ultimo_foco {
            if estado.config.editor.guardado_automatico == GuardadoAutomatico::AlPerderFoco {
                autoguardar(layout);
            }
            ultimo_foco = foco_actual;
        }

        // Si ya hay más teclas esperando (una flecha mantenida apretada,
        // o lo pegado en una terminal sin bracketed paste), se procesan
        // TODAS antes de volver a dibujar. Dibujar entre cada una hacía
        // que la cola creciera más rápido de lo que se vaciaba: la
        // pantalla se congelaba y después el cursor "se ponía al día" de
        // golpe, pasándose de donde se quería ir. Igual se dibuja cada
        // tanto durante una ráfaga larga para que no parezca colgado.
        let hay_mas_eventos = !teclas_sinteticas.is_empty() || crossterm::event::poll(Duration::ZERO).unwrap_or(false);
        let dibujar = !std::mem::take(&mut omitir_dibujo);
        if dibujar && (!hay_mas_eventos || ultimo_dibujo.elapsed() >= INTERVALO_MAXIMO_SIN_DIBUJAR) {
            // Una vez por frame, no por tecla: avisa al LSP del archivo
            // activo (relanzándolo si cambió de lenguaje) y le manda lo
            // que cambió, si algo cambió. Por tecla significaba copiar y
            // serializar el archivo entero en cada carácter tipeado.
            sincronizar_lsp(layout, &mut estado.lsp, &estado.config).await;

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
                    &mut estado.resaltador,
                    &estado.explorador,
                    &estado.paleta_comandos,
                    &estado.buscador_archivos,
                    &estado.estado_busqueda,
                    &estado.guardar_como,
                    &estado.selector_tema,
                    &estado.panel_admin,
                    &estado.config,
                    &estado.capas_config.global,
                    estado.capas_config.proyecto.as_ref(),
                    &estado.keymap,
                    &filas_lenguajes,
                    &estado.editor_tema,
                    &estado.logs_lsp,
                    &estado.prompt_explorador,
                    &estado.confirmar_borrado,
                )
            })?;
            ultimo_dibujo = Instant::now();
        }

        let firma_antes = firma_estructural(layout, &estado.explorador);

        let evento = match teclas_sinteticas.pop_front() {
            Some(key) => Event::Key(key),
            None => tokio::select! {
                evento = eventos.next() => {
                    match evento {
                        Some(Ok(evento)) => evento,
                        _ => continue,
                    }
                }
                mensaje = estado.lsp.siguiente_mensaje() => {
                    if let Some(mensaje) = mensaje {
                        estado.lsp.procesar_mensaje(mensaje, layout).await;
                    }
                    continue;
                }
                // Indicadores de git (BACKLOG.md P2 #6): mientras algún
                // panel espera que `git` devuelva su base de `HEAD` o que
                // termine el cálculo del diff (los dos en hilos aparte,
                // ver `tcode_fs::DiffGit`), se vuelve a dibujar cada tanto
                // aunque no llegue ninguna tecla — así las marcas aparecen
                // solas al abrir un archivo o al dejar de tipear. Sin nada
                // pendiente esta rama ni se arma: cero costo en reposo.
                _ = tokio::time::sleep(INTERVALO_SONDEO_GIT), if layout.cargas_git_pendientes() => continue,
                _ = tick.tick(), if necesita_tick(&estado) => {
                    omitir_dibujo = !procesar_tick(layout, &mut estado);
                    continue;
                }
            },
        };

        let key = match evento {
            Event::Key(key) if key.kind == KeyEventKind::Press => key,
            Event::Paste(texto) => {
                pegar_texto(&texto, layout, &mut estado, &mut teclas_sinteticas);
                necesita_redibujado |= firma_estructural(layout, &estado.explorador) != firma_antes;
                continue;
            }
            // La terminal dejó de tener el foco (otra ventana/pestaña/panel
            // de tmux): mismo significado que cambiar de panel adentro de
            // tcode para "al perder foco" (BACKLOG.md P2 #4). Solo llega si
            // la terminal soporta eventos de foco (ver `iniciar_terminal`).
            Event::FocusLost => {
                if estado.config.editor.guardado_automatico == GuardadoAutomatico::AlPerderFoco {
                    autoguardar(layout);
                }
                continue;
            }
            _ => continue,
        };

        // El aviso transitorio de la barra de estado (p. ej. "Formateado
        // al guardar") dura hasta la próxima tecla.
        layout.panel_activo_mut().mensaje_estado = None;

        // El editor visual de tema (`Ctrl+K Ctrl+P`, PLAN.md §7) es otra
        // vista a pantalla completa que captura el teclado por completo:
        // navegar la lista de colores, o — mientras se está editando uno—
        // escribir el código hex nuevo.
        if estado.editor_tema.activo() {
            estado.confirmar_salida = false;
            match categoria_modo_editor_tema(&estado.editor_tema) {
                CategoriaModoEditorTema::Ninguno => match key.code {
                    KeyCode::Esc => estado.editor_tema.cerrar(),
                    KeyCode::Up => estado.editor_tema.mover_arriba(),
                    KeyCode::Down => estado.editor_tema.mover_abajo(),
                    KeyCode::Enter => estado.editor_tema.iniciar_edicion_hex(),
                    // Sin `Ctrl`: son mnemónicos de una sola tecla (como
                    // en un menú fijo), no hay texto que se pueda estar
                    // escribiendo en esta vista mientras la lista tiene
                    // el foco.
                    KeyCode::Char('p') if sin_modificadores(key) => estado.editor_tema.iniciar_paleta(),
                    KeyCode::Char('h') if sin_modificadores(key) => estado.editor_tema.iniciar_hsl(),
                    _ => {}
                },
                CategoriaModoEditorTema::Hex => match key.code {
                    KeyCode::Esc => estado.editor_tema.cancelar_edicion(),
                    KeyCode::Backspace => estado.editor_tema.borrar_hex(),
                    KeyCode::Enter => {
                        estado.editor_tema.confirmar_hex();
                        refrescar_preview_editor_tema(&mut estado);
                    }
                    KeyCode::Char(c) if sin_modificadores(key) && c.is_ascii_hexdigit() => {
                        estado.editor_tema.escribir_hex(c);
                    }
                    _ => {}
                },
                CategoriaModoEditorTema::Paleta => match key.code {
                    KeyCode::Esc => estado.editor_tema.cancelar_edicion(),
                    KeyCode::Up => estado.editor_tema.mover_paleta_arriba(),
                    KeyCode::Down => estado.editor_tema.mover_paleta_abajo(),
                    KeyCode::Enter => {
                        estado.editor_tema.confirmar_paleta();
                        refrescar_preview_editor_tema(&mut estado);
                    }
                    _ => {}
                },
                CategoriaModoEditorTema::Hsl => match key.code {
                    KeyCode::Esc => {
                        estado.editor_tema.cancelar_edicion();
                        refrescar_preview_editor_tema(&mut estado);
                    }
                    KeyCode::Left => estado.editor_tema.mover_foco_hsl(false),
                    KeyCode::Right => estado.editor_tema.mover_foco_hsl(true),
                    KeyCode::Up => {
                        estado.editor_tema.ajustar_hsl(1);
                        refrescar_preview_editor_tema(&mut estado);
                    }
                    KeyCode::Down => {
                        estado.editor_tema.ajustar_hsl(-1);
                        refrescar_preview_editor_tema(&mut estado);
                    }
                    KeyCode::Enter => {
                        estado.editor_tema.confirmar_hsl();
                        refrescar_preview_editor_tema(&mut estado);
                    }
                    _ => {}
                },
            }
            necesita_redibujado |= firma_estructural(layout, &estado.explorador) != firma_antes;
            continue;
        }

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
            } else if estado.panel_admin.editando_comando_lsp().is_some() {
                // Igual que la captura de atajo de arriba: mientras se
                // edita el comando LSP personalizado de un lenguaje
                // (`c` en "Lenguajes / LSP"), cualquier tecla se consume
                // acá — `Enter`/`Esc` cierran el modo, el resto edita el
                // buffer de texto.
                match key.code {
                    KeyCode::Esc => estado.panel_admin.cancelar_edicion_comando_lsp(),
                    KeyCode::Enter => confirmar_edicion_comando_lsp(&mut estado),
                    KeyCode::Backspace => estado.panel_admin.borrar_comando_lsp(),
                    KeyCode::Char(c) if sin_modificadores(key) => estado.panel_admin.escribir_comando_lsp(c),
                    _ => {}
                }
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
                            let _ = tcode_config::guardar(&estado.capas_config.global);
                        }
                        KeyCode::Enter if estado.panel_admin.seccion_actual() == Seccion::Temas => {
                            ejecutar_accion_temas_admin(&mut estado);
                        }
                        KeyCode::Enter if estado.panel_admin.seccion_actual() == Seccion::Atajos => {
                            ejecutar_fila_atajos(&mut estado, &mut resolvedor);
                        }
                        KeyCode::Backspace if estado.panel_admin.seccion_actual() == Seccion::Atajos => {
                            restablecer_atajo_seleccionado(&mut estado, &mut resolvedor);
                        }
                        KeyCode::Enter | KeyCode::Left | KeyCode::Right
                            if estado.panel_admin.seccion_actual() == Seccion::Lenguajes =>
                        {
                            alternar_lsp_lenguaje_seleccionado(&mut estado);
                        }
                        KeyCode::Char('c')
                            if sin_modificadores(key) && estado.panel_admin.seccion_actual() == Seccion::Lenguajes =>
                        {
                            iniciar_edicion_comando_lsp_seleccionado(&mut estado);
                        }
                        KeyCode::Backspace if estado.panel_admin.seccion_actual() == Seccion::Lenguajes => {
                            quitar_comando_lsp_seleccionado(&mut estado);
                        }
                        KeyCode::Char('f')
                            if sin_modificadores(key) && estado.panel_admin.seccion_actual() == Seccion::Lenguajes =>
                        {
                            alternar_formatear_al_guardar_seleccionado(&mut estado);
                        }
                        KeyCode::Enter | KeyCode::Left | KeyCode::Right
                            if estado.panel_admin.seccion_actual() == Seccion::Interfaz =>
                        {
                            // Todas las filas de "Interfaz" son
                            // booleanas (PLAN.md §5.5) — a diferencia de
                            // "Editor" no hace falta un delta, cualquiera
                            // de las tres teclas simplemente alterna.
                            if let Some(campo) = estado.panel_admin.campo_interfaz_actual() {
                                campo.aplicar(&mut estado.capas_config.global);
                                guardar_config_global(&mut estado);
                            }
                        }
                        KeyCode::Enter | KeyCode::Left | KeyCode::Right => {
                            if let Some(campo) = estado.panel_admin.campo_editor_actual() {
                                let delta = if key.code == KeyCode::Left { -1 } else { 1 };
                                campo.aplicar(&mut estado.capas_config.global, delta);
                                guardar_config_global(&mut estado);
                                // Si se acaba de prender el modo VIM desde
                                // acá, el panel activo pasa a Normal de
                                // inmediato — sin esto quedaría en
                                // Insertar hasta reabrir el archivo (ver
                                // `abrir_ruta_desde_explorador`). Apagarlo
                                // no hace falta reconciliarlo: seguir en
                                // Normal con el modo apagado es inofensivo
                                // (`i`/`Esc` siguen sacando de ahí).
                                if campo == CampoEditor::ModoVim && estado.config.editor.modo_vim {
                                    layout.editor_activo_mut().entrar_modo_normal();
                                }
                                // Ver doc de `EstadoApp::ultimo_autoguardado`.
                                if matches!(
                                    campo,
                                    CampoEditor::GuardadoAutomatico | CampoEditor::SegundosGuardadoAutomatico
                                ) {
                                    estado.ultimo_autoguardado = Instant::now();
                                }
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
                        abrir_ruta_desde_explorador(layout, &mut estado.foco, ruta, &estado.config.editor);
                    }
                }
                KeyCode::Char(c) if sin_modificadores(key) => estado.buscador_archivos.escribir(c),
                _ => {}
            }
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
                        confirmar_tema_seleccionado(&mut estado, &id);
                    }
                }
                _ => {}
            }
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
            necesita_redibujado |= firma_estructural(layout, &estado.explorador) != firma_antes;
            continue;
        }

        // Prompt "Guardar como" (`Ctrl+Shift+S`/`Ctrl+K S`, o `Ctrl+S`
        // sobre un buffer sin ruta): campo de texto de una sola línea con
        // la ruta destino, sin selector de archivos (misma limitación que
        // el resto de la UI). `Enter` intenta guardar; si falla (permiso
        // denegado, directorio inexistente...) el prompt queda abierto
        // con el error en vez de cerrarse como si nada.
        if estado.guardar_como.activa() {
            estado.confirmar_salida = false;
            match key.code {
                KeyCode::Esc => estado.guardar_como.cerrar(),
                KeyCode::Backspace => estado.guardar_como.borrar(),
                KeyCode::Enter => guardar_como_confirmar(layout, &mut estado).await,
                KeyCode::Char(c) if sin_modificadores(key) => estado.guardar_como.escribir(c),
                _ => {}
            }
            necesita_redibujado |= firma_estructural(layout, &estado.explorador) != firma_antes;
            continue;
        }

        // Visor de logs del LSP activo (`Ctrl+K R`): se actualiza solo
        // con el tick del bucle (`procesar_tick`, BACKLOG.md P1 #2); acá
        // solo el filtro de texto, el scroll y `Esc` para cerrar.
        if estado.logs_lsp.activo() {
            estado.confirmar_salida = false;
            match key.code {
                KeyCode::Esc => estado.logs_lsp.cerrar(),
                KeyCode::Up => estado.logs_lsp.mover_arriba(),
                KeyCode::Down => estado.logs_lsp.mover_abajo(),
                KeyCode::PageUp => estado.logs_lsp.pagina_arriba(),
                KeyCode::PageDown => estado.logs_lsp.pagina_abajo(),
                KeyCode::Backspace => estado.logs_lsp.borrar(),
                KeyCode::Char(c) if sin_modificadores(key) => estado.logs_lsp.escribir(c),
                _ => {}
            }
            necesita_redibujado |= firma_estructural(layout, &estado.explorador) != firma_antes;
            continue;
        }

        // Prompt de texto del explorador (`Ctrl+K N`/`Ctrl+K C`/`Ctrl+K
        // M` — nuevo archivo/carpeta/renombrar, BACKLOG.md P0
        // "explorador de solo lectura"): mismo patrón que "Guardar
        // como" — `Enter` confirma e intenta la operación de verdad
        // (`crear_archivo`/`crear_carpeta`/`renombrar_seleccion`), si
        // falla el prompt queda abierto con el error en vez de cerrarse
        // como si nada.
        if estado.prompt_explorador.activo() {
            estado.confirmar_salida = false;
            match key.code {
                KeyCode::Esc => estado.prompt_explorador.cerrar(),
                KeyCode::Backspace => estado.prompt_explorador.borrar(),
                KeyCode::Enter => confirmar_prompt_explorador(&mut estado.explorador, &mut estado.prompt_explorador),
                KeyCode::Char(c) if sin_modificadores(key) => estado.prompt_explorador.escribir(c),
                _ => {}
            }
            necesita_redibujado |= firma_estructural(layout, &estado.explorador) != firma_antes;
            continue;
        }

        // Confirmación de borrado del explorador (`Delete` con el
        // explorador enfocado): acción destructiva e irreversible, así
        // que no hay "Enter confirma" ni tecla por defecto — solo `y`
        // borra de verdad, cualquier otra tecla (incluido `Esc`) cancela
        // sin tocar el disco.
        if estado.confirmar_borrado.activo() {
            estado.confirmar_salida = false;
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    // `confirmar()` ya cierra el prompt; la ruta que
                    // devuelve no hace falta acá — `borrar_seleccion` la
                    // vuelve a sacar de la selección actual del árbol,
                    // que no pudo haber cambiado mientras este prompt
                    // capturaba el teclado por completo.
                    estado.confirmar_borrado.confirmar();
                    let _ = estado.explorador.borrar_seleccion();
                }
                _ => estado.confirmar_borrado.cerrar(),
            }
            necesita_redibujado |= firma_estructural(layout, &estado.explorador) != firma_antes;
            continue;
        }

        // "Salto rápido" del explorador (`Ctrl+K J`): mientras está
        // activo, cualquier tecla asignada como etiqueta
        // (`Explorador::etiqueta_para_fila`) abre ese archivo o expande
        // esa carpeta directamente, sin pasar por la navegación normal
        // con flechas — captura el teclado por completo igual que los
        // bloques anteriores. Una tecla sin etiqueta asignada no hace
        // nada (se sigue esperando una válida); solo `Esc` cancela sin
        // saltar.
        if estado.explorador.modo_salto() {
            estado.confirmar_salida = false;
            match key.code {
                KeyCode::Esc => estado.explorador.salir_modo_salto(),
                KeyCode::Char(c) if sin_modificadores(key) => {
                    if let Ok(Some(ruta)) = estado.explorador.saltar_a_etiqueta(c) {
                        abrir_ruta_desde_explorador(layout, &mut estado.foco, ruta, &estado.config.editor);
                    }
                }
                _ => {}
            }
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
            necesita_redibujado |= firma_estructural(layout, &estado.explorador) != firma_antes;
            continue;
        }

        // Prompt de filtro de la vista CSV/TSV (`Ctrl+K /`, BACKLOG.md P2
        // #9): mismo patrón modal que la edición de celda de arriba.
        if layout.panel_activo().estado_csv.prompt_filtro().is_some() {
            estado.confirmar_salida = false;
            match key.code {
                KeyCode::Esc => {
                    layout.panel_activo_mut().estado_csv.cerrar_prompt_filtro();
                }
                KeyCode::Backspace => layout.panel_activo_mut().estado_csv.borrar_en_prompt_filtro(),
                KeyCode::Enter => confirmar_filtro_csv(layout),
                KeyCode::Char(c) if sin_modificadores(key) => {
                    layout.panel_activo_mut().estado_csv.escribir_en_prompt_filtro(c)
                }
                _ => {}
            }
            necesita_redibujado |= firma_estructural(layout, &estado.explorador) != firma_antes;
            continue;
        }

        // Modo VIM (`config.editor.modo_vim`, M5, apagado por defecto —
        // ninguno de estos dos bloques hace nada si `editor.modo()` nunca
        // llegó a `Normal`, y a eso solo se llega si la config lo prende,
        // ver `abrir_ruta_desde_explorador`). `Esc` en Insertar pasa a
        // Normal en vez de su significado de siempre
        // ("explorador.enfocar_editor", colapsar multi-cursor) — se
        // colapsa el multi-cursor de todos modos, tiene el mismo espíritu
        // de "volver a un solo cursor" al dejar de escribir.
        if estado.config.editor.modo_vim
            && estado.foco == Foco::Editor
            && key.code == KeyCode::Esc
            && layout.editor_activo().modo() == Modo::Insertar
        {
            let editor = layout.editor_activo_mut();
            editor.colapsar_cursores();
            editor.entrar_modo_normal();
            estado.confirmar_salida = false;
            necesita_redibujado |= firma_estructural(layout, &estado.explorador) != firma_antes;
            continue;
        }

        // En modo Normal, un carácter sin modificadores es un comando VIM
        // (movimiento, operador, cambio de modo), no texto a insertar —
        // el resto de atajos de tcode (flechas, `Ctrl+S`, `Ctrl+B`,...)
        // siguen andando igual, por debajo de este bloque (no se captura
        // el teclado por completo como en los bloques anteriores).
        if layout.editor_activo().modo() == Modo::Normal {
            let manejada = match key.code {
                KeyCode::Esc => {
                    vim::cancelar_pendiente(&mut estado.vim);
                    true
                }
                KeyCode::Char(c) if sin_modificadores(key) => {
                    vim::ejecutar_tecla_normal(c, layout, &mut estado.vim);
                    true
                }
                _ => false,
            };
            if manejada {
                estado.confirmar_salida = false;
                necesita_redibujado |= firma_estructural(layout, &estado.explorador) != firma_antes;
                continue;
            }
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

        necesita_redibujado |= firma_estructural(layout, &estado.explorador) != firma_antes;
    }

    estado.lsp.cerrar().await;
    Ok(())
}

/// Cada cuánto se dibuja igual durante una ráfaga larga de eventos (ver
/// el bucle de `ejecutar`): lo suficiente para que se vea avanzar, sin
/// volver a dibujar por cada tecla.
const INTERVALO_MAXIMO_SIN_DIBUJAR: Duration = Duration::from_millis(50);

/// Cada cuánto se vuelve a dibujar (para sondear el resultado) mientras
/// hay una lectura de la base de git en curso — ver la rama
/// correspondiente del `select!` en `ejecutar`. `git cat-file` suele
/// tardar pocos ms, así que casi siempre alcanza con una vuelta.
const INTERVALO_SONDEO_GIT: Duration = Duration::from_millis(30);

/// Período del tick del bucle principal (ver `necesita_tick`): lo que
/// tarda como mucho una línea nueva de stderr del LSP en aparecer en el
/// visor abierto. La resolución del guardado "cada N segundos" también
/// es esta, más que suficiente para N >= 5.
const INTERVALO_TICK: Duration = Duration::from_millis(250);

/// Si el bucle tiene que escuchar el tick periódico ahora mismo: solo
/// con el visor de logs abierto o el guardado automático "cada N
/// segundos" prendido — si no, no hay nada que hacer en cada tick y no
/// tiene sentido despertarse.
fn necesita_tick(estado: &EstadoApp) -> bool {
    estado.logs_lsp.activo() || estado.config.editor.guardado_automatico == GuardadoAutomatico::CadaNSegundos
}

/// Un tick del bucle principal: refresca el visor de logs del LSP si
/// está abierto (BACKLOG.md P1 #2) y corre el guardado automático "cada
/// N segundos" si ya tocaba (P2 #4). Devuelve si cambió algo visible —
/// si no, el bucle se saltea el próximo dibujo.
fn procesar_tick(layout: &mut PanelLayout, estado: &mut EstadoApp) -> bool {
    let mut cambio = false;
    if estado.logs_lsp.activo() {
        let (lineas, total) = estado.lsp.logs_con_total();
        cambio |= estado.logs_lsp.actualizar(lineas, total);
    }
    if estado.config.editor.guardado_automatico == GuardadoAutomatico::CadaNSegundos {
        // `max(1)`: un `0` escrito a mano en `config.toml` no puede
        // convertirse en "guardar en cada tick".
        let periodo = Duration::from_secs(estado.config.editor.segundos_guardado_automatico.max(1));
        if estado.ultimo_autoguardado.elapsed() >= periodo {
            estado.ultimo_autoguardado = Instant::now();
            cambio |= autoguardar(layout);
        }
    }
    cambio
}

/// Qué tiene el foco, para el guardado automático "al perder foco"
/// (BACKLOG.md P2 #4): el índice del panel activo, el archivo que
/// muestra (abrir otro en el mismo panel lo cambia) y si el teclado va
/// al editor o al explorador. Abrir un overlay (paleta, buscador, panel de
/// administración...) NO cuenta como perder el foco — es algo que se
/// hace "sobre" el archivo, y guardar en cada `Ctrl+P` sorprendería.
fn firma_foco(layout: &PanelLayout, estado: &EstadoApp) -> (usize, String, Foco) {
    (layout.indice_activo(), layout.panel_activo().ruta_mostrada.clone(), estado.foco)
}

/// Si el guardado automático tiene que tocar este documento: solo con
/// ruta (un "[Sin nombre]" no tiene dónde escribirse sin preguntar, y
/// abrirle el prompt de "Guardar como" solo sería invasivo) y con
/// cambios sin guardar.
fn necesita_autoguardado(editor: &Editor) -> bool {
    editor.buffer().ruta().is_some() && editor.buffer().modificado()
}

/// Guardado automático (BACKLOG.md P2 #4) de TODOS los paneles con ruta
/// y cambios, no solo el activo: "cada N segundos" no tiene un panel en
/// particular, y "al perder foco" en la práctica solo encuentra
/// modificado al que acaba de perderlo (los demás ya se guardaron cuando
/// lo perdieron ellos), salvo que un guardado anterior haya fallado —
/// entonces se reintenta, que es lo que se quiere. Un fallo no corta
/// nada: queda en la statusbar de ese panel (`guardar_panel`) y el
/// buffer sigue modificado, sin perder nada. Devuelve si intentó guardar
/// alguno (hay que redibujar: cambia el `*` de la statusbar).
fn autoguardar(layout: &mut PanelLayout) -> bool {
    let mut intento = false;
    for panel in layout.paneles_mut() {
        if necesita_autoguardado(&panel.editor) {
            let _ = guardar_panel(panel);
            intento = true;
        }
    }
    intento
}

/// La escritura a disco de un documento en su ruta — el último paso
/// tanto de `archivo.guardar` (`Ctrl+S`, vía `guardar_archivo_activo`,
/// que antes formatea si corresponde) como del guardado automático.
/// Deja el motivo del error en `aviso_guardado` (statusbar) si falla, y
/// lo limpia si funciona.
///
/// El guardado automático NO formatea (llama acá directo, sin pasar por
/// `formatear_antes_de_guardar`): con "cada N segundos" reformatearía el
/// código mientras se escribe, moviendo el texto bajo el cursor — mismo
/// criterio que VSCode, donde "format on save" no corre con el
/// autoguardado por demora. Para formatear, `Ctrl+S`.
fn guardar_panel(panel: &mut PanelEditor) -> Result<()> {
    let resultado = panel.editor.guardar();
    panel.aviso_guardado = resultado.as_ref().err().map(|e| format!("no se pudo guardar: {e:#}"));
    resultado
}

/// Texto pegado desde la terminal (`Event::Paste`, bracketed paste). Con
/// el editor enfocado y sin ningún overlay abierto se inserta entero de
/// una vez (`Editor::insertar_texto`: una sola edición, un solo paso de
/// deshacer, sin re-indentar cada línea). En un prompt de una línea
/// (paleta, buscadores, "Guardar como"...) se reparte en teclas sueltas
/// que el bucle procesa como si se hubieran tipeado, sin los saltos de
/// línea — un `Enter` en medio confirmaría el prompt a medio pegar. En
/// cualquier otro lado (confirmación de borrado, salto rápido del
/// explorador, menús de una tecla) se ignora: ahí cada carácter es una
/// acción, no texto.
fn pegar_texto(texto: &str, layout: &mut PanelLayout, estado: &mut EstadoApp, teclas: &mut VecDeque<KeyEvent>) {
    let prompt_de_texto = estado.paleta_comandos.activa()
        || estado.buscador_archivos.activo()
        || estado.estado_busqueda.activa()
        || estado.guardar_como.activa()
        || estado.logs_lsp.activo()
        || estado.prompt_explorador.activo()
        || layout.panel_activo().estado_csv.editando()
        || layout.panel_activo().estado_csv.prompt_filtro().is_some()
        || (estado.panel_admin.activo() && estado.panel_admin.editando_comando_lsp().is_some());
    if prompt_de_texto {
        teclas.extend(
            texto
                .chars()
                .filter(|c| !c.is_control())
                .map(|c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)),
        );
        return;
    }

    let otro_modal = estado.editor_tema.activo()
        || estado.panel_admin.activo()
        || estado.selector_tema.activa()
        || estado.confirmar_borrado.activo()
        || estado.explorador.modo_salto();
    if otro_modal || estado.foco != Foco::Editor || layout.panel_activo().modo_csv == ModoCsv::Tabla {
        return;
    }
    estado.confirmar_salida = false;
    layout.editor_activo_mut().insertar_texto(texto);
}

/// Le avisa al `EstadoLsp` cuál es el archivo/contenido activos ahora
/// mismo: relanza el cliente si cambió el lenguaje (o si se
/// habilitó/deshabilitó desde el panel de administración, sección
/// "Lenguajes / LSP", PLAN.md §5.3 — `config` es lo que decide eso), y
/// notifica `didChange` si el texto cambió desde el último envío.
///
/// El texto se pide solo si hace falta (sesión nueva, o revisión del
/// buffer distinta de la del último envío): esto corre en cada frame, y
/// antes copiaba el archivo entero en todos, incluso sin LSP activo
/// (BACKLOG.md P1 #14).
async fn sincronizar_lsp(layout: &PanelLayout, lsp: &mut lsp::EstadoLsp, config: &Config) {
    let panel = layout.panel_activo();
    let buffer = panel.editor.buffer();
    lsp.actualizar_para_archivo(&panel.ruta_mostrada, || buffer.a_texto(), config).await;
    lsp.sincronizar_contenido(buffer.revision(), || buffer.a_texto()).await;
}

/// Punto de entrada único para ejecutar un id de comando, venga de un
/// atajo de teclado o de confirmar un resultado en la paleta de comandos.
/// `config.recargar`, `paleta.comandos`, `buscar.archivos` y
/// `archivo.guardar`/`archivo.guardar_como` necesitan estado que no le
/// corresponde a `ejecutar_comando` (la paleta de colores, los propios
/// overlays, el prompt de "Guardar como"), así que se interceptan aquí
/// antes de delegar.
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
        "tema.editor_visual" => {
            abrir_editor_visual_tema(estado);
            Accion::Continuar
        }
        "admin.abrir_panel" => {
            estado.panel_admin.abrir();
            Accion::Continuar
        }
        "lsp.ver_logs" => {
            // Abre con lo que haya AHORA — vacío y con el mensaje
            // correspondiente si no hay sesión LSP activa, en vez de no
            // hacer nada en silencio. De ahí en más se actualiza solo con
            // el tick del bucle (`procesar_tick`, BACKLOG.md P1 #2).
            let (lineas, total) = estado.lsp.logs_con_total();
            estado.logs_lsp.abrir(lineas, total);
            Accion::Continuar
        }
        // Crear/renombrar en el explorador (BACKLOG.md P0, "explorador
        // de solo lectura") — global, mismo criterio que
        // `explorador.saltar`: invocable desde la paleta de comandos sin
        // tener el explorador abierto todavía, lo muestra y le da el
        // foco antes de abrir el prompt.
        "explorador.nuevo_archivo" => {
            estado.explorador.mostrar();
            estado.foco = Foco::Explorador;
            estado.prompt_explorador.abrir(ModoPromptExplorador::NuevoArchivo, "");
            Accion::Continuar
        }
        "explorador.nueva_carpeta" => {
            estado.explorador.mostrar();
            estado.foco = Foco::Explorador;
            estado.prompt_explorador.abrir(ModoPromptExplorador::NuevaCarpeta, "");
            Accion::Continuar
        }
        // Precargado con el nombre actual (no vacío) — mismo criterio
        // que "Guardar como" sobre un archivo ya existente: alcanza con
        // ajustar en vez de reescribir la ruta entera. Sin nada
        // seleccionado (árbol vacío) no abre nada — no hay qué renombrar.
        "explorador.renombrar" => {
            estado.explorador.mostrar();
            estado.foco = Foco::Explorador;
            if let Some(nodo) = estado.explorador.seleccion_actual() {
                estado.prompt_explorador.abrir(ModoPromptExplorador::Renombrar, &nodo.nombre);
            }
            Accion::Continuar
        }
        // Plegado (BACKLOG.md P2 #7, PLAN.md §4): los rangos plegables
        // salen del árbol de tree-sitter que ya mantiene el resaltador
        // (o de la indentación, sin gramática) y se calculan a pedido,
        // solo al plegar. Ni en el explorador ni en la vista de tabla
        // CSV hay líneas de código que plegar.
        "plegar.actual" | "plegar.todo" => {
            if estado.foco == Foco::Editor && layout.panel_activo().modo_csv != ModoCsv::Tabla {
                let candidatos = rangos_plegables_del_activo(layout, &mut estado.resaltador);
                let editor = layout.editor_activo_mut();
                if id == "plegar.todo" {
                    editor.plegar_todo(&candidatos);
                } else {
                    editor.plegar_en_cursor(&candidatos);
                }
            }
            Accion::Continuar
        }
        "plegar.desplegar" => {
            layout.editor_activo_mut().desplegar_en_cursor();
            Accion::Continuar
        }
        "plegar.desplegar_todo" => {
            layout.editor_activo_mut().desplegar_todo();
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
        // `Ctrl+S` sobre un buffer sin ruta asociada (archivo nuevo,
        // "[Sin nombre]") no tiene dónde escribir — en vez de fallar en
        // silencio como antes, abre el mismo prompt que "Guardar como".
        "archivo.guardar" => {
            if layout.editor_activo().buffer().ruta().is_none() {
                estado.guardar_como.abrir("");
            } else {
                // Ver `EstadoApp::guardado_pendiente`.
                estado.guardado_pendiente = true;
            }
            Accion::Continuar
        }
        "archivo.guardar_como" => {
            // Precargado con la ruta actual (si ya tenía una) para poder
            // "guardar como" un archivo existente con otro nombre/ruta
            // sin reescribirla entera — no solo para ponerle nombre a uno
            // nuevo.
            let ruta_inicial =
                layout.editor_activo().buffer().ruta().map(|r| r.display().to_string()).unwrap_or_default();
            estado.guardar_como.abrir(&ruta_inicial);
            Accion::Continuar
        }
        // `Delete` reinterpretado como "pedir confirmación de borrado"
        // en vez de "borrar hacia adelante" (su significado normal en el
        // editor, `editor.borrar_adelante` en `ejecutar_comando` más
        // abajo) — mismo criterio que `cursor.arriba`/`editor.nueva_linea`
        // ya reinterpretados para el explorador, pero este necesita
        // `estado.confirmar_borrado`, al que `ejecutar_comando_explorador`
        // no tiene acceso, así que se resuelve acá en vez de ahí. Nunca
        // borra directo: siempre abre el prompt de confirmación primero
        // (BACKLOG.md P0, "explorador de solo lectura" — acción
        // destructiva e irreversible).
        "editor.borrar_adelante" if estado.foco == Foco::Explorador => {
            if let Some(nodo) = estado.explorador.seleccion_actual() {
                estado.confirmar_borrado.abrir(nodo.ruta.clone(), nodo.nombre.clone(), nodo.es_carpeta);
            }
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
        "explorador.saltar" => {
            // Global (no gated por foco, a diferencia de `cursor.arriba`
            // reinterpretado en `ejecutar_comando_explorador`): invocable
            // desde la paleta de comandos sin tener el explorador abierto
            // todavía — lo muestra y le da el foco antes de activar el
            // modo, en vez de no hacer nada en silencio.
            explorador.mostrar();
            *foco = Foco::Explorador;
            explorador.activar_modo_salto();
            return Accion::Continuar;
        }
        "explorador.enfocar_editor" => {
            // En la vista de tabla CSV/TSV con un filtro activo, `Esc`
            // lo quita (BACKLOG.md P2 #9) — es lo que anuncia la barra
            // del filtro. Solo con el foco ya en el editor: con el foco
            // en el explorador, `Esc` sigue significando "volver al
            // editor" y nada más.
            if *foco == Foco::Editor && layout.panel_activo().modo_csv == ModoCsv::Tabla && quitar_filtro_csv(layout) {
                return Accion::Continuar;
            }
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
        return ejecutar_comando_explorador(comando, layout, explorador, foco, &config.editor);
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
        "cursor.seleccionar_arriba" => editor.seleccionar_arriba(),
        "cursor.seleccionar_abajo" => editor.seleccionar_abajo(),
        "cursor.seleccionar_izquierda" => editor.seleccionar_izquierda(),
        "cursor.seleccionar_derecha" => editor.seleccionar_derecha(),
        "cursor.seleccionar_inicio_linea" => editor.seleccionar_inicio_linea(),
        "cursor.seleccionar_fin_linea" => editor.seleccionar_fin_linea(),
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
fn ejecutar_comando_explorador(
    comando: &str,
    layout: &mut PanelLayout,
    explorador: &mut Explorador,
    foco: &mut Foco,
    config: &ConfigEditor,
) -> Accion {
    match comando {
        "cursor.arriba" => explorador.mover_arriba(),
        "cursor.abajo" => explorador.mover_abajo(),
        "editor.nueva_linea" => {
            if let Ok(Some(ruta)) = explorador.activar_seleccion() {
                abrir_ruta_desde_explorador(layout, foco, ruta, config);
            }
        }
        _ => {}
    }
    Accion::Continuar
}

/// Abre `ruta` en el panel activo y devuelve el foco al editor — el
/// usuario ya eligió qué quería, tiene sentido poder escribir de
/// inmediato en vez de seguir en el árbol. Comparten esto tanto `Enter`
/// sobre una fila del explorador (`ejecutar_comando_explorador`) como
/// acertar una etiqueta en modo "salto rápido" (`Ctrl+K J`) y confirmar
/// una ruta en el buscador de archivos (`Ctrl+P`), en el bucle principal
/// de eventos. Si `config.modo_vim` está prendido,
/// el archivo recién abierto arranca en `Modo::Normal` en vez del
/// `Insertar` con el que `Editor::abrir` siempre construye — `Editor` no
/// conoce esa config, así que la decisión se toma acá.
///
/// Con guardado automático "al perder foco" (BACKLOG.md P2 #4), el
/// documento que se va a reemplazar se guarda ANTES — después ya no
/// existe, el cambio de foco que detecta el bucle llegaría tarde. Si ese
/// guardado falla, no se abre nada: reemplazarlo igual perdería los
/// cambios, y el motivo queda en la statusbar.
fn abrir_ruta_desde_explorador(layout: &mut PanelLayout, foco: &mut Foco, ruta: std::path::PathBuf, config: &ConfigEditor) {
    if let Ok(mut nuevo_editor) = Editor::abrir(&ruta) {
        if config.guardado_automatico == GuardadoAutomatico::AlPerderFoco {
            let panel = layout.panel_activo_mut();
            if necesita_autoguardado(&panel.editor) && guardar_panel(panel).is_err() {
                return;
            }
        }
        if config.modo_vim {
            nuevo_editor.entrar_modo_normal();
        }
        layout.abrir_en_activo(nuevo_editor, ruta.display().to_string());
        *foco = Foco::Editor;
    }
}

/// Comandos genéricos de navegación reinterpretados para la vista de
/// tabla CSV/TSV (PLAN.md §9): mover la celda seleccionada en vez del
/// cursor de texto, `Tab`/`Shift+Tab` para saltar de celda (como en una
/// hoja de cálculo) y `Enter`/`F2` para empezar a editar la celda actual
/// — la edición en sí (escribir/confirmar/cancelar) la captura un bloque
/// modal aparte en el bucle principal, igual que la barra de búsqueda.
///
/// También los comandos propios de la tabla (BACKLOG.md P2 #9), que solo
/// tienen efecto acá (desde la paleta, en cualquier otro archivo o en
/// modo texto, no hacen nada): ordenar, filtrar, insertar/eliminar filas
/// y columnas — estas cuatro modifican el archivo, cada una como UNA
/// sola edición (`aplicar_edicion_csv`), así que se deshace con un único
/// `Ctrl+Z` — y el ancho manual de columna (solo de vista, como el
/// filtro). Con un filtro activo, `estado_csv.fila()` es una posición
/// entre las filas visibles: todo lo que lee o escribe el archivo usa
/// `EstadoCsv::fila_real`.
fn ejecutar_comando_csv(comando: &str, layout: &mut PanelLayout) -> Accion {
    let delimitador = delimitador_por_extension(&layout.panel_activo().ruta_mostrada);
    let texto = layout.panel_activo().editor.buffer().a_texto();
    let tabla = analizar_csv(&texto, delimitador).unwrap_or_default();
    let panel = layout.panel_activo_mut();
    let num_filas = panel.estado_csv.filas_visibles(&tabla).len();
    let num_columnas = tabla.num_columnas();
    let columna = panel.estado_csv.columna();

    match comando {
        "cursor.arriba" => panel.estado_csv.mover_arriba(),
        "cursor.abajo" => panel.estado_csv.mover_abajo(num_filas),
        "cursor.izquierda" => panel.estado_csv.mover_izquierda(),
        "cursor.derecha" => panel.estado_csv.mover_derecha(num_columnas),
        "editor.indentar_o_autocompletar" => panel.estado_csv.tab(num_filas, num_columnas),
        "editor.desindentar" => panel.estado_csv.shift_tab(num_columnas),
        "editor.nueva_linea" | "csv.editar_celda" => {
            let valor_actual =
                panel.estado_csv.fila_real(&tabla).and_then(|r| tabla.filas[r].celdas.get(columna));
            panel.estado_csv.iniciar_edicion(valor_actual.map(String::as_str).unwrap_or(""));
        }
        "csv.ordenar" => {
            let ascendente = panel.estado_csv.siguiente_orden(columna);
            aplicar_edicion_csv(panel, tcode_core::csv::ordenar_por_columna(&texto, &tabla, columna, ascendente));
        }
        "csv.filtrar" => {
            let inicial = match panel.estado_csv.filtro() {
                Some(filtro) if filtro.columna == columna => filtro.texto.clone(),
                _ => String::new(),
            };
            panel.estado_csv.abrir_prompt_filtro(&inicial);
        }
        "csv.quitar_filtro" => {
            panel.estado_csv.quitar_filtro(&tabla);
        }
        // Insertar una fila con un filtro activo quita el filtro primero:
        // la fila nueva está vacía, así que (salvo coincidencia) el
        // filtro la ocultaría apenas creada — insertar algo que no se ve
        // es peor que perder el filtro, que se vuelve a poner con dos
        // teclas. `quitar_filtro` deja la selección sobre la misma fila
        // real, así que la posición de inserción no cambia.
        "csv.insertar_fila_debajo" | "csv.insertar_fila_arriba" => {
            panel.estado_csv.quitar_filtro(&tabla);
            let actual = panel.estado_csv.fila_real(&tabla);
            let indice = match (actual, comando == "csv.insertar_fila_debajo") {
                (Some(r), true) => r + 1,
                (Some(r), false) => r,
                (None, _) => 0,
            };
            aplicar_edicion_csv(panel, tcode_core::csv::insertar_fila(&texto, &tabla, indice).ok());
            // Seleccionar la fila recién insertada (sin filtro, fila
            // visible == fila real). `mover_abajo` avanza de a una.
            if comando == "csv.insertar_fila_debajo" && actual.is_some() {
                panel.estado_csv.mover_abajo(tabla.num_filas() + 1);
            }
        }
        "csv.eliminar_fila" => {
            if let Some(r) = panel.estado_csv.fila_real(&tabla) {
                aplicar_edicion_csv(panel, tcode_core::csv::eliminar_fila(&texto, &tabla, r));
            }
        }
        "csv.insertar_columna_derecha" | "csv.insertar_columna_izquierda" => {
            let indice = if comando == "csv.insertar_columna_derecha" && num_columnas > 0 { columna + 1 } else { columna };
            if aplicar_edicion_csv(panel, tcode_core::csv::insertar_columna(&texto, &tabla, indice).ok().flatten()) {
                panel.estado_csv.columna_insertada(indice);
                if indice > columna {
                    panel.estado_csv.mover_derecha(num_columnas + 1);
                }
            }
        }
        "csv.eliminar_columna" => {
            if aplicar_edicion_csv(panel, tcode_core::csv::eliminar_columna(&texto, &tabla, columna).ok().flatten()) {
                panel.estado_csv.columna_eliminada(columna);
            }
        }
        "csv.ensanchar_columna" | "csv.angostar_columna" if num_columnas > 0 => {
            let actual = ancho_columna_csv(&tabla, &panel.estado_csv, columna);
            let nuevo = if comando == "csv.ensanchar_columna" {
                actual + PASO_ANCHO_COLUMNA_CSV
            } else {
                actual.saturating_sub(PASO_ANCHO_COLUMNA_CSV)
            };
            panel.estado_csv.fijar_ancho(columna, nuevo);
        }
        "csv.restablecer_ancho" => panel.estado_csv.restablecer_ancho(columna),
        _ => {}
    }
    Accion::Continuar
}

/// Cuántas columnas de terminal ensancha/angosta cada `Ctrl+K Shift+→`/
/// `Ctrl+K Shift+←` en la vista CSV. De a 1 haría falta repetir el chord
/// decenas de veces para leer una celda larga; de a 2 sigue siendo fino
/// y la mitad de tedioso.
const PASO_ANCHO_COLUMNA_CSV: usize = 2;

/// Aplica una edición de la vista CSV (ordenar, insertar/eliminar) sobre
/// el buffer como UN solo reemplazo — un solo snapshot en el historial,
/// un solo `Ctrl+Z` para deshacerla. Devuelve si había algo que aplicar
/// (`None` = la operación no cambiaba nada, p. ej. ordenar algo ya
/// ordenado), para que quien llama solo actualice su estado de vista en
/// ese caso.
fn aplicar_edicion_csv(panel: &mut tcode_ui::PanelEditor, edicion: Option<tcode_core::EdicionCsv>) -> bool {
    let Some(edicion) = edicion else { return false };
    panel.editor.reemplazar_rango_bytes(edicion.inicio_byte, edicion.fin_byte, &edicion.reemplazo);
    true
}

/// `Enter` con el prompt de filtro de la vista CSV abierto: aplica el
/// texto escrito como filtro sobre la columna seleccionada, o quita el
/// filtro si quedó vacío (así el mismo prompt sirve para las dos cosas,
/// además de `Esc` con la tabla enfocada).
fn confirmar_filtro_csv(layout: &mut PanelLayout) {
    let delimitador = delimitador_por_extension(&layout.panel_activo().ruta_mostrada);
    let tabla = analizar_csv(&layout.panel_activo().editor.buffer().a_texto(), delimitador).unwrap_or_default();
    let estado_csv = &mut layout.panel_activo_mut().estado_csv;
    let Some(texto) = estado_csv.cerrar_prompt_filtro() else { return };
    if texto.is_empty() {
        estado_csv.quitar_filtro(&tabla);
    } else {
        let columna = estado_csv.columna();
        estado_csv.filtrar(columna, &texto);
    }
}

/// Quita el filtro de la vista CSV del panel activo, si había uno
/// (devuelve si lo había) — `Esc` con la tabla enfocada.
fn quitar_filtro_csv(layout: &mut PanelLayout) -> bool {
    if layout.panel_activo().estado_csv.filtro().is_none() {
        return false;
    }
    let delimitador = delimitador_por_extension(&layout.panel_activo().ruta_mostrada);
    let tabla = analizar_csv(&layout.panel_activo().editor.buffer().a_texto(), delimitador).unwrap_or_default();
    layout.panel_activo_mut().estado_csv.quitar_filtro(&tabla)
}

/// `Enter` con el prompt "Guardar como" abierto: intenta escribir el
/// archivo en la ruta escrita. Una línea vacía no se intenta guardar (no
/// tiene sentido un archivo sin nombre) — deja el error visible en vez de
/// nada, para que quede claro por qué no pasó nada. Si el guardado
/// funciona, actualiza `ruta_mostrada` del panel activo (statusbar,
/// pestaña, detección de lenguaje) y cierra el prompt; si falla (permiso
/// denegado, directorio inexistente...) el prompt queda abierto con el
/// motivo, para poder corregir la ruta sin perder lo ya escrito.
///
/// Con "formatear al guardar" prendido para el lenguaje del archivo
/// (el de su ruta ANTERIOR — es el lenguaje del contenido y el de la
/// sesión LSP abierta sobre él; un buffer "[Sin nombre]" no tiene
/// ninguno) se formatea antes de escribir, igual que con `Ctrl+S`.
async fn guardar_como_confirmar(layout: &mut PanelLayout, estado: &mut EstadoApp) {
    let ruta = estado.guardar_como.ruta().trim();
    if ruta.is_empty() {
        estado.guardar_como.establecer_error("la ruta no puede estar vacía".to_string());
        return;
    }
    let ruta = ruta.to_string();
    formatear_antes_de_guardar(layout, estado).await;
    match layout.editor_activo_mut().guardar_como(ruta.clone()) {
        Ok(()) => {
            let panel = layout.panel_activo_mut();
            panel.ruta_mostrada = ruta;
            panel.aviso_guardado = None;
            estado.guardar_como.cerrar();
        }
        Err(e) => estado.guardar_como.establecer_error(e.to_string()),
    }
}

/// Camino ÚNICO para guardar el archivo del panel activo en su propia
/// ruta (`archivo.guardar`, `Ctrl+S`): primero "formatear al guardar" si
/// corresponde (`formatear_antes_de_guardar`), después escribir a disco.
/// Cualquier otro disparador de guardado del archivo activo (p. ej. el
/// guardado automático, BACKLOG.md P2 #4) debería pasar por acá en vez
/// de llamar a `Editor::guardar` directo, para respetar la misma
/// configuración. Un panel que NO es el activo no puede formatearse (la
/// única sesión LSP es la del panel activo, ver `lsp.rs`): para esos,
/// `Editor::guardar` directo es lo correcto. Falla igual que
/// `Editor::guardar` (buffer sin ruta, error de disco); formatear nunca
/// hace fallar el guardado.
async fn guardar_archivo_activo(layout: &mut PanelLayout, estado: &mut EstadoApp) -> Result<()> {
    formatear_antes_de_guardar(layout, estado).await;
    let resultado = guardar_panel(layout.panel_activo_mut());
    // Guardar no cambia `HEAD`, pero es el momento natural para notar un
    // commit hecho desde otra terminal (BACKLOG.md P2 #6): `Ctrl+S`
    // refresca la base de los indicadores de git aunque no haya cambios.
    layout.refrescar_bases_git();
    resultado
}

/// "Formatear al guardar" (PLAN.md §5 "Editor", BACKLOG.md P2 #5): si
/// está prendido para el lenguaje del archivo activo (`f` en "Lenguajes
/// / LSP" del panel de administración), le pide `textDocument/
/// formatting` al LSP y aplica el resultado como UNA sola edición
/// deshacible (`Editor::aplicar_ediciones`: un `Ctrl+Z` después de
/// guardar vuelve al texto sin formatear). Con la opción apagada no hace
/// absolutamente nada — ni siquiera sincroniza con el LSP —, así que
/// guardar se comporta exactamente igual que antes de esta pieza.
///
/// Nunca bloquea el guardado: si no hay LSP, todavía está iniciando, no
/// soporta formatear, devuelve error o no responde a tiempo
/// (`EstadoLsp::pedir_formateo`, ~2 s como mucho), simplemente no se
/// formatea y el aviso de la barra de estado dice por qué.
async fn formatear_antes_de_guardar(layout: &mut PanelLayout, estado: &mut EstadoApp) {
    let Some(lenguaje) = Lenguaje::detectar_por_extension(&layout.panel_activo().ruta_mostrada) else { return };
    if !estado.config.lenguajes.formatear_al_guardar(lenguaje.id()) {
        return;
    }

    // Mismo paso que el bucle hace una vez por frame: sin esto, lo
    // tipeado desde el último frame todavía no le llegó al servidor, y
    // sus posiciones se referirían a un texto viejo.
    sincronizar_lsp(layout, &mut estado.lsp, &estado.config).await;
    let ruta = layout.panel_activo().ruta_mostrada.clone();
    let texto = layout.editor_activo().buffer().a_texto();
    // rustfmt/prettier/etc. suelen tener su propia config por proyecto
    // que manda sobre esto; es lo que se usa cuando no la hay.
    let opciones = FormattingOptions {
        tab_size: estado.config.editor.tamano_tabulacion as u32,
        insert_spaces: estado.config.editor.usar_espacios,
        ..Default::default()
    };

    let mensaje = match estado.lsp.pedir_formateo(&ruta, &texto, opciones, layout).await {
        Ok(ediciones) => {
            let ediciones: Vec<_> = ediciones.into_iter().map(|e| (e.inicio_byte..e.fin_byte, e.texto)).collect();
            let editor = layout.editor_activo_mut();
            if !editor.aplicar_ediciones(&ediciones) {
                // Ya estaba formateado: nada que avisar.
                return;
            }
            // En modo Normal (VIM) el cursor no puede quedar después del
            // último carácter de la línea — reaplica ese recorte.
            if editor.modo() == Modo::Normal {
                editor.entrar_modo_normal();
            }
            "Formateado al guardar".to_string()
        }
        Err(motivo) => format!("Sin formatear: {motivo}"),
    };
    layout.panel_activo_mut().mensaje_estado = Some(mensaje);
}

/// `Enter` con el prompt de texto del explorador abierto (`Ctrl+K N`/
/// `Ctrl+K C`/`Ctrl+K M`): despacha a la operación de filesystem que
/// corresponda según `ModoPromptExplorador`. Un nombre vacío (o solo
/// espacios) no intenta nada — mismo criterio que "Guardar como" con una
/// ruta vacía. Si la operación falla (ya existe algo con ese nombre,
/// permiso denegado...) el prompt queda abierto con el motivo, en vez de
/// cerrarse como si nada.
fn confirmar_prompt_explorador(explorador: &mut Explorador, prompt: &mut EstadoPromptExplorador) {
    let Some(modo) = prompt.modo() else { return };
    let texto = prompt.texto().trim();
    if texto.is_empty() {
        prompt.establecer_error("el nombre no puede estar vacío".to_string());
        return;
    }
    let resultado = match modo {
        ModoPromptExplorador::NuevoArchivo => explorador.crear_archivo(texto),
        ModoPromptExplorador::NuevaCarpeta => explorador.crear_carpeta(texto),
        ModoPromptExplorador::Renombrar => explorador.renombrar_seleccion(texto),
    };
    match resultado {
        Ok(()) => prompt.cerrar(),
        Err(e) => prompt.establecer_error(e.to_string()),
    }
}

/// `Enter` con una celda de la vista CSV/TSV en edición: reemplaza la
/// fila completa reserializada (ver `tcode_core::csv::serializar_fila`)
/// en el buffer, y si `avanzar` es `true` mueve la selección a la fila
/// siguiente (como confirmar una celda en una hoja de cálculo). Con un
/// filtro activo edita la fila REAL detrás de la fila visible
/// seleccionada (`EstadoCsv::fila_real`); si el valor nuevo ya no
/// coincide con el filtro, la fila deja de verse — el filtro se evalúa
/// siempre sobre el contenido actual, como en una hoja de cálculo.
fn confirmar_edicion_celda_csv(layout: &mut PanelLayout, avanzar: bool) {
    let panel = layout.panel_activo_mut();
    let Some(nuevo_valor) = panel.estado_csv.confirmar_edicion() else { return };

    let delimitador = delimitador_por_extension(&panel.ruta_mostrada);
    let texto = panel.editor.buffer().a_texto();
    let Ok(tabla) = analizar_csv(&texto, delimitador) else { return };
    let Some(fila) = panel.estado_csv.fila_real(&tabla).map(|r| &tabla.filas[r]) else { return };

    let mut celdas = fila.celdas.clone();
    if panel.estado_csv.columna() >= celdas.len() {
        celdas.resize(panel.estado_csv.columna() + 1, String::new());
    }
    celdas[panel.estado_csv.columna()] = nuevo_valor;

    let Ok(nueva_fila_texto) = serializar_fila_csv(&celdas, delimitador) else { return };
    panel.editor.reemplazar_rango_bytes(fila.inicio_byte, fila.fin_byte, &nueva_fila_texto);

    if avanzar {
        let tabla_nueva = analizar_csv(&panel.editor.buffer().a_texto(), delimitador).unwrap_or_default();
        let num_filas = panel.estado_csv.filas_visibles(&tabla_nueva).len();
        // Si la fila editada dejó de pasar el filtro, desapareció y la
        // siguiente ya ocupa su lugar: avanzar además se la saltearía.
        if num_filas == panel.estado_csv.filas_visibles(&tabla).len() {
            panel.estado_csv.mover_abajo(num_filas);
        }
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
    // La global se conserva si ahora no se puede leer (igual que antes de
    // la config por proyecto); la de proyecto se vuelve a buscar desde
    // cero — puede haber aparecido, desaparecido o cambiado
    // (BACKLOG.md P2 #8).
    if let Ok(global) = tcode_config::cargar() {
        estado.capas_config.global = global;
    }
    estado.capas_config.proyecto = tcode_config::cargar_config_proyecto(&estado.capas_config.dir_inicio);
    let nueva = estado.capas_config.efectiva();
    let tema_cambio = nueva.interfaz.tema != estado.config.interfaz.tema;
    estado.config = nueva;
    if tema_cambio {
        estado.paleta = cargar_paleta(&estado.config.interfaz.tema);
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
        estado.paleta = cargar_paleta(&id);
    }
}

/// `Enter` sobre una fila del selector de temas: además del preview que ya
/// se venía aplicando, persiste el cambio en `config.toml` para que
/// sobreviva a reiniciar el editor (best-effort — si no se puede escribir
/// a disco, el tema queda igual aplicado en memoria para esta sesión).
///
/// Con una config de proyecto que pisa `interfaz.tema`, el tema elegido
/// queda guardado en la global pero el que se ve sigue siendo el del
/// proyecto (la paleta se recarga desde la efectiva, no desde `id`).
fn confirmar_tema_seleccionado(estado: &mut EstadoApp, id: &str) {
    estado.capas_config.global.interfaz.tema = id.to_string();
    guardar_config_global(estado);
    estado.paleta = cargar_paleta(&estado.config.interfaz.tema);
}

/// Qué bloque del `match` de más arriba corresponde al modo de edición
/// actual del editor visual de tema — separado de `ModoEdicion` porque
/// ese enum lleva los VALORES en curso (buffer de hex, HSL parcial...) y
/// acá solo hace falta saber a cuál de los cuatro casos ir; extraerlo a
/// una variable de esta forma, sin quedarse con el préstamo de
/// `estado.editor_tema.modo()`, es lo que permite llamar métodos que la
/// mutan (`&mut estado.editor_tema...`) en el cuerpo de cada rama.
enum CategoriaModoEditorTema {
    Ninguno,
    Hex,
    Paleta,
    Hsl,
}

fn categoria_modo_editor_tema(editor_tema: &EstadoEditorTema) -> CategoriaModoEditorTema {
    match editor_tema.modo() {
        ModoEdicion::Ninguno => CategoriaModoEditorTema::Ninguno,
        ModoEdicion::Hex(_) => CategoriaModoEditorTema::Hex,
        ModoEdicion::Paleta(_) => CategoriaModoEditorTema::Paleta,
        ModoEdicion::Hsl { .. } => CategoriaModoEditorTema::Hsl,
    }
}

/// `Ctrl+K Ctrl+P` (`tema.editor_visual`, PLAN.md §7): abre el editor
/// visual sobre una copia editable del tema activo (duplicándolo si
/// hace falta, ver `EstadoEditorTema::abrir`) y lo deja como tema activo
/// de una — así el preview en vivo de cada cambio de color, y el
/// resultado final, se ven de inmediato en el editor real detrás.
fn abrir_editor_visual_tema(estado: &mut EstadoApp) {
    let tema_actual = estado.config.interfaz.tema.clone();
    if estado.editor_tema.abrir(&tema_actual).is_ok() {
        estado.capas_config.global.interfaz.tema = estado.editor_tema.id_tema().to_string();
        guardar_config_global(estado);
        estado.paleta = Paleta::desde_tema(estado.editor_tema.tema()).unwrap_or_else(|_| Paleta::basica());
    }
}

/// Tras aplicar un cambio de color en el editor visual (por cualquiera
/// de los tres métodos — `confirmar_hex`/`confirmar_paleta`/
/// `ajustar_hsl`/`confirmar_hsl`, o `cancelar_edicion` revirtiendo un
/// ajuste HSL a mitad de camino): refresca `estado.paleta` desde la
/// copia de trabajo para que el preview en vivo se vea de inmediato — el
/// guardado a disco, si corresponde, ya lo hizo el método de
/// `EstadoEditorTema` por su cuenta.
fn refrescar_preview_editor_tema(estado: &mut EstadoApp) {
    estado.paleta = Paleta::desde_tema(estado.editor_tema.tema()).unwrap_or_else(|_| Paleta::basica());
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
            let (comando, en_path) = match lsp::comando_efectivo(lenguaje, &estado.config) {
                Some((comando, args, env)) => {
                    let mut texto = if args.is_empty() { comando.clone() } else { format!("{comando} {}", args.join(" ")) };
                    // Indicador chico de que además hay variables de
                    // entorno configuradas (BACKLOG.md P1) — la línea
                    // completa (con nombres y valores) solo se ve al
                    // entrar a editar (`c`), acá alcanza con saber que
                    // hay alguna.
                    if !env.is_empty() {
                        texto.push_str(&format!(" [+{} var{} de entorno]", env.len(), if env.len() == 1 { "" } else { "s" }));
                    }
                    (texto, ruta_en_path(&comando))
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
                // "Habilitado" muestra la GLOBAL (lo que alterna esta
                // fila); si el proyecto lo apaga igual, se marca aparte.
                habilitado: estado.capas_config.global.lenguajes.lsp_habilitado(lenguaje.id()),
                deshabilitado_por_proyecto: estado.capas_config.global.lenguajes.lsp_habilitado(lenguaje.id())
                    && !estado.config.lenguajes.lsp_habilitado(lenguaje.id()),
                personalizado: estado.capas_config.global.lenguajes.comando_configurado(lenguaje.id()).is_some(),
                formatear_al_guardar: estado.capas_config.global.lenguajes.formatear_al_guardar(lenguaje.id()),
                estado: estado_texto,
            }
        })
        .collect()
}

/// Búsqueda simple del ejecutable `comando` en el `PATH` — suficiente
/// para el indicador "¿está instalado?" de la sección "Lenguajes / LSP"
/// del panel de administración (PLAN.md §5.3; un `which`/`where`
/// completo queda para cuando haga falta más precisión). No confirma que
/// el binario funcione, solo que existe un archivo con ese nombre (o, en
/// Windows, ese nombre más alguna extensión de `PATHEXT`) en algún
/// directorio del `PATH`.
fn ruta_en_path(comando: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else { return false };
    let extensiones = if cfg!(windows) { Some(extensiones_pathext()) } else { None };
    std::env::split_paths(&path).any(|dir| existe_ejecutable(&dir, comando, extensiones.as_deref()))
}

/// Contenido de la variable de entorno `PATHEXT` de Windows
/// (`.COM;.EXE;.BAT;.CMD;...`), o un valor de respaldo razonable si no
/// está definida — no debería pasar en un Windows real (el sistema
/// siempre la fija), pero un valor vacío dejaría `ruta_en_path` sin
/// encontrar nada nunca en ese caso límite.
fn extensiones_pathext() -> String {
    std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
}

/// Si `dir/comando` (probado tal cual, y si `extensiones` trae algo,
/// también `dir/comando<ext>` por cada extensión de esa lista separada
/// por `;`) existe como archivo. En Windows, a diferencia de Unix, casi
/// nunca se invoca un ejecutable con su extensión puesta (`npm`, no
/// `npm.cmd`) — probar solo el nombre pelado (lo único que hacía esta
/// función antes) daba un falso negativo para casi cualquier LSP
/// instalado ahí, aunque estuviera perfectamente disponible.
/// `extensiones` es un parámetro (no `PATHEXT` leída acá adentro) para
/// que los tests puedan fijar un valor conocido sin depender de en qué
/// sistema operativo corren de verdad — `ruta_en_path` es quien decide
/// con `cfg!(windows)` al llamarla.
fn existe_ejecutable(dir: &std::path::Path, comando: &str, extensiones: Option<&str>) -> bool {
    if dir.join(comando).is_file() {
        return true;
    }
    let Some(extensiones) = extensiones else { return false };
    extensiones.split(';').filter(|ext| !ext.is_empty()).any(|ext| dir.join(format!("{comando}{ext}")).is_file())
}

/// `Enter`/`←`/`→` sobre una fila de "Lenguajes / LSP": alterna si el
/// LSP de ese lenguaje está habilitado. A diferencia de "Editor", no
/// hace falta un `delta` — es un toggle simple, sin valores numéricos.
fn alternar_lsp_lenguaje_seleccionado(estado: &mut EstadoApp) {
    if estado.panel_admin.seccion_actual() != Seccion::Lenguajes {
        return;
    }
    if let Some(&lenguaje) = Lenguaje::TODOS.get(estado.panel_admin.campo()) {
        estado.capas_config.global.lenguajes.alternar_lsp(lenguaje.id());
        guardar_config_global(estado);
    }
}

/// Lenguaje de la fila seleccionada en "Lenguajes / LSP", o `None` si la
/// sección actual no es esa.
fn lenguaje_seleccionado_en_lenguajes(panel: &tcode_config::EstadoPanelAdmin) -> Option<Lenguaje> {
    if panel.seccion_actual() != Seccion::Lenguajes {
        return None;
    }
    Lenguaje::TODOS.get(panel.campo()).copied()
}

/// `f` sobre una fila de "Lenguajes / LSP": prende/apaga "formatear al
/// guardar" para ese lenguaje (BACKLOG.md P2 #5) y lo persiste al
/// instante, igual que el resto de la sección. Se puede prender aunque
/// el lenguaje no tenga LSP configurado todavía: sin sesión, guardar
/// simplemente avisa que no formateó (ver `formatear_antes_de_guardar`).
fn alternar_formatear_al_guardar_seleccionado(estado: &mut EstadoApp) {
    let Some(lenguaje) = lenguaje_seleccionado_en_lenguajes(&estado.panel_admin) else { return };
    estado.capas_config.global.lenguajes.alternar_formatear_al_guardar(lenguaje.id());
    guardar_config_global(estado);
}

/// `c` sobre una fila de "Lenguajes / LSP": empieza a editar su comando
/// personalizado, precargando el buffer con el comando efectivo actual
/// (personalizado si ya hay uno, o el que trae `tcode_lsp::comando_para`
/// por defecto, o vacío si no hay ninguno) — así se puede ajustar solo
/// los argumentos (o las variables de entorno, con la sintaxis `VAR=val
/// -- comando`, ver `ComandoLsp::como_linea`/`fijar_comando_desde_linea`)
/// sin volver a escribir todo desde cero.
fn iniciar_edicion_comando_lsp_seleccionado(estado: &mut EstadoApp) {
    let Some(lenguaje) = lenguaje_seleccionado_en_lenguajes(&estado.panel_admin) else { return };
    let valor_inicial = lsp::comando_efectivo(lenguaje, &estado.config)
        .map(|(comando, argumentos, env)| ComandoLsp { comando, argumentos, env }.como_linea())
        .unwrap_or_default();
    estado.panel_admin.iniciar_edicion_comando_lsp(valor_inicial);
}

/// `Enter` mientras se edita el comando LSP de un lenguaje: guarda la
/// línea escrita (o no hace nada si quedó vacía — no tiene sentido un
/// comando en blanco) y cierra el modo de edición.
fn confirmar_edicion_comando_lsp(estado: &mut EstadoApp) {
    let Some(lenguaje) = lenguaje_seleccionado_en_lenguajes(&estado.panel_admin) else {
        estado.panel_admin.cancelar_edicion_comando_lsp();
        return;
    };
    if let Some(linea) = estado.panel_admin.confirmar_edicion_comando_lsp() {
        estado.capas_config.global.lenguajes.fijar_comando_desde_linea(lenguaje.id(), &linea);
        guardar_config_global(estado);
    }
}

/// `Backspace` sobre una fila de "Lenguajes / LSP" (fuera del modo de
/// edición): quita el comando personalizado de ese lenguaje, si tenía
/// uno — vuelve a usar el que trae `tcode_lsp::comando_para` por defecto.
fn quitar_comando_lsp_seleccionado(estado: &mut EstadoApp) {
    let Some(lenguaje) = lenguaje_seleccionado_en_lenguajes(&estado.panel_admin) else { return };
    estado.capas_config.global.lenguajes.quitar_comando(lenguaje.id());
    guardar_config_global(estado);
}

/// El comando de `tcode_commands::comandos_disponibles()` que corresponde
/// a la fila seleccionada de la sección "Atajos" del panel de
/// administración — `None` si la fila 0 (la acción especial "restablecer
/// todos") está seleccionada, o si la sección actual no es "Atajos".
/// La sección "Atajos" tiene 3 filas especiales antes de la lista de
/// comandos (ver `filas_atajos` en `tcode-ui`): restablecer todos,
/// exportar, importar.
const FILAS_ESPECIALES_ATAJOS: usize = 3;

fn comando_seleccionado_en_atajos(panel: &tcode_config::EstadoPanelAdmin) -> Option<&'static str> {
    if panel.seccion_actual() != Seccion::Atajos || panel.campo() < FILAS_ESPECIALES_ATAJOS {
        return None;
    }
    tcode_commands::comandos_disponibles().get(panel.campo() - FILAS_ESPECIALES_ATAJOS).map(|c| c.id)
}

/// `Enter` sobre una de las 3 filas especiales de "Atajos" (PLAN.md
/// §5.1) — restablecer todos, exportar, importar — o sobre cualquier
/// otra fila, que entra en modo captura para reasignar ESE comando en
/// particular.
fn ejecutar_fila_atajos(estado: &mut EstadoApp, resolvedor: &mut Resolvedor) {
    match estado.panel_admin.campo() {
        0 => {
            let _ = tcode_keymap::eliminar_override_usuario();
            estado.keymap = tcode_keymap::keymap_por_defecto();
            resolvedor.reemplazar_keymap(estado.keymap.clone());
            estado.panel_admin.establecer_mensaje("Todos los atajos vuelven a su valor por defecto".to_string());
        }
        1 => match estado.keymap.exportar() {
            Ok(ruta) => estado.panel_admin.establecer_mensaje(format!("Exportado a {}", ruta.display())),
            Err(e) => estado.panel_admin.establecer_mensaje(format!("No se pudo exportar: {e}")),
        },
        2 => importar_atajos(estado, resolvedor),
        _ => estado.panel_admin.iniciar_captura(),
    }
}

/// Fila "Importar atajos desde archivo" (PLAN.md §5.1): busca el
/// archivo fijo de `tcode_keymap::ruta_keymap_a_importar()` — mismo
/// espíritu que "poner un archivo en la carpeta de temas" para
/// importar un tema (PLAN.md §7) — y, si está, lo adopta como keymap
/// activo en caliente. Si no hay ningún archivo esperando, avisa dónde
/// tiene que dejarse en vez de fallar en silencio.
fn importar_atajos(estado: &mut EstadoApp, resolvedor: &mut Resolvedor) {
    let mensaje = match tcode_keymap::importar_keymap() {
        Ok(tcode_keymap::ResultadoImportarKeymap::Importado { ruta, keymap }) => {
            estado.keymap = keymap;
            resolvedor.reemplazar_keymap(estado.keymap.clone());
            format!("Importado desde {}", ruta.display())
        }
        Ok(tcode_keymap::ResultadoImportarKeymap::NoHabiaArchivo(ruta)) => {
            format!("No hay nada para importar — dejá el archivo en {}", ruta.display())
        }
        Err(e) => format!("No se pudo importar: {e}"),
    };
    estado.panel_admin.establecer_mensaje(mensaje);
}

/// `Backspace` sobre un comando de "Atajos": lo restablece a lo que ese
/// comando tiene en el keymap por defecto (PLAN.md §5: "Botón
/// 'Restablecer valor por defecto' por atajo"), sin tocar el resto de
/// las personalizaciones. Sobre cualquiera de las 3 filas especiales
/// (restablecer todos / exportar / importar) no hace nada — cada una ya
/// tiene su propio gesto con `Enter`.
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

/// Rangos plegables del documento del panel activo, como `Pliegue`s del
/// `core` (ver `Resaltador::rangos_plegables`). La clave del documento es
/// la misma ruta que usa `vista_codigo` al resaltar, así se reutiliza su
/// árbol ya parseado.
fn rangos_plegables_del_activo(layout: &PanelLayout, resaltador: &mut Resaltador) -> Vec<Pliegue> {
    let panel = layout.panel_activo();
    let ruta = &panel.ruta_mostrada;
    let fuente = panel.editor.buffer().a_texto();
    resaltador
        .rangos_plegables(ruta, Lenguaje::detectar_por_extension(ruta), &fuente)
        .into_iter()
        .map(|r| Pliegue { inicio: r.inicio, fin: r.fin })
        .collect()
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

#[cfg(test)]
mod tests_ruta_en_path {
    use super::*;

    /// Directorio temporal único por test, borrado al final — mismo
    /// patrón que `tcode_core::tests::guardar_y_recargar_archivo`.
    struct DirTemporal(std::path::PathBuf);

    impl DirTemporal {
        fn nuevo(nombre: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("tcode-test-ruta-en-path-{nombre}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn crear_archivo(&self, nombre: &str) {
            std::fs::write(self.0.join(nombre), "").unwrap();
        }
    }

    impl Drop for DirTemporal {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    #[test]
    fn sin_extensiones_solo_encuentra_el_nombre_exacto() {
        let dir = DirTemporal::nuevo("sin-ext");
        dir.crear_archivo("clangd");
        assert!(existe_ejecutable(&dir.0, "clangd", None));
        assert!(!existe_ejecutable(&dir.0, "clangd.exe", None));
        assert!(!existe_ejecutable(&dir.0, "no-existe", None));
    }

    #[test]
    fn con_extensiones_encuentra_el_nombre_pelado_mas_cualquier_extension() {
        let dir = DirTemporal::nuevo("con-ext");
        dir.crear_archivo("pyright-langserver.cmd");
        // El caso real que motiva esta pieza: en Windows casi nunca se
        // invoca (ni se busca) un ejecutable con su extensión puesta.
        assert!(existe_ejecutable(&dir.0, "pyright-langserver", Some(".COM;.EXE;.BAT;.CMD")));
    }

    #[test]
    fn con_extensiones_sigue_encontrando_el_nombre_exacto_sin_extension() {
        // Un ejecutable sin extensión (poco común en Windows, pero
        // válido) no debería dejar de encontrarse solo porque se pasó
        // una lista de extensiones para probar además.
        let dir = DirTemporal::nuevo("exacto-con-lista");
        dir.crear_archivo("script");
        assert!(existe_ejecutable(&dir.0, "script", Some(".COM;.EXE;.BAT;.CMD")));
    }

    #[test]
    fn no_encuentra_nada_si_ninguna_extension_matchea() {
        let dir = DirTemporal::nuevo("ninguna-matchea");
        dir.crear_archivo("otracosa.dll");
        assert!(!existe_ejecutable(&dir.0, "comando", Some(".COM;.EXE;.BAT;.CMD")));
    }

    #[test]
    fn lista_de_extensiones_vacia_o_con_segmentos_vacios_no_hace_fallar_nada() {
        let dir = DirTemporal::nuevo("lista-rara");
        dir.crear_archivo("comando.exe");
        assert!(!existe_ejecutable(&dir.0, "comando", Some("")));
        assert!(existe_ejecutable(&dir.0, "comando", Some(";;.EXE;;")));
    }
}

#[cfg(test)]
mod tests_autoguardado {
    use super::*;

    /// Directorio temporal único por test, borrado al final — mismo
    /// patrón que `tests_ruta_en_path`.
    struct DirTemporal(std::path::PathBuf);

    impl DirTemporal {
        fn nuevo(nombre: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("tcode-test-autoguardado-{nombre}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn archivo(&self, nombre: &str, contenido: &str) -> std::path::PathBuf {
            let ruta = self.0.join(nombre);
            std::fs::write(&ruta, contenido).unwrap();
            ruta
        }
    }

    impl Drop for DirTemporal {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    fn layout_con(ruta: &std::path::Path) -> PanelLayout {
        PanelLayout::nuevo(Editor::abrir(ruta).unwrap(), ruta.display().to_string())
    }

    fn con_guardado_al_perder_foco() -> ConfigEditor {
        ConfigEditor { guardado_automatico: GuardadoAutomatico::AlPerderFoco, ..ConfigEditor::default() }
    }

    #[test]
    fn autoguardar_guarda_los_modificados_con_ruta_y_no_toca_los_sin_nombre() {
        let dir = DirTemporal::nuevo("basico");
        let ruta = dir.archivo("a.txt", "hola");
        let mut layout = layout_con(&ruta);
        layout.editor_activo_mut().insertar_char('X');
        // Panel nuevo por `Ctrl+\`: "[Sin nombre]", también modificado.
        layout.dividir(DireccionSplit::Vertical);
        layout.editor_activo_mut().insertar_char('Y');

        assert!(autoguardar(&mut layout));
        assert_eq!(std::fs::read_to_string(&ruta).unwrap(), "Xhola");
        let paneles = layout.paneles_mut();
        assert!(!paneles[0].editor.buffer().modificado());
        assert!(paneles[1].editor.buffer().modificado(), "sin ruta no se guarda en ningún lado");
    }

    #[test]
    fn autoguardar_sin_nada_modificado_no_hace_nada() {
        let dir = DirTemporal::nuevo("nada");
        let ruta = dir.archivo("a.txt", "hola");
        let mut layout = layout_con(&ruta);
        assert!(!autoguardar(&mut layout));
    }

    #[test]
    fn un_guardado_fallido_queda_en_el_aviso_sin_perder_los_cambios() {
        let dir = DirTemporal::nuevo("falla");
        let sub = dir.0.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        let ruta = sub.join("a.txt");
        std::fs::write(&ruta, "hola").unwrap();
        let mut layout = layout_con(&ruta);
        layout.editor_activo_mut().insertar_char('X');
        std::fs::remove_dir_all(&sub).unwrap(); // guardar va a fallar

        autoguardar(&mut layout);
        assert!(layout.panel_activo().aviso_guardado.is_some());
        assert!(layout.editor_activo().buffer().modificado());

        // Si la carpeta vuelve, el próximo intento funciona y limpia el aviso.
        std::fs::create_dir_all(&sub).unwrap();
        autoguardar(&mut layout);
        assert!(layout.panel_activo().aviso_guardado.is_none());
        assert_eq!(std::fs::read_to_string(&ruta).unwrap(), "Xhola");
    }

    #[test]
    fn abrir_otro_archivo_al_perder_foco_guarda_antes_de_reemplazar() {
        let dir = DirTemporal::nuevo("abrir");
        let a = dir.archivo("a.txt", "a");
        let b = dir.archivo("b.txt", "b");
        let mut layout = layout_con(&a);
        layout.editor_activo_mut().insertar_char('X');
        let mut foco = Foco::Editor;

        abrir_ruta_desde_explorador(&mut layout, &mut foco, b.clone(), &con_guardado_al_perder_foco());
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "Xa");
        assert_eq!(layout.panel_activo().ruta_mostrada, b.display().to_string());
    }

    #[test]
    fn abrir_otro_archivo_con_guardado_nunca_no_escribe_nada() {
        let dir = DirTemporal::nuevo("nunca");
        let a = dir.archivo("a.txt", "a");
        let b = dir.archivo("b.txt", "b");
        let mut layout = layout_con(&a);
        layout.editor_activo_mut().insertar_char('X');
        let mut foco = Foco::Editor;

        abrir_ruta_desde_explorador(&mut layout, &mut foco, b, &ConfigEditor::default());
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "a");
    }

    #[test]
    fn si_el_guardado_previo_falla_no_se_abre_el_otro_archivo() {
        let dir = DirTemporal::nuevo("no-abre");
        let sub = dir.0.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        let a = sub.join("a.txt");
        std::fs::write(&a, "a").unwrap();
        let b = dir.archivo("b.txt", "b");
        let mut layout = layout_con(&a);
        layout.editor_activo_mut().insertar_char('X');
        std::fs::remove_dir_all(&sub).unwrap();
        let mut foco = Foco::Editor;

        abrir_ruta_desde_explorador(&mut layout, &mut foco, b, &con_guardado_al_perder_foco());
        assert_eq!(layout.panel_activo().ruta_mostrada, a.display().to_string(), "no se reemplazó el documento");
        assert!(layout.editor_activo().buffer().modificado());
        assert!(layout.panel_activo().aviso_guardado.is_some());
    }
}
