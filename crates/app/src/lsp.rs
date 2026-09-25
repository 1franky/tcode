//! Gestión del ciclo de vida de los clientes LSP (PLAN.md §2, §5.3, §11
//! M2): cuándo lanzarlos, el handshake `initialize`/`initialized`,
//! mantener a cada servidor al tanto de los documentos abiertos
//! (`didOpen`/`didChange`/`didClose`), y convertir sus notificaciones de
//! diagnósticos en algo que `tcode-ui` pueda dibujar.
//!
//! Una sesión por lenguaje: se lanza la primera vez que aparece un
//! documento de ese lenguaje en cualquier pestaña de cualquier panel, y
//! sigue viva mientras quede alguno abierto — cambiar de pestaña o de
//! panel entre un `.py` y un `.rs` ya no mata ni relanza nada (antes
//! había una sola sesión, atada al documento activo, y volver a un
//! lenguaje significaba esperar otra vez a que el servidor arrancara e
//! indexara). Cada sesión tiene abiertos en el servidor TODOS los
//! documentos de su lenguaje, no solo el visible: así una pestaña de
//! fondo también recibe sus diagnósticos, y al volver a ella ya están.
//!
//! Todo lo que corre por frame (`EstadoLsp::sincronizar`) recorre solo la
//! lista de pestañas y compara revisiones de buffer: el texto de un
//! documento se copia únicamente al abrirlo en el servidor o cuando de
//! verdad cambió (BACKLOG.md P1 #14).

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::task::Poll;
use std::time::Duration;

use anyhow::Result;
use lsp_types::{
    ClientCapabilities, DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DocumentFormattingParams, FormattingOptions, InitializeParams, InitializedParams, Position, TextDocumentIdentifier,
    TextDocumentItem, Uri, VersionedTextDocumentIdentifier,
};
use serde_json::{json, Value};
use tcode_config::Config;
use tcode_lsp::{CapacidadesLsp, Cliente, EdicionTexto, MensajeEntrante, ModoSincronizacion};
use tcode_syntax::Lenguaje;
use tcode_ui::{Layout as PanelLayout, PanelEditor};

/// Comando + argumentos + variables de entorno con los que se lanza un
/// servidor (ver [`comando_efectivo`]).
type ComandoLsp = (String, Vec<String>, BTreeMap<String, String>);

/// Handshake en curso o completo con el servidor de una sesión.
enum Fase {
    /// Se envió `initialize` con este id, se espera su respuesta. Los
    /// documentos que se agregan mientras tanto solo se anotan: su
    /// `didOpen` sale todo junto al completar el handshake.
    Iniciando { id_initialize: i64 },
    /// `initialize`/`initialized` completos: se mandan `didOpen`/
    /// `didChange`/`didClose` y se procesan diagnósticos entrantes.
    /// `modo` es cómo pidió el servidor recibir los cambios (texto
    /// completo o solo el rango editado, ver
    /// `ModoSincronizacion::desde_initialize`).
    Listo { modo: ModoSincronizacion },
}

/// Un documento abierto en el servidor de una sesión. La sincronización
/// incremental es por documento (cada uno tiene su propia versión y su
/// propio texto base para calcular el próximo cambio), no por sesión.
struct DocumentoLsp {
    uri: Uri,
    version: i32,
    /// El texto tal como lo tiene el servidor: base del próximo cambio
    /// incremental y de la conversión UTF-16 → carácter de las columnas
    /// de sus diagnósticos.
    ultimo_texto_enviado: String,
    /// `Buffer::revision` de `ultimo_texto_enviado`: con la misma
    /// revisión el texto es el mismo, y `sincronizar_documento` no
    /// necesita ni copiarlo ni compararlo (BACKLOG.md P1 #14). Las
    /// revisiones son únicas entre TODOS los buffers (no por buffer), así
    /// que esto sigue siendo cierto aunque el mismo archivo esté abierto
    /// en dos paneles con buffers distintos y el que se sincroniza pase
    /// de uno al otro.
    ultima_revision_enviada: u64,
}

struct SesionLsp {
    cliente: Cliente,
    lenguaje: Lenguaje,
    /// `comando_efectivo` en el momento del lanzamiento: si el usuario lo
    /// edita desde el panel de administración (variables de entorno
    /// incluidas), `sincronizar` nota la diferencia y relanza SOLO esta
    /// sesión.
    comando_usado: ComandoLsp,
    fase: Fase,
    /// Si el servidor anunció `documentFormattingProvider` al responder
    /// `initialize` (BACKLOG.md P2 #5) — `false` hasta entonces, y para
    /// siempre en servidores que no formatean (pyright). Se mira ANTES de
    /// mandar `textDocument/formatting`, para no esperar en vano una
    /// respuesta que va a ser un error "método no soportado".
    soporta_formateo: bool,
    /// Qué funciones de navegación/edición anunció el servidor
    /// (BACKLOG.md P1 #17) — todo `false` hasta que responde
    /// `initialize`.
    capacidades: CapacidadesLsp,
    /// Documentos de este lenguaje abiertos en alguna pestaña, uno por
    /// URI (el mismo archivo en dos paneles es un solo documento para el
    /// servidor).
    documentos: Vec<DocumentoLsp>,
}

impl SesionLsp {
    fn indice_documento(&self, uri: &str) -> Option<usize> {
        self.documentos.iter().position(|d| d.uri.as_str() == uri)
    }

    async fn enviar_did_open(&mut self, indice: usize) {
        let documento = &self.documentos[indice];
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: documento.uri.clone(),
                language_id: self.lenguaje.id().to_string(),
                version: documento.version,
                text: documento.ultimo_texto_enviado.clone(),
            },
        };
        let _ = self.cliente.notificacion("textDocument/didOpen", params).await;
    }

    /// Empieza a seguir un documento recién abierto en alguna pestaña.
    /// Con el handshake completo se avisa ya mismo; si todavía está
    /// iniciando, alcanza con anotarlo (ver [`Fase::Iniciando`]).
    async fn abrir_documento(&mut self, uri: Uri, texto: String, revision: u64) {
        self.documentos.push(DocumentoLsp { uri, version: 1, ultimo_texto_enviado: texto, ultima_revision_enviada: revision });
        if let Fase::Listo { .. } = self.fase {
            self.enviar_did_open(self.documentos.len() - 1).await;
        }
    }

    /// Deja de seguir un documento cuya última pestaña se cerró (o que
    /// cambió de ruta con "Guardar como"): `didClose`, así el servidor
    /// libera lo que tenga de él y deja de publicarle diagnósticos.
    async fn cerrar_documento(&mut self, indice: usize) {
        let documento = self.documentos.remove(indice);
        if let Fase::Listo { .. } = self.fase {
            let params = DidCloseTextDocumentParams { text_document: TextDocumentIdentifier { uri: documento.uri } };
            let _ = self.cliente.notificacion("textDocument/didClose", params).await;
        }
    }

    /// Si el contenido del documento `indice` cambió desde el último
    /// envío, notifica `textDocument/didChange`. `revision` es la del
    /// `Buffer` que lo representa (`Buffer::revision`) y `texto_actual`
    /// da su texto: si la revisión es la del último envío no se hace nada
    /// — ni copiar ni comparar el archivo, el caso de casi todos los
    /// frames. Si cambió, se manda solo el rango editado cuando el
    /// servidor anunció sincronización incremental, o el texto completo
    /// si no (`tcode_lsp::cambio_entre`) — con pyright y 10.000 líneas,
    /// mandar y que el servidor procese el texto entero por cada frame
    /// con cambios era el costo más grande que quedaba al tipear.
    async fn sincronizar_documento(&mut self, indice: usize, revision: u64, texto_actual: impl FnOnce() -> String) {
        let Fase::Listo { modo } = self.fase else { return };
        let documento = &mut self.documentos[indice];
        if documento.ultima_revision_enviada == revision {
            return;
        }
        let texto_actual = texto_actual();
        let Some(cambio) = tcode_lsp::cambio_entre(&documento.ultimo_texto_enviado, &texto_actual, modo) else {
            documento.ultima_revision_enviada = revision;
            return;
        };

        documento.version += 1;
        let params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier { uri: documento.uri.clone(), version: documento.version },
            content_changes: vec![cambio],
        };
        if self.cliente.notificacion("textDocument/didChange", params).await.is_ok() {
            // Hace falta el texto entero igual: es la base del próximo
            // cambio incremental y de la conversión UTF-16 → carácter de
            // los diagnósticos (`procesar_mensaje`).
            let documento = &mut self.documentos[indice];
            documento.ultimo_texto_enviado = texto_actual;
            documento.ultima_revision_enviada = revision;
        }
    }
}

/// Un lenguaje cuyo servidor no arrancó (no está instalado, falló el
/// `initialize`) o se murió solo. No se reintenta en cada frame — lanzar
/// un binario inexistente 60 veces por segundo no le sirve a nadie —
/// sino cuando cambia algo que podría arreglarlo: el comando configurado,
/// deshabilitar y volver a habilitar el lenguaje, o cerrar todos sus
/// documentos y volver a abrir alguno. Guarda los últimos logs de stderr
/// (más una línea propia con el motivo) para que `Ctrl+K R` siga
/// mostrando por qué falló.
struct Fallo {
    lenguaje: Lenguaje,
    comando: ComandoLsp,
    logs: Vec<String>,
    total_logs: u64,
}

/// Cuánto se espera, como mucho, la respuesta a `textDocument/formatting`
/// antes de guardar igual sin formatear (BACKLOG.md P2 #5: formatear
/// nunca puede bloquear el guardado). Mientras tanto la UI no se redibuja
/// (ver `EstadoLsp::pedir_formateo`), así que tiene que ser corto; 2 s
/// alcanza de sobra para rust-analyzer + rustfmt (unos 100-300 ms en un
/// archivo normal, medido en tmux) incluso con el primer `rustfmt` en
/// frío.
const TIMEOUT_FORMATEO: Duration = Duration::from_secs(2);

/// Comando + argumentos + variables de entorno a usar para lanzar el LSP
/// de `lenguaje` (PLAN.md §5.3): lo que configuró el usuario a mano en
/// la sección "Lenguajes / LSP", si hay algo, o si no el comando por
/// defecto de `tcode_lsp::comando_para` (que puede no haber ninguno, como
/// para todos los lenguajes salvo Python por ahora) sin ninguna variable
/// de entorno — los defaults embebidos nunca las necesitan.
pub fn comando_efectivo(lenguaje: Lenguaje, config: &Config) -> Option<ComandoLsp> {
    if let Some(personalizado) = config.lenguajes.comando_configurado(lenguaje.id()) {
        return Some((personalizado.comando.clone(), personalizado.argumentos.clone(), personalizado.env.clone()));
    }
    tcode_lsp::comando_para(lenguaje)
        .map(|(comando, args)| (comando.to_string(), args.iter().map(|a| a.to_string()).collect(), BTreeMap::new()))
}

/// Lo que le toca a una sesión según los documentos abiertos ahora: su
/// lenguaje, el comando con el que tiene que estar corriendo, y qué
/// documentos (índices en la lista que recibió [`repartir_por_lenguaje`])
/// tiene que tener abiertos.
#[derive(Debug, PartialEq)]
struct Reparto {
    lenguaje: Lenguaje,
    comando: ComandoLsp,
    documentos: Vec<usize>,
}

/// Qué sesión corresponde a cada documento abierto. `documentos` son
/// pares (ruta mostrada, URI) de todas las pestañas: se agrupan por
/// lenguaje en el orden en que aparecen, dejando afuera los que no tienen
/// lenguaje (un `.txt`, un "[Sin nombre]"), los de lenguajes con el LSP
/// deshabilitado o sin comando conocido, y las repeticiones de un URI ya
/// visto — el mismo archivo abierto en dos paneles se queda con la
/// PRIMERA aparición, por eso quien llama pone primero el documento
/// activo (ver `EstadoLsp::sincronizar`).
fn repartir_por_lenguaje(documentos: &[(&str, &str)], config: &Config) -> Vec<Reparto> {
    let mut repartos: Vec<Reparto> = Vec::new();
    let mut vistos: Vec<&str> = Vec::new();
    for (indice, &(ruta, uri)) in documentos.iter().enumerate() {
        let Some(lenguaje) = Lenguaje::detectar_por_extension(ruta) else { continue };
        if !config.lenguajes.lsp_habilitado(lenguaje.id()) || vistos.contains(&uri) {
            continue;
        }
        match repartos.iter_mut().find(|r| r.lenguaje == lenguaje) {
            Some(reparto) => reparto.documentos.push(indice),
            None => {
                let Some(comando) = comando_efectivo(lenguaje, config) else { continue };
                repartos.push(Reparto { lenguaje, comando, documentos: vec![indice] });
            }
        }
        vistos.push(uri);
    }
    repartos
}

/// Qué documentos abrir y cerrar en una sesión para pasar de los URIs que
/// tiene abiertos (`abiertos`) a los que debería tener (`deseados`):
/// índices en `abiertos` a cerrar (de menor a mayor) e índices en
/// `deseados` a abrir.
fn diferencia_documentos(abiertos: &[&str], deseados: &[&str]) -> (Vec<usize>, Vec<usize>) {
    let cerrar = (0..abiertos.len()).filter(|&i| !deseados.contains(&abiertos[i])).collect();
    let abrir = (0..deseados.len()).filter(|&i| !abiertos.contains(&deseados[i])).collect();
    (cerrar, abrir)
}

/// Lanza el servidor de `lenguaje` con `comando` y le manda `initialize`.
/// Devuelve la sesión en [`Fase::Iniciando`] y sin documentos (quien
/// llama los agrega), o el motivo por el que no arrancó, como línea de
/// log para [`Fallo`].
async fn lanzar_sesion(lenguaje: Lenguaje, comando: ComandoLsp) -> std::result::Result<SesionLsp, String> {
    let (programa, args, env) = &comando;
    let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
    let env_ref: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let mut cliente = Cliente::lanzar(programa, &args_ref, &env_ref).await.map_err(|e| format!("[tcode] {e:#}"))?;

    // Lo que se declara explícitamente: `formatting` (BACKLOG.md P2 #5) y
    // las funciones de navegación/edición de BACKLOG.md P1 #17
    // (definición, referencias, hover, completado, renombrar), todo sin
    // registro dinámico (`tcode` no responde `client/registerCapability`),
    // para que un servidor que decide qué anunciar según lo que soporta
    // el cliente lo anuncie de forma estática. El completado declara NO
    // soportar snippets (el servidor manda texto plano, que es lo que
    // `tcode` inserta) y el hover prefiere texto plano (el markdown igual
    // se muestra como texto, `tcode_lsp::texto_hover`). Los cambios de
    // un renombrado pueden venir como `documentChanges`, pero sin
    // operaciones sobre archivos (crear/renombrar/borrar).
    let capabilities: ClientCapabilities = serde_json::from_value(json!({
        "textDocument": {
            "formatting": { "dynamicRegistration": false },
            "definition": { "dynamicRegistration": false, "linkSupport": true },
            "references": { "dynamicRegistration": false },
            "hover": { "dynamicRegistration": false, "contentFormat": ["plaintext", "markdown"] },
            "completion": {
                "dynamicRegistration": false,
                "completionItem": { "snippetSupport": false, "insertReplaceSupport": true },
                "contextSupport": true,
            },
            "rename": { "dynamicRegistration": false, "prepareSupport": false },
        },
        "workspace": { "workspaceEdit": { "documentChanges": true } },
    }))
    .unwrap_or_default();
    // El directorio actual como carpeta del proyecto (BACKLOG.md P1
    // #17): sin ninguna, pyright trata cada archivo abierto como suelto y
    // renombrar solo toca el archivo actual (buscar referencias sí mira
    // los demás abiertos). Si no se puede convertir, sin carpeta, como
    // antes.
    let carpeta = std::env::current_dir().ok().and_then(|dir| {
        let uri = uri_de_archivo(&dir).ok()?;
        let nombre = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Some(lsp_types::WorkspaceFolder { uri, name: nombre })
    });
    #[allow(deprecated)] // `root_uri`: la spec lo reemplazó por `workspace_folders`, pero hay servidores que solo miran este.
    let params = InitializeParams {
        process_id: Some(std::process::id()),
        capabilities,
        root_uri: carpeta.as_ref().map(|c| c.uri.clone()),
        workspace_folders: carpeta.map(|c| vec![c]),
        ..Default::default()
    };
    let Ok(id_initialize) = cliente.peticion("initialize", params).await else {
        cliente.matar().await;
        return Err(format!("[tcode] '{programa}' se cerró antes de recibir `initialize`"));
    };

    Ok(SesionLsp {
        cliente,
        lenguaje,
        comando_usado: comando,
        fase: Fase::Iniciando { id_initialize },
        soporta_formateo: false,
        capacidades: CapacidadesLsp::default(),
        documentos: Vec::new(),
    })
}

/// Las peticiones de navegación/edición (BACKLOG.md P1 #17). Cada una
/// corresponde a un método de `textDocument/*` y a una capacidad del
/// servidor (`CapacidadesLsp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoPedido {
    Definicion,
    Referencias,
    Hover,
    Completado,
    Renombrar,
}

impl TipoPedido {
    fn metodo(self) -> &'static str {
        match self {
            TipoPedido::Definicion => "textDocument/definition",
            TipoPedido::Referencias => "textDocument/references",
            TipoPedido::Hover => "textDocument/hover",
            TipoPedido::Completado => "textDocument/completion",
            TipoPedido::Renombrar => "textDocument/rename",
        }
    }

    fn soportado(self, capacidades: &CapacidadesLsp) -> bool {
        match self {
            TipoPedido::Definicion => capacidades.definicion,
            TipoPedido::Referencias => capacidades.referencias,
            TipoPedido::Hover => capacidades.hover,
            TipoPedido::Completado => capacidades.completado,
            TipoPedido::Renombrar => capacidades.renombrar,
        }
    }

    /// Aviso para la barra de estado cuando el servidor no lo anuncia.
    fn aviso_no_soportado(self) -> &'static str {
        match self {
            TipoPedido::Definicion => "el LSP no soporta ir a la definición",
            TipoPedido::Referencias => "el LSP no soporta buscar referencias",
            TipoPedido::Hover => "el LSP no soporta mostrar información (hover)",
            TipoPedido::Completado => "el LSP no soporta autocompletar",
            TipoPedido::Renombrar => "el LSP no soporta renombrar",
        }
    }
}

/// Una petición de [`TipoPedido`] en vuelo: se reconoce su respuesta por
/// el lenguaje de la sesión y el id.
struct Pendiente {
    lenguaje: Lenguaje,
    id: i64,
    tipo: TipoPedido,
}

/// La respuesta (cruda) a una petición de [`TipoPedido`], o el motivo
/// del error — `app` la interpreta con `tcode_lsp::parsear_*` contra el
/// estado de la UI en el momento en que llega, no en el que se pidió.
pub struct RespuestaLsp {
    pub tipo: TipoPedido,
    pub resultado: std::result::Result<Value, String>,
}

/// Estado LSP de la aplicación: una sesión por lenguaje con documentos
/// abiertos (ver la nota del módulo).
#[derive(Default)]
pub struct EstadoLsp {
    sesiones: Vec<SesionLsp>,
    fallos: Vec<Fallo>,
    /// Peticiones de navegación/edición en vuelo (BACKLOG.md P1 #17), a
    /// lo sumo una por tipo: pedir otra del mismo tipo cancela la
    /// anterior (`$/cancelRequest`) — el completado se vuelve a pedir en
    /// cada pausa al tipear, y solo importa la última.
    pendientes: Vec<Pendiente>,
    /// Respuestas a esas peticiones ya llegadas, en orden, hasta que
    /// `app` las toma (`tomar_respuestas`). Una cola en vez de
    /// devolverlas desde `procesar_mensaje` porque también las procesa
    /// la espera de `pedir_formateo`, que no sabría qué hacer con ellas:
    /// así ninguna se pierde.
    respuestas: Vec<RespuestaLsp>,
    /// Ruta mostrada → URI, calculado una sola vez por ruta (`uri_de_
    /// archivo` pregunta el directorio actual y codifica la ruta entera:
    /// no es algo para hacer por pestaña en cada frame). `None` si la
    /// ruta no se puede convertir. Lo usan también los diagnósticos para
    /// encontrar las pestañas de un URI.
    uris: HashMap<String, Option<Uri>>,
    /// Lenguaje del documento activo en el último `sincronizar`: a qué
    /// sesión le pertenecen los logs de `Ctrl+K R`.
    lenguaje_activo: Option<Lenguaje>,
    /// Por qué sesión empieza a mirar `siguiente_mensaje` la próxima vez,
    /// para que un servidor muy hablador no tape a los demás.
    turno: usize,
}

impl EstadoLsp {
    pub fn nuevo() -> Self {
        Self::default()
    }

    /// Una vez por frame (y justo antes de formatear al guardar): pone a
    /// las sesiones al día con lo que hay abierto en `layout`. Lanza la
    /// sesión de un lenguaje la primera vez que aparece un documento suyo
    /// (en cualquier pestaña de cualquier panel), cierra la de un
    /// lenguaje del que ya no queda ninguno — o que se deshabilitó desde
    /// el panel de administración, o cuyo comando cambió (en ese caso se
    /// relanza solo esa) —, manda `didOpen`/`didClose` a medida que se
    /// abren y cierran pestañas, y `didChange` de cada documento cuyo
    /// texto cambió.
    ///
    /// Las sesiones que sobran se cierran con el protocolo educado
    /// (`shutdown` + `exit`) en una tarea aparte: acá no se espera a
    /// nadie — esto corre en el camino de cada tecla, y el servidor viejo
    /// puede tardar hasta un segundo en contestar.
    pub async fn sincronizar(&mut self, layout: &PanelLayout, config: &Config) {
        let activo = layout.panel_activo();
        self.lenguaje_activo = Lenguaje::detectar_por_extension(&activo.ruta_mostrada);

        // El activo primero: si el mismo archivo está abierto en otro
        // panel (con otro buffer), es el suyo el que se le manda al
        // servidor — es donde se está tipeando.
        let mut documentos = layout.documentos();
        if let Some(posicion) = documentos.iter().position(|d| std::ptr::eq(*d, activo)) {
            let documento = documentos.remove(posicion);
            documentos.insert(0, documento);
        }

        for documento in &documentos {
            let ruta = documento.ruta_mostrada.as_str();
            if !self.uris.contains_key(ruta) && Lenguaje::detectar_por_extension(ruta).is_some() {
                self.uris.insert(ruta.to_string(), uri_de_archivo(Path::new(ruta)).ok());
            }
        }
        // Que el caché no crezca sin límite en una sesión larga abriendo
        // y cerrando archivos: se poda (rara vez) a lo abierto ahora.
        if self.uris.len() > documentos.len() + 64 {
            self.uris.retain(|ruta, _| documentos.iter().any(|d| d.ruta_mostrada == *ruta));
        }

        let candidatos: Vec<(&PanelEditor, &Uri)> = documentos
            .iter()
            .filter_map(|d| Some((*d, self.uris.get(d.ruta_mostrada.as_str())?.as_ref()?)))
            .collect();
        let pares: Vec<(&str, &str)> = candidatos.iter().map(|(d, uri)| (d.ruta_mostrada.as_str(), uri.as_str())).collect();
        let repartos = repartir_por_lenguaje(&pares, config);

        let mut indice = 0;
        while indice < self.sesiones.len() {
            let sesion = &self.sesiones[indice];
            if repartos.iter().any(|r| r.lenguaje == sesion.lenguaje && r.comando == sesion.comando_usado) {
                indice += 1;
            } else {
                let sesion = self.sesiones.remove(indice);
                tokio::spawn(sesion.cliente.cerrar());
            }
        }
        self.fallos.retain(|f| repartos.iter().any(|r| r.lenguaje == f.lenguaje && r.comando == f.comando));

        for reparto in repartos {
            if self.fallos.iter().any(|f| f.lenguaje == reparto.lenguaje) {
                continue;
            }
            let deseados: Vec<(&PanelEditor, &Uri)> = reparto.documentos.iter().map(|&i| candidatos[i]).collect();

            let Some(sesion) = self.sesiones.iter_mut().find(|s| s.lenguaje == reparto.lenguaje) else {
                match lanzar_sesion(reparto.lenguaje, reparto.comando.clone()).await {
                    Ok(mut sesion) => {
                        for (documento, uri) in deseados {
                            let buffer = documento.editor.buffer();
                            sesion.abrir_documento(uri.clone(), buffer.a_texto(), buffer.revision()).await;
                        }
                        self.sesiones.push(sesion);
                    }
                    Err(motivo) => self.fallos.push(Fallo {
                        lenguaje: reparto.lenguaje,
                        comando: reparto.comando,
                        logs: vec![motivo],
                        total_logs: 1,
                    }),
                }
                continue;
            };

            let (cerrar, abrir) = {
                let abiertos: Vec<&str> = sesion.documentos.iter().map(|d| d.uri.as_str()).collect();
                let uris_deseados: Vec<&str> = deseados.iter().map(|(_, uri)| uri.as_str()).collect();
                diferencia_documentos(&abiertos, &uris_deseados)
            };
            for indice in cerrar.into_iter().rev() {
                sesion.cerrar_documento(indice).await;
            }
            for indice in abrir {
                let (documento, uri) = deseados[indice];
                let buffer = documento.editor.buffer();
                sesion.abrir_documento(uri.clone(), buffer.a_texto(), buffer.revision()).await;
            }
            for (documento, uri) in deseados {
                if let Some(indice) = sesion.indice_documento(uri.as_str()) {
                    let buffer = documento.editor.buffer();
                    sesion.sincronizar_documento(indice, buffer.revision(), || buffer.a_texto()).await;
                }
            }
        }
    }

    /// Espera el siguiente mensaje de CUALQUIERA de las sesiones, junto
    /// con el lenguaje de la sesión que lo mandó (`None` en el mensaje:
    /// ese servidor se murió). Nunca resuelve si no hay sesiones —
    /// pensado para usarse dentro del `tokio::select!` de `ejecutar` junto
    /// al stream de teclado, sin bloquearlo. Se sondea el canal de cada
    /// sesión sin tareas ni canales intermedios: el future se vuelve a
    /// armar en cada vuelta del bucle, así que siempre ve las sesiones
    /// actuales, y cancelarlo (llegó una tecla antes) no pierde nada.
    pub async fn siguiente_mensaje(&mut self) -> (Lenguaje, Option<MensajeEntrante>) {
        std::future::poll_fn(|cx| {
            let total = self.sesiones.len();
            for paso in 0..total {
                let indice = (self.turno + paso) % total;
                let sesion = &mut self.sesiones[indice];
                if let Poll::Ready(mensaje) = sesion.cliente.receptor.poll_recv(cx) {
                    let lenguaje = sesion.lenguaje;
                    self.turno = indice + 1;
                    return Poll::Ready((lenguaje, mensaje));
                }
            }
            Poll::Pending
        })
        .await
    }

    /// Procesa un mensaje ya recibido de la sesión de `lenguaje`:
    /// completa el handshake si era la respuesta a `initialize` (y abre
    /// en el servidor todos los documentos anotados mientras tanto), o
    /// actualiza los diagnósticos de las pestañas de ese archivo si era un
    /// `publishDiagnostics` — sea cual sea el documento, esté visible o
    /// no. `None` quiere decir que el servidor se murió: la sesión pasa a
    /// [`Fallo`] sin tocar a las demás.
    pub async fn procesar_mensaje(&mut self, lenguaje: Lenguaje, mensaje: Option<MensajeEntrante>, layout: &mut PanelLayout) {
        let Some(indice) = self.sesiones.iter().position(|s| s.lenguaje == lenguaje) else { return };
        let Some(mensaje) = mensaje else {
            self.marcar_caida(indice);
            return;
        };
        let sesion = &mut self.sesiones[indice];

        match mensaje {
            MensajeEntrante::Respuesta { id, resultado } => {
                if let Some(posicion) = self.pendientes.iter().position(|p| p.lenguaje == lenguaje && p.id == id) {
                    let pendiente = self.pendientes.remove(posicion);
                    let resultado = resultado.map_err(|error| {
                        error["message"].as_str().unwrap_or("error sin mensaje").lines().next().unwrap_or("").to_string()
                    });
                    self.respuestas.push(RespuestaLsp { tipo: pendiente.tipo, resultado });
                    return;
                }
                if let Fase::Iniciando { id_initialize } = sesion.fase {
                    if let (true, Ok(resultado)) = (id == id_initialize, &resultado) {
                        let modo = ModoSincronizacion::desde_initialize(resultado);
                        sesion.soporta_formateo = tcode_lsp::soporta_formateo(resultado);
                        sesion.capacidades = CapacidadesLsp::desde_initialize(resultado);
                        let _ = sesion.cliente.notificacion("initialized", InitializedParams {}).await;
                        for indice in 0..sesion.documentos.len() {
                            sesion.enviar_did_open(indice).await;
                        }
                        sesion.fase = Fase::Listo { modo };
                    }
                }
            }
            MensajeEntrante::Notificacion { metodo, params } => {
                if metodo != "textDocument/publishDiagnostics" {
                    return;
                }
                let Some(uri) = params.get("uri").and_then(Value::as_str) else { return };
                // Un documento que ya se cerró (o de otro lenguaje) no
                // tiene a quién mostrarle nada.
                let Some(documento) = sesion.documentos.iter().find(|d| d.uri.as_str() == uri) else { return };
                // El texto tal como lo tiene el servidor — hace falta
                // para la conversión UTF-16 → carácter de las columnas.
                let Ok((_, diagnosticos)) = tcode_lsp::parsear_diagnosticos(&params, &documento.ultimo_texto_enviado) else {
                    return;
                };
                for panel in layout.paneles_mut() {
                    let es_este = self
                        .uris
                        .get(panel.ruta_mostrada.as_str())
                        .and_then(Option::as_ref)
                        .is_some_and(|u| u.as_str() == uri);
                    if es_este {
                        panel.diagnosticos = diagnosticos.clone();
                    }
                }
            }
        }
    }

    /// El servidor de la sesión `indice` se murió (su stdout se cerró):
    /// pasa a [`Fallo`], con sus logs más una línea que lo dice, y se
    /// recoge el proceso en segundo plano para que no quede como zombi.
    fn marcar_caida(&mut self, indice: usize) {
        let sesion = self.sesiones.remove(indice);
        let lenguaje = sesion.lenguaje;
        for pendiente in self.pendientes.iter().filter(|p| p.lenguaje == lenguaje) {
            self.respuestas.push(RespuestaLsp { tipo: pendiente.tipo, resultado: Err("el LSP se cerró".to_string()) });
        }
        self.pendientes.retain(|p| p.lenguaje != lenguaje);
        let (mut logs, mut total_logs) = sesion.cliente.logs_con_total();
        logs.push(format!("[tcode] el servidor '{}' se cerró inesperadamente", sesion.comando_usado.0));
        total_logs += 1;
        self.fallos.push(Fallo { lenguaje: sesion.lenguaje, comando: sesion.comando_usado, logs, total_logs });
        tokio::spawn(sesion.cliente.matar());
    }

    /// Pide `textDocument/formatting` para el archivo `ruta` (cuyo texto
    /// actual es `texto`) a la sesión de su lenguaje y espera la
    /// respuesta, como mucho [`TIMEOUT_FORMATEO`] — lo usa el guardado
    /// con "formatear al guardar" prendido (`guardar_archivo_activo`,
    /// `app/main.rs`, BACKLOG.md P2 #5). Devuelve las ediciones ya
    /// traducidas a offsets de bytes sobre `texto`, o el motivo (texto
    /// corto, para la barra de estado) por el que no se formateó: no hay
    /// sesión para ese lenguaje, todavía está iniciando, el servidor no
    /// anuncia `documentFormattingProvider`, el documento no está abierto
    /// en él, respondió con error, o no respondió a tiempo. Nunca falla de
    /// otra forma: quien llama guarda igual en cualquiera de esos casos.
    ///
    /// Quien llama tiene que haber sincronizado el texto antes
    /// (`sincronizar_lsp`): las posiciones de la respuesta se refieren al
    /// documento que tiene el SERVIDOR, así que si no coincide con `texto`
    /// no se pide nada (aplicarlas sobre otro texto rompería el archivo).
    ///
    /// La espera es un bucle propio sobre el canal de esa sesión (el mismo
    /// que sondea `siguiente_mensaje`, no un segundo lector): la respuesta
    /// se reconoce por su id, y cualquier otro mensaje de esa sesión que
    /// llegue mientras tanto se procesa ahí mismo con `procesar_mensaje`,
    /// igual que lo habría hecho el bucle principal; los de las demás
    /// sesiones esperan en sus canales. Si se vence el tiempo se manda
    /// `$/cancelRequest`; si la respuesta llega igual más tarde, el bucle
    /// principal la recibe con un id que nadie espera y la ignora.
    pub async fn pedir_formateo(
        &mut self,
        ruta: &str,
        texto: &str,
        opciones: FormattingOptions,
        layout: &mut PanelLayout,
    ) -> std::result::Result<Vec<EdicionTexto>, &'static str> {
        let Some(lenguaje) = Lenguaje::detectar_por_extension(ruta) else { return Err("sin LSP activo") };
        let Some(sesion) = self.sesiones.iter_mut().find(|s| s.lenguaje == lenguaje) else {
            return Err(if self.fallos.iter().any(|f| f.lenguaje == lenguaje) {
                "el LSP no arrancó o se cerró"
            } else {
                "sin LSP activo"
            });
        };
        let Fase::Listo { .. } = sesion.fase else { return Err("el LSP todavía está iniciando") };
        if !sesion.soporta_formateo {
            return Err("el LSP no soporta formatear");
        }
        let Ok(uri) = uri_de_archivo(Path::new(ruta)) else { return Err("sin LSP activo") };
        let Some(documento) = sesion.documentos.iter().find(|d| d.uri.as_str() == uri.as_str()) else {
            return Err("el documento no está abierto en el LSP");
        };
        if documento.ultimo_texto_enviado != texto {
            return Err("el LSP no tiene el texto al día");
        }

        let params = DocumentFormattingParams {
            text_document: TextDocumentIdentifier { uri },
            options: opciones,
            work_done_progress_params: Default::default(),
        };
        let Ok(id_formateo) = sesion.cliente.peticion("textDocument/formatting", params).await else {
            return Err("no se pudo hablar con el LSP");
        };

        let limite = tokio::time::Instant::now() + TIMEOUT_FORMATEO;
        loop {
            let Some(sesion) = self.sesiones.iter_mut().find(|s| s.lenguaje == lenguaje) else {
                return Err("el LSP se cerró");
            };
            match tokio::time::timeout_at(limite, sesion.cliente.receptor.recv()).await {
                Err(_) => {
                    let _ = sesion.cliente.notificacion("$/cancelRequest", json!({ "id": id_formateo })).await;
                    return Err("el LSP tardó demasiado en formatear");
                }
                Ok(None) => {
                    self.procesar_mensaje(lenguaje, None, layout).await;
                    return Err("el LSP se cerró");
                }
                Ok(Some(MensajeEntrante::Respuesta { id, resultado })) if id == id_formateo => {
                    return match resultado {
                        Ok(valor) => {
                            tcode_lsp::parsear_ediciones_formateo(&valor, texto).map_err(|_| "respuesta de formato inválida")
                        }
                        Err(_) => Err("el LSP devolvió un error al formatear"),
                    };
                }
                Ok(Some(otro)) => self.procesar_mensaje(lenguaje, Some(otro), layout).await,
            }
        }
    }

    /// Capacidades anunciadas por la sesión del lenguaje de `ruta`, si
    /// hay una con el handshake completo — lo mira `app` al tipear, para
    /// saber si un carácter es de disparo del completado. Barato: no toca
    /// el texto de nada.
    pub fn capacidades(&self, ruta: &str) -> Option<&CapacidadesLsp> {
        let lenguaje = Lenguaje::detectar_por_extension(ruta)?;
        let sesion = self.sesiones.iter().find(|s| s.lenguaje == lenguaje)?;
        matches!(sesion.fase, Fase::Listo { .. }).then_some(&sesion.capacidades)
    }

    /// Manda una petición de [`TipoPedido`] sobre el archivo `ruta` en
    /// `posicion` (coordenadas LSP) a la sesión de su lenguaje, SIN
    /// esperar la respuesta: llega más tarde por el `select!` del bucle
    /// de `ejecutar` (`procesar_mensaje` la encola) y `app` la toma con
    /// [`Self::tomar_respuestas`]. `extra` se agrega a los parámetros
    /// (`context` del completado y de las referencias, `newName` del
    /// renombrado). Si ya había una del mismo tipo en vuelo se cancela.
    ///
    /// Quien llama tiene que haber sincronizado antes (`sincronizar_lsp`):
    /// la posición se refiere al texto actual del buffer, que tiene que
    /// ser el que tiene el servidor. El error es un aviso corto para la
    /// barra de estado (sin sesión, iniciando, no soportado...).
    pub async fn pedir(
        &mut self,
        tipo: TipoPedido,
        ruta: &str,
        posicion: Position,
        extra: Value,
    ) -> std::result::Result<(), &'static str> {
        let Some(lenguaje) = Lenguaje::detectar_por_extension(ruta) else { return Err("sin LSP para este archivo") };
        let Some(sesion) = self.sesiones.iter_mut().find(|s| s.lenguaje == lenguaje) else {
            return Err(if self.fallos.iter().any(|f| f.lenguaje == lenguaje) {
                "el LSP no arrancó o se cerró"
            } else {
                "sin LSP activo"
            });
        };
        let Fase::Listo { .. } = sesion.fase else { return Err("el LSP todavía está iniciando") };
        if !tipo.soportado(&sesion.capacidades) {
            return Err(tipo.aviso_no_soportado());
        }
        let Some(uri) = self.uris.get(ruta).cloned().flatten() else { return Err("sin LSP para este archivo") };
        if sesion.indice_documento(uri.as_str()).is_none() {
            return Err("el documento no está abierto en el LSP");
        }

        let mut params = json!({ "textDocument": { "uri": uri.as_str() }, "position": posicion });
        if let (Some(params), Value::Object(extra)) = (params.as_object_mut(), extra) {
            params.extend(extra);
        }
        if let Some(indice) = self.pendientes.iter().position(|p| p.tipo == tipo) {
            let anterior = self.pendientes.remove(indice);
            if let Some(sesion) = self.sesiones.iter_mut().find(|s| s.lenguaje == anterior.lenguaje) {
                let _ = sesion.cliente.notificacion("$/cancelRequest", json!({ "id": anterior.id })).await;
            }
        }
        let Some(sesion) = self.sesiones.iter_mut().find(|s| s.lenguaje == lenguaje) else { return Err("sin LSP activo") };
        let Ok(id) = sesion.cliente.peticion(tipo.metodo(), params).await else {
            return Err("no se pudo hablar con el LSP");
        };
        self.pendientes.push(Pendiente { lenguaje, id, tipo });
        Ok(())
    }

    /// Las respuestas a [`Self::pedir`] llegadas desde la última vez, en
    /// orden de llegada. Vacío casi siempre: no cuesta nada llamarlo en
    /// cada vuelta del bucle.
    pub fn hay_respuestas(&self) -> bool {
        !self.respuestas.is_empty()
    }

    pub fn tomar_respuestas(&mut self) -> Vec<RespuestaLsp> {
        std::mem::take(&mut self.respuestas)
    }

    /// Líneas de stderr acumuladas por la sesión del lenguaje del
    /// documento activo, de la más vieja a la más nueva (PLAN.md §5.3,
    /// "ver logs"; `Ctrl+K R`, `crates/app/src/main.rs`), más el total de
    /// líneas recibidas (`Cliente::logs_con_total`) — lo que usa el visor
    /// en vivo (`EstadoLogsLsp::actualizar`, BACKLOG.md P1 #2) para saber
    /// cuántas son nuevas. Si ese servidor no arrancó o se murió, los
    /// logs que dejó más el motivo ([`Fallo`]). `(vacío, 0)` si el
    /// documento activo no tiene sesión.
    pub fn logs_con_total(&self) -> (Vec<String>, u64) {
        let Some(lenguaje) = self.lenguaje_activo else { return Default::default() };
        if let Some(sesion) = self.sesiones.iter().find(|s| s.lenguaje == lenguaje) {
            return sesion.cliente.logs_con_total();
        }
        self.fallos
            .iter()
            .find(|f| f.lenguaje == lenguaje)
            .map(|f| (f.logs.clone(), f.total_logs))
            .unwrap_or_default()
    }

    /// Texto legible en español del estado de la sesión de `lenguaje` —
    /// `None` si no hay ninguna (el panel muestra "Inactivo" en ese caso,
    /// decidido ahí en vez de acá para no acoplar este módulo a cómo se ve
    /// la fila). "Error" si no arrancó o se murió (los logs de `Ctrl+K R`
    /// dicen por qué).
    pub fn estado_texto(&self, lenguaje: Lenguaje) -> Option<&'static str> {
        if let Some(sesion) = self.sesiones.iter().find(|s| s.lenguaje == lenguaje) {
            return Some(match sesion.fase {
                Fase::Iniciando { .. } => "Iniciando…",
                Fase::Listo { .. } => "Conectado",
            });
        }
        self.fallos.iter().any(|f| f.lenguaje == lenguaje).then_some("Error")
    }

    /// Cierra todas las sesiones al salir de tcode, con el protocolo
    /// educado (`shutdown` + `exit`, `Cliente::cerrar`) y EN PARALELO:
    /// con varios servidores, la salida sigue demorando como mucho lo
    /// mismo que con uno (el tope de `Cliente::cerrar`), no la suma.
    pub async fn cerrar(self) {
        let tareas: Vec<_> = self.sesiones.into_iter().map(|s| tokio::spawn(s.cliente.cerrar())).collect();
        for tarea in tareas {
            let _ = tarea.await;
        }
    }
}

/// Convierte una ruta de archivo a un URI `file://` válido para LSP
/// (RFC 8089), con percent-encoding RFC 3986 completo — antes solo se
/// escapaban los espacios, así que rutas con `#`, `?`, tildes u otros
/// caracteres fuera de ASCII imprimible quedaban truncadas o mal
/// interpretadas por el servidor (todo lo que sigue a un `#`/`?` sin
/// escapar se interpreta como fragmento/query del URI, no como parte de
/// la ruta). Sigue sin cubrir archivos cuyo nombre no sea UTF-8 válido
/// (poco común, y no hay forma simple/portable de acceder a los bytes
/// crudos del nombre sin código específico por SO) — `Path::display`
/// ya reemplaza esos bytes por `�` antes de que esta función los vea.
fn uri_de_archivo(ruta: &Path) -> Result<Uri> {
    let absoluta = if ruta.is_absolute() { ruta.to_path_buf() } else { std::env::current_dir()?.join(ruta) };
    let texto = codificar_ruta_para_uri(&absoluta.display().to_string(), cfg!(windows));
    format!("file://{texto}").parse::<Uri>().map_err(|e| anyhow::anyhow!("ruta no convertible a URI: {e}"))
}

/// Percent-encoding RFC 3986 de una ruta absoluta ya como texto, para
/// concatenar después de `"file://"`. `es_windows` decide la
/// normalización previa (parámetro en vez de `cfg!(windows)` acá adentro
/// para que los tests puedan ejercitar el camino de Windows sin
/// necesitar correr en Windows de verdad):
///
/// - Windows separa carpetas con `\`, no `/` — se convierten antes de
///   codificar (una vez convertida, una barra invertida ya no
///   existe como para que el paso de abajo la toque).
/// - Una ruta absoluta de Windows empieza con la letra de unidad
///   (`C:\...`), no con `/` — RFC 8089 pide anteponerle una barra más
///   para que el URI completo quede `file:///C:/...` (tres barras en
///   total: dos de la autoridad vacía, más el separador inicial del
///   path).
///
/// Después de esa normalización, cualquier byte fuera del conjunto
/// "unreserved" de la RFC (`ALPHA` / `DIGIT` / `-` / `.` / `_` / `~`) se
/// escapa como `%XX` en mayúsculas, salvo `/` que se preserva como
/// separador — incluye los dos puntos de la letra de unidad de Windows
/// (`C:` → `C%3A`), que es justo como lo hace VS Code.
fn codificar_ruta_para_uri(ruta: &str, es_windows: bool) -> String {
    let normalizada = if es_windows { ruta.replace('\\', "/") } else { ruta.to_string() };
    let con_barra_inicial = if es_windows && !normalizada.starts_with('/') {
        format!("/{normalizada}")
    } else {
        normalizada
    };

    con_barra_inicial
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests_uri {
    use super::*;

    #[test]
    fn ruta_unix_simple_no_cambia() {
        assert_eq!(codificar_ruta_para_uri("/home/user/archivo.rs", false), "/home/user/archivo.rs");
    }

    #[test]
    fn espacios_se_codifican() {
        assert_eq!(codificar_ruta_para_uri("/home/user/mi archivo.rs", false), "/home/user/mi%20archivo.rs");
    }

    #[test]
    fn caracteres_especiales_de_uri_se_codifican() {
        // "#" y "?" sin escapar romperían el URI (se interpretarían
        // como el inicio del fragmento/query) — el caso que justifica
        // esta pieza.
        assert_eq!(codificar_ruta_para_uri("/tmp/nota#1.md", false), "/tmp/nota%231.md");
        assert_eq!(codificar_ruta_para_uri("/tmp/¿qué?.txt", false), "/tmp/%C2%BFqu%C3%A9%3F.txt");
    }

    #[test]
    fn puntos_guion_y_guion_bajo_no_se_codifican() {
        assert_eq!(codificar_ruta_para_uri("/tmp/mi-archivo_v2.0.tar.gz", false), "/tmp/mi-archivo_v2.0.tar.gz");
    }

    #[test]
    fn ruta_windows_convierte_barras_y_antepone_barra_inicial() {
        assert_eq!(codificar_ruta_para_uri(r"C:\Users\nombre\archivo.rs", true), "/C%3A/Users/nombre/archivo.rs");
    }

    #[test]
    fn ruta_windows_con_espacios_y_letra_de_unidad_minuscula() {
        assert_eq!(
            codificar_ruta_para_uri(r"c:\Program Files\proyecto\main.rs", true),
            "/c%3A/Program%20Files/proyecto/main.rs"
        );
    }

    #[test]
    fn uri_de_archivo_produce_tres_barras_tras_el_esquema_en_windows() {
        let texto = format!("file://{}", codificar_ruta_para_uri(r"C:\Users\a.rs", true));
        assert_eq!(texto, "file:///C%3A/Users/a.rs");
        assert!(texto.parse::<Uri>().is_ok(), "el URI resultante debe ser válido");
    }
}


#[cfg(test)]
mod tests_ruteo {
    use super::*;
    use tcode_core::Editor;
    use tcode_ui::DireccionSplit;

    fn config_con(comandos: &[(&str, &str)]) -> Config {
        let mut config = Config::default();
        for (lenguaje, linea) in comandos {
            config.lenguajes.fijar_comando_desde_linea(lenguaje, linea);
        }
        config
    }

    #[test]
    fn cada_documento_va_a_la_sesion_de_su_lenguaje() {
        let config = config_con(&[("rust", "rust-analyzer")]);
        let documentos =
            [("a.py", "file:///a.py"), ("b.rs", "file:///b.rs"), ("notas.txt", "file:///notas.txt"), ("c.py", "file:///c.py")];
        let repartos = repartir_por_lenguaje(&documentos, &config);
        assert_eq!(repartos.len(), 2);
        assert_eq!(repartos[0].lenguaje, Lenguaje::Python);
        assert_eq!(repartos[0].documentos, vec![0, 3]);
        assert_eq!(repartos[0].comando.0, "pyright-langserver");
        assert_eq!(repartos[1].lenguaje, Lenguaje::Rust);
        assert_eq!(repartos[1].documentos, vec![1]);
    }

    #[test]
    fn sin_comando_o_deshabilitado_no_hay_sesion() {
        // Rust no tiene comando por defecto; Python está deshabilitado.
        let mut config = Config::default();
        config.lenguajes.alternar_lsp("python");
        let documentos = [("a.py", "file:///a.py"), ("b.rs", "file:///b.rs")];
        assert!(repartir_por_lenguaje(&documentos, &config).is_empty());
    }

    #[test]
    fn el_mismo_uri_en_dos_paneles_es_un_solo_documento_y_gana_el_primero() {
        let config = Config::default();
        // Misma ruta en dos paneles, y la misma escrita relativa y
        // absoluta (distinta ruta mostrada, mismo URI).
        let documentos = [("/p/a.py", "file:///p/a.py"), ("/p/a.py", "file:///p/a.py"), ("a.py", "file:///p/a.py")];
        let repartos = repartir_por_lenguaje(&documentos, &config);
        assert_eq!(repartos[0].documentos, vec![0]);
    }

    #[test]
    fn diferencia_abre_lo_nuevo_y_cierra_lo_que_ya_no_esta() {
        let (cerrar, abrir) = diferencia_documentos(&["u1", "u2", "u3"], &["u3", "u4", "u1"]);
        assert_eq!(cerrar, vec![1]);
        assert_eq!(abrir, vec![1]);
        let (cerrar, abrir) = diferencia_documentos(&["u1"], &["u1"]);
        assert!(cerrar.is_empty() && abrir.is_empty());
    }

    /// Panel 1: `a.py`. Panel 2 (split): `b.rs` y `c.py` en pestañas, con
    /// `c.py` activa.
    fn layout_de_prueba() -> PanelLayout {
        let mut layout = PanelLayout::nuevo(editor_con("a = 1\n"), "a.py".to_string());
        layout.dividir(DireccionSplit::Vertical);
        layout.abrir_en_activo(editor_con("fn main() {}\n"), "b.rs".to_string());
        layout.abrir_en_activo(editor_con("c = 2\n"), "c.py".to_string());
        layout
    }

    /// Con texto, para que abrir otro archivo no lo tome por un "[Sin
    /// nombre]" descartable y lo reemplace.
    fn editor_con(texto: &str) -> Editor {
        let mut editor = Editor::nuevo();
        editor.insertar_texto(texto);
        editor
    }

    fn documentos_de(lsp: &EstadoLsp, lenguaje: Lenguaje) -> usize {
        lsp.sesiones.iter().find(|s| s.lenguaje == lenguaje).map_or(0, |s| s.documentos.len())
    }

    /// Con procesos de verdad pero sin servidores LSP reales: `cat` nunca
    /// contesta `initialize` (se queda "Iniciando…"), un binario
    /// inexistente no arranca y `true` se muere enseguida.
    #[tokio::test]
    async fn una_sesion_por_lenguaje_que_sigue_a_las_pestanas_y_aisla_fallos() {
        let mut layout = layout_de_prueba();
        let mut lsp = EstadoLsp::nuevo();
        let config = config_con(&[("python", "cat"), ("rust", "comando-que-no-existe-tcode")]);

        lsp.sincronizar(&layout, &config).await;
        assert_eq!(lsp.sesiones.len(), 1);
        assert_eq!(documentos_de(&lsp, Lenguaje::Python), 2, "a.py y c.py, de paneles distintos, en la misma sesión");
        assert_eq!(lsp.estado_texto(Lenguaje::Python), Some("Iniciando…"));
        assert_eq!(lsp.estado_texto(Lenguaje::Rust), Some("Error"), "no arrancó, pero no afecta a Python");
        assert_eq!(lsp.lenguaje_activo, Some(Lenguaje::Python));

        // Otro frame sin cambios: nada se relanza ni se reintenta.
        lsp.sincronizar(&layout, &config).await;
        assert_eq!(lsp.sesiones.len(), 1);
        assert_eq!(lsp.fallos.len(), 1);

        // Cerrar la pestaña de c.py: la sesión sigue, con un documento menos.
        layout.cerrar_pestana_activa();
        lsp.sincronizar(&layout, &config).await;
        assert_eq!(documentos_de(&lsp, Lenguaje::Python), 1);
        assert_eq!(lsp.lenguaje_activo, Some(Lenguaje::Rust));
        assert!(lsp.logs_con_total().0[0].contains("comando-que-no-existe-tcode"), "Ctrl+K R muestra por qué no arrancó");

        // Cambiar el comando de Rust relanza solo Rust; Python pasa a un
        // servidor que se muere solo, y eso tampoco toca a Rust.
        let config = config_con(&[("python", "true"), ("rust", "cat")]);
        lsp.sincronizar(&layout, &config).await;
        assert_eq!(lsp.estado_texto(Lenguaje::Rust), Some("Iniciando…"));
        // `cat` le devuelve a tcode lo que recibe (métodos que se
        // ignoran): se procesan hasta que llega el cierre de Python.
        loop {
            let (lenguaje, mensaje) =
                tokio::time::timeout(Duration::from_secs(5), lsp.siguiente_mensaje()).await.expect("llega el cierre");
            let cerro_python = lenguaje == Lenguaje::Python && mensaje.is_none();
            assert!(mensaje.is_some() || cerro_python, "solo Python se muere");
            lsp.procesar_mensaje(lenguaje, mensaje, &mut layout).await;
            if cerro_python {
                break;
            }
        }
        assert_eq!(lsp.estado_texto(Lenguaje::Python), Some("Error"));
        assert_eq!(lsp.estado_texto(Lenguaje::Rust), Some("Iniciando…"));
        lsp.sincronizar(&layout, &config).await;
        assert_eq!(lsp.sesiones.len(), 1, "el caído no se relanza en cada frame");

        // Sin documentos de ningún lenguaje con LSP, no queda nada.
        let layout = PanelLayout::nuevo(Editor::nuevo(), "notas.txt".to_string());
        lsp.sincronizar(&layout, &config).await;
        assert!(lsp.sesiones.is_empty());
        assert!(lsp.fallos.is_empty(), "sin documentos de Python, su fallo se olvida y se reintenta al volver");
        lsp.cerrar().await;
    }
}
