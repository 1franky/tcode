//! Soporte de mouse (BACKLOG.md P0 #18): clic, arrastre, doble clic y
//! rueda sobre el código, las pestañas, el explorador, la tabla CSV y
//! las listas de los overlays.
//!
//! La terminal solo dice columna y fila; qué hay ahí lo dicen las
//! [`ZonasMouse`] que anotó la UI en el último frame (`EstadoApp::zonas`,
//! ver `tcode_ui::zonas`). Las prioridades son las mismas que las del
//! teclado en `ejecutar`: mientras un overlay modal está abierto, el mouse
//! solo actúa sobre él (un clic afuera lo cierra, como `Esc`); las vistas
//! a pantalla completa (panel de administración, editor de tema) y los
//! prompts de texto lo ignoran — un clic perdido no tiene que borrar lo
//! que se estaba escribiendo.
//!
//! Todo lo que cambia texto o cursor pasa por los métodos públicos del
//! `Editor` (`fijar_seleccion`, `seleccionar_palabra_en`...), igual que
//! las teclas.

use std::time::{Duration, Instant};

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use tcode_core::{Cursor, Modo};
use tcode_keymap::Resolvedor;
use tcode_ui::{contiene, Layout as PanelLayout, ZonaOverlay, ZonaPanel};

use crate::{funciones_lsp, procesar_comando, Accion, EstadoApp, Foco};

/// Filas que desplaza cada evento de rueda en el código, el explorador y
/// la tabla CSV (lo habitual en los editores; la mayoría de las
/// terminales manda un evento por "muesca"). En las listas de los
/// overlays la rueda mueve la selección de a uno: son cortas y así se
/// puede elegir con precisión antes de hacer clic.
const FILAS_POR_RUEDA: usize = 3;

/// Dos clics en la misma celda dentro de este intervalo son un doble clic
/// (tres, un triple clic).
const INTERVALO_DOBLE_CLIC: Duration = Duration::from_millis(400);

/// Estado del mouse entre eventos: el último clic (para contar dobles y
/// triples) y el arrastre en curso.
#[derive(Default)]
pub struct EstadoMouse {
    ultimo_clic: Option<(Instant, u16, u16, u8)>,
    arrastre: Option<Arrastre>,
}

/// Un arrastre con el botón izquierdo que empezó en el código del panel
/// `panel`: selecciona desde `ancla` hasta donde esté el mouse.
#[derive(Clone, Copy)]
struct Arrastre {
    panel: usize,
    ancla: Cursor,
}

/// Lleva la selección de una lista de `actual` a `destino` con sus
/// propios `mover_arriba`/`mover_abajo` (las listas no exponen "elegir el
/// ítem N"): a lo sumo el alto de la pantalla de pasos, porque el destino
/// es una fila visible.
pub fn llevar_seleccion<T>(lista: &mut T, actual: usize, destino: usize, arriba: fn(&mut T), abajo: fn(&mut T)) {
    for _ in destino..actual {
        arriba(lista);
    }
    for _ in actual..destino {
        abajo(lista);
    }
}

/// Qué pasó con un evento de mouse.
pub enum Resultado {
    /// No cambió nada: no hace falta redibujar (movimientos sin botón,
    /// clics en lugares sin efecto).
    Nada,
    Cambio,
    Salir,
}

/// Procesa un evento de mouse. Solo con `interfaz.usar_mouse` prendido
/// (apagado, la captura también se apaga, ver `ejecutar`).
pub fn manejar(
    evento: MouseEvent,
    layout: &mut PanelLayout,
    estado: &mut EstadoApp,
    resolvedor: &mut Resolvedor,
) -> Resultado {
    if !estado.config.interfaz.usar_mouse {
        return Resultado::Nada;
    }
    let (x, y) = (evento.column, evento.row);
    let cuenta = match evento.kind {
        MouseEventKind::Down(boton) => {
            // Un clic cuenta como una "tecla" para lo que dura hasta la
            // siguiente: el aviso de la barra de estado y el cierre armado
            // de `Ctrl+W` (ver `EstadoApp::cierre_pedido`).
            layout.panel_activo_mut().mensaje_estado = None;
            estado.cierre_armado = estado.cierre_pedido.take();
            estado.cambio_git = None;
            estado.confirmar_salida = false;
            contar_clic(&mut estado.mouse, boton, x, y)
        }
        MouseEventKind::Up(_) => {
            estado.mouse.arrastre = None;
            return Resultado::Nada;
        }
        MouseEventKind::Moved => return Resultado::Nada,
        _ => 0,
    };

    // Vistas a pantalla completa y prompts de texto: el mouse no hace nada.
    if estado.editor_tema.activo()
        || estado.panel_admin.activo()
        || estado.guardar_como.activa()
        || estado.prompt_explorador.activo()
        || estado.funciones_lsp.renombrar.is_some()
        || estado.vim.linea_comando.activa()
        || estado.ir_a_linea.activo()
        || estado.recuperacion.is_some()
        || layout.panel_activo().estado_csv.editando()
        || layout.panel_activo().estado_csv.prompt_filtro().is_some()
    {
        return Resultado::Nada;
    }
    // La confirmación de borrado se cancela con cualquier tecla: también
    // con cualquier clic.
    if estado.confirmar_borrado.activo() {
        if cuenta > 0 {
            estado.confirmar_borrado.cerrar();
            return Resultado::Cambio;
        }
        return Resultado::Nada;
    }

    if let Some(lista) = lista_modal(estado) {
        return manejar_lista_modal(lista, evento, cuenta, layout, estado, resolvedor);
    }

    // La barra de `Ctrl+F` no es modal para el mouse: un clic afuera la
    // cierra (como `Esc`) y sigue de largo — así el cursor que se ubica
    // con ese clic se ve.
    if estado.estado_busqueda.activa() && cuenta > 0 {
        match &estado.zonas.overlay {
            Some(zona) if contiene(zona.area, x, y) => return Resultado::Nada,
            _ => estado.estado_busqueda.cerrar(),
        }
    }
    if estado.explorador.modo_salto() && cuenta > 0 {
        estado.explorador.salir_modo_salto();
    }

    // Popups del LSP pegados al cursor: clic en un ítem del completado lo
    // acepta, la rueda encima lo recorre; cualquier otro clic los cierra y
    // sigue de largo.
    if let Some(popup) = estado.zonas.popup.clone() {
        if contiene(popup.area, x, y) {
            return manejar_popup(&popup, evento, cuenta, layout, estado);
        }
        if cuenta > 0 || es_rueda(evento.kind) {
            funciones_lsp::cerrar_popups(estado);
        }
    }

    // Terminal integrada (BACKLOG.md P3 #26), con su fila de título: un
    // clic le da el foco, la rueda recorre el historial. Los clics no se
    // le mandan a la shell.
    if let Some(area) = estado.zonas.terminal {
        let con_titulo = ratatui::layout::Rect { y: area.y.saturating_sub(1), height: area.height + 1, ..area };
        if contiene(con_titulo, x, y) {
            return match evento.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    estado.foco = Foco::Terminal;
                    Resultado::Cambio
                }
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                    let filas = FILAS_POR_RUEDA as isize;
                    let delta = if evento.kind == MouseEventKind::ScrollUp { filas } else { -filas };
                    if let Some(sesion) = &mut estado.terminal.sesion {
                        sesion.desplazar_historial(delta);
                    }
                    Resultado::Cambio
                }
                _ => Resultado::Nada,
            };
        }
    }

    match evento.kind {
        MouseEventKind::Down(MouseButton::Left) => clic_izquierdo(x, y, evento.modifiers, cuenta, layout, estado),
        MouseEventKind::Down(MouseButton::Middle) => clic_medio(x, y, layout, estado, resolvedor),
        MouseEventKind::Drag(MouseButton::Left) => arrastrar(x, y, layout, estado),
        MouseEventKind::ScrollUp
        | MouseEventKind::ScrollDown
        | MouseEventKind::ScrollLeft
        | MouseEventKind::ScrollRight => rueda(evento, layout, estado),
        _ => Resultado::Nada,
    }
}

fn es_rueda(kind: MouseEventKind) -> bool {
    matches!(
        kind,
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown | MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight
    )
}

/// Cuántos clics seguidos lleva este (1, 2 = doble, 3 = triple): mismo
/// botón izquierdo, misma celda, dentro de [`INTERVALO_DOBLE_CLIC`].
fn contar_clic(mouse: &mut EstadoMouse, boton: MouseButton, x: u16, y: u16) -> u8 {
    if boton != MouseButton::Left {
        mouse.ultimo_clic = None;
        return 1;
    }
    let ahora = Instant::now();
    let cuenta = match mouse.ultimo_clic {
        Some((cuando, cx, cy, n)) if cx == x && cy == y && ahora.duration_since(cuando) <= INTERVALO_DOBLE_CLIC => {
            n % 3 + 1
        }
        _ => 1,
    };
    mouse.ultimo_clic = Some((ahora, x, y, cuenta));
    cuenta
}

/// Los overlays de lista modales, en el mismo orden de prioridad que el
/// teclado en `ejecutar`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ListaModal {
    Paleta,
    Simbolos,
    BusquedaProyecto,
    Buscador,
    SelectorTema,
    Logs,
    UbicacionesLsp,
}

fn lista_modal(estado: &EstadoApp) -> Option<ListaModal> {
    Some(if estado.paleta_comandos.activa() {
        ListaModal::Paleta
    } else if estado.selector_simbolos.activo() {
        ListaModal::Simbolos
    } else if estado.busqueda_proyecto.activo() {
        ListaModal::BusquedaProyecto
    } else if estado.buscador_archivos.activo() {
        ListaModal::Buscador
    } else if estado.selector_tema.activa() {
        ListaModal::SelectorTema
    } else if estado.logs_lsp.activo() {
        ListaModal::Logs
    } else if estado.funciones_lsp.lista.activo() {
        ListaModal::UbicacionesLsp
    } else {
        return None;
    })
}

/// Clic en un ítem: lo elige y confirma (como `Enter`). Clic afuera del
/// recuadro: cierra (como `Esc`). Rueda: mueve la selección de a uno.
fn manejar_lista_modal(
    lista: ListaModal,
    evento: MouseEvent,
    cuenta: u8,
    layout: &mut PanelLayout,
    estado: &mut EstadoApp,
    resolvedor: &mut Resolvedor,
) -> Resultado {
    let (x, y) = (evento.column, evento.row);
    let zona = estado.zonas.overlay.clone().unwrap_or_default();
    if lista == ListaModal::BusquedaProyecto {
        estado.busqueda_proyecto.nueva_tecla();
        if estado.busqueda_proyecto.confirmando() && cuenta > 0 {
            estado.busqueda_proyecto.cancelar_confirmacion();
            return Resultado::Cambio;
        }
    }
    match evento.kind {
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            mover_lista(lista, evento.kind == MouseEventKind::ScrollDown, estado);
            Resultado::Cambio
        }
        MouseEventKind::Down(MouseButton::Left) if !contiene(zona.area, x, y) => {
            cerrar_lista(lista, estado);
            Resultado::Cambio
        }
        MouseEventKind::Down(MouseButton::Left) => {
            let Some(indice) = zona.lista.as_ref().and_then(|l| l.item_en(x, y)) else { return Resultado::Nada };
            elegir_en_lista(lista, indice, layout, estado, resolvedor)
        }
        _ => Resultado::Nada,
    }
}

fn mover_lista(lista: ListaModal, abajo: bool, estado: &mut EstadoApp) {
    match (lista, abajo) {
        (ListaModal::Paleta, false) => estado.paleta_comandos.mover_arriba(),
        (ListaModal::Paleta, true) => estado.paleta_comandos.mover_abajo(),
        (ListaModal::Simbolos, false) => estado.selector_simbolos.mover_arriba(),
        (ListaModal::Simbolos, true) => estado.selector_simbolos.mover_abajo(),
        (ListaModal::BusquedaProyecto, false) => estado.busqueda_proyecto.mover_arriba(1),
        (ListaModal::BusquedaProyecto, true) => estado.busqueda_proyecto.mover_abajo(1),
        (ListaModal::Buscador, false) => estado.buscador_archivos.mover_arriba(),
        (ListaModal::Buscador, true) => estado.buscador_archivos.mover_abajo(),
        (ListaModal::SelectorTema, abajo) => {
            if abajo {
                estado.selector_tema.mover_abajo();
            } else {
                estado.selector_tema.mover_arriba();
            }
            crate::aplicar_preview_tema(estado);
        }
        (ListaModal::Logs, false) => estado.logs_lsp.mover_arriba(),
        (ListaModal::Logs, true) => estado.logs_lsp.mover_abajo(),
        (ListaModal::UbicacionesLsp, false) => estado.funciones_lsp.lista.mover_arriba(),
        (ListaModal::UbicacionesLsp, true) => estado.funciones_lsp.lista.mover_abajo(),
    }
}

fn cerrar_lista(lista: ListaModal, estado: &mut EstadoApp) {
    match lista {
        ListaModal::Paleta => estado.paleta_comandos.cerrar(),
        ListaModal::Simbolos => estado.selector_simbolos.cerrar(),
        ListaModal::BusquedaProyecto => estado.busqueda_proyecto.cerrar(),
        ListaModal::Buscador => estado.buscador_archivos.cerrar(),
        ListaModal::SelectorTema => {
            // Como `Esc`: vuelve al tema que había antes del preview.
            let original = estado.selector_tema.tema_original().to_string();
            estado.selector_tema.cerrar();
            estado.paleta = crate::cargar_paleta(&original);
        }
        ListaModal::Logs => estado.logs_lsp.cerrar(),
        ListaModal::UbicacionesLsp => estado.funciones_lsp.lista.cerrar(),
    }
}

fn elegir_en_lista(
    lista: ListaModal,
    indice: usize,
    layout: &mut PanelLayout,
    estado: &mut EstadoApp,
    resolvedor: &mut Resolvedor,
) -> Resultado {
    match lista {
        ListaModal::Paleta => {
            let p = &mut estado.paleta_comandos;
            let actual = p.seleccion();
            llevar_seleccion(p, actual, indice, |p| p.mover_arriba(), |p| p.mover_abajo());
            if let Some(id) = estado.paleta_comandos.confirmar() {
                if let Accion::Salir = procesar_comando(id, layout, estado, resolvedor) {
                    return Resultado::Salir;
                }
            }
        }
        ListaModal::Simbolos => {
            let s = &mut estado.selector_simbolos;
            let actual = s.seleccion();
            llevar_seleccion(s, actual, indice, |s| s.mover_arriba(), |s| s.mover_abajo());
            if let Some(byte) = estado.selector_simbolos.confirmar() {
                layout.editor_activo_mut().mover_cursor_a_byte(byte);
            }
        }
        ListaModal::BusquedaProyecto => {
            let b = &mut estado.busqueda_proyecto;
            let actual = b.seleccion();
            if indice > actual {
                b.mover_abajo(indice - actual);
            } else {
                b.mover_arriba(actual - indice);
            }
            crate::abrir_resultado_proyecto(layout, estado);
        }
        ListaModal::Buscador => {
            let b = &mut estado.buscador_archivos;
            let actual = b.seleccion();
            llevar_seleccion(b, actual, indice, |b| b.mover_arriba(), |b| b.mover_abajo());
            if let Some(ruta) = estado.buscador_archivos.confirmar() {
                crate::abrir_ruta_desde_explorador(layout, &mut estado.foco, ruta, &estado.config.editor);
            }
        }
        ListaModal::SelectorTema => {
            let s = &mut estado.selector_tema;
            let actual = s.seleccion();
            llevar_seleccion(s, actual, indice, |s| s.mover_arriba(), |s| s.mover_abajo());
            if let Some(id) = estado.selector_tema.confirmar() {
                crate::confirmar_tema_seleccionado(estado, &id);
            }
        }
        // Las líneas de log no se "eligen": el clic no hace nada.
        ListaModal::Logs => return Resultado::Nada,
        ListaModal::UbicacionesLsp => funciones_lsp::clic_en_lista(indice, layout, estado),
    }
    Resultado::Cambio
}

/// Clic o rueda sobre el popup de completado/hover.
fn manejar_popup(
    popup: &ZonaOverlay,
    evento: MouseEvent,
    cuenta: u8,
    layout: &mut PanelLayout,
    estado: &mut EstadoApp,
) -> Resultado {
    let Some(lista) = &popup.lista else { return Resultado::Nada };
    match evento.kind {
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            funciones_lsp::rueda_en_completado(evento.kind == MouseEventKind::ScrollDown, estado);
            Resultado::Cambio
        }
        MouseEventKind::Down(MouseButton::Left) if cuenta > 0 => {
            match lista.item_en(evento.column, evento.row) {
                Some(indice) => {
                    funciones_lsp::clic_en_completado(indice, layout, estado);
                    Resultado::Cambio
                }
                None => Resultado::Nada,
            }
        }
        _ => Resultado::Nada,
    }
}

/// Clic izquierdo fuera de los overlays: explorador, pestañas, código o
/// tabla.
fn clic_izquierdo(
    x: u16,
    y: u16,
    modificadores: KeyModifiers,
    cuenta: u8,
    layout: &mut PanelLayout,
    estado: &mut EstadoApp,
) -> Resultado {
    estado.mouse.arrastre = None;
    if let Some(explorador) = &estado.zonas.explorador {
        if contiene(explorador.area, x, y) {
            let Some(indice) = explorador.item_en(x, y) else { return Resultado::Nada };
            return clic_en_explorador(indice, cuenta, layout, estado);
        }
    }
    let Some(zona) = estado.zonas.panel_en(x, y).cloned() else { return Resultado::Nada };
    layout.activar_panel(zona.indice);
    estado.foco = Foco::Editor;
    if let Some(pestana) = zona.pestana_en(x, y) {
        layout.ir_a_pestana(pestana);
        return Resultado::Cambio;
    }
    if let Some(tabla) = &zona.tabla {
        if let Some((fila, columna)) = tabla.celda_en(x, y) {
            let csv = &mut layout.panel_activo_mut().estado_csv;
            let actual = csv.fila();
            let num_filas = tabla.num_filas;
            for _ in fila..actual {
                csv.mover_arriba();
            }
            for _ in actual..fila {
                csv.mover_abajo(num_filas);
            }
            let actual = csv.columna();
            for _ in columna..actual {
                csv.mover_izquierda();
            }
            for _ in actual..columna {
                csv.mover_derecha(usize::MAX);
            }
        }
        return Resultado::Cambio;
    }
    if let Some(codigo) = &zona.codigo {
        let en_codigo = contiene(codigo.texto, x, y) || codigo.gutter.is_some_and(|g| contiene(g, x, y));
        if en_codigo {
            clic_en_codigo(&zona, x, y, modificadores, cuenta, layout, estado);
        }
    }
    Resultado::Cambio
}

/// Clic en una fila del explorador: la selecciona y, como en VSCode, la
/// abre con un solo clic (un archivo se abre y pasa el foco al editor; una
/// carpeta se expande o colapsa). El segundo clic de un doble clic no hace
/// nada — si no, un doble clic en una carpeta la abriría y la volvería a
/// cerrar.
fn clic_en_explorador(indice: usize, cuenta: u8, layout: &mut PanelLayout, estado: &mut EstadoApp) -> Resultado {
    estado.foco = Foco::Explorador;
    let explorador = &mut estado.explorador;
    let actual = explorador.seleccion();
    llevar_seleccion(explorador, actual, indice, |e| e.mover_arriba(), |e| e.mover_abajo());
    if cuenta == 1 {
        if let Ok(Some(ruta)) = estado.explorador.activar_seleccion() {
            crate::abrir_ruta_desde_explorador(layout, &mut estado.foco, ruta, &estado.config.editor);
        }
    }
    Resultado::Cambio
}

/// Si el modo VIM está en juego para el editor activo (prendido en la
/// config y fuera de Insertar).
fn en_modo_vim(layout: &PanelLayout, estado: &EstadoApp) -> bool {
    estado.config.editor.modo_vim && layout.editor_activo().modo() != Modo::Insertar
}

/// Clic en el código (o su gutter) del panel `zona`, ya activo.
fn clic_en_codigo(
    zona: &ZonaPanel,
    x: u16,
    y: u16,
    modificadores: KeyModifiers,
    cuenta: u8,
    layout: &mut PanelLayout,
    estado: &mut EstadoApp,
) {
    let Some(pos) = layout.posicion_en_codigo(zona, x, y) else { return };
    funciones_lsp::cerrar_popups(estado);
    let destino = Cursor { linea: pos.linea, columna: pos.columna };
    let vim = en_modo_vim(layout, estado);
    let editor = layout.editor_activo_mut();

    // El marcador ` ... ` de un bloque plegado lo despliega.
    if pos.sobre_pliegue {
        editor.fijar_seleccion(destino, destino);
        editor.desplegar_en_cursor();
        return;
    }

    if vim {
        // Modo VIM: el clic solo mueve el cursor (saliendo de Visual si
        // hacía falta); doble clic y `Shift+clic` no seleccionan — la
        // selección de VIM es la de Visual, que se arma arrastrando.
        if editor.modo() != Modo::Normal {
            tcode_core::vim::cancelar(editor, &mut estado.vim);
        }
        editor.fijar_seleccion(destino, destino);
        editor.entrar_modo_normal();
        let ancla = editor.cursor();
        estado.mouse.arrastre = Some(Arrastre { panel: zona.indice, ancla });
        return;
    }

    match cuenta {
        2 if !pos.en_gutter => {
            editor.seleccionar_palabra_en(destino);
        }
        3 => {
            // Triple clic: la línea entera, con su salto (como VSCode).
            let linea = destino.linea;
            let fin = if linea + 1 < editor.buffer().num_lineas() {
                Cursor { linea: linea + 1, columna: 0 }
            } else {
                Cursor { linea, columna: editor.buffer().longitud_visible_linea(linea) }
            };
            editor.fijar_seleccion(Cursor { linea, columna: 0 }, fin);
        }
        _ => {
            let ancla = if modificadores.contains(KeyModifiers::SHIFT) { editor.cursores()[0].ancla } else { destino };
            editor.fijar_seleccion(ancla, destino);
            estado.mouse.arrastre = Some(Arrastre { panel: zona.indice, ancla });
        }
    }
}

/// Arrastre con el botón izquierdo: extiende la selección desde donde
/// empezó hasta el mouse. Si el mouse se pasa por arriba o por abajo del
/// código, la vista se corre una fila por evento (la terminal solo manda
/// eventos mientras el mouse se mueve). En modo VIM, arrastrar desde
/// Normal entra a Visual.
fn arrastrar(x: u16, y: u16, layout: &mut PanelLayout, estado: &mut EstadoApp) -> Resultado {
    let Some(arrastre) = estado.mouse.arrastre else { return Resultado::Nada };
    let Some(zona) = estado.zonas.paneles.iter().find(|z| z.indice == arrastre.panel).cloned() else {
        return Resultado::Nada;
    };
    let Some(codigo) = zona.codigo else { return Resultado::Nada };
    if layout.indice_activo() != arrastre.panel {
        return Resultado::Nada;
    }
    if y < codigo.texto.y {
        layout.desplazar_vista(&zona, -1);
    } else if y >= codigo.texto.y + codigo.texto.height {
        layout.desplazar_vista(&zona, 1);
    }
    let Some(pos) = layout.posicion_en_codigo(&zona, x, y) else { return Resultado::Nada };
    let destino = Cursor { linea: pos.linea, columna: pos.columna };
    let vim = en_modo_vim(layout, estado);
    let editor = layout.editor_activo_mut();
    if vim {
        if editor.modo() == Modo::Normal {
            if destino == arrastre.ancla {
                return Resultado::Nada;
            }
            estado.vim.ancla_visual = Some(arrastre.ancla);
            editor.entrar_modo_visual(false);
        }
        editor.fijar_seleccion(destino, destino);
        tcode_core::vim::refrescar_visual(editor, &mut estado.vim);
    } else {
        editor.fijar_seleccion(arrastre.ancla, destino);
    }
    Resultado::Cambio
}

/// Clic medio en una pestaña: la cierra, con la misma confirmación que
/// `Ctrl+W` si tiene cambios sin guardar (un segundo clic medio, o
/// `Ctrl+W`, confirma).
fn clic_medio(x: u16, y: u16, layout: &mut PanelLayout, estado: &mut EstadoApp, resolvedor: &mut Resolvedor) -> Resultado {
    let Some(zona) = estado.zonas.panel_en(x, y).cloned() else { return Resultado::Nada };
    let Some(pestana) = zona.pestana_en(x, y) else { return Resultado::Nada };
    layout.activar_panel(zona.indice);
    layout.ir_a_pestana(pestana);
    estado.foco = Foco::Editor;
    match procesar_comando("pestana.cerrar", layout, estado, resolvedor) {
        Accion::Salir => Resultado::Salir,
        Accion::Continuar => Resultado::Cambio,
    }
}

/// Rueda sobre el explorador o un panel (sin activarlo, como en VSCode):
/// el código se desplaza sin mover el cursor; en el explorador y la tabla
/// CSV se mueve la selección. `Shift+rueda` (o la rueda horizontal) solo
/// hace algo en la tabla: cambia de columna — la vista de código no tiene
/// scroll horizontal (sin ajuste de línea, las líneas largas se cortan).
fn rueda(evento: MouseEvent, layout: &mut PanelLayout, estado: &mut EstadoApp) -> Resultado {
    let (x, y) = (evento.column, evento.row);
    let shift = evento.modifiers.contains(KeyModifiers::SHIFT);
    let (horizontal, adelante) = match evento.kind {
        MouseEventKind::ScrollUp => (shift, false),
        MouseEventKind::ScrollDown => (shift, true),
        MouseEventKind::ScrollLeft => (true, false),
        _ => (true, true),
    };

    if let Some(explorador) = &estado.zonas.explorador {
        if contiene(explorador.area, x, y) {
            if horizontal {
                return Resultado::Nada;
            }
            for _ in 0..FILAS_POR_RUEDA {
                if adelante {
                    estado.explorador.mover_abajo();
                } else {
                    estado.explorador.mover_arriba();
                }
            }
            return Resultado::Cambio;
        }
    }

    let Some(zona) = estado.zonas.panel_en(x, y).cloned() else { return Resultado::Nada };
    if !contiene(zona.contenido, x, y) {
        return Resultado::Nada;
    }
    if let Some(tabla) = &zona.tabla {
        // La tabla no tiene scroll propio: la ventana sigue a la celda
        // seleccionada, así que se mueve la selección (y eso sí necesita
        // el panel activo).
        layout.activar_panel(zona.indice);
        let csv = &mut layout.panel_activo_mut().estado_csv;
        for _ in 0..FILAS_POR_RUEDA {
            match (horizontal, adelante) {
                (false, true) => csv.mover_abajo(tabla.num_filas),
                (false, false) => csv.mover_arriba(),
                (true, true) => csv.mover_derecha(tabla.num_columnas),
                (true, false) => csv.mover_izquierda(),
            }
        }
        return Resultado::Cambio;
    }
    if horizontal {
        return Resultado::Nada;
    }
    let delta = FILAS_POR_RUEDA as isize;
    layout.desplazar_vista(&zona, if adelante { delta } else { -delta });
    Resultado::Cambio
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llevar_seleccion_va_hacia_arriba_o_hacia_abajo() {
        let mut n = 5usize;
        llevar_seleccion(&mut n, 5, 2, |n| *n -= 1, |n| *n += 1);
        assert_eq!(n, 2);
        llevar_seleccion(&mut n, 2, 7, |n| *n -= 1, |n| *n += 1);
        assert_eq!(n, 7);
    }

    #[test]
    fn doble_y_triple_clic_en_la_misma_celda() {
        let mut mouse = EstadoMouse::default();
        assert_eq!(contar_clic(&mut mouse, MouseButton::Left, 3, 4), 1);
        assert_eq!(contar_clic(&mut mouse, MouseButton::Left, 3, 4), 2);
        assert_eq!(contar_clic(&mut mouse, MouseButton::Left, 3, 4), 3);
        // Después del triple vuelve a empezar.
        assert_eq!(contar_clic(&mut mouse, MouseButton::Left, 3, 4), 1);
        // Otra celda, u otro botón en el medio, cortan la cuenta.
        assert_eq!(contar_clic(&mut mouse, MouseButton::Left, 5, 4), 1);
        assert_eq!(contar_clic(&mut mouse, MouseButton::Middle, 5, 4), 1);
        assert_eq!(contar_clic(&mut mouse, MouseButton::Left, 5, 4), 1);
    }
}
