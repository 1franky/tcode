//! "Confiar en este proyecto": la lista de proyectos cuya `.tcode/
//! config.toml` puede fijar claves que ejecutan comandos (comandos LSP,
//! sus variables de entorno y los formateadores externos — ver
//! `CLAVES_QUE_EJECUTAN_COMANDOS` en `proyecto`).
//!
//! La lista vive SOLO en la config global del usuario (sección
//! `[confianza]`): un proyecto no puede declararse confiable a sí mismo
//! (`proyecto` descarta esa sección de cualquier `.tcode/config.toml`).
//! Cada entrada identifica al proyecto por la ruta canónica de su raíz
//! (la carpeta que contiene `.tcode/`) Y por el SHA-256 del contenido de
//! su `.tcode/config.toml` en el momento en que el usuario confió: si el
//! archivo cambia (un `git pull` que trae un `lsp_comando` nuevo, por
//! ejemplo), el hash deja de coincidir y el proyecto vuelve a ser no
//! confiable hasta que el usuario renueve la confianza a mano.
//!
//! El SHA-256 está implementado acá (unas decenas de líneas, verificado
//! con los vectores de prueba del estándar) en vez de sumar una
//! dependencia solo para esto: se usa para detectar CAMBIOS del archivo,
//! no para nada criptográficamente más delicado, pero un hash de 64 bits
//! (`DefaultHasher`) sí sería forzable a mano por quien controla el repo.

use serde::{Deserialize, Serialize};

/// Un proyecto marcado como confiable (ver doc del módulo).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct ProyectoConfiable {
    /// Ruta canónica de la raíz del proyecto (la carpeta con `.tcode/`).
    pub ruta: String,
    /// SHA-256 en hex (minúsculas) del contenido de `.tcode/config.toml`
    /// cuando se confió.
    pub sha256: String,
}

/// Sección `[confianza]` de la config global.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct ConfigConfianza {
    pub proyectos: Vec<ProyectoConfiable>,
}

impl ConfigConfianza {
    /// `true` solo si hay una entrada con esta MISMA ruta y este MISMO
    /// hash — un proyecto confiable cuyo archivo cambió no lo es más.
    pub fn confia_en(&self, ruta: &str, sha256: &str) -> bool {
        self.proyectos.iter().any(|p| p.ruta == ruta && p.sha256 == sha256)
    }

    /// Marca `ruta` como confiable con el contenido de hash `sha256`,
    /// reemplazando la entrada anterior de esa ruta si había una (renovar
    /// la confianza tras un cambio del archivo no deja entradas viejas).
    pub fn confiar(&mut self, ruta: &str, sha256: &str) {
        self.dejar_de_confiar(ruta);
        self.proyectos.push(ProyectoConfiable { ruta: ruta.to_string(), sha256: sha256.to_string() });
    }

    /// Quita cualquier entrada de `ruta` (con el hash que sea).
    pub fn dejar_de_confiar(&mut self, ruta: &str) {
        self.proyectos.retain(|p| p.ruta != ruta);
    }
}

/// SHA-256 de `datos` en hex, minúsculas (FIPS 180-4).
pub fn sha256_hex(datos: &[u8]) -> String {
    sha256(datos).iter().map(|b| format!("{b:02x}")).collect()
}

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98,
    0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
    0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8,
    0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819,
    0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
    0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
    0xc67178f2,
];

fn sha256(datos: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] =
        [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];

    // Relleno: un bit 1, ceros hasta 56 mod 64, y el largo en bits (u64
    // big-endian).
    let mut mensaje = datos.to_vec();
    mensaje.push(0x80);
    while mensaje.len() % 64 != 56 {
        mensaje.push(0);
    }
    mensaje.extend_from_slice(&((datos.len() as u64).wrapping_mul(8)).to_be_bytes());

    for bloque in mensaje.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (i, palabra) in bloque.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes(*palabra);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (acumulado, nuevo) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *acumulado = acumulado.wrapping_add(nuevo);
        }
    }

    let mut resultado = [0u8; 32];
    for (i, palabra) in h.iter().enumerate() {
        resultado[i * 4..i * 4 + 4].copy_from_slice(&palabra.to_be_bytes());
    }
    resultado
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_coincide_con_los_vectores_de_prueba_del_estandar() {
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        // 56 bytes: el relleno no entra en el mismo bloque.
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        let millon_de_a = vec![b'a'; 1_000_000];
        assert_eq!(sha256_hex(&millon_de_a), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
    }

    #[test]
    fn confiar_exige_misma_ruta_y_mismo_hash() {
        let mut confianza = ConfigConfianza::default();
        confianza.confiar("/p", "aaa");
        assert!(confianza.confia_en("/p", "aaa"));
        assert!(!confianza.confia_en("/p", "bbb"), "el archivo cambió: ya no es confiable");
        assert!(!confianza.confia_en("/otro", "aaa"));
    }

    #[test]
    fn renovar_la_confianza_reemplaza_la_entrada_anterior() {
        let mut confianza = ConfigConfianza::default();
        confianza.confiar("/p", "aaa");
        confianza.confiar("/q", "ccc");
        confianza.confiar("/p", "bbb");
        assert_eq!(confianza.proyectos.len(), 2);
        assert!(confianza.confia_en("/p", "bbb"));
        assert!(!confianza.confia_en("/p", "aaa"));

        confianza.dejar_de_confiar("/p");
        assert!(!confianza.confia_en("/p", "bbb"));
        assert!(confianza.confia_en("/q", "ccc"));
    }
}
