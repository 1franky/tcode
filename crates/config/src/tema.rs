use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::color::analizar_color_hex;
use crate::config::directorio_temas_usuario;

/// Un color de sintaxis puede definirse como un string simple
/// (`string = "#a6e3a1"`) o como una tabla con estilo adicional
/// (`keyword = { fg = "#cba6f7", style = "bold" }`), tal como describe
/// PLAN.md §7. `Serialize` (además de `Deserialize`) porque el editor
/// visual de tema (`Ctrl+K Ctrl+P`, M4) necesita volver a escribir el
/// `Tema` completo a disco tras cada cambio de color.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum EstiloToken {
    Color(String),
    ConEstilo {
        fg: String,
        #[serde(default)]
        style: Option<String>,
    },
}

impl EstiloToken {
    pub fn color_rgb(&self) -> Result<(u8, u8, u8)> {
        let hex = match self {
            EstiloToken::Color(c) => c,
            EstiloToken::ConEstilo { fg, .. } => fg,
        };
        analizar_color_hex(hex)
    }

    pub fn negrita(&self) -> bool {
        matches!(self, EstiloToken::ConEstilo { style: Some(s), .. } if s.contains("bold"))
    }

    pub fn cursiva(&self) -> bool {
        matches!(self, EstiloToken::ConEstilo { style: Some(s), .. } if s.contains("italic"))
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TemaUi {
    pub background: String,
    pub foreground: String,
    pub cursor: String,
    pub selection: String,
    pub line_number: String,
    pub line_number_active: String,
    pub current_line: String,
}

impl TemaUi {
    pub fn background_rgb(&self) -> Result<(u8, u8, u8)> {
        analizar_color_hex(&self.background)
    }
    pub fn foreground_rgb(&self) -> Result<(u8, u8, u8)> {
        analizar_color_hex(&self.foreground)
    }
    pub fn cursor_rgb(&self) -> Result<(u8, u8, u8)> {
        analizar_color_hex(&self.cursor)
    }
    pub fn seleccion_rgb(&self) -> Result<(u8, u8, u8)> {
        analizar_color_hex(&self.selection)
    }
    pub fn linea_actual_rgb(&self) -> Result<(u8, u8, u8)> {
        analizar_color_hex(&self.current_line)
    }
    pub fn numero_linea_rgb(&self) -> Result<(u8, u8, u8)> {
        analizar_color_hex(&self.line_number)
    }
    pub fn numero_linea_activa_rgb(&self) -> Result<(u8, u8, u8)> {
        analizar_color_hex(&self.line_number_active)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TemaStatusbar {
    pub background: String,
    pub foreground: String,
    #[serde(default)]
    pub modo_insertar: Option<String>,
    #[serde(default)]
    pub modo_seleccion: Option<String>,
}

impl TemaStatusbar {
    pub fn background_rgb(&self) -> Result<(u8, u8, u8)> {
        analizar_color_hex(&self.background)
    }
    pub fn foreground_rgb(&self) -> Result<(u8, u8, u8)> {
        analizar_color_hex(&self.foreground)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct TemaSintaxis {
    pub keyword: Option<EstiloToken>,
    pub string: Option<EstiloToken>,
    pub number: Option<EstiloToken>,
    pub comment: Option<EstiloToken>,
    pub function: Option<EstiloToken>,
    #[serde(rename = "type")]
    pub tipo: Option<EstiloToken>,
    pub variable: Option<EstiloToken>,
    pub constant: Option<EstiloToken>,
    pub operator: Option<EstiloToken>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct TemaDiagnosticos {
    pub error: Option<String>,
    pub warning: Option<String>,
    pub info: Option<String>,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct TemaGit {
    pub added: Option<String>,
    pub modified: Option<String>,
    pub deleted: Option<String>,
}

/// Colores de la barra de búsqueda/reemplazo (`Ctrl+F`/`Ctrl+H`, PLAN.md
/// §4): el fondo de la coincidencia sobre la que está el cursor de
/// búsqueda, y el de las demás coincidencias visibles en el buffer.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct TemaBusqueda {
    pub coincidencia_actual: Option<String>,
    pub otras_coincidencias: Option<String>,
}

/// Un tema completo, tal como se define en `runtime/themes/*.toml`
/// (PLAN.md §7). `git` todavía no se usa (la rama git en la statusbar es
/// una feature que no existe todavía, PLAN.md §5.5); se parsea desde ya
/// para no tener que cambiar el formato del archivo cuando se conecte.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Tema {
    pub name: String,
    #[serde(rename = "type")]
    pub tipo: String,
    /// Eje independiente de `type` (PLAN.md §7: "Filtro claro/oscuro/alto
    /// contraste" — los tres son filtros distintos, no un tercer valor de
    /// `type`): un tema de alto contraste sigue siendo "dark" o "light"
    /// para ese otro filtro, además de "alto contraste" para este.
    /// `false` por defecto — ningún tema existente antes de esta pieza lo
    /// declara, y no habría forma de distinguir "no es de alto contraste"
    /// de "todavía no se actualizó el archivo" sin un default explícito.
    #[serde(default)]
    pub alto_contraste: bool,
    pub ui: TemaUi,
    pub statusbar: TemaStatusbar,
    #[serde(default)]
    pub syntax: TemaSintaxis,
    #[serde(default)]
    pub diagnostics: TemaDiagnosticos,
    #[serde(default)]
    pub git: TemaGit,
    #[serde(default)]
    pub search: TemaBusqueda,
}

const TEMA_DRACULA: &str = include_str!("../../../runtime/themes/dracula.toml");
const TEMA_OSCURO: &str = include_str!("../../../runtime/themes/oscuro.toml");
const TEMA_CLARO: &str = include_str!("../../../runtime/themes/claro.toml");
const TEMA_MONOKAI: &str = include_str!("../../../runtime/themes/monokai.toml");
const TEMA_ONE_DARK: &str = include_str!("../../../runtime/themes/one-dark.toml");
const TEMA_NORD: &str = include_str!("../../../runtime/themes/nord.toml");
const TEMA_GRUVBOX_DARK: &str = include_str!("../../../runtime/themes/gruvbox-dark.toml");
const TEMA_TOKYO_NIGHT: &str = include_str!("../../../runtime/themes/tokyo-night.toml");
const TEMA_CATPPUCCIN_MOCHA: &str = include_str!("../../../runtime/themes/catppuccin-mocha.toml");
const TEMA_SOLARIZED_DARK: &str = include_str!("../../../runtime/themes/solarized-dark.toml");
const TEMA_SOLARIZED_LIGHT: &str = include_str!("../../../runtime/themes/solarized-light.toml");
const TEMA_GITHUB_LIGHT: &str = include_str!("../../../runtime/themes/github-light.toml");
const TEMA_ALTO_CONTRASTE: &str = include_str!("../../../runtime/themes/alto-contraste.toml");

/// Nombre del tema usado si la config no especifica uno, o si el
/// especificado no se encuentra.
pub const TEMA_POR_DEFECTO: &str = "dracula";

fn tema_embebido(nombre: &str) -> Option<&'static str> {
    match nombre {
        "dracula" => Some(TEMA_DRACULA),
        "oscuro" => Some(TEMA_OSCURO),
        "claro" => Some(TEMA_CLARO),
        "monokai" => Some(TEMA_MONOKAI),
        "one-dark" => Some(TEMA_ONE_DARK),
        "nord" => Some(TEMA_NORD),
        "gruvbox-dark" => Some(TEMA_GRUVBOX_DARK),
        "tokyo-night" => Some(TEMA_TOKYO_NIGHT),
        "catppuccin-mocha" => Some(TEMA_CATPPUCCIN_MOCHA),
        "solarized-dark" => Some(TEMA_SOLARIZED_DARK),
        "solarized-light" => Some(TEMA_SOLARIZED_LIGHT),
        "github-light" => Some(TEMA_GITHUB_LIGHT),
        "alto-contraste" => Some(TEMA_ALTO_CONTRASTE),
        _ => None,
    }
}

/// Metadatos de un tema embebido para mostrar en el selector (`Ctrl+K
/// Ctrl+T`, PLAN.md §7) sin tener que parsear el TOML solo para listar
/// nombres. `tipo` es `"dark"` o `"light"`, igual que el campo `type` del
/// propio archivo; `alto_contraste` es un eje independiente (ver doc de
/// `Tema::alto_contraste`) — ambos duplicados aquí a propósito para poder
/// filtrar la lista sin cargar cada tema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InfoTema {
    pub id: &'static str,
    pub nombre: &'static str,
    pub tipo: &'static str,
    pub alto_contraste: bool,
}

/// Los 10 temas por defecto de PLAN.md §7 (Dracula primero, orden de la
/// tabla), los dos temas genéricos "oscuro"/"claro" que ya existían desde
/// M0/M1 como fallback simple, y "Alto contraste" (M5) — el tercer filtro
/// de PLAN.md §7 que quedó pendiente hasta que existiera un tema
/// realmente diseñado para eso. El resto de temas *bonus* del plan
/// (Gruvbox Light, GitHub Dark, familia Ayu) queda fuera de esta pieza.
pub const TEMAS_EMBEBIDOS: &[InfoTema] = &[
    InfoTema { id: "dracula", nombre: "Dracula", tipo: "dark", alto_contraste: false },
    InfoTema { id: "monokai", nombre: "Monokai", tipo: "dark", alto_contraste: false },
    InfoTema { id: "one-dark", nombre: "One Dark", tipo: "dark", alto_contraste: false },
    InfoTema { id: "nord", nombre: "Nord", tipo: "dark", alto_contraste: false },
    InfoTema { id: "gruvbox-dark", nombre: "Gruvbox Dark", tipo: "dark", alto_contraste: false },
    InfoTema { id: "tokyo-night", nombre: "Tokyo Night", tipo: "dark", alto_contraste: false },
    InfoTema { id: "catppuccin-mocha", nombre: "Catppuccin Mocha", tipo: "dark", alto_contraste: false },
    InfoTema { id: "solarized-dark", nombre: "Solarized Dark", tipo: "dark", alto_contraste: false },
    InfoTema { id: "solarized-light", nombre: "Solarized Light", tipo: "light", alto_contraste: false },
    InfoTema { id: "github-light", nombre: "GitHub Light", tipo: "light", alto_contraste: false },
    InfoTema { id: "oscuro", nombre: "Oscuro", tipo: "dark", alto_contraste: false },
    InfoTema { id: "claro", nombre: "Claro", tipo: "light", alto_contraste: false },
    InfoTema { id: "alto-contraste", nombre: "Alto contraste", tipo: "dark", alto_contraste: true },
];

/// Vista de un tema listable en el selector (`Ctrl+K Ctrl+T`) que no le
/// importa a quien la usa si el tema es uno embebido (`TEMAS_EMBEBIDOS`,
/// `&'static str`) o uno que el usuario dejó en su carpeta de temas
/// (`descubrir_temas_usuario`, leído de disco — necesita `String`
/// propio, no puede ser `&'static`). Separada de `InfoTema` a propósito:
/// cambiar `InfoTema` para que sea owned habría obligado a clonar los 13
/// embebidos en cada arranque sin necesidad; acá el costo de clonar sólo
/// se paga al construir la lista para el selector, que ya hace I/O
/// (`read_dir`) igual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoTemaListado {
    pub id: String,
    pub nombre: String,
    pub tipo: String,
    pub alto_contraste: bool,
}

impl From<&InfoTema> for InfoTemaListado {
    fn from(info: &InfoTema) -> Self {
        Self { id: info.id.to_string(), nombre: info.nombre.to_string(), tipo: info.tipo.to_string(), alto_contraste: info.alto_contraste }
    }
}

/// Temas que el usuario dejó como archivos `.toml` sueltos en
/// `directorio_temas_usuario()` (PLAN.md §7, "Compartir temas": "importar
/// temas desde archivo") — cualquiera que no sea ya uno de los 13
/// embebidos, ni una copia editable de uno de ellos
/// (`<id-embebido>-mio.toml`, que ya sustituye transparentemente al
/// original vía `tema_texto_crudo`: listarlo aparte sería mostrar el
/// mismo tema dos veces). Un archivo que no exista, no se pueda leer, o
/// no parsee como `Tema` válido simplemente se ignora — un tema de
/// terceros corrupto no debería poder romper el selector.
///
/// Vuelve a leer el directorio entero cada vez que se llama (sin cachear
/// nada): mismo criterio de "correctitud primero, optimizar si hace
/// falta de verdad" que ya usa el resto del proyecto (el resaltador de
/// sintaxis re-parsea el archivo completo en cada frame) — en la
/// práctica un usuario tiene a lo sumo un puñado de temas propios, así
/// que el costo de un `read_dir` + parsear cada uno es insignificante
/// frente a redibujar toda la UI de cualquier forma.
pub fn descubrir_temas_usuario() -> Vec<InfoTemaListado> {
    descubrir_temas_en(&directorio_temas_usuario())
}

/// La parte testeable de [`descubrir_temas_usuario`], separada porque
/// `directorio_temas_usuario()` siempre apunta a la carpeta REAL de
/// configuración del usuario (no hay forma de pisarla con una variable
/// de entorno) — un test no puede llamar a la función pública sin
/// arriesgarse a leer/depender del `~/.config/tcode/themes` real de
/// quien corra los tests. Recibe el directorio como parámetro para poder
/// apuntarlo a uno temporal en los tests, mismo criterio que
/// `codificar_ruta_para_uri(..., es_windows: bool)` en `crates/app/src/
/// lsp.rs` (parametrizar en vez de leer el entorno adentro de la función
/// para poder ejercitar el camino desde cualquier host).
fn descubrir_temas_en(dir: &std::path::Path) -> Vec<InfoTemaListado> {
    let Ok(entradas) = std::fs::read_dir(dir) else { return Vec::new() };

    let mut encontrados: Vec<InfoTemaListado> = entradas
        .flatten()
        .filter_map(|entrada| {
            let ruta = entrada.path();
            if ruta.extension().and_then(|e| e.to_str()) != Some("toml") {
                return None;
            }
            let id = ruta.file_stem()?.to_str()?.to_string();
            if tema_embebido(&id).is_some() {
                return None; // ya está en TEMAS_EMBEBIDOS, no lo dupliques
            }
            if let Some(base) = id.strip_suffix("-mio") {
                if tema_embebido(base).is_some() {
                    return None; // copia editable de un embebido
                }
            }
            let texto = std::fs::read_to_string(&ruta).ok()?;
            let tema: Tema = toml::from_str(&texto).ok()?;
            Some(InfoTemaListado { id, nombre: tema.name, tipo: tema.tipo, alto_contraste: tema.alto_contraste })
        })
        .collect();
    encontrados.sort_by(|a, b| a.nombre.cmp(&b.nombre));
    encontrados
}

/// El TOML crudo de un tema, sin parsear: primero busca un archivo de
/// usuario en `~/.config/tcode/themes/<nombre>.toml` (o el directorio
/// portable en Windows — ver M5), y si no existe recurre a los temas
/// embebidos en el binario. Separado de [`cargar_tema`] para poder
/// reutilizar el texto tal cual (por ejemplo al duplicar un tema para
/// editarlo, PLAN.md §7 "Compartir temas") sin tener que volver a
/// serializar la struct `Tema` ya parseada — evita que un duplicado
/// pierda comentarios o el formato original del archivo.
fn tema_texto_crudo(nombre: &str) -> Result<String> {
    let ruta_usuario = directorio_temas_usuario().join(format!("{nombre}.toml"));
    if ruta_usuario.exists() {
        return std::fs::read_to_string(&ruta_usuario)
            .with_context(|| format!("no se pudo leer el tema '{}'", ruta_usuario.display()));
    }
    if let Some(embebido) = tema_embebido(nombre) {
        return Ok(embebido.to_string());
    }
    anyhow::bail!("tema '{nombre}' no encontrado (ni de usuario ni embebido)")
}

/// Carga un tema por nombre, ya parseado. El resto de los 10+ temas de
/// PLAN.md §7 llega en M4.
pub fn cargar_tema(nombre: &str) -> Result<Tema> {
    let texto = tema_texto_crudo(nombre)?;
    toml::from_str(&texto).with_context(|| format!("el tema '{nombre}' tiene TOML inválido"))
}

/// Carga el tema por defecto. Si falla (no debería: es contenido embebido
/// validado en tests), entra en pánico — sin tema no hay UI que dibujar.
pub fn tema_por_defecto() -> Tema {
    cargar_tema(TEMA_POR_DEFECTO).expect("el tema embebido por defecto debe ser válido")
}

/// Resultado de [`duplicar_tema_para_editar`]: si ya existía una copia de
/// antes no se pisa (podría tener ediciones a mano del usuario) — se
/// informa la diferencia para que quien llama pueda mostrar un mensaje
/// distinto en cada caso.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultadoDuplicarTema {
    Creado(std::path::PathBuf),
    YaExistia(std::path::PathBuf),
}

/// Duplica el tema `nombre` a `~/.config/tcode/themes/<nombre>-mio.toml`
/// (o el directorio portable en Windows), listo para editarlo a mano sin
/// tocar el original — mismo nombre de archivo que sugiere el ejemplo de
/// PLAN.md §7 ("Duplicar tema base"). El archivo resultante, al ser TOML
/// plano, también sirve como el "exportar" de esa misma sección: se
/// puede copiar a cualquier otra instalación de `tcode` sin cambios.
/// Nunca sobreescribe una copia que ya existía.
pub fn duplicar_tema_para_editar(nombre: &str) -> Result<ResultadoDuplicarTema> {
    let destino = directorio_temas_usuario().join(format!("{nombre}-mio.toml"));
    if destino.exists() {
        return Ok(ResultadoDuplicarTema::YaExistia(destino));
    }
    let texto = tema_texto_crudo(nombre)?;
    let dir = directorio_temas_usuario();
    std::fs::create_dir_all(&dir).with_context(|| format!("no se pudo crear '{}'", dir.display()))?;
    std::fs::write(&destino, texto).with_context(|| format!("no se pudo escribir '{}'", destino.display()))?;
    Ok(ResultadoDuplicarTema::Creado(destino))
}

/// Persiste `tema` completo como TOML en `ruta` — usado por el editor
/// visual de tema (`Ctrl+K Ctrl+P`, PLAN.md §7) tras cada cambio de
/// color. A diferencia de `tema_texto_crudo`/`duplicar_tema_para_editar`
/// (que copian el archivo tal cual, preservando formato/comentarios),
/// esto SÍ reserializa desde la struct ya parseada — es justamente lo
/// que hay que hacer cuando el contenido cambió en memoria y hay que
/// bajarlo a disco.
pub fn guardar_tema(tema: &Tema, ruta: &std::path::Path) -> Result<()> {
    let texto = toml::to_string_pretty(tema).context("no se pudo serializar el tema")?;
    if let Some(dir) = ruta.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("no se pudo crear '{}'", dir.display()))?;
    }
    std::fs::write(ruta, texto).with_context(|| format!("no se pudo escribir '{}'", ruta.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todos_los_temas_embebidos_parsean_y_tienen_colores_validos() {
        for info in TEMAS_EMBEBIDOS {
            let tema = cargar_tema(info.id).unwrap_or_else(|e| panic!("tema '{}': {e}", info.id));
            assert!(!tema.name.is_empty());
            tema.ui.background_rgb().unwrap_or_else(|e| panic!("tema '{}': {e}", info.id));
            tema.ui.foreground_rgb().unwrap_or_else(|e| panic!("tema '{}': {e}", info.id));
            tema.statusbar.background_rgb().unwrap_or_else(|e| panic!("tema '{}': {e}", info.id));
            tema.statusbar.foreground_rgb().unwrap_or_else(|e| panic!("tema '{}': {e}", info.id));
            assert_eq!(tema.tipo, info.tipo, "tipo declarado en {}.toml no coincide con InfoTema", info.id);
            assert_eq!(
                tema.alto_contraste, info.alto_contraste,
                "alto_contraste declarado en {}.toml no coincide con InfoTema",
                info.id
            );
        }
    }

    #[test]
    fn solo_el_tema_alto_contraste_declara_esa_bandera() {
        let con_bandera: Vec<&str> =
            TEMAS_EMBEBIDOS.iter().filter(|t| t.alto_contraste).map(|t| t.id).collect();
        assert_eq!(con_bandera, vec!["alto-contraste"]);
    }

    #[test]
    fn los_ids_de_temas_embebidos_son_unicos_y_coinciden_con_tema_embebido() {
        let mut ids: Vec<&str> = TEMAS_EMBEBIDOS.iter().map(|t| t.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), TEMAS_EMBEBIDOS.len(), "hay ids repetidos en TEMAS_EMBEBIDOS");
        for info in TEMAS_EMBEBIDOS {
            assert!(tema_embebido(info.id).is_some(), "'{}' está en TEMAS_EMBEBIDOS pero no en tema_embebido", info.id);
        }
    }

    #[test]
    fn tema_desconocido_da_error() {
        assert!(cargar_tema("no-existe-este-tema").is_err());
    }

    #[test]
    fn estilo_token_distingue_color_simple_de_color_con_estilo() {
        #[derive(Deserialize)]
        struct Envoltorio {
            v: EstiloToken,
        }

        let simple: Envoltorio = toml::from_str("v = \"#ffffff\"").unwrap();
        assert!(!simple.v.negrita());
        assert_eq!(simple.v.color_rgb().unwrap(), (255, 255, 255));

        let con_estilo: Envoltorio =
            toml::from_str("v = { fg = \"#000000\", style = \"bold\" }").unwrap();
        assert!(con_estilo.v.negrita());
        assert_eq!(con_estilo.v.color_rgb().unwrap(), (0, 0, 0));
    }

    #[test]
    fn todos_los_temas_embebidos_sobreviven_un_round_trip_de_serializacion() {
        // El editor visual de tema (Ctrl+K Ctrl+P, M4) reserializa el
        // Tema completo tras cada cambio — confirmar que ninguno de los
        // temas embebidos (13, ver TEMAS_EMBEBIDOS) pierde información
        // al ir y volver.
        for info in TEMAS_EMBEBIDOS {
            let original = cargar_tema(info.id).unwrap();
            let texto = toml::to_string_pretty(&original).unwrap_or_else(|e| panic!("tema '{}': {e}", info.id));
            let recuperado: Tema = toml::from_str(&texto).unwrap_or_else(|e| panic!("tema '{}': {e}", info.id));
            assert_eq!(recuperado.ui.background, original.ui.background, "tema '{}'", info.id);
            assert_eq!(recuperado.syntax.keyword.is_some(), original.syntax.keyword.is_some(), "tema '{}'", info.id);
        }
    }

    #[test]
    fn guardar_tema_escribe_un_archivo_que_vuelve_a_cargar_igual() {
        let original = tema_por_defecto();
        let dir = std::env::temp_dir().join(format!("tcode-test-guardar-tema-{}", std::process::id()));
        let ruta = dir.join("prueba.toml");

        guardar_tema(&original, &ruta).unwrap();
        let texto = std::fs::read_to_string(&ruta).unwrap();
        let recuperado: Tema = toml::from_str(&texto).unwrap();
        assert_eq!(recuperado.name, original.name);
        assert_eq!(recuperado.ui.background, original.ui.background);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Directorio temporal único por test (mismo criterio que
    /// `guardar_tema_escribe_un_archivo_que_vuelve_a_cargar_igual` más
    /// arriba — el crate no tiene `tempfile` como dependencia) para los
    /// tests de `descubrir_temas_en`, que nunca deben tocar la carpeta de
    /// temas REAL del usuario que corre los tests.
    fn dir_temporal(sufijo: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("tcode-test-descubrir-temas-{sufijo}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn descubrir_temas_en_ignora_un_directorio_inexistente() {
        let dir = std::env::temp_dir().join(format!("tcode-test-no-existe-{}", std::process::id()));
        assert_eq!(descubrir_temas_en(&dir), Vec::new());
    }

    #[test]
    fn descubrir_temas_en_encuentra_un_toml_valido_con_id_nuevo() {
        let dir = dir_temporal("valido");
        std::fs::write(
            dir.join("mi-tema-lindo.toml"),
            r##"name = "Mi Tema Lindo"
type = "dark"
[ui]
background = "#000000"
foreground = "#ffffff"
cursor = "#ffffff"
selection = "#333333"
line_number = "#555555"
line_number_active = "#ffffff"
current_line = "#111111"
[statusbar]
background = "#000000"
foreground = "#ffffff"
"##,
        )
        .unwrap();

        let encontrados = descubrir_temas_en(&dir);
        assert_eq!(encontrados.len(), 1);
        assert_eq!(encontrados[0].id, "mi-tema-lindo");
        assert_eq!(encontrados[0].nombre, "Mi Tema Lindo");
        assert_eq!(encontrados[0].tipo, "dark");
        assert!(!encontrados[0].alto_contraste);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn descubrir_temas_en_ignora_un_toml_invalido_sin_romper_nada() {
        let dir = dir_temporal("invalido");
        std::fs::write(dir.join("roto.toml"), "esto no es un tema válido {{{").unwrap();

        assert_eq!(descubrir_temas_en(&dir), Vec::new());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn descubrir_temas_en_no_duplica_un_id_embebido() {
        let dir = dir_temporal("embebido-duplicado");
        // Mismo id que un embebido real ("dracula") — no debería listarse
        // aparte aunque el archivo exista y sea válido.
        std::fs::write(dir.join("dracula.toml"), tema_embebido("dracula").unwrap()).unwrap();

        assert_eq!(descubrir_temas_en(&dir), Vec::new());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn descubrir_temas_en_no_lista_una_copia_editable_de_un_embebido() {
        let dir = dir_temporal("copia-mio");
        // "dracula-mio.toml" es la copia editable de PLAN.md §7
        // ("Duplicar tema base") — ya sustituye transparentemente al
        // original, listarla aparte la mostraría dos veces.
        std::fs::write(dir.join("dracula-mio.toml"), tema_embebido("dracula").unwrap()).unwrap();

        assert_eq!(descubrir_temas_en(&dir), Vec::new());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn descubrir_temas_en_ordena_alfabeticamente_por_nombre() {
        let dir = dir_temporal("orden");
        for (archivo, nombre) in [("z.toml", "Zeta"), ("a.toml", "Alfa")] {
            std::fs::write(
                dir.join(archivo),
                format!(
                    r##"name = "{nombre}"
type = "dark"
[ui]
background = "#000000"
foreground = "#ffffff"
cursor = "#ffffff"
selection = "#333333"
line_number = "#555555"
line_number_active = "#ffffff"
current_line = "#111111"
[statusbar]
background = "#000000"
foreground = "#ffffff"
"##
                ),
            )
            .unwrap();
        }

        let encontrados = descubrir_temas_en(&dir);
        assert_eq!(encontrados.iter().map(|t| t.nombre.as_str()).collect::<Vec<_>>(), vec!["Alfa", "Zeta"]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
