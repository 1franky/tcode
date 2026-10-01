//! Funciones de LSP más allá de diagnósticos y formateo (BACKLOG.md P1
//! #17): ir a la definición (y volver), autocompletado, hover, buscar
//! referencias y renombrar símbolo. La traducción del protocolo vive en
//! `tcode_lsp` (sin UI, con sus tests) y el envío en `EstadoLsp::pedir`;
//! acá, cuándo pedir, qué hacer con cada respuesta y las teclas de sus
//! popups/listas.
//!
//! Nada de esto espera al servidor: un comando (o una pausa al tipear)
//! deja un [`Pedido`] que se manda al principio de la próxima vuelta del
//! bucle de `ejecutar` (`enviar_pedido`, después de sincronizar el texto),
//! y la respuesta llega cuando llega por el mismo `select!` que los
//! diagnósticos — `procesar_respuestas` la interpreta contra lo que haya
//! en pantalla EN ESE MOMENTO (si el cursor ya se fue de la palabra, un
//! completado viejo se descarta).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use serde_json::{json, Value};
use tcode_commands::{EntradaUbicacion, EstadoListaUbicaciones};
use tcode_core::{Editor, Modo};
use tcode_lsp::{AccionRapida, AyudaFirma, EdicionArchivo, EstadoCompletado, ItemCompletado, Ubicacion};
use tcode_ui::{Layout as PanelLayout, ModoCsv, Paleta, ZonaOverlay};

use crate::lsp::{RespuestaLsp, TipoPedido};
use crate::{abrir_ruta_desde_explorador, sin_modificadores, sincronizar_lsp, EstadoApp, Foco};

/// Pausa al tipear un identificador antes de pedir completado: lo
/// bastante corta para que la lista aparezca sola al dudar, y lo bastante
/// larga para que tipear de corrido no mande una petición por tecla
/// (tipear nunca espera al servidor de todos modos: pedir es escribir un
/// mensaje en su stdin).
const PAUSA_COMPLETADO: Duration = Duration::from_millis(150);

/// Lo mismo tras un carácter de disparo (`.`): casi inmediato, pero sin
/// pedir nada mientras se tipea de corrido (`c.saldo` escrito de un
/// tirón no necesita la lista de `c.`) — cada respuesta que llega es un
/// redibujo, y con varias por palabra se notaba al tipear rápido.
const PAUSA_DISPARADOR: Duration = Duration::from_millis(40);

/// Cuántos saltos se recuerdan para "Volver" (`Alt+←`).
const MAX_PILA_VOLVER: usize = 50;

/// Una petición que se manda al principio de la próxima vuelta del bucle
/// (ver la nota del módulo).
#[derive(Debug, Clone, PartialEq)]
pub enum Pedido {
    Definicion,
    Referencias,
    Hover,
    /// `disparador`: el carácter de disparo recién tipeado (`.`), si fue
    /// eso; `manual`: `Ctrl+Espacio` (avisa si no hay sugerencias).
    /// `reintento`: la lista anterior vino incompleta.
    Completado { disparador: Option<char>, manual: bool, reintento: bool },
    Renombrar(String),
    /// Acciones rápidas (BACKLOG.md P2 #23) para la selección o el cursor.
    AccionesRapidas,
    /// El comando (`Command` crudo) de la acción rápida elegida.
    EjecutarComando(Value),
}

/// Un pedido de ayuda de firma: `disparador` es el carácter recién tipeado
/// que lo disparó (`(`, `,`), si fue eso; `manual`, el comando.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PedidoFirma {
    pub disparador: Option<char>,
    pub manual: bool,
}

/// Estado de las funciones de este módulo, un campo de `EstadoApp`.
#[derive(Default)]
pub struct EstadoFuncionesLsp {
    pub pedido: Option<Pedido>,
    pub completado: EstadoCompletado,
    /// Cuándo pedir completado por la pausa al tipear (ver
    /// [`PAUSA_COMPLETADO`]); lo vigila una rama del `select!`.
    pub completado_programado: Option<Instant>,
    /// El carácter de disparo que programó la pausa, si fue uno.
    disparador_programado: Option<char>,
    /// Se pidió completado y todavía se quiere la respuesta: cualquier
    /// tecla que no sigue la palabra lo apaga, y una respuesta que llega
    /// después se ignora.
    esperando_completado: bool,
    completado_manual: bool,
    /// Texto del popup de hover abierto (se cierra con cualquier tecla).
    pub hover: Option<String>,
    /// Varias definiciones, o las referencias.
    pub lista: EstadoListaUbicaciones,
    /// Prompt de "Renombrar símbolo" abierto, con el nombre escrito.
    pub renombrar: Option<String>,
    /// `Buffer::revision` del documento activo al pedir el renombrado: si
    /// cambió cuando llega la respuesta, sus posiciones ya no valen.
    revision_renombrar: Option<u64>,
    /// Acciones rápidas ofrecidas, mientras la lista las muestra (la
    /// lista es la misma de las ubicaciones: cada entrada guarda en
    /// `linea` su índice acá), y la revisión del documento al pedirlas.
    acciones: Option<Vec<AccionRapida>>,
    revision_acciones: Option<u64>,
    /// Ayuda de firma (BACKLOG.md P2 #23) a la vista, si hay.
    pub firma: Option<AyudaFirma>,
    /// Pedido de ayuda de firma por mandar: aparte de `pedido` porque se
    /// dispara al tipear igual que el completado y no tiene que pisarlo.
    pub firma_pedida: Option<PedidoFirma>,
    /// El último pedido mandado: si fue a mano (avisa si no hay firma) y
    /// en qué línea (una respuesta que llega con el cursor en otra línea
    /// se descarta).
    firma_manual: bool,
    firma_linea: usize,
    /// De dónde se saltó (archivo, byte), para "Volver".
    pila_volver: Vec<(PathBuf, usize)>,
}

impl EstadoFuncionesLsp {
    /// Si hay un campo de texto de este módulo capturando el teclado (lo
    /// pegado se reparte en teclas, ver `pegar_texto`).
    pub fn captura_texto(&self) -> bool {
        self.lista.activo() || self.renombrar.is_some()
    }

    /// Abre la lista con ubicaciones (a las que `Enter` salta), no con
    /// acciones rápidas.
    pub fn abrir_ubicaciones(&mut self, titulo: impl Into<String>, entradas: Vec<EntradaUbicacion>) {
        self.lista.abrir(titulo, entradas);
        self.acciones = None;
    }

    fn cerrar_completado(&mut self) {
        self.completado.cerrar();
        self.completado_programado = None;
        self.disparador_programado = None;
        self.esperando_completado = false;
    }
}

fn es_identificador(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// La línea del cursor principal, su texto, el byte (dentro de la línea)
/// del cursor y el byte donde empieza la palabra que termina en él.
struct ContextoCursor {
    linea: usize,
    texto: String,
    byte_cursor: usize,
    byte_palabra: usize,
    inicio_linea: usize,
}

fn contexto_cursor(editor: &Editor) -> ContextoCursor {
    let cursor = editor.cursor();
    let buffer = editor.buffer();
    let texto = buffer.linea_texto(cursor.linea);
    let byte_cursor = texto.char_indices().nth(cursor.columna).map(|(b, _)| b).unwrap_or(texto.len());
    let byte_palabra = texto[..byte_cursor]
        .char_indices()
        .rev()
        .take_while(|(_, c)| es_identificador(*c))
        .last()
        .map(|(b, _)| b)
        .unwrap_or(byte_cursor);
    ContextoCursor { linea: cursor.linea, byte_cursor, byte_palabra, inicio_linea: buffer.inicio_byte_linea(cursor.linea), texto }
}

/// Si el documento activo es uno sobre el que tienen sentido estas
/// funciones: foco en el editor, vista de texto (no la tabla CSV).
fn editor_de_texto(layout: &PanelLayout, estado: &EstadoApp) -> bool {
    estado.foco == Foco::Editor && layout.panel_activo().modo_csv != ModoCsv::Tabla
}

pub(crate) fn avisar(layout: &mut PanelLayout, texto: impl Into<String>) {
    layout.panel_activo_mut().mensaje_estado = Some(texto.into());
}

/// Los comandos `lsp.*` de este módulo (atajos y paleta). Devuelve
/// `false` si no aplica acá y el comando tiene que seguir su camino
/// normal (`F2` en la vista de tabla CSV edita la celda).
pub fn comando(id: &str, layout: &mut PanelLayout, estado: &mut EstadoApp) -> bool {
    if !editor_de_texto(layout, estado) {
        return false;
    }
    let funciones = &mut estado.funciones_lsp;
    match id {
        "lsp.ir_a_definicion" => funciones.pedido = Some(Pedido::Definicion),
        "lsp.referencias" => funciones.pedido = Some(Pedido::Referencias),
        "lsp.hover" => funciones.pedido = Some(Pedido::Hover),
        "lsp.acciones_rapidas" => funciones.pedido = Some(Pedido::AccionesRapidas),
        "lsp.ayuda_firma" => funciones.firma_pedida = Some(PedidoFirma { disparador: None, manual: true }),
        "lsp.completar" => {
            funciones.pedido = Some(Pedido::Completado { disparador: None, manual: true, reintento: false })
        }
        "lsp.renombrar" => {
            // Precargado con la palabra bajo el cursor (a los dos lados),
            // como "Guardar como" con la ruta actual.
            let contexto = contexto_cursor(layout.editor_activo());
            let fin = contexto.texto[contexto.byte_cursor..]
                .char_indices()
                .find(|(_, c)| !es_identificador(*c))
                .map(|(b, _)| contexto.byte_cursor + b)
                .unwrap_or(contexto.texto.len());
            let palabra = &contexto.texto[contexto.byte_palabra..fin];
            let soporta = estado.lsp.capacidades(&layout.panel_activo().ruta_mostrada).map(|c| c.renombrar);
            if soporta != Some(true) {
                // Sin preguntar el nombre en vano: el mismo aviso que
                // daría `EstadoLsp::pedir`.
                let motivo = if soporta.is_some() { "el LSP no soporta renombrar" } else { "sin LSP activo" };
                avisar(layout, format!("LSP: {motivo}"));
            } else if palabra.is_empty() {
                avisar(layout, "Renombrar: el cursor no está sobre un nombre");
            } else {
                estado.funciones_lsp.renombrar = Some(palabra.to_string());
            }
        }
        "lsp.volver" => volver(layout, estado),
        _ => return false,
    }
    true
}

/// Manda el [`Pedido`] pendiente (ver la nota del módulo): sincroniza
/// antes el texto con el servidor, para que la posición del cursor sea
/// la misma para los dos. Un error (sin LSP, no soportado...) queda como
/// aviso en la barra de estado, salvo en el completado automático, que
/// tiene que ser invisible si no hay nada que ofrecer.
pub async fn enviar_pedido(layout: &mut PanelLayout, estado: &mut EstadoApp) {
    let Some(pedido) = estado.funciones_lsp.pedido.take() else { return };
    if !editor_de_texto(layout, estado) {
        return;
    }
    sincronizar_lsp(layout, &mut estado.lsp, &estado.config).await;

    let ruta = layout.panel_activo().ruta_mostrada.clone();
    if let Pedido::EjecutarComando(comando) = &pedido {
        if let Err(motivo) = estado.lsp.ejecutar_comando(&ruta, comando).await {
            avisar(layout, format!("LSP: {motivo}"));
        }
        return;
    }
    let contexto = contexto_cursor(layout.editor_activo());
    let posicion = tcode_lsp::posicion_en_linea(contexto.linea, &contexto.texto[..contexto.byte_cursor]);
    let (tipo, extra) = match &pedido {
        Pedido::Definicion => (TipoPedido::Definicion, Value::Null),
        Pedido::Referencias => (TipoPedido::Referencias, json!({ "context": { "includeDeclaration": true } })),
        Pedido::Hover => (TipoPedido::Hover, Value::Null),
        Pedido::Completado { disparador, reintento, .. } => {
            // `triggerKind`: 1 invocado (a mano o por la pausa), 2 por un
            // carácter de disparo, 3 porque la lista vino incompleta.
            let contexto = match (disparador, reintento) {
                (Some(c), _) => json!({ "triggerKind": 2, "triggerCharacter": c.to_string() }),
                (None, true) => json!({ "triggerKind": 3 }),
                (None, false) => json!({ "triggerKind": 1 }),
            };
            (TipoPedido::Completado, json!({ "context": contexto }))
        }
        Pedido::Renombrar(nombre) => (TipoPedido::Renombrar, json!({ "newName": nombre })),
        Pedido::AccionesRapidas => (TipoPedido::AccionesRapidas, parametros_acciones(layout)),
        Pedido::EjecutarComando(_) => unreachable!("se mandó arriba"),
    };
    match estado.lsp.pedir(tipo, &ruta, posicion, extra).await {
        Ok(()) => match pedido {
            Pedido::Completado { manual, .. } => {
                let funciones = &mut estado.funciones_lsp;
                funciones.esperando_completado = true;
                funciones.completado_manual = manual;
                funciones.completado.ruta = ruta;
                funciones.completado.linea = contexto.linea;
                funciones.completado.inicio_palabra = contexto.inicio_linea + contexto.byte_palabra;
                funciones.completado.caracter_pedido = posicion.character;
            }
            Pedido::Renombrar(_) => {
                estado.funciones_lsp.revision_renombrar = Some(layout.editor_activo().buffer().revision());
            }
            Pedido::AccionesRapidas => {
                estado.funciones_lsp.revision_acciones = Some(layout.editor_activo().buffer().revision());
            }
            _ => {}
        },
        Err(motivo) => {
            let automatico = matches!(pedido, Pedido::Completado { manual: false, .. });
            if !automatico {
                avisar(layout, format!("LSP: {motivo}"));
            }
        }
    }
}

/// Manda el pedido de ayuda de firma pendiente (ver `firma_pedida`).
/// `triggerKind`: 1 a mano, 2 por un carácter de disparo, 3 porque
/// cambió el texto con la firma ya a la vista (`isRetrigger`).
pub async fn enviar_firma(layout: &mut PanelLayout, estado: &mut EstadoApp) {
    let Some(pedido) = estado.funciones_lsp.firma_pedida.take() else { return };
    if !editor_de_texto(layout, estado) {
        return;
    }
    sincronizar_lsp(layout, &mut estado.lsp, &estado.config).await;
    let ruta = layout.panel_activo().ruta_mostrada.clone();
    let contexto = contexto_cursor(layout.editor_activo());
    let posicion = tcode_lsp::posicion_en_linea(contexto.linea, &contexto.texto[..contexto.byte_cursor]);
    let mut contexto_firma = json!({
        "triggerKind": if pedido.manual { 1 } else if pedido.disparador.is_some() { 2 } else { 3 },
        "isRetrigger": estado.funciones_lsp.firma.is_some(),
    });
    if let (Some(c), false) = (pedido.disparador, pedido.manual) {
        contexto_firma["triggerCharacter"] = json!(c.to_string());
    }
    match estado.lsp.pedir(TipoPedido::AyudaFirma, &ruta, posicion, json!({ "context": contexto_firma })).await {
        Ok(()) => {
            estado.funciones_lsp.firma_manual = pedido.manual;
            estado.funciones_lsp.firma_linea = contexto.linea;
        }
        Err(motivo) if pedido.manual => avisar(layout, format!("LSP: {motivo}")),
        Err(_) => estado.funciones_lsp.firma = None,
    }
}

/// Llegó la ayuda de firma: se muestra si el cursor sigue en la línea en
/// la que se pidió (si no, ya no corresponde).
fn mostrar_firma(layout: &mut PanelLayout, estado: &mut EstadoApp, valor: &Value) {
    let funciones = &mut estado.funciones_lsp;
    let vigente = layout.editor_activo().cursor().linea == funciones.firma_linea
        && layout.editor_activo().modo() == Modo::Insertar;
    funciones.firma = if vigente { tcode_lsp::parsear_ayuda_firma(valor) } else { None };
    if funciones.firma.is_none() && funciones.firma_manual {
        avisar(layout, "Sin firma para mostrar acá");
    }
}

/// Interpreta las respuestas llegadas (`EstadoLsp::tomar_respuestas`).
/// Casi siempre no hay ninguna: se llama en cada vuelta del bucle.
/// Devuelve si cambió algo visible — un completado que llegó tarde (ya
/// se siguió escribiendo otra cosa) se descarta sin redibujar.
pub fn procesar_respuestas(layout: &mut PanelLayout, estado: &mut EstadoApp) -> bool {
    let mut cambio = false;
    // Cambios que pidió un servidor al ejecutar el comando de una acción
    // rápida (`workspace/applyEdit`): se aplican y se le contesta si se
    // pudo (la contestación sale en la próxima vuelta del bucle).
    for pedida in estado.lsp.tomar_ediciones_pedidas() {
        cambio = true;
        let aplicada = tcode_lsp::parsear_workspace_edit(&pedida.edicion).map_err(|e| e.to_string()).and_then(|archivos| {
            let resultado = aplicar_workspace_edit(layout, estado, &archivos);
            avisar(layout, format!("Acción aplicada: {}", resultado.resumen(archivos.len())));
            if resultado.fallidos > 0 {
                Err(format!("{} archivo(s) no se pudieron cambiar", resultado.fallidos))
            } else {
                Ok(())
            }
        });
        if let Err(motivo) = &aplicada {
            avisar(layout, format!("Acción: {motivo}"));
        }
        estado.lsp.responder_edicion(&pedida, aplicada);
    }
    for RespuestaLsp { tipo, resultado } in estado.lsp.tomar_respuestas() {
        cambio |= tipo != TipoPedido::Completado;
        let valor = match resultado {
            Ok(valor) => valor,
            Err(motivo) => {
                if tipo == TipoPedido::PistasInlay {
                    crate::pistas::respuesta(layout, estado, None);
                    continue;
                }
                if tipo == TipoPedido::AyudaFirma && !estado.funciones_lsp.firma_manual {
                    estado.funciones_lsp.firma = None;
                    continue;
                }
                if tipo == TipoPedido::Completado {
                    let manual = estado.funciones_lsp.completado_manual;
                    estado.funciones_lsp.esperando_completado = false;
                    if !manual {
                        continue;
                    }
                    cambio = true;
                }
                avisar(layout, format!("LSP: {motivo}"));
                continue;
            }
        };
        match tipo {
            TipoPedido::Definicion => {
                let ubicaciones = tcode_lsp::parsear_ubicaciones(&valor);
                match ubicaciones.as_slice() {
                    [] => avisar(layout, "No se encontró la definición"),
                    [unica] => saltar_a(layout, estado, unica.ruta.clone(), unica.linea, unica.caracter),
                    _ => abrir_lista(layout, estado, "Definiciones", ubicaciones),
                }
            }
            TipoPedido::Referencias => {
                let ubicaciones = tcode_lsp::parsear_ubicaciones(&valor);
                if ubicaciones.is_empty() {
                    avisar(layout, "No se encontraron referencias");
                } else {
                    let titulo = format!("Referencias ({})", ubicaciones.len());
                    abrir_lista(layout, estado, &titulo, ubicaciones);
                }
            }
            TipoPedido::Hover => match tcode_lsp::texto_hover(&valor) {
                Some(texto) => estado.funciones_lsp.hover = Some(texto),
                None => avisar(layout, "Sin información para mostrar"),
            },
            TipoPedido::Completado => cambio |= abrir_completado(layout, estado, &valor),
            TipoPedido::Renombrar => aplicar_renombrado(layout, estado, &valor),
            TipoPedido::AccionesRapidas => abrir_acciones(layout, estado, &valor),
            // Lo que haya cambiado llegó antes como `workspace/applyEdit`.
            TipoPedido::EjecutarComando => {}
            TipoPedido::AyudaFirma => mostrar_firma(layout, estado, &valor),
            TipoPedido::PistasInlay => crate::pistas::respuesta(layout, estado, Some(&valor)),
        }
    }
    cambio
}

/// Si `a` y `b` son el mismo archivo: iguales tal cual o una vez
/// resueltos los enlaces (en macOS `/tmp` es `/private/tmp`, y un
/// servidor puede devolver cualquiera de las dos) y las rutas relativas.
pub(crate) fn mismo_archivo(a: &Path, b: &Path) -> bool {
    a == b || matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y)
}

/// Deja activo el archivo `ruta` en el panel activo: el que ya se ve, una
/// pestaña que ya lo tenía, o una nueva (`abrir_ruta_desde_explorador`).
/// `false` si no se pudo abrir.
fn activar_archivo(layout: &mut PanelLayout, estado: &mut EstadoApp, ruta: &Path) -> bool {
    let ya_abierto = layout
        .documentos_panel_activo()
        .filter_map(|d| d.editor.buffer().ruta())
        .find(|r| mismo_archivo(r, ruta))
        .map(Path::to_path_buf);
    match ya_abierto {
        Some(propia) => {
            layout.activar_pestana_de(&propia);
        }
        None => {
            // Relativa al directorio actual si está adentro (como al
            // abrirlo desde el explorador): el servidor devuelve rutas
            // absolutas, que en la pestaña y la barra de estado ocupan
            // demasiado.
            let relativa = std::env::current_dir()
                .and_then(std::fs::canonicalize)
                .ok()
                .and_then(|actual| Some(std::fs::canonicalize(ruta).ok()?.strip_prefix(actual).ok()?.to_path_buf()));
            let a_abrir = relativa.unwrap_or_else(|| ruta.to_path_buf());
            abrir_ruta_desde_explorador(layout, &mut estado.foco, a_abrir, &estado.config.editor)
        }
    }
    layout.editor_activo().buffer().ruta().is_some_and(|r| mismo_archivo(r, ruta))
}

/// Byte del buffer de la posición LSP `linea`/`caracter` (UTF-16).
fn byte_de(editor: &Editor, linea: u32, caracter: u32) -> usize {
    let buffer = editor.buffer();
    let linea = (linea as usize).min(buffer.num_lineas().saturating_sub(1));
    buffer.inicio_byte_linea(linea) + tcode_lsp::byte_en_linea(&buffer.linea_texto(linea), caracter)
}

/// Salta a una ubicación (abriendo el archivo en una pestaña si hace
/// falta), recordando de dónde se venía para "Volver".
pub(crate) fn saltar_a(layout: &mut PanelLayout, estado: &mut EstadoApp, ruta: PathBuf, linea: u32, caracter: u32) {
    let editor = layout.editor_activo();
    let origen = editor.buffer().ruta().map(|r| {
        let cursor = editor.cursor();
        (r.to_path_buf(), editor.buffer().offset_byte(cursor.linea, cursor.columna))
    });
    if !activar_archivo(layout, estado, &ruta) {
        avisar(layout, format!("No se pudo abrir {}", ruta.display()));
        return;
    }
    if let Some(origen) = origen {
        let pila = &mut estado.funciones_lsp.pila_volver;
        pila.push(origen);
        if pila.len() > MAX_PILA_VOLVER {
            pila.remove(0);
        }
    }
    let editor = layout.editor_activo_mut();
    let byte = byte_de(editor, linea, caracter);
    editor.mover_cursor_a_byte(byte);
    // Desde el panel de problemas se puede saltar con el foco en el
    // explorador.
    estado.foco = Foco::Editor;
}

/// "Volver" (`Alt+←`): al lugar de antes del último salto.
fn volver(layout: &mut PanelLayout, estado: &mut EstadoApp) {
    let Some((ruta, byte)) = estado.funciones_lsp.pila_volver.pop() else {
        avisar(layout, "No hay a dónde volver");
        return;
    };
    if activar_archivo(layout, estado, &ruta) {
        let editor = layout.editor_activo_mut();
        let byte = byte.min(editor.buffer().len_bytes());
        editor.mover_cursor_a_byte(byte);
    }
}

/// Abre la lista de ubicaciones: cada fila es `ruta:línea  código`, con
/// la ruta relativa al directorio actual si está adentro, y el texto de
/// esa línea sacado del buffer si el archivo está abierto (puede tener
/// cambios sin guardar) o del disco si no (leído una vez por archivo).
fn abrir_lista(layout: &mut PanelLayout, estado: &mut EstadoApp, titulo: &str, ubicaciones: Vec<Ubicacion>) {
    let actual = std::env::current_dir().ok().and_then(|d| std::fs::canonicalize(d).ok());
    let mut textos: Vec<(PathBuf, Option<Vec<String>>)> = Vec::new();
    let entradas = ubicaciones
        .into_iter()
        .map(|u| {
            if !textos.iter().any(|(r, _)| *r == u.ruta) {
                let abierto = layout
                    .documentos()
                    .into_iter()
                    .find(|d| d.editor.buffer().ruta().is_some_and(|r| mismo_archivo(r, &u.ruta)))
                    .map(|d| d.editor.buffer().lineas_texto());
                let lineas = abierto
                    .or_else(|| std::fs::read_to_string(&u.ruta).ok().map(|t| t.lines().map(str::to_string).collect()));
                textos.push((u.ruta.clone(), lineas));
            }
            let codigo = textos
                .iter()
                .find(|(r, _)| *r == u.ruta)
                .and_then(|(_, l)| l.as_ref()?.get(u.linea as usize))
                .map(|l| l.trim().to_string())
                .unwrap_or_default();
            let canonica = std::fs::canonicalize(&u.ruta).unwrap_or_else(|_| u.ruta.clone());
            let mostrada = actual
                .as_ref()
                .and_then(|a| canonica.strip_prefix(a).ok())
                .map(|r| r.display().to_string())
                .unwrap_or_else(|| u.ruta.display().to_string());
            EntradaUbicacion {
                etiqueta: format!("{mostrada}:{}  {codigo}", u.linea + 1),
                ruta: u.ruta,
                linea: u.linea,
                caracter: u.caracter,
            }
        })
        .collect();
    estado.funciones_lsp.abrir_ubicaciones(titulo, entradas);
}

/// Si el popup de completado puede seguir abierto (o abrirse) con el
/// cursor donde está: mismo archivo y línea que al pedir, en modo de
/// inserción, un solo cursor, y todavía en la palabra que empezaba en
/// `inicio_palabra`. Devuelve lo escrito de esa palabra (el filtro).
fn prefijo_vigente(layout: &PanelLayout, estado: &EstadoApp) -> Option<String> {
    let completado = &estado.funciones_lsp.completado;
    let panel = layout.panel_activo();
    let editor = &panel.editor;
    if !editor_de_texto(layout, estado)
        || panel.ruta_mostrada != completado.ruta
        || editor.modo() != Modo::Insertar
        || editor.tiene_multiples_cursores()
        || editor.cursor().linea != completado.linea
    {
        return None;
    }
    let contexto = contexto_cursor(editor);
    (contexto.inicio_linea + contexto.byte_palabra == completado.inicio_palabra)
        .then(|| contexto.texto[contexto.byte_palabra..contexto.byte_cursor].to_string())
}

/// Devuelve si cambió algo visible (se abrió el popup o hay aviso).
fn abrir_completado(layout: &mut PanelLayout, estado: &mut EstadoApp, valor: &Value) -> bool {
    let funciones = &mut estado.funciones_lsp;
    if !std::mem::take(&mut funciones.esperando_completado) {
        return false;
    }
    let manual = funciones.completado_manual;
    // Primero si todavía sirve (barato), después parsear (la lista de
    // pyright puede traer miles de items con los de auto-import).
    let Some(prefijo) = prefijo_vigente(layout, estado) else { return false };
    let (items, incompleto) = tcode_lsp::parsear_completado(valor);
    let completado = &mut estado.funciones_lsp.completado;
    completado.abrir(items, incompleto, &prefijo);
    if !completado.activo() && manual {
        avisar(layout, "Sin sugerencias");
    }
    completado.activo() || manual
}

/// Aplica el item elegido del completado como UN paso de deshacer
/// (`Editor::aplicar_ediciones`): su texto reemplaza la palabra escrita
/// hasta el cursor (o el rango de su `textEdit`, estirado hasta el cursor
/// por lo que se siguió escribiendo después de pedir), más sus
/// `additionalTextEdits` (un `use`/`import`). El cursor queda al final de
/// lo insertado.
fn aceptar_completado(layout: &mut PanelLayout, estado: &mut EstadoApp) {
    let Some(item) = estado.funciones_lsp.completado.aceptar() else { return };
    let caracter_pedido = estado.funciones_lsp.completado.caracter_pedido;
    let inicio_palabra = estado.funciones_lsp.completado.inicio_palabra;
    let editor = layout.editor_activo_mut();
    let ediciones = ediciones_de_item(editor, &item, inicio_palabra, caracter_pedido);
    let Some((principal, _)) = ediciones.first().cloned() else { return };
    // Lo que agregan/quitan las ediciones adicionales ANTES del reemplazo
    // principal corre el final de lo insertado.
    let corrimiento: isize = ediciones[1..]
        .iter()
        .filter(|(rango, _)| rango.end <= principal.start)
        .map(|(rango, texto)| texto.len() as isize - rango.len() as isize)
        .sum();
    let Some(fuente) = &item.snippet else {
        editor.aplicar_ediciones(&ediciones);
        let fin = (principal.start as isize + corrimiento) as usize + item.insertar.len();
        editor.mover_cursor_a_byte(fin.min(editor.buffer().len_bytes()));
        return;
    };
    // Un snippet (BACKLOG.md P2 #24): primero las ediciones adicionales
    // (un `import`), después el principal con sus campos — todo en un
    // solo paso de deshacer.
    let ruta = layout.panel_activo().ruta_mostrada.clone();
    let tab = crate::snippets::tab(&estado.config);
    let editor = layout.editor_activo_mut();
    let snippet = tcode_core::parsear_snippet(fuente, |nombre| crate::snippets::variable(editor, &ruta, nombre));
    editor.abrir_grupo_deshacer();
    editor.aplicar_ediciones(&ediciones[1..]);
    let correr = |p: usize| (p as isize + corrimiento).max(0) as usize;
    editor.insertar_snippet(correr(principal.start)..correr(principal.end), &snippet, &tab);
    editor.cerrar_grupo_deshacer();
}

/// Las ediciones (en bytes del buffer actual) de aceptar `item`: la
/// principal primero. Ver [`aceptar_completado`].
fn ediciones_de_item(
    editor: &Editor,
    item: &ItemCompletado,
    inicio_palabra: usize,
    caracter_pedido: u32,
) -> Vec<(std::ops::Range<usize>, String)> {
    let contexto = contexto_cursor(editor);
    let cursor = contexto.inicio_linea + contexto.byte_cursor;
    // Columna UTF-16 del cursor ahora (lo escrito después de pedir la
    // corrió respecto de `caracter_pedido`).
    let caracter_cursor = tcode_lsp::posicion_en_linea(contexto.linea, &contexto.texto[..contexto.byte_cursor]).character;
    let (inicio, fin) = match item.rango {
        Some(rango) if rango.start.line as usize == contexto.linea && rango.start.character <= caracter_pedido => {
            let inicio = contexto.inicio_linea + tcode_lsp::byte_en_linea(&contexto.texto, rango.start.character);
            let sobrante = rango.end.character.saturating_sub(caracter_pedido);
            let fin = contexto.inicio_linea + tcode_lsp::byte_en_linea(&contexto.texto, caracter_cursor + sobrante);
            (inicio.min(cursor), fin.max(cursor))
        }
        _ => (inicio_palabra.min(cursor), cursor),
    };
    let mut ediciones = vec![(inicio..fin, item.insertar.clone())];
    if !item.adicionales.is_empty() {
        let texto = editor.buffer().a_texto();
        for (rango, nuevo) in &item.adicionales {
            let a = tcode_lsp::byte_de_posicion(&texto, rango.start);
            let b = tcode_lsp::byte_de_posicion(&texto, rango.end);
            // Una adicional que pise el reemplazo principal (no debería
            // pasar) se descarta en vez de invalidar todo.
            if a <= b && (b <= inicio || a >= fin) {
                ediciones.push((a..b, nuevo.clone()));
            }
        }
    }
    ediciones
}

/// Aplica la respuesta a `textDocument/rename` (un `WorkspaceEdit`). En
/// los documentos abiertos (en cualquier pestaña de cualquier panel), con
/// `aplicar_ediciones`: un paso de deshacer por archivo. Los que no están
/// abiertos se abren en pestañas nuevas del panel activo y se editan ahí,
/// SIN guardar: quedan como cambios pendientes para revisar (y guardar o
/// deshacer) — lo más seguro, `tcode` nunca escribe al disco sin que se
/// vea. Si el documento activo cambió desde que se pidió, no se aplica
/// nada: las posiciones ya no valen.
fn aplicar_renombrado(layout: &mut PanelLayout, estado: &mut EstadoApp, valor: &Value) {
    let revision = estado.funciones_lsp.revision_renombrar.take();
    let archivos = match tcode_lsp::parsear_workspace_edit(valor) {
        Ok(archivos) => archivos,
        Err(error) => return avisar(layout, format!("Renombrar: {error}")),
    };
    if archivos.is_empty() {
        return avisar(layout, "Renombrar: el LSP no devolvió cambios");
    }
    if revision != Some(layout.editor_activo().buffer().revision()) {
        return avisar(layout, "Renombrar: el archivo cambió mientras tanto, probá de nuevo");
    }
    let resultado = aplicar_workspace_edit(layout, estado, &archivos);
    avisar(layout, format!("Renombrado: {}", resultado.resumen(archivos.len())));
}

/// Cuánto cambió [`aplicar_workspace_edit`].
struct ResultadoEdicion {
    cambios: usize,
    abiertos_nuevos: usize,
    fallidos: usize,
}

impl ResultadoEdicion {
    fn resumen(&self, archivos: usize) -> String {
        let mut aviso = format!("{} cambios en {archivos} archivo(s)", self.cambios);
        if self.abiertos_nuevos > 0 {
            aviso.push_str(&format!(", {} abierto(s) en pestañas sin guardar", self.abiertos_nuevos));
        }
        if self.fallidos > 0 {
            aviso.push_str(&format!(" ({} no se pudieron aplicar)", self.fallidos));
        }
        aviso
    }
}

/// Aplica los cambios de un `WorkspaceEdit` (renombrar, acciones
/// rápidas): en los archivos ya abiertos (en cualquier pestaña o panel)
/// ahí mismo, un paso de deshacer por archivo; los que no, se abren en
/// pestañas nuevas con los cambios sin guardar — `tcode` nunca escribe al
/// disco algo que no se vio. Deja activo el documento que lo estaba.
fn aplicar_workspace_edit(layout: &mut PanelLayout, estado: &mut EstadoApp, archivos: &[EdicionArchivo]) -> ResultadoEdicion {
    let original = layout.editor_activo().buffer().ruta().map(Path::to_path_buf);
    let mut resultado = ResultadoEdicion { cambios: 0, abiertos_nuevos: 0, fallidos: 0 };
    for archivo in archivos {
        let aplicar = |editor: &mut Editor| {
            let texto = editor.buffer().a_texto();
            let ediciones: Vec<(std::ops::Range<usize>, String)> = archivo
                .ediciones
                .iter()
                .map(|(r, t)| (tcode_lsp::byte_de_posicion(&texto, r.start)..tcode_lsp::byte_de_posicion(&texto, r.end), t.clone()))
                .collect();
            editor.aplicar_ediciones(&ediciones)
        };
        let mut encontrado = false;
        for panel in layout.paneles_mut() {
            if panel.editor.buffer().ruta().is_some_and(|r| mismo_archivo(r, &archivo.ruta)) {
                encontrado = true;
                if !aplicar(&mut panel.editor) {
                    resultado.fallidos += 1;
                }
            }
        }
        if !encontrado {
            if activar_archivo(layout, estado, &archivo.ruta) {
                resultado.abiertos_nuevos += 1;
                if !aplicar(layout.editor_activo_mut()) {
                    resultado.fallidos += 1;
                }
            } else {
                resultado.fallidos += 1;
            }
        }
        resultado.cambios += archivo.ediciones.len();
    }
    if let Some(original) = original {
        activar_archivo(layout, estado, &original);
    }
    resultado
}

/// Parámetros de `textDocument/codeAction` (además del documento): el
/// rango de la selección del cursor principal (o el cursor solo) y los
/// diagnósticos de esas líneas, tal como los mandó el servidor.
fn parametros_acciones(layout: &PanelLayout) -> Value {
    let panel = layout.panel_activo();
    let editor = &panel.editor;
    let principal = &editor.cursores()[0];
    let (inicio, fin) = if (principal.ancla.linea, principal.ancla.columna) <= (principal.cursor.linea, principal.cursor.columna) {
        (principal.ancla, principal.cursor)
    } else {
        (principal.cursor, principal.ancla)
    };
    let posicion = |c: tcode_core::Cursor| {
        let linea = editor.buffer().linea_texto(c.linea);
        let prefijo: String = linea.chars().take(c.columna).collect();
        tcode_lsp::posicion_en_linea(c.linea, &prefijo)
    };
    let diagnosticos: Vec<_> = panel
        .diagnosticos
        .iter()
        .filter(|d| d.linea_inicio as usize <= fin.linea && d.linea_fin as usize >= inicio.linea)
        .map(|d| &d.original)
        .collect();
    json!({
        "range": { "start": posicion(inicio), "end": posicion(fin) },
        "context": { "diagnostics": diagnosticos, "triggerKind": 1 },
    })
}

/// Llegaron las acciones rápidas: la lista para elegir (`Enter` aplica).
fn abrir_acciones(layout: &mut PanelLayout, estado: &mut EstadoApp, valor: &Value) {
    let acciones = tcode_lsp::parsear_acciones(valor);
    if acciones.is_empty() {
        return avisar(layout, "No hay acciones rápidas acá");
    }
    let entradas = acciones
        .iter()
        .enumerate()
        .map(|(i, a)| EntradaUbicacion {
            etiqueta: if a.preferida { format!("{} (recomendada)", a.titulo) } else { a.titulo.clone() },
            ruta: PathBuf::new(),
            linea: i as u32,
            caracter: 0,
        })
        .collect();
    estado.funciones_lsp.lista.abrir("Acciones rápidas", entradas);
    estado.funciones_lsp.acciones = Some(acciones);
}

/// `Enter` (o clic) en la lista: aplica la acción rápida si la lista las
/// mostraba, o salta a la ubicación.
fn elegir_de_lista(entrada: EntradaUbicacion, layout: &mut PanelLayout, estado: &mut EstadoApp) {
    let Some(acciones) = estado.funciones_lsp.acciones.take() else {
        return saltar_a(layout, estado, entrada.ruta, entrada.linea, entrada.caracter);
    };
    let Some(accion) = acciones.into_iter().nth(entrada.linea as usize) else { return };
    let revision = estado.funciones_lsp.revision_acciones.take();
    if revision != Some(layout.editor_activo().buffer().revision()) {
        return avisar(layout, "Acción: el archivo cambió mientras tanto, probá de nuevo");
    }
    if let Some(edicion) = &accion.edicion {
        match tcode_lsp::parsear_workspace_edit(edicion) {
            Ok(archivos) => {
                let resultado = aplicar_workspace_edit(layout, estado, &archivos);
                avisar(layout, format!("{}: {}", accion.titulo, resultado.resumen(archivos.len())));
            }
            Err(error) => return avisar(layout, format!("Acción: {error}")),
        }
    }
    // El comando va después de los cambios (así lo pide la spec); lo que
    // cambie llega como `workspace/applyEdit`.
    if let Some(comando) = accion.comando {
        estado.funciones_lsp.pedido = Some(Pedido::EjecutarComando(comando));
    }
}

/// Teclas que capturan la lista de ubicaciones, el prompt de renombrar,
/// el popup de hover y el de completado, ANTES del sistema de atajos.
/// Devuelve si la tecla se consumió. La lista y el prompt capturan todo;
/// el hover se cierra con cualquier tecla (que después sigue su camino,
/// salvo `Esc`); el completado solo se queda con `↑`/`↓`/`Tab`/`Enter`/
/// `Esc` — lo demás se sigue escribiendo y [`despues_de_tecla`] refiltra.
pub fn manejar_tecla(key: KeyEvent, layout: &mut PanelLayout, estado: &mut EstadoApp) -> bool {
    let funciones = &mut estado.funciones_lsp;
    if funciones.lista.activo() {
        match key.code {
            KeyCode::Esc => funciones.lista.cerrar(),
            KeyCode::Up => funciones.lista.mover_arriba(),
            KeyCode::Down => funciones.lista.mover_abajo(),
            KeyCode::Backspace => funciones.lista.borrar(),
            KeyCode::Enter => {
                if let Some(entrada) = funciones.lista.confirmar() {
                    elegir_de_lista(entrada, layout, estado);
                }
            }
            KeyCode::Char(c) if sin_modificadores(key) => funciones.lista.escribir(c),
            _ => {}
        }
        return true;
    }
    // `Esc` cierra la ayuda de firma y sigue su camino (cierra también el
    // completado, o pasa a Normal en modo VIM).
    if key.code == KeyCode::Esc {
        funciones.firma = None;
    }
    if let Some(nombre) = &mut funciones.renombrar {
        match key.code {
            KeyCode::Esc => funciones.renombrar = None,
            KeyCode::Backspace => {
                nombre.pop();
            }
            KeyCode::Enter => {
                let nombre = funciones.renombrar.take().unwrap_or_default();
                if !nombre.is_empty() {
                    funciones.pedido = Some(Pedido::Renombrar(nombre));
                }
            }
            KeyCode::Char(c) if sin_modificadores(key) => nombre.push(c),
            _ => {}
        }
        return true;
    }
    if funciones.hover.take().is_some() && key.code == KeyCode::Esc {
        return true;
    }
    if funciones.completado.activo() {
        let simple = key.modifiers.is_empty();
        match key.code {
            KeyCode::Up if simple => funciones.completado.mover_arriba(),
            KeyCode::Down if simple => funciones.completado.mover_abajo(),
            KeyCode::Esc => funciones.cerrar_completado(),
            KeyCode::Tab | KeyCode::Enter if simple => {
                funciones.completado_programado = None;
                aceptar_completado(layout, estado);
            }
            _ => return false,
        }
        return true;
    }
    false
}

/// Qué hizo con el texto la tecla que acaba de pasar por el camino normal
/// (atajos o texto): lo que le importa al completado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tecleo {
    /// Se insertó este carácter (no era un atajo).
    Caracter(char),
    /// `editor.borrar_atras`.
    Borrar,
    /// Cualquier otra cosa: un atajo, una tecla sin efecto, un chord a
    /// medias.
    Otro,
}

/// Después de que una tecla llegó al editor por el camino normal: decide
/// qué pasa con el completado. Un carácter de identificador filtra el
/// popup abierto (y si la lista vino incompleta, programa otra petición),
/// o programa una tras la pausa si no había popup; un carácter de disparo
/// del servidor (`.`) pide ya mismo; `Backspace` refiltra; cualquier otra
/// cosa lo cierra. Barato: no toca más que la línea del cursor.
pub fn despues_de_tecla(tecleo: Tecleo, layout: &PanelLayout, estado: &mut EstadoApp) {
    let editor = layout.editor_activo();
    let apto = editor_de_texto(layout, estado) && editor.modo() == Modo::Insertar && !editor.tiene_multiples_cursores();
    actualizar_firma(tecleo, apto, layout, estado);
    let caracter = match tecleo {
        Tecleo::Caracter(c) if apto => Some(c),
        Tecleo::Borrar if apto => None,
        _ => {
            estado.funciones_lsp.cerrar_completado();
            return;
        }
    };

    if estado.funciones_lsp.completado.activo() && caracter.is_none_or(es_identificador) {
        match prefijo_vigente(layout, estado) {
            Some(prefijo) => {
                let completado = &mut estado.funciones_lsp.completado;
                completado.filtrar(&prefijo);
                if completado.incompleto && caracter.is_some() {
                    estado.funciones_lsp.completado_programado = Some(Instant::now() + PAUSA_COMPLETADO);
                }
            }
            None => estado.funciones_lsp.cerrar_completado(),
        }
        return;
    }
    estado.funciones_lsp.cerrar_completado();
    let Some(c) = caracter else { return };
    let ruta = &layout.panel_activo().ruta_mostrada;
    let Some(capacidades) = estado.lsp.capacidades(ruta).filter(|c| c.completado) else { return };
    if capacidades.disparadores_completado.contains(&c) {
        estado.funciones_lsp.completado_programado = Some(Instant::now() + PAUSA_DISPARADOR);
        estado.funciones_lsp.disparador_programado = Some(c);
    } else if es_identificador(c) {
        estado.funciones_lsp.completado_programado = Some(Instant::now() + PAUSA_COMPLETADO);
    }
}

/// Ayuda de firma al tipear: un carácter de disparo del servidor (`(`,
/// `,`) la pide; con la firma a la vista, cualquier letra o borrado la
/// vuelve a pedir (el parámetro activo cambia, o el cursor salió de la
/// llamada y el servidor contesta que no hay firma, lo que la cierra).
/// Cualquier otra cosa (un atajo, moverse, `Enter`) la cierra.
fn actualizar_firma(tecleo: Tecleo, apto: bool, layout: &PanelLayout, estado: &mut EstadoApp) {
    let disparadores = estado
        .lsp
        .capacidades(&layout.panel_activo().ruta_mostrada)
        .filter(|c| c.ayuda_firma)
        .map(|c| c.disparadores_firma.clone());
    let funciones = &mut estado.funciones_lsp;
    match (tecleo, disparadores) {
        (Tecleo::Caracter(c), Some(disparadores)) if apto && disparadores.contains(&c) => {
            funciones.firma_pedida = Some(PedidoFirma { disparador: Some(c), manual: false });
        }
        (Tecleo::Caracter(_) | Tecleo::Borrar, Some(_)) if apto && funciones.firma.is_some() => {
            funciones.firma_pedida = Some(PedidoFirma { disparador: None, manual: false });
        }
        (Tecleo::Caracter(_) | Tecleo::Borrar, _) if apto => {}
        _ => funciones.firma = None,
    }
}

/// Venció la pausa al tipear (rama del `select!` de `ejecutar`): pide el
/// completado. `reintento` si ya había una lista abierta (incompleta).
pub fn pausa_vencida(estado: &mut EstadoApp) {
    let funciones = &mut estado.funciones_lsp;
    funciones.completado_programado = None;
    let reintento = funciones.completado.activo();
    let disparador = funciones.disparador_programado.take();
    funciones.pedido = Some(Pedido::Completado { disparador, manual: false, reintento });
}

/// Dibuja lo de este módulo encima de todo lo demás.
/// Dibuja lo de este módulo que esté abierto. Devuelve, para el mouse
/// (BACKLOG.md P0 #18), dónde quedó la lista de ubicaciones (un overlay
/// modal) y el popup de completado o de hover (pegados al cursor).
pub fn dibujar(
    frame: &mut Frame,
    layout: &PanelLayout,
    funciones: &EstadoFuncionesLsp,
    paleta: &Paleta,
) -> (Option<ZonaOverlay>, Option<ZonaOverlay>) {
    let area = frame.area();
    if funciones.lista.activo() {
        return (tcode_ui::panel_lsp::dibujar_lista_ubicaciones(frame, area, &funciones.lista, paleta), None);
    }
    if let Some(nombre) = &funciones.renombrar {
        tcode_ui::panel_lsp::dibujar_prompt_renombrar(frame, area, nombre, paleta);
        return (None, None);
    }
    let Some(cursor) = layout.panel_activo().estado_ui.posicion_cursor() else { return (None, None) };
    // La firma va arriba del cursor y el completado abajo: se pueden ver
    // los dos a la vez.
    if let Some(firma) = &funciones.firma {
        tcode_ui::panel_lsp::dibujar_firma(frame, frame.area(), cursor, firma, paleta);
    }
    if funciones.completado.activo() {
        // Anclado al inicio de la palabra (no al cursor), para que no se
        // corra a cada letra escrita.
        let contexto = contexto_cursor(layout.editor_activo());
        let escritas = contexto.texto[contexto.byte_palabra..contexto.byte_cursor].chars().count() as u16;
        let ancla = (cursor.0.saturating_sub(escritas), cursor.1);
        (None, tcode_ui::panel_lsp::dibujar_completado(frame, area, ancla, &funciones.completado, paleta))
    } else if let Some(texto) = &funciones.hover {
        (None, tcode_ui::panel_lsp::dibujar_hover(frame, area, cursor, texto, paleta))
    } else {
        (None, None)
    }
}

/// Clic sobre un ítem de la lista de ubicaciones (BACKLOG.md P0 #18):
/// igual que elegirlo con las flechas y `Enter`.
pub fn clic_en_lista(indice: usize, layout: &mut PanelLayout, estado: &mut EstadoApp) {
    let lista = &mut estado.funciones_lsp.lista;
    let actual = lista.seleccion();
    crate::mouse::llevar_seleccion(lista, actual, indice, EstadoListaUbicaciones::mover_arriba, EstadoListaUbicaciones::mover_abajo);
    if let Some(entrada) = estado.funciones_lsp.lista.confirmar() {
        elegir_de_lista(entrada, layout, estado);
    }
}

/// Clic sobre un ítem del completado: igual que elegirlo y `Enter`.
pub fn clic_en_completado(indice: usize, layout: &mut PanelLayout, estado: &mut EstadoApp) {
    let completado = &mut estado.funciones_lsp.completado;
    let actual = completado.seleccion();
    crate::mouse::llevar_seleccion(completado, actual, indice, EstadoCompletado::mover_arriba, EstadoCompletado::mover_abajo);
    estado.funciones_lsp.completado_programado = None;
    aceptar_completado(layout, estado);
}

/// La rueda sobre el popup de completado: mueve la selección.
pub fn rueda_en_completado(abajo: bool, estado: &mut EstadoApp) {
    if abajo {
        estado.funciones_lsp.completado.mover_abajo();
    } else {
        estado.funciones_lsp.completado.mover_arriba();
    }
}

/// Un clic en otro lado cierra el completado y el hover, como una tecla
/// que no sigue la palabra.
pub fn cerrar_popups(estado: &mut EstadoApp) {
    estado.funciones_lsp.cerrar_completado();
    estado.funciones_lsp.hover = None;
    estado.funciones_lsp.firma = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Position, Range};

    fn editor_con(texto: &str) -> Editor {
        let mut editor = Editor::nuevo();
        editor.insertar_texto(texto);
        editor
    }

    fn item(insertar: &str, rango: Option<Range>, adicionales: Vec<(Range, String)>) -> ItemCompletado {
        ItemCompletado {
            etiqueta: insertar.to_string(),
            detalle: None,
            tipo: "",
            texto_filtro: insertar.to_string(),
            orden: String::new(),
            insertar: insertar.to_string(),
            rango,
            adicionales,
            snippet: None,
        }
    }

    fn rango(linea: u32, c1: u32, c2: u32) -> Range {
        Range { start: Position { line: linea, character: c1 }, end: Position { line: linea, character: c2 } }
    }

    #[test]
    fn la_palabra_del_cursor_con_acentos() {
        let editor = editor_con("x = señal.año");
        let contexto = contexto_cursor(&editor);
        assert_eq!(&contexto.texto[contexto.byte_palabra..contexto.byte_cursor], "año");
    }

    #[test]
    fn sin_text_edit_reemplaza_la_palabra_hasta_el_cursor() {
        // Pedido tras `ñ.` (cursor en la columna UTF-16 2); después se
        // escribió `pu` y se elige `push`.
        let mut editor = editor_con("ñ.pu");
        let ediciones = ediciones_de_item(&editor, &item("push", None, Vec::new()), 3, 2);
        assert_eq!(ediciones, vec![(3..5, "push".to_string())]);
        editor.aplicar_ediciones(&ediciones);
        assert_eq!(editor.buffer().a_texto(), "ñ.push");
    }

    #[test]
    fn el_text_edit_se_estira_hasta_lo_escrito_despues_de_pedir() {
        // `😀.p|` pedido con el cursor en UTF-16 4 (el emoji ocupa 2
        // unidades); el servidor reemplaza [3, 4) (la `p`), y después se
        // escribió `u`.
        let editor = editor_con("😀.pu");
        let ediciones = ediciones_de_item(&editor, &item("push", Some(rango(0, 3, 4)), Vec::new()), 5, 4);
        assert_eq!(ediciones, vec![(5..7, "push".to_string())]);
    }

    #[test]
    fn el_text_edit_que_reemplaza_mas_alla_del_cursor_se_corre_con_lo_escrito() {
        // Pedido en `a.b|c` (UTF-16 3) con un rango que llega a 4 (la `c`
        // de la derecha); después se escribió una `x`: el fin se corre 1.
        let mut editor = editor_con("a.bc");
        editor.mover_izquierda();
        editor.insertar_char('x');
        let ediciones = ediciones_de_item(&editor, &item("bcd", Some(rango(0, 2, 4)), Vec::new()), 2, 3);
        assert_eq!(ediciones, vec![(2..5, "bcd".to_string())]);
    }

    #[test]
    fn las_ediciones_adicionales_van_aparte_y_la_que_pisa_se_descarta() {
        let editor = editor_con("fn a() {}\nlet x = Ha");
        let adicionales =
            vec![(rango(0, 0, 0), "use std::collections::HashMap;\n".to_string()), (rango(1, 8, 9), "!".to_string())];
        let ediciones = ediciones_de_item(&editor, &item("HashMap", None, adicionales), 18, 10);
        assert_eq!(ediciones.len(), 2);
        assert_eq!(ediciones[0], (18..20, "HashMap".to_string()));
        assert_eq!(ediciones[1].0, 0..0);
    }
}
