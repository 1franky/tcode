//! Portapapeles del sistema (BACKLOG.md P0 #15): `Ctrl+C`/`Ctrl+X`
//! escriben, `Ctrl+V` lee. Sin nada de UI — solo la terminal como flujo
//! de bytes (la secuencia OSC 52 se escribe directo en stdout, entre dos
//! frames de ratatui) y procesos del sistema.
//!
//! Dos caminos para escribir, que en modo automático se usan los dos:
//!
//! - **OSC 52** (`ESC ] 52 ; c ; <base64> BEL`): le pide a la TERMINAL
//!   que ponga el texto en el portapapeles. Anda por SSH (el portapapeles
//!   que se llena es el de la máquina donde corre la terminal) y en casi
//!   todas las terminales modernas (kitty, WezTerm, Alacritty, iTerm2,
//!   Windows Terminal, foot, Ghostty...); Terminal.app de macOS la ignora.
//!   Dentro de tmux hace falta `set -g set-clipboard on` para que tmux la
//!   reenvíe. No se usa para LEER: la mayoría de las terminales bloquean
//!   la lectura (por seguridad) y la respuesta llegaría mezclada con las
//!   teclas.
//! - **Herramientas del sistema** (`pbcopy`, `wl-copy`, `xclip`, `xsel`,
//!   `clip.exe`): funcionan aunque la terminal no sepa nada de OSC 52, y
//!   son las únicas que sirven para leer (`pbpaste`, `wl-paste`,
//!   `xclip -o`, `Get-Clipboard`).
//!
//! Además se guarda siempre una copia interna de lo último copiado: con
//! el portapapeles desactivado, o si no se puede leer el del sistema,
//! `Ctrl+V` pega eso. Y como la copia interna sabe si eran líneas
//! enteras (`Ctrl+C` sin selección), pegar el mismo texto desde el
//! sistema también inserta la línea arriba, como VSCode.
//!
//! Los procesos corren con un límite de tiempo corto: en el caso normal
//! tardan unos milisegundos, y uno colgado (un `xclip` sin servidor X
//! que responda) no puede congelar el editor.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tcode_config::ModoPortapapeles;
use tcode_core::TextoCopiado;

/// Tope del texto en base64 dentro de la secuencia OSC 52. Las
/// terminales tienen límites propios (tmux y xterm rondan los 100 KB,
/// algunas cortan antes) y una secuencia gigante en stdout se nota; más
/// allá de esto se usa solo la herramienta del sistema.
pub const LIMITE_OSC52: usize = 100_000;

/// Límites de los procesos: escribir es lo que pasa en cada `Ctrl+C`;
/// leer puede ser PowerShell, que arranca lento.
const LIMITE_ESCRITURA: Duration = Duration::from_millis(500);
const LIMITE_LECTURA: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plataforma {
    MacOs,
    Windows,
    /// Linux, BSD y demás: depende de Wayland/X11 (o WSL).
    Unix,
}

/// Lo que decide qué herramientas probar — separado de `detectar` para
/// poder testear la elección sin depender de la máquina donde corren los
/// tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entorno {
    pub plataforma: Plataforma,
    pub wayland: bool,
    pub x11: bool,
    /// Linux dentro de WSL: el portapapeles es el de Windows.
    pub wsl: bool,
}

impl Entorno {
    pub fn detectar() -> Self {
        let definida = |nombre: &str| std::env::var_os(nombre).is_some_and(|v| !v.is_empty());
        let plataforma = if cfg!(target_os = "macos") {
            Plataforma::MacOs
        } else if cfg!(windows) {
            Plataforma::Windows
        } else {
            Plataforma::Unix
        };
        Self {
            plataforma,
            wayland: definida("WAYLAND_DISPLAY"),
            x11: definida("DISPLAY"),
            wsl: definida("WSL_DISTRO_NAME") || definida("WSL_INTEROP"),
        }
    }
}

/// Un programa del sistema para escribir o leer el portapapeles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Herramienta {
    pub programa: &'static str,
    pub argumentos: &'static [&'static str],
    /// `clip.exe` interpreta la entrada en la página de códigos de la
    /// consola salvo que venga en UTF-16 con BOM.
    pub utf16: bool,
}

const fn herramienta(programa: &'static str, argumentos: &'static [&'static str]) -> Herramienta {
    Herramienta { programa, argumentos, utf16: false }
}

const CLIP_EXE: Herramienta = Herramienta { programa: "clip.exe", argumentos: &[], utf16: true };
/// `-Raw` para no perder los saltos de línea; la salida se fuerza a
/// UTF-8 (por defecto sería la página de códigos de la consola).
const ARGUMENTOS_GET_CLIPBOARD: &[&str] = &[
    "-NoProfile",
    "-NonInteractive",
    "-Command",
    "[Console]::OutputEncoding = [Text.Encoding]::UTF8; Get-Clipboard -Raw",
];

/// Herramientas para escribir, en orden de preferencia (se usa la
/// primera que exista y termine bien). Wayland antes que X11: en una
/// sesión Wayland `DISPLAY` suele estar definida también (XWayland).
pub fn herramientas_escritura(entorno: &Entorno) -> Vec<Herramienta> {
    let mut lista = Vec::new();
    match entorno.plataforma {
        Plataforma::MacOs => lista.push(herramienta("pbcopy", &[])),
        Plataforma::Windows => lista.push(CLIP_EXE),
        Plataforma::Unix => {
            if entorno.wayland {
                lista.push(herramienta("wl-copy", &[]));
            }
            if entorno.x11 {
                lista.push(herramienta("xclip", &["-selection", "clipboard"]));
                lista.push(herramienta("xsel", &["--clipboard", "--input"]));
            }
            if entorno.wsl {
                lista.push(CLIP_EXE);
            }
        }
    }
    lista
}

/// Herramientas para leer, mismo criterio que `herramientas_escritura`.
pub fn herramientas_lectura(entorno: &Entorno) -> Vec<Herramienta> {
    let mut lista = Vec::new();
    match entorno.plataforma {
        Plataforma::MacOs => lista.push(herramienta("pbpaste", &[])),
        Plataforma::Windows => lista.push(herramienta("powershell", ARGUMENTOS_GET_CLIPBOARD)),
        Plataforma::Unix => {
            if entorno.wayland {
                lista.push(herramienta("wl-paste", &["--no-newline"]));
            }
            if entorno.x11 {
                lista.push(herramienta("xclip", &["-selection", "clipboard", "-o"]));
                lista.push(herramienta("xsel", &["--clipboard", "--output"]));
            }
            if entorno.wsl {
                lista.push(herramienta("powershell.exe", ARGUMENTOS_GET_CLIPBOARD));
            }
        }
    }
    lista
}

/// La secuencia OSC 52 que pone `texto` en el portapapeles ("c") de la
/// terminal, o `None` si pasa `LIMITE_OSC52`. Termina en BEL y no en
/// `ESC \`: es la forma que acepta más terminales (y tmux).
pub fn secuencia_osc52(texto: &str) -> Option<String> {
    let codificado = base64(texto.as_bytes());
    (codificado.len() <= LIMITE_OSC52).then(|| format!("\x1b]52;c;{codificado}\x07"))
}

/// Base64 estándar (RFC 4648, con relleno `=`) — son veinte líneas, no
/// justifican una dependencia.
pub fn base64(bytes: &[u8]) -> String {
    const ALFABETO: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut salida = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for bloque in bytes.chunks(3) {
        let b = [bloque[0], *bloque.get(1).unwrap_or(&0), *bloque.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= bloque.len() {
                salida.push(ALFABETO[((n >> (18 - 6 * i)) & 0x3f) as usize] as char);
            } else {
                salida.push('=');
            }
        }
    }
    salida
}

/// Bytes que recibe la herramienta por stdin.
fn entrada_para(herramienta: &Herramienta, texto: &str) -> Vec<u8> {
    if !herramienta.utf16 {
        return texto.as_bytes().to_vec();
    }
    let mut bytes = vec![0xff, 0xfe];
    // `clip.exe` espera finales de línea de Windows.
    for unidad in texto.replace("\r\n", "\n").replace('\n', "\r\n").encode_utf16() {
        bytes.extend_from_slice(&unidad.to_le_bytes());
    }
    bytes
}

/// Corre `herramienta` con `entrada` por stdin (si hay) y devuelve su
/// stdout si terminó bien antes de `limite`; si no, la mata y devuelve
/// `None`. stdin y stdout van en hilos aparte: un texto grande llenaría
/// el pipe y se trabarían el uno al otro.
fn ejecutar(herramienta: &Herramienta, entrada: Option<Vec<u8>>, leer_salida: bool, limite: Duration) -> Option<Vec<u8>> {
    let mut comando = Command::new(herramienta.programa);
    comando
        .args(herramienta.argumentos)
        .stdin(if entrada.is_some() { Stdio::piped() } else { Stdio::null() })
        // `wl-copy`/`xclip` se quedan en segundo plano sirviendo la
        // selección: si heredaran un stdout nuestro abierto, leerlo no
        // terminaría nunca.
        .stdout(if leer_salida { Stdio::piped() } else { Stdio::null() })
        .stderr(Stdio::null());
    // `pbcopy`/`pbpaste` interpretan los bytes según el locale: sin
    // uno UTF-8 (`LANG` vacío, típico en SSH o launchd) rompen los
    // acentos.
    if herramienta.programa.starts_with("pb") {
        comando.env("LC_CTYPE", "UTF-8");
    }
    let mut hijo = comando.spawn().ok()?;
    if let (Some(bytes), Some(mut stdin)) = (entrada, hijo.stdin.take()) {
        std::thread::spawn(move || {
            let _ = stdin.write_all(&bytes);
        });
    }
    let lector = hijo.stdout.take().map(|mut stdout| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stdout.read_to_end(&mut bytes);
            bytes
        })
    });
    let inicio = Instant::now();
    let estado = loop {
        match hijo.try_wait() {
            Ok(Some(estado)) => break estado,
            Ok(None) if inicio.elapsed() < limite => std::thread::sleep(Duration::from_millis(2)),
            _ => {
                let _ = hijo.kill();
                let _ = hijo.wait();
                return None;
            }
        }
    };
    if !estado.success() {
        return None;
    }
    Some(lector.and_then(|h| h.join().ok()).unwrap_or_default())
}

/// A dónde llegó lo copiado (para el aviso de la barra de estado).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResultadoCopia {
    /// Se mandó la secuencia OSC 52 (no hay forma de saber si la terminal
    /// la aceptó).
    pub osc52: bool,
    /// Alguna herramienta del sistema la tomó.
    pub sistema: bool,
}

impl ResultadoCopia {
    pub fn salio_de_tcode(&self) -> bool {
        self.osc52 || self.sistema
    }
}

pub struct Portapapeles {
    entorno: Entorno,
    /// Lo último copiado desde tcode (`Ctrl+C`/`Ctrl+X`, `"+y`...).
    interno: Option<TextoCopiado>,
}

impl Portapapeles {
    pub fn nuevo() -> Self {
        Self { entorno: Entorno::detectar(), interno: None }
    }

    /// Copia `copiado` según `modo`: la secuencia OSC 52 a la terminal y/o
    /// la primera herramienta del sistema que funcione. Siempre queda
    /// también como copia interna.
    pub fn copiar(&mut self, copiado: TextoCopiado, modo: ModoPortapapeles) -> ResultadoCopia {
        let mut resultado = ResultadoCopia { osc52: false, sistema: false };
        if modo.usa_osc52() {
            if let Some(secuencia) = secuencia_osc52(&copiado.texto) {
                let mut stdout = std::io::stdout();
                resultado.osc52 = stdout.write_all(secuencia.as_bytes()).and_then(|_| stdout.flush()).is_ok();
            }
        }
        if modo.usa_sistema() {
            resultado.sistema = herramientas_escritura(&self.entorno).iter().any(|h| {
                ejecutar(h, Some(entrada_para(h, &copiado.texto)), false, LIMITE_ESCRITURA).is_some()
            });
        }
        self.interno = Some(copiado);
        resultado
    }

    /// Lo que pega `Ctrl+V`: el portapapeles del sistema si el modo lo
    /// permite y alguna herramienta lo pudo leer; si no, la copia
    /// interna. Si lo leído es lo mismo que se copió desde tcode, conserva
    /// si eran líneas enteras.
    pub fn leer(&self, modo: ModoPortapapeles) -> Option<TextoCopiado> {
        let del_sistema = if modo.usa_sistema() {
            herramientas_lectura(&self.entorno).iter().find_map(|h| {
                let bytes = ejecutar(h, None, true, LIMITE_LECTURA)?;
                String::from_utf8(bytes).ok()
            })
        } else {
            None
        };
        match del_sistema {
            Some(texto) => Some(self.con_tipo_conocido(texto)),
            None => self.interno.clone(),
        }
    }

    fn con_tipo_conocido(&self, texto: String) -> TextoCopiado {
        let texto = texto.replace("\r\n", "\n");
        let lineal = self.interno.as_ref().is_some_and(|i| i.lineal && i.texto == texto);
        TextoCopiado { texto, lineal }
    }
}

/// "3 líneas"/"1 línea": cuántas líneas ocupa lo copiado, para el aviso
/// de la barra de estado. Un texto lineal termina en `\n` (ese salto no
/// abre otra línea).
pub fn describir_lineas(copiado: &TextoCopiado) -> String {
    let saltos = copiado.texto.matches('\n').count();
    let lineas = if copiado.texto.ends_with('\n') { saltos.max(1) } else { saltos + 1 };
    if lineas == 1 {
        "1 línea".to_string()
    } else {
        format!("{lineas} líneas")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unix(wayland: bool, x11: bool, wsl: bool) -> Entorno {
        Entorno { plataforma: Plataforma::Unix, wayland, x11, wsl }
    }

    fn programas(lista: Vec<Herramienta>) -> Vec<&'static str> {
        lista.into_iter().map(|h| h.programa).collect()
    }

    #[test]
    fn base64_con_y_sin_relleno() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64("ñandú\n".as_bytes()), "w7FhbmTDugo=");
    }

    #[test]
    fn secuencia_osc52_y_su_limite() {
        assert_eq!(secuencia_osc52("hola").as_deref(), Some("\x1b]52;c;aG9sYQ==\x07"));
        // 75 000 bytes son exactamente 100 000 en base64: entra justo.
        assert!(secuencia_osc52(&"a".repeat(75_000)).is_some());
        assert!(secuencia_osc52(&"a".repeat(75_003)).is_none());
    }

    #[test]
    fn elige_las_herramientas_segun_la_plataforma() {
        let mac = Entorno { plataforma: Plataforma::MacOs, wayland: false, x11: false, wsl: false };
        assert_eq!(programas(herramientas_escritura(&mac)), ["pbcopy"]);
        assert_eq!(programas(herramientas_lectura(&mac)), ["pbpaste"]);
        let windows = Entorno { plataforma: Plataforma::Windows, ..mac };
        assert_eq!(programas(herramientas_escritura(&windows)), ["clip.exe"]);
        assert_eq!(programas(herramientas_lectura(&windows)), ["powershell"]);
    }

    #[test]
    fn en_linux_wayland_va_antes_que_x11_y_sin_pantalla_no_hay_ninguna() {
        assert_eq!(programas(herramientas_escritura(&unix(true, true, false))), ["wl-copy", "xclip", "xsel"]);
        assert_eq!(programas(herramientas_lectura(&unix(true, true, false))), ["wl-paste", "xclip", "xsel"]);
        assert_eq!(programas(herramientas_escritura(&unix(false, true, false))), ["xclip", "xsel"]);
        // Un servidor por SSH: solo queda OSC 52.
        assert!(herramientas_escritura(&unix(false, false, false)).is_empty());
        assert!(herramientas_lectura(&unix(false, false, false)).is_empty());
        assert_eq!(programas(herramientas_escritura(&unix(false, false, true))), ["clip.exe"]);
        assert_eq!(programas(herramientas_lectura(&unix(false, false, true))), ["powershell.exe"]);
    }

    #[test]
    fn clip_exe_recibe_utf16_con_bom_y_crlf() {
        let bytes = entrada_para(&CLIP_EXE, "a\nñ");
        assert_eq!(bytes, [0xff, 0xfe, b'a', 0, b'\r', 0, b'\n', 0, 0xf1, 0]);
        assert_eq!(entrada_para(&herramienta("pbcopy", &[]), "ñ"), "ñ".as_bytes());
    }

    #[cfg(unix)]
    #[test]
    fn un_proceso_colgado_no_pasa_del_limite() {
        let dormir = herramienta("sleep", &["5"]);
        let inicio = Instant::now();
        assert!(ejecutar(&dormir, None, true, Duration::from_millis(100)).is_none());
        assert!(inicio.elapsed() < Duration::from_secs(2));
        // Uno que no existe tampoco rompe nada.
        assert!(ejecutar(&herramienta("tcode-no-existe", &[]), None, true, LIMITE_LECTURA).is_none());
        let eco = herramienta("cat", &[]);
        assert_eq!(ejecutar(&eco, Some(b"hola".to_vec()), true, LIMITE_LECTURA).as_deref(), Some(&b"hola"[..]));
    }

    #[test]
    fn desactivado_usa_solo_la_copia_interna_y_recuerda_si_era_lineal() {
        let mut portapapeles = Portapapeles { entorno: unix(false, false, false), interno: None };
        assert_eq!(portapapeles.leer(ModoPortapapeles::Desactivado), None);
        let copiado = TextoCopiado { texto: "linea\n".to_string(), lineal: true };
        let resultado = portapapeles.copiar(copiado.clone(), ModoPortapapeles::Desactivado);
        assert!(!resultado.salio_de_tcode());
        assert_eq!(portapapeles.leer(ModoPortapapeles::Desactivado), Some(copiado));
        // El mismo texto desde el sistema (con CRLF de Windows) sigue
        // siendo lineal; otro texto, no.
        assert!(portapapeles.con_tipo_conocido("linea\r\n".to_string()).lineal);
        assert!(!portapapeles.con_tipo_conocido("otra\n".to_string()).lineal);
    }

    #[test]
    fn describir_lineas_cuenta_como_la_barra_de_estado() {
        let t = |texto: &str, lineal| describir_lineas(&TextoCopiado { texto: texto.to_string(), lineal });
        assert_eq!(t("gato", false), "1 línea");
        assert_eq!(t("uno\n", true), "1 línea");
        assert_eq!(t("uno\ndos\n", true), "2 líneas");
        assert_eq!(t("a\nb", false), "2 líneas");
    }
}
