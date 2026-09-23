//! Indicadores de git en el gutter (BACKLOG.md P2 #6): qué líneas del
//! buffer están agregadas, modificadas o tienen líneas borradas al lado,
//! respecto de la versión del archivo en `HEAD`.
//!
//! Dos piezas, ninguna con dependencias nuevas:
//!
//! - **La base** (el contenido en `HEAD`) se obtiene UNA vez por archivo
//!   invocando el binario `git` (`git cat-file blob HEAD:./<nombre>`, con
//!   el directorio del archivo como cwd) en un hilo aparte — nunca bloquea
//!   el dibujado. Se descartó `git2`/libgit2: es una dependencia nativa
//!   pesada y el proyecto publica binarios estáticos musl para Linux. Si
//!   `git` no está instalado, tarda más de [`TIEMPO_MAXIMO_CARGA`], el
//!   archivo no está en un repo o no está trackeado en `HEAD` (archivo
//!   nuevo sin commitear), simplemente no hay base y no hay marcas — sin
//!   errores visibles. Un archivo sin trackear NO se marca entero como
//!   agregado (mismo criterio que VSCode y Helix): la opción más
//!   conservadora, y evita una columna entera de `+` en cada archivo
//!   nuevo.
//! - **El diff por líneas** se calcula en proceso contra el texto actual
//!   del buffer ([`calcular_marcas`], Myers sobre lo que queda después de
//!   recortar prefijo y sufijo comunes), así las marcas se actualizan en
//!   vivo mientras se escribe, no solo al guardar. [`DiffGit`] solo lo
//!   recalcula cuando el texto cambió desde el último cálculo, y en un
//!   hilo propio: el frame nunca espera al diff (ver su documentación).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Marca de una línea del buffer respecto de `HEAD`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarcaGit {
    /// Línea nueva: no existía en `HEAD`.
    Agregada,
    /// Línea que reemplaza a una o más de `HEAD` (un hunk con líneas
    /// borradas Y agregadas: todas las agregadas se marcan así, igual que
    /// en VSCode).
    Modificada,
    /// Justo ANTES de esta línea había líneas en `HEAD` que ya no están
    /// (la línea en sí no cambió). Si el hueco queda al final del
    /// archivo, va en la última línea — ver [`calcular_marcas`].
    Borrada,
}

/// Si `git` no respondió en este tiempo (repo en un disco de red colgado,
/// un hook raro...), se abandona la carga: el archivo queda sin marcas en
/// vez de dejar al bucle principal sondeando para siempre.
pub const TIEMPO_MAXIMO_CARGA: Duration = Duration::from_secs(5);

/// Por encima de esta cantidad de líneas distintas (la "D" de Myers)
/// entre la base y el texto actual, se deja de buscar el diff mínimo y
/// todo el tramo que difiere se marca como un solo bloque modificado. El
/// costo de Myers crece con D² (tiempo y memoria del rastro para
/// reconstruir el camino); con este tope el peor caso medido (10.000
/// líneas todas distintas) queda en ~4 ms por cálculo — en el hilo del
/// calculador, no en el de la UI — y un archivo tan distinto de `HEAD`
/// tampoco gana mucho con marcas línea por línea.
const MAX_DISTANCIA_MYERS: usize = 500;

/// Contenido de `ruta` en `HEAD`, con los finales de línea normalizados
/// a `\n` (igual que `tcode_core::Buffer::desde_archivo`, para que un
/// repo con CRLF no marque todas las líneas como modificadas). `None` si
/// `git` no está instalado, el archivo no está en un repo, no está
/// trackeado en `HEAD`, o el repo todavía no tiene commits. Bloqueante:
/// usar desde un hilo aparte (ver [`DiffGit`]).
///
/// `HEAD:./<nombre>` con `-C <carpeta del archivo>` resuelve la ruta
/// relativa al cwd, sin necesitar un `git rev-parse --show-toplevel`
/// previo para calcular la ruta relativa a la raíz del repo — un solo
/// proceso en vez de dos. `cat-file blob` (en vez de `show`) devuelve el
/// blob tal cual, sin pasar por `textconv` ni por el pager.
pub fn leer_base_head(ruta: &Path) -> Option<String> {
    let carpeta = ruta.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let nombre = ruta.file_name()?.to_str()?;
    let salida = Command::new("git")
        .arg("-C")
        .arg(carpeta)
        .arg("cat-file")
        .arg("blob")
        .arg(format!("HEAD:./{nombre}"))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !salida.status.success() {
        return None;
    }
    let texto = String::from_utf8_lossy(&salida.stdout);
    Some(if texto.contains("\r\n") { texto.replace("\r\n", "\n") } else { texto.into_owned() })
}

/// Líneas de `texto` tal como las cuenta el buffer (`ropey`: una por cada
/// `\n`, más la del final), SIN la línea vacía "fantasma" que queda
/// después de un `\n` final. Quitarla de los dos lados evita marcar esa
/// línea vacía como agregada cuando el archivo pasa a terminar en `\n`
/// (o al revés): la diferencia es invisible en pantalla. Un texto vacío
/// tiene cero líneas (no una vacía), así borrar todo el archivo es un
/// hunk de solo borradas en vez de "una línea modificada".
fn lineas_de(texto: &str) -> Vec<&str> {
    if texto.is_empty() {
        return Vec::new();
    }
    let mut lineas: Vec<&str> = texto.split('\n').collect();
    if lineas.len() > 1 && lineas.last() == Some(&"") {
        lineas.pop();
    }
    lineas
}

/// Marca de cada línea de `actual` respecto de `base` (índice = número de
/// línea del buffer, 0-based; `None` = sin cambios). Tiene al menos un
/// elemento (el buffer vacío igual muestra una línea), y puede ser más
/// corto que el buffer si este termina en `\n` (la línea vacía final
/// nunca lleva marca, ver [`lineas_de`]) — quien dibuja usa `.get()`.
///
/// Las líneas borradas no existen en `actual`: el hueco se marca en la
/// línea que quedó justo después ([`MarcaGit::Borrada`]), o en la última
/// si el hueco está al final del archivo. Si esa línea ya tenía otra
/// marca, gana la otra (agregada/modificada dicen más).
pub fn calcular_marcas(base: &str, actual: &str) -> Vec<Option<MarcaGit>> {
    let a = lineas_de(base);
    let b = lineas_de(actual);
    let mut marcas = vec![None; b.len().max(1)];

    // Prefijo y sufijo comunes fuera del Myers: una edición típica toca
    // un tramo chico, y así el algoritmo solo corre sobre ese tramo en
    // vez del archivo entero.
    let prefijo = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let sufijo =
        a[prefijo..].iter().rev().zip(b[prefijo..].iter().rev()).take_while(|(x, y)| x == y).count();
    let medio_a = &a[prefijo..a.len() - sufijo];
    let medio_b = &b[prefijo..b.len() - sufijo];

    for hunk in hunks(medio_a, medio_b) {
        let (borradas, agregadas) = (hunk.base_fin - hunk.base_inicio, hunk.actual_fin - hunk.actual_inicio);
        let inicio = prefijo + hunk.actual_inicio;
        if agregadas > 0 {
            let marca = if borradas > 0 { MarcaGit::Modificada } else { MarcaGit::Agregada };
            for m in &mut marcas[inicio..prefijo + hunk.actual_fin] {
                *m = Some(marca);
            }
        } else if borradas > 0 {
            let idx = if inicio < b.len() { inicio } else { inicio.saturating_sub(1) };
            if marcas[idx].is_none() {
                marcas[idx] = Some(MarcaGit::Borrada);
            }
        }
    }
    marcas
}

/// Un tramo contiguo de diferencias: las líneas `base_inicio..base_fin`
/// de la base se reemplazan por `actual_inicio..actual_fin` del texto
/// actual (cualquiera de los dos rangos puede estar vacío).
#[derive(Debug, PartialEq, Eq)]
struct Hunk {
    base_inicio: usize,
    base_fin: usize,
    actual_inicio: usize,
    actual_fin: usize,
}

/// Diff por líneas de `a` contra `b` (algoritmo de Myers, "An O(ND)
/// Difference Algorithm", 1986), agrupado en hunks. Si la distancia
/// supera [`MAX_DISTANCIA_MYERS`], devuelve un único hunk que cubre todo
/// (ver ese comentario).
fn hunks(a: &[&str], b: &[&str]) -> Vec<Hunk> {
    let (n, m) = (a.len(), b.len());
    if n == 0 && m == 0 {
        return Vec::new();
    }
    let Some((borrada, insertada)) = myers(a, b) else {
        return vec![Hunk { base_inicio: 0, base_fin: n, actual_inicio: 0, actual_fin: m }];
    };

    // Las líneas no borradas de `a` y no insertadas de `b` se emparejan
    // en orden: se recorren las dos a la vez, y cada racha de
    // borradas/insertadas entre dos emparejadas es un hunk.
    let mut resultado = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n || j < m {
        let (inicio_i, inicio_j) = (i, j);
        loop {
            let antes = (i, j);
            while i < n && borrada[i] {
                i += 1;
            }
            while j < m && insertada[j] {
                j += 1;
            }
            if (i, j) == antes {
                break;
            }
        }
        if (i, j) != (inicio_i, inicio_j) {
            resultado.push(Hunk { base_inicio: inicio_i, base_fin: i, actual_inicio: inicio_j, actual_fin: j });
        } else {
            i += 1;
            j += 1;
        }
    }
    resultado
}

/// Myers hacia adelante guardando el frente `V` de cada paso, y después
/// el camino hacia atrás para saber qué líneas de `a` se borran y cuáles
/// de `b` se insertan. `None` si hacen falta más de
/// [`MAX_DISTANCIA_MYERS`] pasos. `V` se indexa por diagonal `k = x - y`
/// desplazada en `desp` para que no quede negativa.
fn myers(a: &[&str], b: &[&str]) -> Option<(Vec<bool>, Vec<bool>)> {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let limite = (a.len() + b.len()).min(MAX_DISTANCIA_MYERS) as isize;
    let desp = limite + 1;
    let mut v = vec![0isize; 2 * limite as usize + 3];
    // `rastro[d]` = frente después del paso `d`, solo las diagonales
    // `-d..=d` (las únicas alcanzables en `d` pasos): memoria O(D²) en
    // vez de O(D·(N+M)).
    let mut rastro: Vec<Vec<isize>> = Vec::new();

    let mut d_final = None;
    'pasos: for d in 0..=limite {
        let mut k = -d;
        while k <= d {
            let mut x = if k == -d || (k != d && v[(k - 1 + desp) as usize] < v[(k + 1 + desp) as usize]) {
                v[(k + 1 + desp) as usize]
            } else {
                v[(k - 1 + desp) as usize] + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[(k + desp) as usize] = x;
            if x >= n && y >= m {
                rastro.push(v[(desp - d) as usize..=(desp + d) as usize].to_vec());
                d_final = Some(d);
                break 'pasos;
            }
            k += 2;
        }
        rastro.push(v[(desp - d) as usize..=(desp + d) as usize].to_vec());
    }
    let d_final = d_final?;

    let mut borrada = vec![false; a.len()];
    let mut insertada = vec![false; b.len()];
    let (mut x, mut y) = (n, m);
    for d in (1..=d_final).rev() {
        let previo = &rastro[(d - 1) as usize];
        // Frente del paso `d - 1` en la diagonal `k` (que va de `-(d-1)`
        // a `d-1`).
        let en = |k: isize| previo[(k + d - 1) as usize];
        let k = x - y;
        let k_previo = if k == -d || (k != d && en(k - 1) < en(k + 1)) { k + 1 } else { k - 1 };
        let x_previo = en(k_previo);
        let y_previo = x_previo - k_previo;
        if k_previo == k + 1 {
            insertada[y_previo as usize] = true;
        } else {
            borrada[x_previo as usize] = true;
        }
        x = x_previo;
        y = y_previo;
    }
    Some((borrada, insertada))
}

/// Estado de los indicadores de git de UN documento abierto: la base de
/// `HEAD` y las marcas calculadas contra el último texto visto. Pensado
/// para llamarse una vez por frame con [`DiffGit::actualizar`] — barato
/// cuando nada cambió (compara el texto trozo a trozo, sin copiarlo).
///
/// Nada de esto corre en el hilo de la UI: la base la lee un hilo aparte
/// (un `git cat-file` por carga), y el diff lo calcula un hilo "calculador"
/// propio de cada `DiffGit`, que vive mientras viva el `DiffGit`. Medido
/// con 10.000 líneas, [`calcular_marcas`] tarda ~2-3 ms con cambios
/// repartidos por todo el archivo (partir los dos textos en líneas ya es
/// ~1,7 ms) — hacerlo en cada tecla dentro del frame duplicaba su costo
/// (BACKLOG.md P1 #14). En el hilo de la UI solo queda comparar el texto
/// y, si cambió, copiarlo para mandarlo al calculador (medido con 10.000
/// líneas: ~25 µs por frame sin edición, ~60 µs con edición); las marcas
/// nuevas se ven en el frame siguiente a que termine (en la práctica, el
/// de la tecla que sigue o el sondeo de `app`, ver [`DiffGit::pendiente`]).
///
/// Cuándo se vuelve a leer la base: al abrir otro archivo (cambia la
/// ruta que recibe `actualizar`), y cuando quien lo usa llama
/// [`DiffGit::refrescar_base`] (la app lo hace al guardar). Un commit
/// hecho desde otra terminal no se detecta solo: la base queda vieja
/// hasta el próximo guardado (`Ctrl+S` alcanza, aunque no haya cambios).
pub struct DiffGit {
    ruta: Option<PathBuf>,
    carga: Option<(Receiver<Option<String>>, Instant)>,
    base: Option<Arc<str>>,
    /// Texto que se mandó a calcular por última vez. Se compara trozo a
    /// trozo contra el buffer en cada `actualizar` (sin copiar nada si
    /// no cambió).
    texto: String,
    marcas: Vec<Option<MarcaGit>>,
    /// `false` cuando hay que mandar a calcular aunque el texto no haya
    /// cambiado (acaba de llegar una base nueva).
    vigente: bool,
    calculador: Option<Calculador>,
    /// Número del último trabajo mandado al calculador y del último
    /// resultado recibido: distintos = hay un cálculo en curso.
    enviado: u64,
    recibido: u64,
}

/// Canal de ida y vuelta con el hilo que calcula el diff. Si llegan varios
/// trabajos mientras calcula uno (tipeo rápido), al terminar se saltea
/// todos menos el último — nunca se acumula atraso. Al soltar el
/// `DiffGit` se cierra `emisor` y el hilo termina solo.
struct Calculador {
    emisor: Sender<Trabajo>,
    receptor: Receiver<(u64, Vec<Option<MarcaGit>>)>,
}

struct Trabajo {
    numero: u64,
    base: Arc<str>,
    texto: String,
}

impl Calculador {
    fn lanzar() -> Self {
        let (emisor, trabajos) = mpsc::channel::<Trabajo>();
        let (emisor_resultados, receptor) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(mut trabajo) = trabajos.recv() {
                while let Ok(mas_nuevo) = trabajos.try_recv() {
                    trabajo = mas_nuevo;
                }
                let marcas = calcular_marcas(&trabajo.base, &trabajo.texto);
                if emisor_resultados.send((trabajo.numero, marcas)).is_err() {
                    break;
                }
            }
        });
        Self { emisor, receptor }
    }
}

impl Default for DiffGit {
    fn default() -> Self {
        Self::nuevo()
    }
}

impl DiffGit {
    pub fn nuevo() -> Self {
        Self {
            ruta: None,
            carga: None,
            base: None,
            texto: String::new(),
            marcas: Vec::new(),
            vigente: false,
            calculador: None,
            enviado: 0,
            recibido: 0,
        }
    }

    /// Pone al día las marcas para el documento en `ruta` (`None` = buffer
    /// sin archivo: nunca hay marcas), cuyo texto actual son los `trozos`
    /// concatenados (los chunks del rope, para no tener que copiar el
    /// archivo entero en cada frame solo para ver si cambió). Si cambió
    /// la ruta, lanza la lectura de la base; si la base ya llegó y el
    /// texto cambió, manda a recalcular el diff; y recoge lo que hayan
    /// terminado los dos hilos desde el frame anterior.
    pub fn actualizar<'a, I>(&mut self, ruta: Option<&Path>, trozos: I)
    where
        I: Iterator<Item = &'a str> + Clone,
    {
        if self.ruta.as_deref() != ruta {
            // Otro archivo: se descarta todo (incluido el calculador, que
            // podría devolver marcas del archivo anterior).
            *self = Self::nuevo();
            self.ruta = ruta.map(Path::to_path_buf);
            self.lanzar_carga();
        }

        if let Some((receptor, inicio)) = &self.carga {
            match receptor.try_recv() {
                Ok(base) => {
                    self.base = base.map(Arc::from);
                    self.carga = None;
                    self.vigente = false;
                }
                Err(TryRecvError::Disconnected) => self.carga = None,
                Err(TryRecvError::Empty) => {
                    if inicio.elapsed() >= TIEMPO_MAXIMO_CARGA {
                        self.carga = None;
                    }
                }
            }
        }

        let Some(base) = &self.base else {
            self.marcas.clear();
            return;
        };

        if let Some(calculador) = &self.calculador {
            loop {
                match calculador.receptor.try_recv() {
                    Ok((numero, marcas)) => {
                        self.marcas = marcas;
                        self.recibido = numero;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        // El hilo murió (no debería pasar): sin marcas en
                        // vez de quedar "calculando" para siempre.
                        self.calculador = None;
                        self.recibido = self.enviado;
                        break;
                    }
                }
            }
        }

        if self.vigente && igual_a_trozos(&self.texto, trozos.clone()) {
            return;
        }
        self.texto.clear();
        self.texto.extend(trozos);
        self.vigente = true;
        let calculador = self.calculador.get_or_insert_with(Calculador::lanzar);
        self.enviado += 1;
        let trabajo = Trabajo { numero: self.enviado, base: Arc::clone(base), texto: self.texto.clone() };
        if calculador.emisor.send(trabajo).is_err() {
            self.calculador = None;
            self.recibido = self.enviado;
        }
    }

    /// Vuelve a leer la base de `HEAD` para la ruta actual (tras guardar:
    /// el usuario pudo haber commiteado desde otra terminal). Las marcas
    /// viejas se siguen mostrando hasta que llega la base nueva, en vez de
    /// parpadear a "sin marcas" en el medio.
    pub fn refrescar_base(&mut self) {
        self.lanzar_carga();
    }

    /// Si hay una lectura de la base o un cálculo del diff en curso — la
    /// app lo usa para volver a dibujar cuando termine aunque no llegue
    /// ninguna tecla.
    pub fn pendiente(&self) -> bool {
        self.carga.is_some() || self.enviado != self.recibido
    }

    /// Si el documento tiene base (está trackeado en `HEAD`): recién ahí
    /// tiene sentido reservarle una columna en el gutter.
    pub fn tiene_base(&self) -> bool {
        self.base.is_some()
    }

    /// Marca de cada línea (ver [`calcular_marcas`]); vacío si no hay
    /// base.
    pub fn marcas(&self) -> &[Option<MarcaGit>] {
        &self.marcas
    }

    fn lanzar_carga(&mut self) {
        let Some(ruta) = self.ruta.clone() else {
            self.carga = None;
            return;
        };
        let (emisor, receptor) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = emisor.send(leer_base_head(&ruta));
        });
        self.carga = Some((receptor, Instant::now()));
    }
}


/// Si `texto` es exactamente la concatenación de `trozos`, sin armarla.
fn igual_a_trozos<'a>(texto: &str, trozos: impl Iterator<Item = &'a str>) -> bool {
    let bytes = texto.as_bytes();
    let mut pos = 0usize;
    for trozo in trozos {
        let fin = pos + trozo.len();
        if fin > bytes.len() || &bytes[pos..fin] != trozo.as_bytes() {
            return false;
        }
        pos = fin;
    }
    pos == bytes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    use MarcaGit::{Agregada, Borrada, Modificada};

    #[test]
    fn sin_cambios_no_hay_marcas() {
        let texto = "a\nb\nc\n";
        assert_eq!(calcular_marcas(texto, texto), vec![None, None, None]);
    }

    #[test]
    fn linea_agregada_en_el_medio() {
        assert_eq!(calcular_marcas("a\nb\n", "a\nx\nb\n"), vec![None, Some(Agregada), None]);
    }

    #[test]
    fn lineas_agregadas_al_final_y_al_principio() {
        assert_eq!(calcular_marcas("a\n", "a\nx\ny\n"), vec![None, Some(Agregada), Some(Agregada)]);
        assert_eq!(calcular_marcas("a\n", "x\na\n"), vec![Some(Agregada), None]);
    }

    #[test]
    fn linea_modificada() {
        assert_eq!(calcular_marcas("a\nb\nc\n", "a\nB\nc\n"), vec![None, Some(Modificada), None]);
    }

    #[test]
    fn hunk_con_mas_agregadas_que_borradas_marca_todo_como_modificado() {
        assert_eq!(
            calcular_marcas("a\nb\nc\n", "a\nB1\nB2\nc\n"),
            vec![None, Some(Modificada), Some(Modificada), None]
        );
    }

    #[test]
    fn linea_borrada_marca_la_siguiente() {
        assert_eq!(calcular_marcas("a\nb\nc\n", "a\nc\n"), vec![None, Some(Borrada)]);
    }

    #[test]
    fn borrado_al_principio_marca_la_primera() {
        assert_eq!(calcular_marcas("a\nb\nc\n", "b\nc\n"), vec![Some(Borrada), None]);
    }

    #[test]
    fn borrado_al_final_marca_la_ultima() {
        assert_eq!(calcular_marcas("a\nb\nc\n", "a\nb\n"), vec![None, Some(Borrada)]);
    }

    #[test]
    fn borrar_todo_deja_una_marca_en_la_linea_vacia() {
        assert_eq!(calcular_marcas("a\nb\n", ""), vec![Some(Borrada)]);
    }

    #[test]
    fn archivo_nuevo_vacio_contra_contenido_es_todo_agregado() {
        assert_eq!(calcular_marcas("", "a\nb\n"), vec![Some(Agregada), Some(Agregada)]);
    }

    #[test]
    fn el_salto_de_linea_final_no_genera_marcas() {
        assert_eq!(calcular_marcas("a\nb", "a\nb\n"), vec![None, None]);
        assert_eq!(calcular_marcas("a\nb\n", "a\nb"), vec![None, None]);
    }

    #[test]
    fn varios_hunks_separados() {
        let base = "1\n2\n3\n4\n5\n6\n7\n";
        let actual = "1\nX\n3\n4\n6\n7\nnueva\n";
        assert_eq!(
            calcular_marcas(base, actual),
            vec![None, Some(Modificada), None, None, Some(Borrada), None, Some(Agregada)]
        );
    }

    #[test]
    fn lineas_repetidas_no_confunden_el_diff() {
        // Insertar una "}" entre varias "}" iguales: tiene que marcar
        // exactamente una línea como agregada, no un bloque modificado.
        let base = "{\n}\n}\n}\n";
        let actual = "{\n}\n}\n}\n}\n";
        let marcas = calcular_marcas(base, actual);
        assert_eq!(marcas.iter().filter(|m| m.is_some()).count(), 1);
        assert_eq!(marcas.iter().flatten().next(), Some(&Agregada));
    }

    #[test]
    fn hunks_coincide_con_un_diff_conocido() {
        // Ejemplo del paper de Myers: ABCABBA -> CBABAC (D = 5).
        let a: Vec<&str> = "ABCABBA".split("").filter(|s| !s.is_empty()).collect();
        let b: Vec<&str> = "CBABAC".split("").filter(|s| !s.is_empty()).collect();
        let (borrada, insertada) = myers(&a, &b).unwrap();
        let d = borrada.iter().filter(|x| **x).count() + insertada.iter().filter(|x| **x).count();
        assert_eq!(d, 5);
        // Lo que queda sin tocar de un lado tiene que ser igual a lo que
        // queda del otro (una subsecuencia común).
        let comun_a: Vec<&str> = a.iter().zip(&borrada).filter(|(_, b)| !**b).map(|(s, _)| *s).collect();
        let comun_b: Vec<&str> = b.iter().zip(&insertada).filter(|(_, i)| !**i).map(|(s, _)| *s).collect();
        assert_eq!(comun_a, comun_b);
    }

    #[test]
    fn distancia_enorme_cae_a_un_solo_bloque_modificado() {
        let base: String = (0..2000).map(|i| format!("base {i}\n")).collect();
        let actual: String = (0..2000).map(|i| format!("nueva {i}\n")).collect();
        let marcas = calcular_marcas(&base, &actual);
        assert_eq!(marcas.len(), 2000);
        assert!(marcas.iter().all(|m| *m == Some(Modificada)));
    }

    #[test]
    fn igual_a_trozos_compara_sin_importar_como_esta_partido() {
        assert!(igual_a_trozos("hola mundo", ["hola", " ", "mundo"].into_iter()));
        assert!(igual_a_trozos("", std::iter::empty()));
        assert!(!igual_a_trozos("hola", ["hola", "!"].into_iter()));
        assert!(!igual_a_trozos("hola!", ["hola"].into_iter()));
        assert!(!igual_a_trozos("hola", ["hoLa"].into_iter()));
    }

    #[test]
    fn diff_git_sin_ruta_nunca_tiene_marcas() {
        let mut diff = DiffGit::nuevo();
        diff.actualizar(None, ["a\n"].into_iter());
        assert!(!diff.pendiente());
        assert!(!diff.tiene_base());
        assert!(diff.marcas().is_empty());
    }

    #[test]
    fn leer_base_head_fuera_de_un_repo_da_none() {
        let dir = tempfile::tempdir().unwrap();
        let ruta = dir.path().join("suelto.txt");
        std::fs::write(&ruta, "hola\n").unwrap();
        // Con o sin `git` instalado, fuera de un repo no hay base.
        assert_eq!(leer_base_head(&ruta), None);
    }

    /// Repo real con `git` (se saltea en silencio si no está instalado):
    /// lee la base de un archivo trackeado, y `None` para uno sin
    /// trackear en el mismo repo.
    #[test]
    fn leer_base_head_en_un_repo_real() {
        let dir = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(["-c", "user.name=tcode", "-c", "user.email=tcode@example.com"])
                .args(args)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        };
        if !git(&["init", "-q"]) {
            return;
        }
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/a.txt"), "uno\r\ndos\r\n").unwrap();
        assert!(git(&["add", "."]));
        assert!(git(&["commit", "-q", "-m", "inicial"]));
        std::fs::write(dir.path().join("sub/nuevo.txt"), "x\n").unwrap();

        assert_eq!(leer_base_head(&dir.path().join("sub/a.txt")).as_deref(), Some("uno\ndos\n"));
        assert_eq!(leer_base_head(&dir.path().join("sub/nuevo.txt")), None);
    }

    /// Llama `actualizar` hasta que el calculador devuelva su resultado
    /// (como hace la app con su sondeo), con un tope para no colgar el
    /// test si algo se rompe.
    fn esperar(diff: &mut DiffGit, texto: &str) {
        let inicio = Instant::now();
        diff.actualizar(Some(Path::new("x")), std::iter::once(texto));
        while diff.pendiente() && inicio.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(1));
            diff.actualizar(Some(Path::new("x")), std::iter::once(texto));
        }
        assert!(!diff.pendiente());
    }

    /// Un `DiffGit` con la base ya cargada, sin pasar por `git`.
    fn diff_con_base(base: &str) -> DiffGit {
        let mut diff = DiffGit::nuevo();
        diff.ruta = Some(PathBuf::from("x"));
        diff.base = Some(Arc::from(base));
        diff
    }

    #[test]
    fn diff_git_calcula_en_segundo_plano_y_sigue_las_ediciones() {
        let mut diff = diff_con_base("a\nb\nc\n");
        esperar(&mut diff, "a\nb\nc\n");
        assert_eq!(diff.marcas(), &[None, None, None]);

        esperar(&mut diff, "a\nB\nc\n");
        assert_eq!(diff.marcas(), &[None, Some(Modificada), None]);

        // Varias ediciones seguidas sin esperar (tipeo rápido): al final
        // quedan las marcas del ÚLTIMO texto, no de uno intermedio.
        for texto in ["a\nB\nc\nd\n", "a\nb\nc\nd\n", "a\nc\nd\n"] {
            diff.actualizar(Some(Path::new("x")), std::iter::once(texto));
        }
        esperar(&mut diff, "a\nc\nd\n");
        assert_eq!(diff.marcas(), &[None, Some(Borrada), Some(Agregada)]);
    }

    #[test]
    fn diff_git_sin_cambios_de_texto_no_vuelve_a_calcular() {
        let mut diff = diff_con_base("a\n");
        esperar(&mut diff, "a\nx\n");
        let enviados = diff.enviado;
        // Mismo contenido partido en otros trozos: no cuenta como cambio.
        diff.actualizar(Some(Path::new("x")), ["a\n", "x", "\n"].into_iter());
        assert_eq!(diff.enviado, enviados);
        assert!(!diff.pendiente());
    }

    #[test]
    fn diff_git_al_cambiar_de_archivo_descarta_las_marcas_anteriores() {
        let mut diff = diff_con_base("a\n");
        esperar(&mut diff, "b\n");
        assert!(!diff.marcas().is_empty());
        diff.actualizar(None, std::iter::once("b\n"));
        assert!(!diff.tiene_base());
        assert!(diff.marcas().is_empty());
    }
}
