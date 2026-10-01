//! Blame en línea (BACKLOG.md P2 #25): quién cambió por última vez una
//! línea, cuándo y en qué commit, con `git blame` (el mismo binario que
//! usan los indicadores del gutter, sin dependencias nuevas).
//!
//! Se le pasa a `git` el texto ACTUAL del buffer (`--contents -`), no el
//! del disco: así los números de línea coinciden aunque haya cambios sin
//! guardar, y las líneas nuevas salen como "sin commitear". Bloqueante:
//! usar desde un hilo aparte.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// La última modificación de una línea.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InfoBlame {
    /// La línea no está en ningún commit (cambio sin commitear).
    SinCommitear,
    Commit { autor: String, tiempo_unix: i64, resumen: String },
}

/// `git blame` de la línea `linea` (0-based) del archivo `ruta`, cuyo
/// contenido actual es `contenido`. `None` si `git` falla (fuera de un
/// repo, archivo sin trackear, línea fuera de rango).
pub fn blame_linea(ruta: &Path, linea: usize, contenido: &str) -> Option<InfoBlame> {
    let carpeta = ruta.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let nombre = ruta.file_name()?.to_str()?;
    let numero = linea + 1;
    let mut hijo = Command::new("git")
        .arg("-C")
        .arg(carpeta)
        .args(["blame", "--porcelain", "--contents", "-", "-L"])
        .arg(format!("{numero},{numero}"))
        .arg("--")
        .arg(nombre)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // En otro hilo: con un archivo grande, `git` puede empezar a escribir
    // (y llenar el pipe de salida) antes de terminar de leer la entrada.
    let mut entrada = hijo.stdin.take()?;
    let contenido = contenido.to_string();
    let escritor = std::thread::spawn(move || {
        let _ = entrada.write_all(contenido.as_bytes());
    });
    let salida = hijo.wait_with_output().ok()?;
    let _ = escritor.join();
    if !salida.status.success() {
        return None;
    }
    parsear_porcelain(&String::from_utf8_lossy(&salida.stdout))
}

/// La salida de `git blame --porcelain` para una línea: la cabecera
/// `<sha> <línea original> <línea final> <cantidad>` y los campos
/// `author`, `author-time` y `summary`. Un sha de puros ceros es una
/// línea sin commitear.
pub fn parsear_porcelain(salida: &str) -> Option<InfoBlame> {
    let mut lineas = salida.lines();
    let sha = lineas.next()?.split_whitespace().next()?;
    if sha.chars().all(|c| c == '0') {
        return Some(InfoBlame::SinCommitear);
    }
    let (mut autor, mut tiempo_unix, mut resumen) = (None, None, None);
    for linea in lineas {
        if let Some(valor) = linea.strip_prefix("author ") {
            autor = Some(valor.to_string());
        } else if let Some(valor) = linea.strip_prefix("author-time ") {
            tiempo_unix = valor.trim().parse().ok();
        } else if let Some(valor) = linea.strip_prefix("summary ") {
            resumen = Some(valor.to_string());
        }
    }
    Some(InfoBlame::Commit { autor: autor?, tiempo_unix: tiempo_unix?, resumen: resumen.unwrap_or_default() })
}

/// Lo que se muestra al final de la línea: "Autor, hace 3 días · resumen"
/// o "Sin commitear". `ahora_unix` es la hora actual (parámetro para que
/// los tests no dependan del reloj).
pub fn texto_blame(info: &InfoBlame, ahora_unix: i64) -> String {
    match info {
        InfoBlame::SinCommitear => "Sin commitear".to_string(),
        InfoBlame::Commit { autor, tiempo_unix, resumen } => {
            format!("{autor}, {} · {resumen}", hace(ahora_unix - tiempo_unix))
        }
    }
}

fn hace(segundos: i64) -> String {
    let (n, unidad, plural) = match segundos.max(0) {
        s if s < 60 => return "recién".to_string(),
        s if s < 3600 => (s / 60, "minuto", "minutos"),
        s if s < 86_400 => (s / 3600, "hora", "horas"),
        s if s < 30 * 86_400 => (s / 86_400, "día", "días"),
        s if s < 365 * 86_400 => (s / (30 * 86_400), "mes", "meses"),
        s => (s / (365 * 86_400), "año", "años"),
    };
    format!("hace {n} {}", if n == 1 { unidad } else { plural })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PORCELAIN: &str = "\
3f1c0a2b9d8e7f6a5b4c3d2e1f0a9b8c7d6e5f4a 12 12 1
author Ada Lovelace
author-mail <ada@example.com>
author-time 1700000000
author-tz -0300
committer Ada Lovelace
summary fix: el cálculo del motor analítico
filename a.rs
\tlet x = 1;
";

    #[test]
    fn parsea_un_commit_y_una_linea_sin_commitear() {
        assert_eq!(
            parsear_porcelain(PORCELAIN),
            Some(InfoBlame::Commit {
                autor: "Ada Lovelace".to_string(),
                tiempo_unix: 1_700_000_000,
                resumen: "fix: el cálculo del motor analítico".to_string(),
            })
        );
        let sin = "0000000000000000000000000000000000000000 3 3 1\nauthor Not Committed Yet\n";
        assert_eq!(parsear_porcelain(sin), Some(InfoBlame::SinCommitear));
        assert_eq!(parsear_porcelain(""), None);
    }

    #[test]
    fn texto_con_tiempo_relativo() {
        let info = parsear_porcelain(PORCELAIN).unwrap();
        let t = 1_700_000_000;
        assert_eq!(texto_blame(&info, t + 30), "Ada Lovelace, recién · fix: el cálculo del motor analítico");
        assert!(texto_blame(&info, t + 3 * 86_400).contains("hace 3 días"));
        assert!(texto_blame(&info, t + 3600).contains("hace 1 hora"));
        assert!(texto_blame(&info, t + 2 * 365 * 86_400).contains("hace 2 años"));
        assert_eq!(texto_blame(&InfoBlame::SinCommitear, t), "Sin commitear");
    }

    /// Contra un repo de verdad (si hay `git`): una línea commiteada y
    /// una agregada en el buffer sin guardar.
    #[test]
    fn blame_de_un_repo_real_con_cambios_sin_guardar() {
        let dir = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            Command::new("git").arg("-C").arg(dir.path()).args(args).output().map(|s| s.status.success()).unwrap_or(false)
        };
        if !git(&["init", "-q"]) {
            return; // Sin `git` instalado no hay nada que probar.
        }
        std::fs::write(dir.path().join("a.txt"), "uno\ndos\n").unwrap();
        assert!(git(&["add", "a.txt"]));
        assert!(git(&["-c", "user.name=Prueba", "-c", "user.email=p@p", "commit", "-qm", "primero"]));
        let ruta = dir.path().join("a.txt");
        let buffer = "uno\nnueva\ndos\n";
        match blame_linea(&ruta, 0, buffer) {
            Some(InfoBlame::Commit { autor, resumen, .. }) => assert_eq!((autor.as_str(), resumen.as_str()), ("Prueba", "primero")),
            otro => panic!("se esperaba el commit, no {otro:?}"),
        }
        assert_eq!(blame_linea(&ruta, 1, buffer), Some(InfoBlame::SinCommitear));
        // La línea 2 del buffer es la 1 del commit.
        assert!(matches!(blame_linea(&ruta, 2, buffer), Some(InfoBlame::Commit { .. })));
        assert_eq!(blame_linea(&ruta, 99, buffer), None);
    }
}
