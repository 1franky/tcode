use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::combinacion::{formatear_atajo, parsear_atajo, Combinacion};

/// Directorio de configuración de tcode. Reutiliza la misma resolución que
/// `tcode-config` (mismo `config.toml`), en vez de duplicarla.
fn directorio_config() -> PathBuf {
    tcode_config::directorio_config()
}

/// Forma cruda del archivo `keymap.toml` (PLAN.md §4): una tabla por
/// ámbito, cada una mapeando el texto del atajo al nombre del comando en
/// español.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
struct KeymapCrudo {
    #[serde(default)]
    global: HashMap<String, String>,
    #[serde(default)]
    editor: HashMap<String, String>,
    #[serde(default)]
    markdown: HashMap<String, String>,
    #[serde(default)]
    csv: HashMap<String, String>,
}

/// Keymap ya resuelto: secuencia de combinaciones -> nombre de comando.
/// `[global]`, `[editor]`, `[markdown]` y `[csv]` se combinan en un único
/// mapa plano — no hay todavía un concepto de "ámbito activo" (el comando
/// mismo decide si aplica, p. ej. `markdown.alternar_preview` no hace
/// nada si el archivo activo no es Markdown), así que no hace falta
/// mantenerlos separados.
#[derive(Debug, Clone)]
pub struct Keymap {
    atajos: HashMap<Vec<Combinacion>, String>,
}

impl Keymap {
    fn desde_crudo(crudo: KeymapCrudo) -> Result<Self> {
        let mut atajos = HashMap::new();
        let todos = crudo.global.iter().chain(crudo.editor.iter()).chain(crudo.markdown.iter()).chain(crudo.csv.iter());
        for (texto, comando) in todos {
            let secuencia = parsear_atajo(texto)
                .with_context(|| format!("atajo inválido '{texto}' -> '{comando}'"))?;
            atajos.insert(secuencia, comando.clone());
        }
        Ok(Self { atajos })
    }

    /// Busca el comando exacto para una secuencia de combinaciones.
    pub fn buscar(&self, secuencia: &[Combinacion]) -> Option<&str> {
        self.atajos.get(secuencia).map(String::as_str)
    }

    /// `true` si `secuencia` es un prefijo estricto de algún atajo más
    /// largo (es decir, hace falta seguir esperando teclas de un chord).
    pub fn es_prefijo(&self, secuencia: &[Combinacion]) -> bool {
        self.atajos
            .keys()
            .any(|clave| clave.len() > secuencia.len() && clave.starts_with(secuencia))
    }

    pub fn num_atajos(&self) -> usize {
        self.atajos.len()
    }

    /// Todas las secuencias de teclas asignadas a `comando` (normalmente
    /// una sola, pero nada impide tener más de un atajo para el mismo
    /// comando). Se usa para mostrar el atajo junto al comando en la
    /// paleta de comandos (`Ctrl+Shift+P`, M2).
    pub fn atajos_para(&self, comando: &str) -> Vec<&[Combinacion]> {
        self.atajos
            .iter()
            .filter(|(_, c)| c.as_str() == comando)
            .map(|(secuencia, _)| secuencia.as_slice())
            .collect()
    }

    /// Todas las entradas (secuencia, comando) del keymap, en ningún
    /// orden en particular — base de `guardar` y de `rebindear`, que
    /// necesitan reconstruir el mapa completo aunque solo cambie una
    /// entrada.
    pub fn entradas(&self) -> Vec<(Vec<Combinacion>, String)> {
        self.atajos.iter().map(|(s, c)| (s.clone(), c.clone())).collect()
    }

    /// Reconstruye un `Keymap` a partir de una lista de entradas — inverso
    /// de `entradas`. Si dos entradas comparten la misma secuencia, gana
    /// la última (mismo comportamiento que `HashMap::insert` repetido);
    /// no debería pasar en la práctica porque `rebindear` y
    /// `restablecer_comando` ya filtran duplicados antes de llamar acá.
    fn desde_entradas(entradas: Vec<(Vec<Combinacion>, String)>) -> Self {
        Self { atajos: entradas.into_iter().collect() }
    }

    /// Reemplaza TODAS las combinaciones de `comando` por una única
    /// `nueva` (PLAN.md §5: "seleccionás una fila, pulsás Enter, presionás
    /// la nueva combinación y se guarda" — un solo atajo, no un chord ni
    /// varias alternativas a la vez; si el comando tenía más de un atajo
    /// por defecto, como `paleta.comandos`, personalizarlo desde el panel
    /// los colapsa a este único nuevo). Si `nueva` ya estaba asignada a
    /// OTRO comando distinto, no se aplica el cambio — devuelve el nombre
    /// de ese otro comando para que quien llama pueda avisar en vez de
    /// robarle su atajo en silencio.
    pub fn rebindear(&self, comando: &str, nueva: Combinacion) -> Result<Keymap, String> {
        if let Some(otro) = self.atajos.get(&vec![nueva]) {
            if otro != comando {
                return Err(otro.clone());
            }
        }
        let mut entradas: Vec<(Vec<Combinacion>, String)> =
            self.entradas().into_iter().filter(|(_, c)| c != comando).collect();
        entradas.push((vec![nueva], comando.to_string()));
        Ok(Self::desde_entradas(entradas))
    }

    /// Descarta la personalización de `comando` (si tenía alguna) y le
    /// devuelve exactamente los atajos que tiene en el keymap por defecto
    /// (puede ser más de uno, como `paleta.comandos` con `Ctrl+Shift+P` y
    /// `F1`) — "Restablecer valor por defecto" por atajo, PLAN.md §5.
    pub fn restablecer_comando(&self, comando: &str) -> Keymap {
        let mut entradas: Vec<(Vec<Combinacion>, String)> =
            self.entradas().into_iter().filter(|(_, c)| c != comando).collect();
        for secuencia in keymap_por_defecto().atajos_para(comando) {
            entradas.push((secuencia.to_vec(), comando.to_string()));
        }
        Self::desde_entradas(entradas)
    }

    /// Serializa el keymap completo a un único bloque `[global]` de TOML
    /// (las secciones `[editor]`/`[markdown]`/`[csv]` de
    /// `runtime/keymaps/default.toml` son solo organización dentro del
    /// archivo — ya se combinan en un único mapa al cargar, así que no
    /// hace falta reconstruirlas para guardar).
    fn a_texto_toml(&self) -> Result<String> {
        let mut global = HashMap::new();
        for (secuencia, comando) in self.entradas() {
            global.insert(formatear_atajo(&secuencia), comando);
        }
        let crudo = KeymapCrudo { global, ..Default::default() };
        toml::to_string_pretty(&crudo).context("no se pudo serializar el keymap")
    }

    /// Persiste el keymap completo en `~/.config/tcode/keymap.toml` (o el
    /// directorio portable en Windows) — sobreescribe cualquier
    /// personalización previa del usuario, porque ya la incluye (viene de
    /// `entradas()`, que parte del keymap activo completo).
    pub fn guardar(&self) -> Result<()> {
        let texto = self.a_texto_toml()?;
        let ruta = ruta_keymap_usuario();
        if let Some(dir) = ruta.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("no se pudo crear '{}'", dir.display()))?;
        }
        std::fs::write(&ruta, texto).with_context(|| format!("no se pudo escribir '{}'", ruta.display()))
    }

    /// "Exportar keymap" (PLAN.md §5.1): escribe el keymap completo en
    /// una ruta fija y predecible (mismo directorio que `keymap.toml`,
    /// nombre distinto) — no hay selector de ruta en esta TUI, así que
    /// en vez de eso se documenta dónde queda el archivo para que se
    /// pueda mover/compartir a mano. Devuelve la ruta donde quedó.
    pub fn exportar(&self) -> Result<PathBuf> {
        let texto = self.a_texto_toml()?;
        let ruta = ruta_keymap_exportado();
        if let Some(dir) = ruta.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("no se pudo crear '{}'", dir.display()))?;
        }
        std::fs::write(&ruta, &texto).with_context(|| format!("no se pudo escribir '{}'", ruta.display()))?;
        Ok(ruta)
    }
}

/// "Restablecer TODOS los atajos por defecto" (PLAN.md §5): borra el
/// `keymap.toml` de usuario si existe, para que la próxima `cargar()`
/// vuelva a caer en el embebido. No hace falta reescribirlo con los
/// valores por defecto — simplemente no tener archivo de usuario ya
/// significa "usar el embebido" (ver `cargar`).
pub fn eliminar_override_usuario() -> Result<()> {
    let ruta = ruta_keymap_usuario();
    if ruta.exists() {
        std::fs::remove_file(&ruta).with_context(|| format!("no se pudo borrar '{}'", ruta.display()))?;
    }
    Ok(())
}

const KEYMAP_POR_DEFECTO: &str = include_str!("../../../runtime/keymaps/default.toml");

/// Carga el keymap por defecto embebido en el binario.
pub fn keymap_por_defecto() -> Keymap {
    parsear_keymap(KEYMAP_POR_DEFECTO).expect("el keymap embebido por defecto debe ser válido")
}

fn parsear_keymap(texto: &str) -> Result<Keymap> {
    let crudo: KeymapCrudo = toml::from_str(texto)?;
    Keymap::desde_crudo(crudo)
}

pub fn ruta_keymap_usuario() -> PathBuf {
    directorio_config().join("keymap.toml")
}

/// Ruta fija de "Exportar keymap" (`Keymap::exportar`, PLAN.md §5.1).
pub fn ruta_keymap_exportado() -> PathBuf {
    directorio_config().join("keymap-exportado.toml")
}

/// Ruta fija de "Importar keymap" (`importar_keymap`, PLAN.md §5.1):
/// mismo espíritu que importar un tema (PLAN.md §7 — dejar el archivo
/// en una carpeta conocida en vez de necesitar un selector de ruta),
/// pero con un nombre de archivo distinto al de exportar para no
/// confundir "el que yo generé" con "el que me compartieron".
pub fn ruta_keymap_a_importar() -> PathBuf {
    directorio_config().join("keymap-importar.toml")
}

/// Carga el keymap activo: el de usuario (`~/.config/tcode/keymap.toml` o
/// equivalente) si existe, si no el embebido por defecto. Es lo que hace
/// posible editar atajos sin recompilar — el editor visual del panel de
/// administración (PLAN.md §5) llega en M4.
pub fn cargar() -> Result<Keymap> {
    let ruta = ruta_keymap_usuario();
    if !ruta.exists() {
        return Ok(keymap_por_defecto());
    }
    let texto = std::fs::read_to_string(&ruta)
        .with_context(|| format!("no se pudo leer '{}'", ruta.display()))?;
    parsear_keymap(&texto).with_context(|| format!("'{}' tiene TOML inválido", ruta.display()))
}

/// Resultado de `importar_keymap`: si no hay ningún archivo esperando en
/// `ruta_keymap_a_importar()`, no es un error — es el caso normal
/// mientras nadie dejó nada ahí para importar.
pub enum ResultadoImportarKeymap {
    Importado { ruta: PathBuf, keymap: Keymap },
    NoHabiaArchivo(PathBuf),
}

/// "Importar keymap" (PLAN.md §5.1): si hay un archivo en
/// `ruta_keymap_a_importar()`, lo parsea y lo adopta como el keymap
/// activo (`Keymap::guardar`, sobreescribiendo cualquier
/// personalización previa del usuario — mismo criterio que "Restablecer
/// TODOS": importar es una decisión deliberada, no algo para lo que
/// tenga sentido mezclar con lo que ya había). Quien llama es
/// responsable de refrescar el `Resolvedor` con el `Keymap` devuelto
/// para que tome efecto en caliente, igual que tras personalizar un
/// atajo desde el panel.
pub fn importar_keymap() -> Result<ResultadoImportarKeymap> {
    let ruta = ruta_keymap_a_importar();
    if !ruta.exists() {
        return Ok(ResultadoImportarKeymap::NoHabiaArchivo(ruta));
    }
    let texto = std::fs::read_to_string(&ruta).with_context(|| format!("no se pudo leer '{}'", ruta.display()))?;
    let keymap =
        parsear_keymap(&texto).with_context(|| format!("'{}' tiene TOML inválido", ruta.display()))?;
    keymap.guardar()?;
    Ok(ResultadoImportarKeymap::Importado { ruta, keymap })
}

/// Un atajo es conflictivo si su secuencia exacta es también el prefijo de
/// otro atajo más largo: el resolvedor siempre dispara el más corto de
/// inmediato, dejando el más largo inalcanzable. Base para el detector de
/// conflictos en tiempo real del editor de atajos (PLAN.md §5, UI en M4).
pub struct Conflicto {
    pub atajo_corto: Vec<Combinacion>,
    pub comando_corto: String,
    pub atajo_bloqueado: Vec<Combinacion>,
    pub comando_bloqueado: String,
}

pub fn detectar_conflictos(keymap: &Keymap) -> Vec<Conflicto> {
    let mut conflictos = Vec::new();
    for (secuencia, comando) in &keymap.atajos {
        for (otra, otro_comando) in &keymap.atajos {
            if otra.len() > secuencia.len() && otra.starts_with(secuencia.as_slice()) {
                conflictos.push(Conflicto {
                    atajo_corto: secuencia.clone(),
                    comando_corto: comando.clone(),
                    atajo_bloqueado: otra.clone(),
                    comando_bloqueado: otro_comando.clone(),
                });
            }
        }
    }
    conflictos
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combinacion::parsear_combinacion;

    #[test]
    fn el_keymap_por_defecto_carga_y_no_esta_vacio() {
        let keymap = keymap_por_defecto();
        assert!(keymap.num_atajos() > 0);
        let guardar = vec![parsear_combinacion("Ctrl+S").unwrap()];
        assert_eq!(keymap.buscar(&guardar), Some("archivo.guardar"));
    }

    #[test]
    fn atajos_para_encuentra_la_secuencia_de_un_comando() {
        let keymap = keymap_por_defecto();
        let atajos = keymap.atajos_para("archivo.guardar");
        assert_eq!(atajos.len(), 1);
        assert_eq!(atajos[0], [parsear_combinacion("Ctrl+S").unwrap()]);
        assert!(keymap.atajos_para("comando.inexistente").is_empty());
    }

    #[test]
    fn el_keymap_por_defecto_no_tiene_conflictos() {
        let conflictos = detectar_conflictos(&keymap_por_defecto());
        assert!(
            conflictos.is_empty(),
            "el keymap por defecto no debería tener atajos ambiguos"
        );
    }

    #[test]
    fn detecta_un_conflicto_de_prefijo() {
        let crudo: KeymapCrudo = toml::from_str(
            r#"
            [global]
            "Ctrl+K" = "comando.a"
            "Ctrl+K Ctrl+O" = "comando.b"
            "#,
        )
        .unwrap();
        let keymap = Keymap::desde_crudo(crudo).unwrap();
        let conflictos = detectar_conflictos(&keymap);
        assert_eq!(conflictos.len(), 1);
        assert_eq!(conflictos[0].comando_corto, "comando.a");
        assert_eq!(conflictos[0].comando_bloqueado, "comando.b");
    }

    #[test]
    fn seccion_markdown_se_combina_con_los_atajos_activos() {
        let crudo: KeymapCrudo = toml::from_str(
            r#"
            [markdown]
            "Ctrl+K V" = "markdown.alternar_preview"
            "#,
        )
        .unwrap();
        let keymap = Keymap::desde_crudo(crudo).unwrap();
        assert_eq!(keymap.num_atajos(), 1);
    }

    #[test]
    fn seccion_csv_se_combina_con_los_atajos_activos() {
        let crudo: KeymapCrudo = toml::from_str(
            r#"
            [csv]
            "Ctrl+K T" = "csv.alternar_vista_tabla"
            "#,
        )
        .unwrap();
        let keymap = Keymap::desde_crudo(crudo).unwrap();
        assert_eq!(keymap.num_atajos(), 1);
    }

    #[test]
    fn rebindear_reemplaza_el_atajo_de_un_comando() {
        let keymap = keymap_por_defecto();
        let nueva = parsear_combinacion("Ctrl+Alt+G").unwrap();
        let actualizado = keymap.rebindear("archivo.guardar", nueva).unwrap();
        assert_eq!(actualizado.buscar(&[nueva]), Some("archivo.guardar"));
        // El atajo viejo (Ctrl+S) ya no apunta a nada.
        let vieja = parsear_combinacion("Ctrl+S").unwrap();
        assert_eq!(actualizado.buscar(&[vieja]), None);
    }

    #[test]
    fn rebindear_no_pisa_el_atajo_de_otro_comando() {
        let keymap = keymap_por_defecto();
        let ocupada = parsear_combinacion("Ctrl+Q").unwrap(); // app.salir
        let resultado = keymap.rebindear("archivo.guardar", ocupada);
        assert_eq!(resultado.unwrap_err(), "app.salir");
    }

    #[test]
    fn rebindear_permite_reasignar_el_mismo_atajo_al_mismo_comando() {
        let keymap = keymap_por_defecto();
        let misma = parsear_combinacion("Ctrl+S").unwrap();
        // No debe fallar por "ya ocupada" cuando la ocupa el propio
        // comando que se está reasignando.
        assert!(keymap.rebindear("archivo.guardar", misma).is_ok());
    }

    #[test]
    fn restablecer_comando_recupera_los_atajos_por_defecto() {
        let keymap = keymap_por_defecto();
        let nueva = parsear_combinacion("Ctrl+Alt+G").unwrap();
        let personalizado = keymap.rebindear("archivo.guardar", nueva).unwrap();
        assert_eq!(personalizado.buscar(&[parsear_combinacion("Ctrl+S").unwrap()]), None);

        let restablecido = personalizado.restablecer_comando("archivo.guardar");
        assert_eq!(restablecido.buscar(&[parsear_combinacion("Ctrl+S").unwrap()]), Some("archivo.guardar"));
        assert_eq!(restablecido.buscar(&[nueva]), None);
    }

    #[test]
    fn restablecer_comando_recupera_varios_atajos_por_defecto() {
        // "paleta.comandos" tiene dos atajos por defecto (Ctrl+Shift+P y
        // F1) — restablecerlo después de personalizarlo debe devolver
        // los dos, no solo uno.
        let keymap = keymap_por_defecto();
        let nueva = parsear_combinacion("Ctrl+Alt+P").unwrap();
        let personalizado = keymap.rebindear("paleta.comandos", nueva).unwrap();
        let restablecido = personalizado.restablecer_comando("paleta.comandos");
        assert_eq!(restablecido.atajos_para("paleta.comandos").len(), 2);
    }

    #[test]
    fn a_texto_toml_produce_un_keymap_toml_valido_que_vuelve_a_parsear_igual() {
        let keymap = keymap_por_defecto();
        let texto = keymap.a_texto_toml().unwrap();
        let recargado = parsear_keymap(&texto).unwrap();
        assert_eq!(recargado.num_atajos(), keymap.num_atajos());
        assert_eq!(recargado.buscar(&[parsear_combinacion("Ctrl+S").unwrap()]), Some("archivo.guardar"));
    }
}
