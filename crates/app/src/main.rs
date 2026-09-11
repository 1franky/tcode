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
use tcode_config::Config;
use tcode_core::{CampoBusqueda, Editor, EstadoBusqueda};
use tcode_fs::{BuscadorArchivos, Explorador};
use tcode_keymap::{Keymap, Resolucion, Resolvedor};
use tcode_syntax::Resaltador;
use tcode_ui::{DireccionSplit, Layout as PanelLayout, Paleta};

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
    let resultado = ejecutar(&mut terminal, &mut layout, config, &keymap, explorador, ruta_arg.as_deref()).await;
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
    lsp: lsp::EstadoLsp,
}

fn sin_modificadores(key: KeyEvent) -> bool {
    !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT)
}

async fn ejecutar(
    terminal: &mut Terminal<Backend>,
    layout: &mut PanelLayout,
    config: Config,
    keymap: &Keymap,
    explorador: Explorador,
    ruta_arg: Option<&str>,
) -> Result<()> {
    let mut resaltador = Resaltador::nuevo();
    let mut resolvedor = Resolvedor::nuevo(keymap);
    let mut eventos = EventStream::new();

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
        lsp: lsp::EstadoLsp::nuevo(),
    };

    // El archivo abierto al arrancar también dispara el LSP si su
    // lenguaje tiene uno configurado.
    sincronizar_lsp(layout, &mut estado.lsp).await;

    // Ver `forzar_redibujado_completo`: en Windows, si la "forma" de la
    // pantalla cambió (otro archivo activo, otro número de paneles, el
    // explorador se mostró/ocultó), se limpia antes del próximo draw.
    let mut necesita_redibujado = false;

    loop {
        if necesita_redibujado {
            forzar_redibujado_completo(terminal)?;
            necesita_redibujado = false;
        }

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
                        if let Accion::Salir = procesar_comando(id, layout, &mut estado) {
                            break;
                        }
                    }
                }
                KeyCode::Char(c) if sin_modificadores(key) => estado.paleta_comandos.escribir(c),
                _ => {}
            }
            sincronizar_lsp(layout, &mut estado.lsp).await;
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
            sincronizar_lsp(layout, &mut estado.lsp).await;
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
            sincronizar_lsp(layout, &mut estado.lsp).await;
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
                if let Accion::Salir = procesar_comando(&nombre, layout, &mut estado) {
                    break;
                }
            }
            Resolucion::Pendiente | Resolucion::Cancelado => {}
            Resolucion::SinCoincidencia => {
                // Ninguna tecla/chord configurado coincide: si es un
                // carácter imprimible sin Ctrl/Alt Y el foco está en el
                // editor, se inserta como texto normal (escribir no pasa
                // por el sistema de atajos; el explorador no recibe texto).
                if estado.foco == Foco::Editor {
                    if let KeyCode::Char(c) = key.code {
                        if sin_modificadores(key) {
                            layout.editor_activo_mut().insertar_char(c);
                        }
                    }
                }
            }
        }

        sincronizar_lsp(layout, &mut estado.lsp).await;
        necesita_redibujado |= firma_estructural(layout, &estado.explorador) != firma_antes;
    }

    estado.lsp.cerrar().await;
    Ok(())
}

/// Le avisa al `EstadoLsp` cuál es el archivo/contenido activos ahora
/// mismo: relanza el cliente si cambió el lenguaje, y notifica
/// `didChange` si el texto cambió desde el último envío.
async fn sincronizar_lsp(layout: &PanelLayout, lsp: &mut lsp::EstadoLsp) {
    let panel = layout.panel_activo();
    let contenido = panel.editor.buffer().a_texto();
    lsp.actualizar_para_archivo(&panel.ruta_mostrada, &contenido).await;
    lsp.sincronizar_contenido(&contenido).await;
}

/// Punto de entrada único para ejecutar un id de comando, venga de un
/// atajo de teclado o de confirmar un resultado en la paleta de comandos.
/// `config.recargar`, `paleta.comandos` y `buscar.archivos` necesitan
/// estado que no le corresponde a `ejecutar_comando` (la paleta de
/// colores, los propios overlays), así que se interceptan aquí antes de
/// delegar.
fn procesar_comando(id: &str, layout: &mut PanelLayout, estado: &mut EstadoApp) -> Accion {
    match id {
        "config.recargar" => {
            recargar_config_y_tema(&mut estado.config, &mut estado.paleta);
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
        _ => {}
    }

    if *foco == Foco::Explorador {
        return ejecutar_comando_explorador(comando, layout, explorador, foco);
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

/// `config.recargar` (`Ctrl+K Ctrl+L`): recarga `config.toml` desde disco
/// sin reiniciar el editor (PLAN.md §4) y reconstruye la paleta de colores
/// si el tema activo cambió.
fn recargar_config_y_tema(config: &mut Config, paleta: &mut Paleta) {
    if let Ok(nueva) = tcode_config::cargar() {
        let tema_cambio = nueva.interfaz.tema != config.interfaz.tema;
        *config = nueva;
        if tema_cambio {
            *paleta = cargar_paleta(&config.interfaz.tema);
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
