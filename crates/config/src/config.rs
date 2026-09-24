use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::tema::TEMA_POR_DEFECTO;

/// Ajustes generales del editor, persistidos en `config.toml`. Corresponde
/// a las secciones "Editor" e "Interfaz" del panel de administración
/// (PLAN.md §5) — ese panel visual llega en M4; por ahora el archivo se
/// edita a mano o se regenera con los valores por defecto.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub editor: ConfigEditor,
    pub interfaz: ConfigInterfaz,
    pub lenguajes: ConfigLenguajes,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct ConfigEditor {
    pub tamano_tabulacion: usize,
    pub usar_espacios: bool,
    pub ajuste_linea: bool,
    pub numeros_de_linea: bool,
    /// Indicadores de git en el gutter (BACKLOG.md P2 #6): marca las
    /// líneas agregadas/modificadas/borradas respecto de `HEAD`, en vivo
    /// mientras se escribe. Prendido por defecto — no cambia nada en
    /// archivos fuera de un repo o sin trackear (ni siquiera reserva la
    /// columna, ver `tcode_ui::vista_codigo`).
    pub indicadores_git: bool,
    /// Modo VIM (M5, alcance "lo esencial" — sin operadores combinables
    /// como `dw`, sin conteos numéricos, sin `:`): modos Normal/Insertar
    /// con `Esc`/`i`/`a`/`o`, movimientos `hjkl`/`0`/`$`/`gg`/`G`, y
    /// `x`/`dd`/`yy`/`p`/`u`. Apagado por defecto — no cambia el
    /// comportamiento de nadie que no lo prenda a propósito, ni acá ni en
    /// la sección "Editor" del panel de administración.
    pub modo_vim: bool,
    /// Regla vertical / guía de columna (BACKLOG.md P1 #5): marca una
    /// columna fija de la vista de código con un fondo distinto, para
    /// usarla como guía de ancho de línea (80/100/120...). `None` =
    /// apagada (default) — un solo campo en vez de un booleano +
    /// número separados porque el propio valor ya expresa "prendida en
    /// esta columna" o "apagada" sin un segundo estado que pueda quedar
    /// inconsistente (p. ej. "prendida" pero con la columna en 0).
    pub columna_regla: Option<usize>,
    /// Guardado automático (PLAN.md §5.4, BACKLOG.md P2 #4). `Nunca` por
    /// defecto — no le cambia el comportamiento a nadie que no lo prenda
    /// a propósito. Solo afecta a buffers CON ruta y modificados (un
    /// "[Sin nombre]" no tiene dónde escribirse sin preguntar).
    pub guardado_automatico: GuardadoAutomatico,
    /// Cada cuántos segundos guarda `GuardadoAutomatico::CadaNSegundos`
    /// — un campo aparte (en vez de un dato dentro de la variante) para
    /// que el TOML quede plano y legible a mano
    /// (`guardado_automatico = "cada_n_segundos"` +
    /// `segundos_guardado_automatico = 30`), y para que el número se
    /// recuerde aunque se cambie de modo y se vuelva. Ignorado en los
    /// otros dos modos.
    pub segundos_guardado_automatico: u64,
}

/// Modos de guardado automático de PLAN.md §5.4 ("nunca / al perder foco
/// / cada N segundos"). "Perder foco" = el panel/archivo activo cambia
/// (otro panel de un split, abrir otro archivo, pasar al explorador) o la
/// terminal avisa que perdió el foco (si soporta esos eventos) — lo
/// decide `app`, este crate solo guarda la elección.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardadoAutomatico {
    #[default]
    Nunca,
    AlPerderFoco,
    CadaNSegundos,
}

impl GuardadoAutomatico {
    pub const TODOS: [GuardadoAutomatico; 3] =
        [GuardadoAutomatico::Nunca, GuardadoAutomatico::AlPerderFoco, GuardadoAutomatico::CadaNSegundos];

    /// El modo siguiente (`delta` > 0) o anterior (`delta` < 0) en
    /// [`Self::TODOS`], dando la vuelta en los extremos — así `←`/`→`/
    /// `Enter` en el panel de administración recorren los tres sin
    /// quedarse trabados en una punta.
    pub fn rotar(self, delta: i32) -> Self {
        let total = Self::TODOS.len() as i32;
        let actual = Self::TODOS.iter().position(|m| *m == self).unwrap_or(0) as i32;
        Self::TODOS[(actual + delta).rem_euclid(total) as usize]
    }
}

/// Valor por defecto de `segundos_guardado_automatico` — suficientemente
/// seguido como para no perder mucho si algo se cuelga, suficientemente
/// espaciado como para no estar escribiendo a disco todo el tiempo.
pub const SEGUNDOS_GUARDADO_AUTOMATICO_POR_DEFECTO: u64 = 30;

impl Default for ConfigEditor {
    fn default() -> Self {
        Self {
            tamano_tabulacion: 4,
            usar_espacios: true,
            ajuste_linea: false,
            numeros_de_linea: true,
            indicadores_git: true,
            modo_vim: false,
            columna_regla: None,
            guardado_automatico: GuardadoAutomatico::Nunca,
            segundos_guardado_automatico: SEGUNDOS_GUARDADO_AUTOMATICO_POR_DEFECTO,
        }
    }
}

/// Corresponde a la sección "Interfaz" del panel de administración
/// (PLAN.md §5.5). Los `statusbar_*` son los elementos "marcar/
/// desmarcar" que menciona el plan — salvo la rama git, que todavía no
/// existe como feature (no tiene sentido un toggle para algo que nunca
/// se muestra); densidad de UI y breadcrumbs quedan para cuando esos
/// widgets existan.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct ConfigInterfaz {
    pub tema: String,
    pub mostrar_statusbar: bool,
    /// Barra de pestañas arriba del código de cada panel (BACKLOG.md P3
    /// #10). Prendida, se muestra también con una sola pestaña.
    pub mostrar_pestanas: bool,
    pub statusbar_posicion_cursor: bool,
    pub statusbar_codificacion: bool,
    pub statusbar_eol: bool,
    pub statusbar_lenguaje: bool,
    pub statusbar_diagnosticos: bool,
    pub statusbar_modo: bool,
}

impl Default for ConfigInterfaz {
    fn default() -> Self {
        Self {
            tema: TEMA_POR_DEFECTO.to_string(),
            mostrar_statusbar: true,
            mostrar_pestanas: true,
            statusbar_posicion_cursor: true,
            statusbar_codificacion: true,
            statusbar_eol: true,
            statusbar_lenguaje: true,
            statusbar_diagnosticos: true,
            statusbar_modo: true,
        }
    }
}

/// Comando + argumentos + variables de entorno configurados a mano para
/// el LSP de un lenguaje (PLAN.md §5.3: "Configurar comando, argumentos y
/// variables de entorno") — sobreescribe lo que `tcode_lsp::comando_para`
/// trae fijo para ese lenguaje (que puede ser nada, como todos salvo
/// Python por ahora). `env` sirve para casos reales como un LSP que
/// necesita `JAVA_HOME` propio, o acotar el `PATH` a una versión
/// distinta solo para esa sesión — `BTreeMap` en vez de `HashMap` para
/// que el orden sea determinístico al mostrarlas (`ComandoLsp::
/// como_linea`), no por necesitarlo en ningún otro lado.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct ComandoLsp {
    pub comando: String,
    pub argumentos: Vec<String>,
    pub env: BTreeMap<String, String>,
}

impl ComandoLsp {
    /// Representación como línea de texto editable — inversa de
    /// `ConfigLenguajes::fijar_comando_desde_linea`, usada para
    /// precargar el buffer de edición (`c` en "Lenguajes / LSP",
    /// `crates/app/src/main.rs`) con el comando actual, variables de
    /// entorno incluidas si tiene alguna. Sin variables de entorno da
    /// exactamente lo mismo que antes de esta pieza (comando + args,
    /// sin ningún `--` de más) — no cambia el comportamiento de nadie
    /// que no las use.
    pub fn como_linea(&self) -> String {
        let mut partes: Vec<String> = self.env.iter().map(|(k, v)| format!("{k}={v}")).collect();
        if !partes.is_empty() {
            partes.push("--".to_string());
        }
        partes.push(self.comando.clone());
        partes.extend(self.argumentos.iter().cloned());
        partes.join(" ")
    }
}

/// Corresponde a la sección "Lenguajes / LSP" del panel de
/// administración (PLAN.md §5.3).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct ConfigLenguajes {
    /// Ids de lenguaje (`tcode_syntax::Lenguaje::id`, ej. `"python"`)
    /// cuyo LSP no se lanza aunque haya uno configurado, aun si el
    /// archivo activo es de ese lenguaje.
    pub lsp_deshabilitado: Vec<String>,
    /// Comando personalizado por lenguaje — si un id no está acá, se
    /// usa el que trae `tcode_lsp::comando_para` (si alguno).
    pub lsp_comando: HashMap<String, ComandoLsp>,
    /// Ids de lenguaje con "formatear al guardar" prendido (PLAN.md §5
    /// "Editor", BACKLOG.md P2 #5): al guardar un archivo de uno de
    /// estos lenguajes se le pide `textDocument/formatting` al LSP
    /// activo antes de escribir a disco. Lista de los PRENDIDOS (no de
    /// los apagados, al revés que `lsp_deshabilitado`) porque el
    /// default es apagado para todos — reformatear el archivo de alguien
    /// sin que lo haya pedido sería un cambio de comportamiento sorpresa.
    pub formatear_al_guardar: Vec<String>,
}

impl ConfigLenguajes {
    pub fn lsp_habilitado(&self, id_lenguaje: &str) -> bool {
        !self.lsp_deshabilitado.iter().any(|l| l == id_lenguaje)
    }

    /// Habilita el LSP de `id_lenguaje` si estaba deshabilitado, o
    /// viceversa.
    pub fn alternar_lsp(&mut self, id_lenguaje: &str) {
        if let Some(pos) = self.lsp_deshabilitado.iter().position(|l| l == id_lenguaje) {
            self.lsp_deshabilitado.remove(pos);
        } else {
            self.lsp_deshabilitado.push(id_lenguaje.to_string());
        }
    }

    /// `true` si `id_lenguaje` tiene "formatear al guardar" prendido —
    /// `false` por defecto para todos (ver doc del campo).
    pub fn formatear_al_guardar(&self, id_lenguaje: &str) -> bool {
        self.formatear_al_guardar.iter().any(|l| l == id_lenguaje)
    }

    /// Prende "formatear al guardar" para `id_lenguaje` si estaba
    /// apagado, o viceversa (`f` en "Lenguajes / LSP").
    pub fn alternar_formatear_al_guardar(&mut self, id_lenguaje: &str) {
        if let Some(pos) = self.formatear_al_guardar.iter().position(|l| l == id_lenguaje) {
            self.formatear_al_guardar.remove(pos);
        } else {
            self.formatear_al_guardar.push(id_lenguaje.to_string());
        }
    }

    pub fn comando_configurado(&self, id_lenguaje: &str) -> Option<&ComandoLsp> {
        self.lsp_comando.get(id_lenguaje)
    }

    /// Guarda (o reemplaza) el comando personalizado de `id_lenguaje`.
    /// `linea` es la línea completa tal como se escribió, separada por
    /// espacios en blanco. Sintaxis extendida para variables de entorno
    /// (PLAN.md §5.3): si aparece un token `--` SOLO (no pegado a nada),
    /// todo lo que está ANTES se interpreta como pares `VAR=valor` (un
    /// token sin `=` se ignora sin romper el resto) y todo lo que sigue
    /// es comando + argumentos como siempre. Sin ningún `--`, la línea
    /// entera es comando + argumentos — igual que antes de esta sintaxis,
    /// para no cambiarle el comportamiento a nadie que no use variables
    /// de entorno. `None` si no queda ningún comando para guardar (línea
    /// vacía, o solo variables de entorno sin comando después del `--`).
    pub fn fijar_comando_desde_linea(&mut self, id_lenguaje: &str, linea: &str) -> Option<()> {
        let tokens: Vec<&str> = linea.split_whitespace().collect();
        let separador = tokens.iter().position(|t| *t == "--");
        let (tokens_env, tokens_comando): (&[&str], &[&str]) = match separador {
            Some(idx) => (&tokens[..idx], &tokens[idx + 1..]),
            None => (&[], &tokens[..]),
        };

        let mut resto = tokens_comando.iter();
        let comando = resto.next()?.to_string();
        let argumentos = resto.map(|s| s.to_string()).collect();
        let env = tokens_env.iter().filter_map(|t| t.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect();

        self.lsp_comando.insert(id_lenguaje.to_string(), ComandoLsp { comando, argumentos, env });
        Some(())
    }

    /// Quita el comando personalizado de `id_lenguaje` — vuelve a usar
    /// el que trae `tcode_lsp::comando_para`, si alguno.
    pub fn quitar_comando(&mut self, id_lenguaje: &str) {
        self.lsp_comando.remove(id_lenguaje);
    }
}

/// Directorio de configuración de tcode. En M1 solo se resuelve el modo
/// "usuario" (vía el directorio de config estándar del SO); el modo
/// portable de Windows (buscar `config.toml` junto al ejecutable) es
/// trabajo de M5 (PLAN.md §10).
pub fn directorio_config() -> PathBuf {
    dirs::config_dir()
        .map(|base| base.join("tcode"))
        .unwrap_or_else(|| PathBuf::from(".tcode"))
}

pub fn directorio_temas_usuario() -> PathBuf {
    directorio_config().join("themes")
}

pub fn ruta_config() -> PathBuf {
    directorio_config().join("config.toml")
}

/// Carga `config.toml`. Si todavía no existe, crea uno con los valores por
/// defecto (best-effort: si el sistema de archivos no lo permite, sigue
/// igual con los valores en memoria) y lo devuelve.
pub fn cargar() -> Result<Config> {
    let ruta = ruta_config();
    if !ruta.exists() {
        let config = Config::default();
        let _ = guardar(&config);
        return Ok(config);
    }
    let texto = std::fs::read_to_string(&ruta)
        .with_context(|| format!("no se pudo leer '{}'", ruta.display()))?;
    toml::from_str(&texto).with_context(|| format!("'{}' tiene TOML inválido", ruta.display()))
}

/// Guarda `config` como la config GLOBAL del usuario (`ruta_config()`).
/// Con una `.tcode/config.toml` de proyecto activa (BACKLOG.md P2 #8),
/// lo que se pasa acá tiene que ser la config global sin mezclar — nunca
/// la efectiva (`ConfigProyecto::aplicar_sobre`), o los valores del
/// proyecto terminarían copiados a la config de todos los demás
/// proyectos del usuario.
pub fn guardar(config: &Config) -> Result<()> {
    guardar_en(config, &ruta_config())
}

/// Igual que [`guardar`] pero en una ruta arbitraria — separado para que
/// los tests puedan ejercitar el mismo camino de escritura sin tocar la
/// config real del usuario.
pub fn guardar_en(config: &Config, ruta: &Path) -> Result<()> {
    if let Some(dir) = ruta.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("no se pudo crear '{}'", dir.display()))?;
    }
    let texto = toml::to_string_pretty(config).context("no se pudo serializar la configuración")?;
    std::fs::write(ruta, texto).with_context(|| format!("no se pudo escribir '{}'", ruta.display()))
}

/// Recarga la config desde disco reemplazando `actual` en el sitio — esto
/// es lo que permite aplicar cambios sin reiniciar el editor
/// (`config.recargar`, PLAN.md §4). No decide qué hacer si cambió el tema;
/// quien llama compara `actual.interfaz.tema` antes/después y actúa.
pub fn recargar(actual: &mut Config) -> Result<()> {
    *actual = cargar()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_por_defecto_tiene_valores_razonables() {
        let config = Config::default();
        assert_eq!(config.editor.tamano_tabulacion, 4);
        assert!(config.editor.usar_espacios);
        assert!(config.editor.numeros_de_linea);
        assert_eq!(config.interfaz.tema, TEMA_POR_DEFECTO);
    }

    #[test]
    fn round_trip_serializar_y_parsear() {
        let original = Config {
            editor: ConfigEditor {
                tamano_tabulacion: 2,
                usar_espacios: false,
                ajuste_linea: true,
                numeros_de_linea: false,
                indicadores_git: false,
                modo_vim: true,
                columna_regla: Some(80),
                guardado_automatico: GuardadoAutomatico::CadaNSegundos,
                segundos_guardado_automatico: 10,
            },
            interfaz: ConfigInterfaz {
                tema: "claro".into(),
                mostrar_statusbar: false,
                mostrar_pestanas: false,
                statusbar_posicion_cursor: false,
                statusbar_codificacion: true,
                statusbar_eol: false,
                statusbar_lenguaje: true,
                statusbar_diagnosticos: false,
                statusbar_modo: true,
            },
            lenguajes: ConfigLenguajes {
                lsp_deshabilitado: vec!["python".to_string()],
                lsp_comando: HashMap::from([(
                    "rust".to_string(),
                    ComandoLsp {
                        comando: "rust-analyzer".to_string(),
                        argumentos: vec![],
                        env: BTreeMap::from([("RUST_LOG".to_string(), "debug".to_string())]),
                    },
                )]),
                formatear_al_guardar: vec!["rust".to_string()],
            },
        };
        let texto = toml::to_string_pretty(&original).unwrap();
        let recuperado: Config = toml::from_str(&texto).unwrap();
        assert_eq!(recuperado, original);
    }

    #[test]
    fn toml_parcial_completa_con_valores_por_defecto() {
        // Un config.toml que solo fija el tema debe heredar el resto de
        // los valores por defecto gracias a #[serde(default)].
        let config: Config = toml::from_str("[interfaz]\ntema = \"claro\"\n").unwrap();
        assert_eq!(config.interfaz.tema, "claro");
        assert_eq!(config.editor.tamano_tabulacion, 4);
    }

    #[test]
    fn guardado_automatico_arranca_en_nunca() {
        let config = Config::default();
        assert_eq!(config.editor.guardado_automatico, GuardadoAutomatico::Nunca);
        // Un config.toml viejo, anterior a este campo, tampoco lo prende.
        let viejo: Config = toml::from_str("[editor]\ntamano_tabulacion = 2\n").unwrap();
        assert_eq!(viejo.editor.guardado_automatico, GuardadoAutomatico::Nunca);
        assert_eq!(viejo.editor.segundos_guardado_automatico, SEGUNDOS_GUARDADO_AUTOMATICO_POR_DEFECTO);
    }

    #[test]
    fn guardado_automatico_se_lee_en_snake_case_desde_toml() {
        let config: Config =
            toml::from_str("[editor]\nguardado_automatico = \"al_perder_foco\"\n").unwrap();
        assert_eq!(config.editor.guardado_automatico, GuardadoAutomatico::AlPerderFoco);
        let config: Config = toml::from_str(
            "[editor]\nguardado_automatico = \"cada_n_segundos\"\nsegundos_guardado_automatico = 5\n",
        )
        .unwrap();
        assert_eq!(config.editor.guardado_automatico, GuardadoAutomatico::CadaNSegundos);
        assert_eq!(config.editor.segundos_guardado_automatico, 5);
    }

    #[test]
    fn rotar_guardado_automatico_da_la_vuelta_en_ambos_sentidos() {
        assert_eq!(GuardadoAutomatico::Nunca.rotar(1), GuardadoAutomatico::AlPerderFoco);
        assert_eq!(GuardadoAutomatico::CadaNSegundos.rotar(1), GuardadoAutomatico::Nunca);
        assert_eq!(GuardadoAutomatico::Nunca.rotar(-1), GuardadoAutomatico::CadaNSegundos);
    }

    #[test]
    fn todos_los_lenguajes_empiezan_habilitados() {
        let lenguajes = ConfigLenguajes::default();
        assert!(lenguajes.lsp_habilitado("python"));
    }

    #[test]
    fn alternar_lsp_deshabilita_y_vuelve_a_habilitar() {
        let mut lenguajes = ConfigLenguajes::default();
        lenguajes.alternar_lsp("python");
        assert!(!lenguajes.lsp_habilitado("python"));
        assert!(lenguajes.lsp_habilitado("rust")); // no afecta a otros lenguajes

        lenguajes.alternar_lsp("python");
        assert!(lenguajes.lsp_habilitado("python"));
    }

    #[test]
    fn formatear_al_guardar_arranca_apagado_para_todos() {
        let lenguajes = ConfigLenguajes::default();
        assert!(!lenguajes.formatear_al_guardar("rust"));
        assert!(!lenguajes.formatear_al_guardar("python"));
    }

    #[test]
    fn alternar_formatear_al_guardar_prende_y_apaga_solo_ese_lenguaje() {
        let mut lenguajes = ConfigLenguajes::default();
        lenguajes.alternar_formatear_al_guardar("rust");
        assert!(lenguajes.formatear_al_guardar("rust"));
        assert!(!lenguajes.formatear_al_guardar("python"));

        lenguajes.alternar_formatear_al_guardar("rust");
        assert!(!lenguajes.formatear_al_guardar("rust"));
    }

    #[test]
    fn config_vieja_sin_formatear_al_guardar_sigue_parseando() {
        // Un config.toml escrito antes de esta opción no la tiene: tiene
        // que parsear igual, con todo apagado (`#[serde(default)]`).
        let config: Config = toml::from_str("[lenguajes]\nlsp_deshabilitado = [\"python\"]\n").unwrap();
        assert!(config.lenguajes.formatear_al_guardar.is_empty());
    }

    #[test]
    fn fijar_comando_desde_linea_separa_comando_y_argumentos() {
        let mut lenguajes = ConfigLenguajes::default();
        lenguajes.fijar_comando_desde_linea("rust", "rust-analyzer --stdio --log-file /tmp/ra.log").unwrap();

        let comando = lenguajes.comando_configurado("rust").unwrap();
        assert_eq!(comando.comando, "rust-analyzer");
        assert_eq!(comando.argumentos, vec!["--stdio", "--log-file", "/tmp/ra.log"]);
        assert!(comando.env.is_empty(), "sin '--' no debería haber ninguna variable de entorno");
    }

    #[test]
    fn fijar_comando_desde_linea_con_variables_de_entorno_antes_del_separador() {
        let mut lenguajes = ConfigLenguajes::default();
        lenguajes.fijar_comando_desde_linea("java", "JAVA_HOME=/opt/java17 NODE_ENV=dev -- jdtls -data /tmp/ws").unwrap();

        let comando = lenguajes.comando_configurado("java").unwrap();
        assert_eq!(comando.comando, "jdtls");
        assert_eq!(comando.argumentos, vec!["-data", "/tmp/ws"]);
        assert_eq!(comando.env.len(), 2);
        assert_eq!(comando.env.get("JAVA_HOME"), Some(&"/opt/java17".to_string()));
        assert_eq!(comando.env.get("NODE_ENV"), Some(&"dev".to_string()));
    }

    #[test]
    fn fijar_comando_desde_linea_ignora_tokens_sin_signo_igual_antes_del_separador() {
        let mut lenguajes = ConfigLenguajes::default();
        // "ESTOROTO" no tiene '=' — se ignora en vez de romper el resto.
        lenguajes.fijar_comando_desde_linea("go", "ESTOROTO VALIDO=si -- gopls").unwrap();

        let comando = lenguajes.comando_configurado("go").unwrap();
        assert_eq!(comando.env.len(), 1);
        assert_eq!(comando.env.get("VALIDO"), Some(&"si".to_string()));
    }

    #[test]
    fn fijar_comando_desde_linea_solo_con_separador_y_sin_comando_no_guarda_nada() {
        let mut lenguajes = ConfigLenguajes::default();
        assert!(lenguajes.fijar_comando_desde_linea("go", "VAR=valor --").is_none());
        assert!(lenguajes.comando_configurado("go").is_none());
    }

    #[test]
    fn como_linea_sin_variables_de_entorno_es_igual_que_antes_de_esta_sintaxis() {
        let comando = ComandoLsp { comando: "gopls".to_string(), argumentos: vec!["-vv".to_string()], env: BTreeMap::new() };
        assert_eq!(comando.como_linea(), "gopls -vv");
    }

    #[test]
    fn como_linea_con_variables_de_entorno_antepone_el_separador() {
        let comando = ComandoLsp {
            comando: "jdtls".to_string(),
            argumentos: vec!["-data".to_string(), "/tmp/ws".to_string()],
            env: BTreeMap::from([("JAVA_HOME".to_string(), "/opt/java17".to_string())]),
        };
        assert_eq!(comando.como_linea(), "JAVA_HOME=/opt/java17 -- jdtls -data /tmp/ws");
    }

    #[test]
    fn como_linea_es_la_inversa_de_fijar_comando_desde_linea() {
        let mut lenguajes = ConfigLenguajes::default();
        let original = "JAVA_HOME=/opt/java17 NODE_ENV=dev -- jdtls -data /tmp/ws";
        lenguajes.fijar_comando_desde_linea("java", original).unwrap();
        let comando = lenguajes.comando_configurado("java").unwrap();
        assert_eq!(comando.como_linea(), original);
    }

    #[test]
    fn fijar_comando_desde_linea_vacia_no_guarda_nada() {
        let mut lenguajes = ConfigLenguajes::default();
        assert!(lenguajes.fijar_comando_desde_linea("rust", "   ").is_none());
        assert!(lenguajes.comando_configurado("rust").is_none());
    }

    #[test]
    fn quitar_comando_borra_solo_ese_lenguaje() {
        let mut lenguajes = ConfigLenguajes::default();
        lenguajes.fijar_comando_desde_linea("rust", "rust-analyzer").unwrap();
        lenguajes.fijar_comando_desde_linea("go", "gopls").unwrap();

        lenguajes.quitar_comando("rust");
        assert!(lenguajes.comando_configurado("rust").is_none());
        assert!(lenguajes.comando_configurado("go").is_some());
    }
}
