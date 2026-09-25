//! Búsqueda (y reemplazo) de texto en todo el proyecto (`Ctrl+Shift+F`/
//! `Ctrl+K B`, BACKLOG.md P1 #16): el motor — recorrido del proyecto
//! respetando `.gitignore` (crate `ignore`, la de ripgrep), matching con
//! el mismo regex que `Ctrl+F` (`tcode_core::compilar_patron`) y armado
//! de reemplazos — más el estado de la vista de resultados
//! ([`EstadoBusquedaProyecto`]). Sin nada de terminal/`ratatui`: el crate
//! `ui` la dibuja y `app` decide qué tecla llega acá.
//!
//! La búsqueda corre en un hilo aparte y manda los resultados archivo por
//! archivo por un canal: la UI los va mostrando mientras busca y nunca se
//! congela. Cambiar la consulta (o cerrar la vista) cancela la búsqueda en
//! curso vía un `AtomicBool` que el hilo revisa antes de cada archivo.

use std::collections::HashMap;
use std::io::Write;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use ignore::overrides::OverrideBuilder;
use ignore::{WalkBuilder, WalkState};
use regex::Regex;
use tcode_core::{compilar_patron, OpcionesBusqueda};

use crate::buscador::CARPETAS_IGNORADAS;

/// Tope de coincidencias de una búsqueda: al llegar se corta el recorrido
/// y la vista avisa que hay más ("afiná la búsqueda"). Una consulta de una
/// letra en un proyecto grande daría cientos de miles — ni se pueden
/// recorrer a mano ni vale la pena tenerlas en memoria.
pub const TOPE_COINCIDENCIAS: usize = 5_000;

/// Archivos más grandes que esto se saltean (logs, dumps, bundles
/// minificados): casi nunca son lo que se busca y leerlos entero cuesta.
pub const TAMANO_MAXIMO_ARCHIVO: u64 = 4 * 1024 * 1024;

/// Cuántos bytes del principio de un archivo se miran para decidir si es
/// binario (un `\0` ahí = binario, mismo criterio que git y ripgrep).
const BYTES_DETECCION_BINARIO: usize = 8 * 1024;

/// Largo máximo (en bytes) del fragmento de línea que se guarda por
/// coincidencia para mostrar: una línea minificada de 50 KB no tiene que
/// viajar entera por el canal ni ocupar memoria por cada coincidencia.
const MAX_BYTES_FRAGMENTO: usize = 240;

/// Contexto que se deja antes de la coincidencia cuando la línea es más
/// larga que [`MAX_BYTES_FRAGMENTO`] y hay que recortarla.
const CONTEXTO_ANTES: usize = 40;

/// Una coincidencia dentro de un archivo. `linea` (0-based) y
/// `columna_byte` (offset en bytes dentro de esa línea, sin contar el
/// `\r` de un CRLF) son lo que se usa para saltar: valen igual sobre el
/// texto del disco que sobre el buffer del editor, que normaliza CRLF a
/// `\n` (ver `tcode_core::Buffer::desde_archivo`) y por eso tiene otros
/// offsets absolutos. `fragmento` es la línea (o un pedazo, si es muy
/// larga) para mostrar, y `resaltado` el rango de la coincidencia dentro
/// de él.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoincidenciaProyecto {
    pub linea: usize,
    pub columna_byte: usize,
    pub fragmento: String,
    pub resaltado: Range<usize>,
}

/// Todas las coincidencias de un archivo, en orden de aparición.
/// `desde_buffer`: se buscó sobre el buffer abierto en el editor (con
/// cambios sin guardar, quizás), no sobre el disco.
#[derive(Debug, Clone)]
pub struct ArchivoConCoincidencias {
    pub ruta: PathBuf,
    pub ruta_mostrada: String,
    pub coincidencias: Vec<CoincidenciaProyecto>,
    pub desde_buffer: bool,
}

/// Cómo terminó un recorrido completo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ResumenBusqueda {
    pub archivos_revisados: usize,
    /// Se llegó a [`TOPE_COINCIDENCIAS`] y se cortó antes de terminar.
    pub truncado: bool,
    /// Se canceló desde afuera (consulta nueva, vista cerrada).
    pub cancelado: bool,
}

/// Si `bytes` (el principio de un archivo) parece binario.
pub fn es_binario(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(BYTES_DETECCION_BINARIO)].contains(&0)
}

/// Coincidencias de `re` en `texto`, con línea y fragmento para mostrar,
/// hasta `maximo` como mucho. Las coincidencias vacías (un regex como
/// `x*`) se descartan: no hay nada que mostrar ni que reemplazar.
pub fn buscar_en_texto(texto: &str, re: &Regex, maximo: usize) -> Vec<CoincidenciaProyecto> {
    let mut salida = Vec::new();
    // Línea actual y dónde empieza, avanzando solo hacia adelante: contar
    // los `\n` desde la coincidencia anterior, no desde el principio.
    let mut linea = 0;
    let mut inicio_linea = 0;
    let mut contado_hasta = 0;
    for m in re.find_iter(texto) {
        if salida.len() >= maximo {
            break;
        }
        if m.start() == m.end() {
            continue;
        }
        for (i, b) in texto.as_bytes()[contado_hasta..m.start()].iter().enumerate() {
            if *b == b'\n' {
                linea += 1;
                inicio_linea = contado_hasta + i + 1;
            }
        }
        contado_hasta = m.start();
        let fin_linea = texto[inicio_linea..].find('\n').map_or(texto.len(), |i| inicio_linea + i);
        let texto_linea = texto[inicio_linea..fin_linea].trim_end_matches('\r');
        let columna_byte = m.start() - inicio_linea;
        let fin_en_linea = (m.end() - inicio_linea).min(texto_linea.len());
        let (fragmento, resaltado) = fragmento_linea(texto_linea, columna_byte, fin_en_linea);
        salida.push(CoincidenciaProyecto { linea, columna_byte, fragmento, resaltado });
    }
    salida
}

/// El pedazo de `linea` que se muestra para una coincidencia en
/// `[inicio, fin)`: la línea entera sin la indentación si entra en
/// [`MAX_BYTES_FRAGMENTO`]; si no, una ventana que arranca un poco antes
/// de la coincidencia, con `...` donde se cortó. Tabs y otros caracteres
/// de control pasan a espacio (mismo largo en bytes, así que los offsets
/// no se corren) — la lista de resultados no es la vista de código.
pub fn fragmento_linea(linea: &str, inicio: usize, fin: usize) -> (String, Range<usize>) {
    let sin_sangria = linea.len() - linea.trim_start().len();
    let (desde, prefijo) = if linea.len() - sin_sangria <= MAX_BYTES_FRAGMENTO || inicio < CONTEXTO_ANTES {
        (sin_sangria.min(inicio), "")
    } else {
        (limite_char(linea, inicio - CONTEXTO_ANTES), "...")
    };
    let hasta_ideal = (desde + MAX_BYTES_FRAGMENTO).max(fin.min(linea.len()));
    let hasta = limite_char(linea, hasta_ideal.min(linea.len()));
    let sufijo = if hasta < linea.len() { "..." } else { "" };
    let cuerpo: String = linea[desde..hasta].chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let resaltado = (prefijo.len() + inicio - desde)..(prefijo.len() + fin.min(hasta) - desde);
    (format!("{prefijo}{cuerpo}{sufijo}"), resaltado)
}

/// El límite de carácter más cercano a `byte` sin pasarse.
fn limite_char(texto: &str, mut byte: usize) -> usize {
    while !texto.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

/// Las ediciones `(rango, texto nuevo)` que reemplazan cada coincidencia
/// de `re` en `texto` por `reemplazo`, tal cual — sin expandir `$1`, igual
/// que el reemplazo de `Ctrl+H`. Rangos sobre `texto`, sin solaparse y en
/// orden: lo que espera `Editor::aplicar_ediciones` (un solo paso de
/// deshacer).
pub fn ediciones_de_reemplazo(texto: &str, re: &Regex, reemplazo: &str) -> Vec<(Range<usize>, String)> {
    re.find_iter(texto)
        .filter(|m| m.start() != m.end())
        .map(|m| (m.start()..m.end(), reemplazo.to_string()))
        .collect()
}

/// `texto` con todas las coincidencias reemplazadas, y cuántas eran.
pub fn reemplazar_en_texto(texto: &str, re: &Regex, reemplazo: &str) -> (String, usize) {
    let ediciones = ediciones_de_reemplazo(texto, re, reemplazo);
    let mut salida = String::with_capacity(texto.len());
    let mut ultimo = 0;
    for (rango, nuevo) in &ediciones {
        salida.push_str(&texto[ultimo..rango.start]);
        salida.push_str(nuevo);
        ultimo = rango.end;
    }
    salida.push_str(&texto[ultimo..]);
    (salida, ediciones.len())
}

/// Lee `ruta` como texto buscable: `None` si es muy grande, binario o no
/// es UTF-8 válido (no hay forma de mostrar ni reemplazar ahí sin
/// arriesgar romperlo).
fn leer_texto(ruta: &Path) -> Option<String> {
    let meta = std::fs::metadata(ruta).ok()?;
    if meta.len() > TAMANO_MAXIMO_ARCHIVO {
        return None;
    }
    let bytes = std::fs::read(ruta).ok()?;
    if es_binario(&bytes) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

/// Reemplaza en un archivo CERRADO (sin buffer en el editor): lo vuelve a
/// leer del disco y a buscar en ese momento — no confía en los resultados
/// que se mostraron, que pueden haber quedado viejos — y lo escribe de
/// forma atómica ([`escribir_atomico`]). Los `\r\n` quedan como estaban:
/// se trabaja sobre el texto crudo. Devuelve cuántas reemplazó (0 = el
/// archivo no se tocó).
pub fn reemplazar_en_archivo(ruta: &Path, re: &Regex, reemplazo: &str) -> Result<usize> {
    let Some(texto) = leer_texto(ruta) else {
        bail!("'{}' no se puede leer como texto", ruta.display());
    };
    let (nuevo, cantidad) = reemplazar_en_texto(&texto, re, reemplazo);
    if cantidad > 0 && nuevo != texto {
        escribir_atomico(ruta, &nuevo)?;
    }
    Ok(cantidad)
}

/// Escribe `contenido` en `ruta` sin dejarlo nunca a medio escribir: va a
/// un archivo temporal en la misma carpeta (mismo sistema de archivos,
/// así el `rename` es atómico), con los permisos del original, y recién
/// con todo escrito y sincronizado reemplaza al original. Si algo falla
/// antes del `rename`, el original queda intacto.
pub fn escribir_atomico(ruta: &Path, contenido: &str) -> Result<()> {
    let carpeta = ruta.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let nombre = ruta.file_name().context("ruta sin nombre de archivo")?.to_string_lossy();
    let temporal = carpeta.join(format!(".{nombre}.tcode-{}.tmp", std::process::id()));
    let resultado = (|| -> Result<()> {
        let mut archivo = std::fs::File::create(&temporal)
            .with_context(|| format!("no se pudo crear '{}'", temporal.display()))?;
        archivo.write_all(contenido.as_bytes())?;
        archivo.sync_all()?;
        if let Ok(meta) = std::fs::metadata(ruta) {
            std::fs::set_permissions(&temporal, meta.permissions())?;
        }
        std::fs::rename(&temporal, ruta).with_context(|| format!("no se pudo reemplazar '{}'", ruta.display()))
    })();
    if resultado.is_err() {
        let _ = std::fs::remove_file(&temporal);
    }
    resultado
}

/// Recorre el proyecto en `raiz` buscando `re` y llama a `al_encontrar`
/// por cada archivo con coincidencias (desde varios hilos, en cualquier
/// orden: ver el comentario del recorrido). Respeta
/// `.gitignore`/`.ignore` (aunque no haya repo git), saltea ocultos y las
/// mismas carpetas que el buscador de archivos (`target`,
/// `node_modules`), archivos de más de [`TAMANO_MAXIMO_ARCHIVO`],
/// binarios y no UTF-8. `filtro`: globs separados por coma, estilo
/// `.gitignore` (`*.rs, src/**`); con `!` adelante excluyen (`!tests`).
/// Los archivos que están en `buffers` (ruta canónica -> texto) se buscan
/// sobre ese texto, no sobre el disco. `cancelado` se revisa antes de
/// cada archivo.
pub fn buscar_en_proyecto(
    raiz: &Path,
    re: &Regex,
    filtro: &str,
    buffers: &HashMap<PathBuf, String>,
    cancelado: &AtomicBool,
    al_encontrar: impl Fn(ArchivoConCoincidencias) + Sync,
) -> Result<ResumenBusqueda> {
    let mut constructor = WalkBuilder::new(raiz);
    constructor
        .hidden(true)
        .require_git(false)
        .max_filesize(Some(TAMANO_MAXIMO_ARCHIVO))
        .filter_entry(|e| !CARPETAS_IGNORADAS.iter().any(|c| e.file_name() == *c));
    let globs: Vec<&str> = filtro.split(',').map(str::trim).filter(|g| !g.is_empty()).collect();
    if !globs.is_empty() {
        let mut overrides = OverrideBuilder::new(raiz);
        for glob in globs {
            overrides.add(glob).with_context(|| format!("filtro de archivos inválido: '{glob}'"))?;
        }
        constructor.overrides(overrides.build().context("filtro de archivos inválido")?);
    }

    // Recorrido en paralelo (un hilo por núcleo, lo que decide `ignore`,
    // igual que ripgrep): medido sobre ~8000 archivos, 1,7 s con un solo
    // hilo contra unas décimas así — casi todo el tiempo es esperar al
    // disco. Los archivos llegan en cualquier orden; quien los junta los
    // ordena (`EstadoBusquedaProyecto::recibir`).
    let total = AtomicUsize::new(0);
    let revisados = AtomicUsize::new(0);
    let truncado = AtomicBool::new(false);
    constructor.build_parallel().run(|| {
        let (total, revisados, truncado, al_encontrar) = (&total, &revisados, &truncado, &al_encontrar);
        Box::new(move |entrada| {
            if cancelado.load(Ordering::Relaxed) || truncado.load(Ordering::Relaxed) {
                return WalkState::Quit;
            }
            let Ok(entrada) = entrada else { return WalkState::Continue };
            if !entrada.file_type().is_some_and(|t| t.is_file()) {
                return WalkState::Continue;
            }
            let ruta = entrada.path();
            let (texto, desde_buffer) = match buffers.get(ruta) {
                Some(texto) => (std::borrow::Cow::Borrowed(texto.as_str()), true),
                None => match leer_texto(ruta) {
                    Some(texto) => (std::borrow::Cow::Owned(texto), false),
                    None => return WalkState::Continue,
                },
            };
            revisados.fetch_add(1, Ordering::Relaxed);
            let restantes = TOPE_COINCIDENCIAS.saturating_sub(total.load(Ordering::Relaxed));
            let mut coincidencias = buscar_en_texto(&texto, re, restantes);
            if coincidencias.is_empty() {
                return WalkState::Continue;
            }
            // Entre la lectura de `total` de arriba y acá otro hilo pudo
            // sumar las suyas: la reserva de verdad es este `fetch_add`, y
            // lo que se pase del tope se descarta.
            let previo = total.fetch_add(coincidencias.len(), Ordering::Relaxed);
            if previo + coincidencias.len() >= TOPE_COINCIDENCIAS {
                truncado.store(true, Ordering::Relaxed);
                coincidencias.truncate(TOPE_COINCIDENCIAS.saturating_sub(previo));
            }
            if !coincidencias.is_empty() {
                al_encontrar(ArchivoConCoincidencias {
                    ruta: ruta.to_path_buf(),
                    ruta_mostrada: ruta.strip_prefix(raiz).unwrap_or(ruta).display().to_string(),
                    coincidencias,
                    desde_buffer,
                });
            }
            WalkState::Continue
        })
    });
    Ok(ResumenBusqueda {
        archivos_revisados: revisados.into_inner(),
        truncado: truncado.into_inner(),
        cancelado: cancelado.load(Ordering::Relaxed),
    })
}

/// Lo que manda el hilo de búsqueda a la vista.
enum Mensaje {
    Archivo(ArchivoConCoincidencias),
    Fin(Result<ResumenBusqueda, String>),
}

/// Qué campo de la vista recibe lo que se escribe (`Tab` rota).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampoProyecto {
    Consulta,
    Reemplazo,
    Filtro,
}

/// Estado de la vista "Buscar en el proyecto": los tres campos, las
/// opciones (las mismas `Alt+R`/`Alt+C`/`Alt+W` de `Ctrl+F`), los
/// resultados que van llegando del hilo de búsqueda y la selección (un
/// índice sobre TODAS las coincidencias, en orden; los encabezados de
/// archivo no se seleccionan).
///
/// Cerrar y volver a abrir conserva consulta, resultados y selección
/// (como el panel de búsqueda de VSCode): se puede abrir un resultado,
/// mirar, y volver por el siguiente. Cualquier cambio en consulta,
/// filtro u opciones vuelve a buscar.
pub struct EstadoBusquedaProyecto {
    raiz: PathBuf,
    activo: bool,
    consulta: String,
    reemplazo: String,
    filtro: String,
    campo: CampoProyecto,
    opciones: OpcionesBusqueda,
    archivos: Vec<ArchivoConCoincidencias>,
    total: usize,
    seleccion: usize,
    /// Se movió la selección desde que empezó esta búsqueda (ver `recibir`).
    seleccion_movida: bool,
    resumen: Option<ResumenBusqueda>,
    error: Option<String>,
    /// Esperando `y` para "reemplazar todo" (ver `pedir_reemplazo`).
    confirmando: bool,
    /// Aviso de una sola vez (resultado de un reemplazo, motivo por el
    /// que no se puede reemplazar) — se borra con la próxima tecla.
    aviso: Option<String>,
    receptor: Option<Receiver<Mensaje>>,
    cancelado: Option<Arc<AtomicBool>>,
}

impl EstadoBusquedaProyecto {
    pub fn nuevo(raiz: impl Into<PathBuf>) -> Self {
        let raiz = raiz.into();
        // Canónica: las rutas que devuelve el recorrido cuelgan de acá y
        // se comparan con las (canónicas) de los buffers abiertos.
        let raiz = std::fs::canonicalize(&raiz).unwrap_or(raiz);
        Self {
            raiz,
            activo: false,
            consulta: String::new(),
            reemplazo: String::new(),
            filtro: String::new(),
            campo: CampoProyecto::Consulta,
            opciones: OpcionesBusqueda::default(),
            archivos: Vec::new(),
            total: 0,
            seleccion: 0,
            seleccion_movida: false,
            resumen: None,
            error: None,
            confirmando: false,
            aviso: None,
            receptor: None,
            cancelado: None,
        }
    }

    pub fn activo(&self) -> bool {
        self.activo
    }
    pub fn raiz(&self) -> &Path {
        &self.raiz
    }
    pub fn consulta(&self) -> &str {
        &self.consulta
    }
    pub fn reemplazo(&self) -> &str {
        &self.reemplazo
    }
    pub fn filtro(&self) -> &str {
        &self.filtro
    }
    pub fn campo(&self) -> CampoProyecto {
        self.campo
    }
    pub fn opciones(&self) -> OpcionesBusqueda {
        self.opciones
    }
    pub fn archivos(&self) -> &[ArchivoConCoincidencias] {
        &self.archivos
    }
    pub fn total(&self) -> usize {
        self.total
    }
    pub fn seleccion(&self) -> usize {
        self.seleccion
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn confirmando(&self) -> bool {
        self.confirmando
    }
    pub fn aviso(&self) -> Option<&str> {
        self.aviso.as_deref()
    }
    /// Hay un hilo de búsqueda corriendo (la app sondea [`Self::recibir`]
    /// mientras tanto).
    pub fn buscando(&self) -> bool {
        self.receptor.is_some()
    }
    /// Cómo terminó la última búsqueda (`None` mientras corre o si no hubo).
    pub fn resumen(&self) -> Option<ResumenBusqueda> {
        self.resumen
    }

    pub fn abrir(&mut self) {
        self.activo = true;
        self.campo = CampoProyecto::Consulta;
        self.confirmando = false;
        self.aviso = None;
    }

    /// Cierra la vista y cancela la búsqueda en curso, si hay una: los
    /// resultados que ya llegaron se conservan.
    pub fn cerrar(&mut self) {
        self.activo = false;
        self.confirmando = false;
        self.cancelar();
        if self.resumen.is_none() && !self.archivos.is_empty() {
            self.resumen = Some(ResumenBusqueda { cancelado: true, ..Default::default() });
        }
    }

    /// Esconde la vista SIN cancelar la búsqueda (al abrir un resultado):
    /// si seguía corriendo, termina de fondo y reabrir la muestra
    /// completa.
    pub fn ocultar(&mut self) {
        self.activo = false;
        self.confirmando = false;
    }

    fn cancelar(&mut self) {
        if let Some(cancelado) = self.cancelado.take() {
            cancelado.store(true, Ordering::Relaxed);
        }
        self.receptor = None;
    }

    /// Borra el aviso de una sola vez — la app lo llama con cada tecla
    /// antes de procesarla.
    pub fn nueva_tecla(&mut self) {
        self.aviso = None;
    }

    /// Escribe en el campo activo. Devuelve si hay que volver a buscar
    /// (cambió la consulta o el filtro; el texto de reemplazo no).
    pub fn escribir(&mut self, c: char) -> bool {
        self.campo_mut().push(c);
        self.campo != CampoProyecto::Reemplazo
    }

    pub fn borrar(&mut self) -> bool {
        self.campo_mut().pop().is_some() && self.campo != CampoProyecto::Reemplazo
    }

    fn campo_mut(&mut self) -> &mut String {
        match self.campo {
            CampoProyecto::Consulta => &mut self.consulta,
            CampoProyecto::Reemplazo => &mut self.reemplazo,
            CampoProyecto::Filtro => &mut self.filtro,
        }
    }

    /// `Tab`: consulta -> reemplazo -> filtro -> consulta.
    pub fn alternar_campo(&mut self) {
        self.campo = match self.campo {
            CampoProyecto::Consulta => CampoProyecto::Reemplazo,
            CampoProyecto::Reemplazo => CampoProyecto::Filtro,
            CampoProyecto::Filtro => CampoProyecto::Consulta,
        };
    }

    pub fn alternar_regex(&mut self) {
        self.opciones.regex = !self.opciones.regex;
    }
    pub fn alternar_mayusculas(&mut self) {
        self.opciones.sensible_mayusculas = !self.opciones.sensible_mayusculas;
    }
    pub fn alternar_palabra(&mut self) {
        self.opciones.palabra_completa = !self.opciones.palabra_completa;
    }

    /// El regex de la consulta actual (`None` si está vacía o es inválida).
    pub fn patron(&self) -> Option<Regex> {
        if self.consulta.is_empty() {
            return None;
        }
        compilar_patron(&self.consulta, self.opciones).ok()
    }

    /// Cancela la búsqueda anterior y lanza una nueva en un hilo aparte
    /// con la consulta, el filtro y las opciones actuales. `buffers`: el
    /// texto de los documentos abiertos en el editor, por ruta canónica —
    /// se buscan sobre eso en vez del disco (cambios sin guardar).
    pub fn buscar(&mut self, buffers: HashMap<PathBuf, String>) {
        self.cancelar();
        self.archivos.clear();
        self.total = 0;
        self.seleccion = 0;
        self.seleccion_movida = false;
        self.resumen = None;
        self.error = None;
        self.confirmando = false;
        if self.consulta.is_empty() {
            return;
        }
        let re = match compilar_patron(&self.consulta, self.opciones) {
            Ok(re) => re,
            Err(e) => {
                self.error = Some(format!("{e:#}"));
                return;
            }
        };
        let (emisor, receptor) = mpsc::channel();
        let cancelado = Arc::new(AtomicBool::new(false));
        let cancelado_hilo = Arc::clone(&cancelado);
        let raiz = self.raiz.clone();
        let filtro = self.filtro.clone();
        std::thread::spawn(move || {
            let resultado = buscar_en_proyecto(&raiz, &re, &filtro, &buffers, &cancelado_hilo, |archivo| {
                let _ = emisor.send(Mensaje::Archivo(archivo));
            });
            let _ = emisor.send(Mensaje::Fin(resultado.map_err(|e| format!("{e:#}"))));
        });
        self.receptor = Some(receptor);
        self.cancelado = Some(cancelado);
    }

    /// Incorpora lo que haya mandado el hilo de búsqueda desde la última
    /// vez, sin bloquear. Devuelve si llegó algo (hay que redibujar).
    pub fn recibir(&mut self) -> bool {
        let mut hubo = false;
        loop {
            let Some(receptor) = &self.receptor else { return hubo };
            match receptor.try_recv() {
                // Los hilos del recorrido mandan en cualquier orden: cada
                // archivo se inserta en su lugar por ruta. Si ya se movió
                // la selección, se corre lo que haga falta para que siga
                // sobre la MISMA coincidencia; si no, queda arriba de todo.
                Ok(Mensaje::Archivo(archivo)) => {
                    self.incorporar(archivo);
                    hubo = true;
                }
                Ok(Mensaje::Fin(resultado)) => {
                    match resultado {
                        Ok(resumen) => self.resumen = Some(resumen),
                        Err(e) => self.error = Some(e),
                    }
                    self.receptor = None;
                    self.cancelado = None;
                    return true;
                }
                Err(TryRecvError::Empty) => return hubo,
                Err(TryRecvError::Disconnected) => {
                    self.receptor = None;
                    self.cancelado = None;
                    return true;
                }
            }
        }
    }

    fn incorporar(&mut self, archivo: ArchivoConCoincidencias) {
        let posicion = self.archivos.partition_point(|a| a.ruta < archivo.ruta);
        let antes: usize = self.archivos[..posicion].iter().map(|a| a.coincidencias.len()).sum();
        let cantidad = archivo.coincidencias.len();
        if self.seleccion_movida && self.seleccion >= antes {
            self.seleccion += cantidad;
        }
        self.total += cantidad;
        self.archivos.insert(posicion, archivo);
    }

    pub fn mover_abajo(&mut self, filas: usize) {
        if self.total > 0 {
            self.seleccion = (self.seleccion + filas).min(self.total - 1);
            self.seleccion_movida = true;
        }
    }

    pub fn mover_arriba(&mut self, filas: usize) {
        self.seleccion = self.seleccion.saturating_sub(filas);
        self.seleccion_movida = true;
    }

    /// Archivo y coincidencia de un índice global (ver `seleccion`).
    pub fn coincidencia(&self, indice: usize) -> Option<(&ArchivoConCoincidencias, &CoincidenciaProyecto)> {
        let mut resto = indice;
        for archivo in &self.archivos {
            if resto < archivo.coincidencias.len() {
                return Some((archivo, &archivo.coincidencias[resto]));
            }
            resto -= archivo.coincidencias.len();
        }
        None
    }

    /// La coincidencia seleccionada, para abrirla (`Enter`).
    pub fn seleccionada(&self) -> Option<(&ArchivoConCoincidencias, &CoincidenciaProyecto)> {
        self.coincidencia(self.seleccion)
    }

    /// `Alt+Enter`: pide confirmación para reemplazar todo. Solo con la
    /// búsqueda terminada y completa — si se cortó en el tope, lo que se
    /// ve no es todo lo que se reemplazaría. Devuelve `false` (con el
    /// motivo en `aviso`) si no se puede.
    pub fn pedir_reemplazo(&mut self) -> bool {
        let motivo = if self.total == 0 {
            Some("No hay coincidencias que reemplazar")
        } else if self.buscando() {
            Some("Esperá a que termine la búsqueda para reemplazar")
        } else if self.resumen.is_some_and(|r| r.truncado || r.cancelado) {
            Some("La búsqueda no está completa (tope de resultados): afiná la consulta o el filtro")
        } else {
            None
        };
        if let Some(motivo) = motivo {
            self.aviso = Some(motivo.to_string());
            return false;
        }
        self.confirmando = true;
        true
    }

    pub fn cancelar_confirmacion(&mut self) {
        self.confirmando = false;
    }

    /// Sale del modo confirmación y deja `aviso` como mensaje (lo usa la
    /// app para informar el resultado del reemplazo).
    pub fn terminar_reemplazo(&mut self, aviso: String) {
        self.confirmando = false;
        self.aviso = Some(aviso);
    }
}

impl Drop for EstadoBusquedaProyecto {
    fn drop(&mut self) {
        self.cancelar();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn re(patron: &str, opciones: OpcionesBusqueda) -> Regex {
        compilar_patron(patron, opciones).unwrap()
    }

    fn literal(patron: &str) -> Regex {
        re(patron, OpcionesBusqueda::default())
    }

    #[test]
    fn busca_con_linea_columna_y_fragmento() {
        let texto = "uno\n    dos foo\ntres foo foo\n";
        let c = buscar_en_texto(texto, &literal("foo"), 100);
        assert_eq!(c.len(), 3);
        assert_eq!((c[0].linea, c[0].columna_byte), (1, 8));
        // Sin la indentación, y el resaltado corrido en consecuencia.
        assert_eq!(c[0].fragmento, "dos foo");
        assert_eq!(&c[0].fragmento[c[0].resaltado.clone()], "foo");
        assert_eq!((c[2].linea, c[2].columna_byte), (2, 9));
    }

    #[test]
    fn crlf_no_corre_la_columna_ni_aparece_en_el_fragmento() {
        let texto = "a\r\nb foo\r\n";
        let c = buscar_en_texto(texto, &literal("foo"), 100);
        assert_eq!((c[0].linea, c[0].columna_byte), (1, 2));
        assert_eq!(c[0].fragmento, "b foo");
    }

    #[test]
    fn respeta_las_opciones_de_ctrl_f() {
        let texto = "Foo foo foobar";
        assert_eq!(buscar_en_texto(texto, &literal("foo"), 100).len(), 3);
        let mayus = OpcionesBusqueda { sensible_mayusculas: true, ..Default::default() };
        assert_eq!(buscar_en_texto(texto, &re("foo", mayus), 100).len(), 2);
        let palabra = OpcionesBusqueda { palabra_completa: true, ..Default::default() };
        assert_eq!(buscar_en_texto(texto, &re("foo", palabra), 100).len(), 2);
        let regex = OpcionesBusqueda { regex: true, ..Default::default() };
        assert_eq!(buscar_en_texto(texto, &re("fo+b", regex), 100).len(), 1);
    }

    #[test]
    fn descarta_coincidencias_vacias_y_respeta_el_maximo() {
        let regex = OpcionesBusqueda { regex: true, ..Default::default() };
        assert_eq!(buscar_en_texto("abc", &re("x*", regex), 100).len(), 0);
        assert_eq!(buscar_en_texto("a a a a", &literal("a"), 2).len(), 2);
    }

    #[test]
    fn linea_larga_se_recorta_alrededor_de_la_coincidencia() {
        let linea = format!("{}AQUI{}", "x".repeat(1000), "y".repeat(1000));
        let (fragmento, resaltado) = fragmento_linea(&linea, 1000, 1004);
        assert!(fragmento.len() <= MAX_BYTES_FRAGMENTO + 6);
        assert!(fragmento.starts_with("...") && fragmento.ends_with("..."));
        assert_eq!(&fragmento[resaltado], "AQUI");
    }

    #[test]
    fn fragmento_respeta_limites_de_caracter_y_cambia_tabs() {
        let linea = format!("{}\tñandú", "é".repeat(300));
        let (fragmento, resaltado) = fragmento_linea(&linea, 601, 608);
        assert_eq!(&fragmento[resaltado], "ñandú");
        assert!(!fragmento.contains('\t'));
    }

    #[test]
    fn reemplaza_literal_sin_expandir_grupos() {
        let regex = OpcionesBusqueda { regex: true, ..Default::default() };
        let (nuevo, n) = reemplazar_en_texto("a1 b2 c3", &re(r"[a-z](\d)", regex), "$1");
        assert_eq!(n, 3);
        assert_eq!(nuevo, "$1 $1 $1");
    }

    #[test]
    fn detecta_binarios() {
        assert!(es_binario(b"abc\0def"));
        assert!(!es_binario("texto común ñ".as_bytes()));
    }

    fn proyecto() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        std::fs::create_dir_all(p.join("src")).unwrap();
        std::fs::create_dir_all(p.join("generado")).unwrap();
        std::fs::create_dir_all(p.join("target")).unwrap();
        std::fs::write(p.join(".gitignore"), "generado/\n*.log\n").unwrap();
        std::fs::write(p.join("src/a.rs"), "fn buscado() {}\n").unwrap();
        std::fs::write(p.join("src/b.txt"), "nada\notro buscado\r\n").unwrap();
        std::fs::write(p.join("generado/c.rs"), "buscado").unwrap();
        std::fs::write(p.join("x.log"), "buscado").unwrap();
        std::fs::write(p.join("target/d.rs"), "buscado").unwrap();
        std::fs::write(p.join(".oculto"), "buscado").unwrap();
        std::fs::write(p.join("binario.bin"), b"buscado\0\x01\x02").unwrap();
        std::fs::write(p.join("latin1.txt"), b"buscado \xf1").unwrap();
        dir
    }

    fn buscar(dir: &Path, filtro: &str, buffers: &HashMap<PathBuf, String>) -> Vec<ArchivoConCoincidencias> {
        let raiz = std::fs::canonicalize(dir).unwrap();
        let salida = std::sync::Mutex::new(Vec::new());
        let resumen = buscar_en_proyecto(&raiz, &literal("buscado"), filtro, buffers, &AtomicBool::new(false), |a| {
            salida.lock().unwrap().push(a)
        })
        .unwrap();
        assert!(!resumen.truncado && !resumen.cancelado);
        // Llegan en cualquier orden (recorrido en paralelo).
        let mut salida = salida.into_inner().unwrap();
        salida.sort_by(|a, b| a.ruta.cmp(&b.ruta));
        salida
    }

    fn rutas(archivos: &[ArchivoConCoincidencias]) -> Vec<String> {
        archivos.iter().map(|a| a.ruta_mostrada.replace('\\', "/")).collect()
    }

    #[test]
    fn respeta_gitignore_ocultos_target_binarios_y_no_utf8() {
        let dir = proyecto();
        let archivos = buscar(dir.path(), "", &HashMap::new());
        assert_eq!(rutas(&archivos), vec!["src/a.rs", "src/b.txt"]);
        assert_eq!(archivos[1].coincidencias[0].linea, 1);
    }

    #[test]
    fn filtro_de_rutas_incluye_y_excluye() {
        let dir = proyecto();
        assert_eq!(rutas(&buscar(dir.path(), "*.rs", &HashMap::new())), vec!["src/a.rs"]);
        assert_eq!(rutas(&buscar(dir.path(), "!*.rs", &HashMap::new())), vec!["src/b.txt"]);
    }

    #[test]
    fn filtro_invalido_da_error() {
        let dir = proyecto();
        let r = buscar_en_proyecto(dir.path(), &literal("x"), "a[", &HashMap::new(), &AtomicBool::new(false), |_| {});
        assert!(r.is_err());
    }

    #[test]
    fn los_buffers_abiertos_se_buscan_en_vez_del_disco() {
        let dir = proyecto();
        let raiz = std::fs::canonicalize(dir.path()).unwrap();
        let buffers = HashMap::from([(raiz.join("src/a.rs"), "sin nada\nbuscado buscado".to_string())]);
        let archivos = buscar(dir.path(), "", &buffers);
        assert!(archivos[0].desde_buffer);
        assert_eq!(archivos[0].coincidencias.len(), 2);
        assert_eq!(archivos[0].coincidencias[0].linea, 1);
    }

    #[test]
    fn cancelado_corta_el_recorrido() {
        let dir = proyecto();
        let n = AtomicUsize::new(0);
        let r = buscar_en_proyecto(dir.path(), &literal("buscado"), "", &HashMap::new(), &AtomicBool::new(true), |_| {
            n.fetch_add(1, Ordering::Relaxed);
        })
        .unwrap();
        assert!(r.cancelado);
        assert_eq!(n.into_inner(), 0);
    }

    #[test]
    fn corta_en_el_tope_de_coincidencias() {
        let dir = tempfile::tempdir().unwrap();
        let contenido = "z\n".repeat(TOPE_COINCIDENCIAS / 2 + 1);
        std::fs::write(dir.path().join("a.txt"), &contenido).unwrap();
        std::fs::write(dir.path().join("b.txt"), &contenido).unwrap();
        std::fs::write(dir.path().join("c.txt"), &contenido).unwrap();
        let total = AtomicUsize::new(0);
        let r = buscar_en_proyecto(dir.path(), &literal("z"), "", &HashMap::new(), &AtomicBool::new(false), |a| {
            total.fetch_add(a.coincidencias.len(), Ordering::Relaxed);
        })
        .unwrap();
        let total = total.into_inner();
        assert!(r.truncado);
        assert_eq!(total, TOPE_COINCIDENCIAS);
    }

    #[test]
    fn reemplazar_en_archivo_es_atomico_y_conserva_crlf() {
        let dir = tempfile::tempdir().unwrap();
        let ruta = dir.path().join("a.txt");
        std::fs::write(&ruta, "foo\r\nbar foo\r\n").unwrap();
        assert_eq!(reemplazar_en_archivo(&ruta, &literal("foo"), "xy").unwrap(), 2);
        assert_eq!(std::fs::read_to_string(&ruta).unwrap(), "xy\r\nbar xy\r\n");
        // No quedan temporales en la carpeta.
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        // Un binario no se toca.
        let binario = dir.path().join("b.bin");
        std::fs::write(&binario, b"foo\0").unwrap();
        assert!(reemplazar_en_archivo(&binario, &literal("foo"), "x").is_err());
        assert_eq!(std::fs::read(&binario).unwrap(), b"foo\0");
    }

    #[test]
    fn estado_busca_en_hilo_y_navega() {
        let dir = proyecto();
        let mut estado = EstadoBusquedaProyecto::nuevo(dir.path());
        estado.abrir();
        for c in "buscado".chars() {
            assert!(estado.escribir(c));
        }
        estado.buscar(HashMap::new());
        let inicio = std::time::Instant::now();
        while estado.buscando() && inicio.elapsed() < std::time::Duration::from_secs(10) {
            estado.recibir();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(estado.total(), 2);
        assert_eq!(estado.archivos().len(), 2);
        estado.mover_abajo(10);
        assert_eq!(estado.seleccion(), 1);
        let (archivo, c) = estado.seleccionada().unwrap();
        assert!(archivo.ruta_mostrada.ends_with("b.txt"));
        assert_eq!(c.linea, 1);
        assert!(estado.pedir_reemplazo());
        assert!(estado.confirmando());
    }

    #[test]
    fn no_permite_reemplazar_sin_resultados_ni_con_regex_invalido() {
        let dir = proyecto();
        let mut estado = EstadoBusquedaProyecto::nuevo(dir.path());
        estado.alternar_regex();
        estado.escribir('(');
        estado.buscar(HashMap::new());
        assert!(estado.error().is_some());
        assert!(!estado.buscando());
        assert!(!estado.pedir_reemplazo());
        assert!(estado.aviso().is_some());
    }

    fn archivo_de_prueba(ruta: &str, lineas: usize) -> ArchivoConCoincidencias {
        let c = CoincidenciaProyecto { linea: 0, columna_byte: 0, fragmento: "x".into(), resaltado: 0..1 };
        ArchivoConCoincidencias {
            ruta: PathBuf::from(ruta),
            ruta_mostrada: ruta.into(),
            coincidencias: vec![c; lineas],
            desde_buffer: false,
        }
    }

    #[test]
    fn los_resultados_se_ordenan_por_ruta_y_la_seleccion_sigue_a_su_coincidencia() {
        let mut estado = EstadoBusquedaProyecto::nuevo(".");
        estado.incorporar(archivo_de_prueba("/p/m.rs", 2));
        estado.incorporar(archivo_de_prueba("/p/z.rs", 1));
        // Sin mover la selección, un archivo que llega antes no la corre:
        // queda arriba de todo.
        estado.incorporar(archivo_de_prueba("/p/b.rs", 1));
        assert_eq!(estado.seleccion(), 0);
        let rutas: Vec<&str> = estado.archivos().iter().map(|a| a.ruta_mostrada.as_str()).collect();
        assert_eq!(rutas, ["/p/b.rs", "/p/m.rs", "/p/z.rs"]);
        // Movida a la de z.rs (índice 3), llega a.rs (2) antes: sigue en z.rs.
        estado.mover_abajo(3);
        estado.incorporar(archivo_de_prueba("/p/a.rs", 2));
        assert_eq!(estado.seleccion(), 5);
        assert!(estado.seleccionada().unwrap().0.ruta_mostrada.ends_with("z.rs"));
        assert_eq!(estado.total(), 6);
    }

    #[test]
    fn escribir_en_reemplazo_no_pide_volver_a_buscar() {
        let mut estado = EstadoBusquedaProyecto::nuevo(".");
        estado.alternar_campo();
        assert_eq!(estado.campo(), CampoProyecto::Reemplazo);
        assert!(!estado.escribir('x'));
        assert_eq!(estado.reemplazo(), "x");
        estado.alternar_campo();
        assert!(estado.escribir('*'));
        assert_eq!(estado.filtro(), "*");
    }
}
