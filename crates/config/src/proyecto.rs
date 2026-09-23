//! Config por proyecto (`.tcode/config.toml`, BACKLOG.md P2 #8 /
//! PLAN.md §12 decisión abierta #5: "config global vs. per-project: sí a
//! ambas, con override").
//!
//! La config de proyecto es un TOML PARCIAL: solo define lo que quiere
//! pisar de la global del usuario. Se mezcla a nivel `toml::Table` ANTES
//! de deserializar a [`Config`] (ver [`mezclar_toml`]) en vez de campo
//! por campo sobre el struct — así cualquier campo nuevo que se agregue a
//! `Config` queda soportado automáticamente, sin tener que acordarse de
//! tocar este módulo.
//!
//! La config global sigue siendo la única que se edita/guarda desde el
//! panel de administración: la efectiva (global + proyecto) se usa para
//! todo lo demás, pero nunca se escribe a disco (ver `guardar`).

use std::path::{Path, PathBuf};

use crate::config::{ruta_config, Config};

/// Nombre de la carpeta de proyecto (PLAN.md §12 decisión abierta #4:
/// marcador propio `.tcode/`) y del archivo dentro de ella.
const CARPETA_PROYECTO: &str = ".tcode";
const ARCHIVO_PROYECTO: &str = "config.toml";

/// Claves (`sección.clave`) que una config de PROYECTO no puede fijar:
/// las que terminan ejecutando un comando arbitrario. Abrir un repo
/// clonado de un tercero no debería lanzar un binario que ese tercero
/// eligió (un `lsp_comando` apuntando a `./instalar.sh`, o sus `env`
/// con un `LD_PRELOAD`/`PATH` propio) — la opción conservadora es
/// ignorarlas acá, sin ningún mecanismo de "confiar en este proyecto"
/// (que habría que persistir, mostrar y poder revocar: bastante más
/// alcance del que justifica esta pieza). Se avisa en la cabecera del
/// panel de administración cuáles se ignoraron. Si en el futuro se
/// agrega otra clave que lance procesos (un formateador externo por
/// lenguaje, por ejemplo), va en esta lista.
const CLAVES_SOLO_GLOBALES: &[(&str, &str)] = &[("lenguajes", "lsp_comando")];

/// Una `.tcode/config.toml` encontrada al arrancar (o en
/// `config.recargar`), ya parseada y saneada. Nunca hace fallar el
/// arranque: si el TOML es inválido (o tiene un tipo equivocado, como
/// `tamano_tabulacion = "cuatro"`), `error` queda con el motivo, `tabla`
/// vacía, y la efectiva es simplemente la global.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigProyecto {
    ruta: PathBuf,
    tabla: toml::Table,
    error: Option<String>,
    claves_ignoradas: Vec<String>,
    claves_pisadas: Vec<String>,
    claves_desconocidas: Vec<String>,
}

impl ConfigProyecto {
    /// Lee y sanea `ruta`. Un error de lectura cuenta igual que un TOML
    /// inválido: queda registrado en `error`, sin propagarse.
    pub fn cargar(ruta: &Path) -> Self {
        match std::fs::read_to_string(ruta) {
            Ok(texto) => Self::desde_texto(ruta, &texto),
            Err(e) => Self::con_error(ruta, format!("no se pudo leer: {e}")),
        }
    }

    /// Igual que [`ConfigProyecto::cargar`] pero con el contenido ya en
    /// memoria (lo usan los tests; `ruta` solo se guarda para mostrarla).
    pub fn desde_texto(ruta: &Path, texto: &str) -> Self {
        let mut tabla: toml::Table = match toml::from_str(texto) {
            Ok(tabla) => tabla,
            // `message()` de toml puede traer varias líneas ("invalid table
            // header\nexpected ..."): se aplana para la cabecera del panel.
            Err(e) => return Self::con_error(ruta, format!("TOML inválido: {}", e.message().replace('\n', " — "))),
        };

        let mut claves_ignoradas = Vec::new();
        for (seccion, clave) in CLAVES_SOLO_GLOBALES {
            if let Some(toml::Value::Table(t)) = tabla.get_mut(*seccion) {
                if t.remove(*clave).is_some() {
                    claves_ignoradas.push(format!("{seccion}.{clave}"));
                }
            }
        }

        // Validar los TIPOS ya al cargar (no solo la sintaxis), mezclando
        // sobre los valores por defecto: si esto falla, fallaría igual
        // sobre cualquier global (la mezcla deja en cada clave el valor
        // del proyecto, con su tipo), así que es el momento de avisar y
        // descartar el proyecto entero en vez de fallar más adelante.
        let efectiva_por_defecto = match mezclar_sobre(&Config::default(), &tabla) {
            Ok(config) => config,
            Err(e) => return Self::con_error(ruta, format!("valor inválido: {}", e.message())),
        };

        // Claves "pisadas" = hojas del TOML del proyecto que sobreviven a
        // un ida y vuelta por `Config`; las que no sobreviven son claves
        // que `Config` no conoce (un typo, o algo de otra versión de
        // tcode) — serde las ignora sin error, pero vale la pena
        // avisarlas en el panel en vez de que no hagan nada en silencio.
        let conocidas = toml::Table::try_from(&efectiva_por_defecto).unwrap_or_default();
        let mut claves_pisadas = Vec::new();
        let mut claves_desconocidas = Vec::new();
        for ruta_clave in hojas(&tabla, "") {
            if existe_ruta(&conocidas, &ruta_clave) {
                claves_pisadas.push(ruta_clave);
            } else {
                claves_desconocidas.push(ruta_clave);
            }
        }

        Self { ruta: ruta.to_path_buf(), tabla, error: None, claves_ignoradas, claves_pisadas, claves_desconocidas }
    }

    fn con_error(ruta: &Path, error: String) -> Self {
        Self {
            ruta: ruta.to_path_buf(),
            tabla: toml::Table::new(),
            error: Some(error),
            claves_ignoradas: Vec::new(),
            claves_pisadas: Vec::new(),
            claves_desconocidas: Vec::new(),
        }
    }

    pub fn ruta(&self) -> &Path {
        &self.ruta
    }

    /// Motivo por el que la config de proyecto se descartó entera (TOML
    /// inválido, tipo equivocado, no se pudo leer), si alguno.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Claves que el proyecto intentó fijar pero se ignoraron por
    /// seguridad (ver `CLAVES_SOLO_GLOBALES`), como `"sección.clave"`.
    pub fn claves_ignoradas(&self) -> &[String] {
        &self.claves_ignoradas
    }

    /// Claves (`"sección.clave"`) que el proyecto efectivamente pisa.
    pub fn claves_pisadas(&self) -> &[String] {
        &self.claves_pisadas
    }

    /// Claves del TOML del proyecto que `Config` no conoce (se ignoran).
    pub fn claves_desconocidas(&self) -> &[String] {
        &self.claves_desconocidas
    }

    /// Si el proyecto pisa `seccion.clave` — lo usa el panel de
    /// administración para marcar la fila (ver `CampoEditor::clave_toml`).
    pub fn pisa(&self, seccion: &str, clave: &str) -> bool {
        self.tabla.get(seccion).and_then(toml::Value::as_table).is_some_and(|t| t.contains_key(clave))
    }

    /// Config efectiva: `global` con este proyecto mezclado encima. Nunca
    /// modifica `global` (que es lo único que se guarda a disco). Si la
    /// mezcla fallara igual (no debería: los tipos ya se validaron al
    /// cargar), se queda con la global tal cual.
    ///
    /// Excepción a "el proyecto reemplaza los arrays": `lenguajes.
    /// lsp_deshabilitado` se UNE con el de la global — un proyecto puede
    /// apagar el LSP de un lenguaje, pero no volver a prender uno que el
    /// usuario apagó a propósito en su config global (misma idea que
    /// `CLAVES_SOLO_GLOBALES`: un repo ajeno no debería poder hacer que
    /// se lance un proceso que el usuario decidió no lanzar).
    pub fn aplicar_sobre(&self, global: &Config) -> Config {
        let mut efectiva = mezclar_sobre(global, &self.tabla).unwrap_or_else(|_| global.clone());
        for id in &global.lenguajes.lsp_deshabilitado {
            if !efectiva.lenguajes.lsp_deshabilitado.contains(id) {
                efectiva.lenguajes.lsp_deshabilitado.push(id.clone());
            }
        }
        efectiva
    }
}

/// Mezcla `encima` sobre `base`, clave por clave: si ambos lados tienen
/// una TABLA en la misma clave se mezclan recursivamente (así un
/// `[editor]` del proyecto con una sola clave no borra el resto de
/// `[editor]` de la global, y lo mismo con tablas por lenguaje como
/// `lenguajes.lsp_comando.<id>`); cualquier otro valor (escalar, array,
/// o una tabla que pisa algo que no era tabla) reemplaza entero al de
/// `base`. Los arrays reemplazan en vez de concatenarse porque es lo
/// único que permite a un proyecto "sacar" elementos de una lista.
pub fn mezclar_toml(base: &mut toml::Table, encima: &toml::Table) {
    for (clave, valor) in encima {
        match (base.get_mut(clave), valor) {
            (Some(toml::Value::Table(b)), toml::Value::Table(e)) => mezclar_toml(b, e),
            _ => {
                base.insert(clave.clone(), valor.clone());
            }
        }
    }
}

/// `global` serializada a TOML, con `proyecto` mezclado encima
/// ([`mezclar_toml`]) y vuelta a deserializar a [`Config`].
fn mezclar_sobre(global: &Config, proyecto: &toml::Table) -> Result<Config, toml::de::Error> {
    // Serializar un `Config` a tabla no falla en la práctica (son todos
    // tipos simples); si pasara, se mezcla sobre una tabla vacía — los
    // `#[serde(default)]` completan igual lo que falte.
    let mut base = toml::Table::try_from(global).unwrap_or_default();
    mezclar_toml(&mut base, proyecto);
    toml::Value::Table(base).try_into()
}

/// Rutas `"a.b.c"` de todas las hojas (valores que no son tabla) de
/// `tabla`, recorriendo sub-tablas.
fn hojas(tabla: &toml::Table, prefijo: &str) -> Vec<String> {
    let mut resultado = Vec::new();
    for (clave, valor) in tabla {
        let ruta = if prefijo.is_empty() { clave.clone() } else { format!("{prefijo}.{clave}") };
        match valor {
            // Una tabla vacía (p. ej. `[lenguajes]` después de quitarle
            // `lsp_comando` por seguridad) no pisa nada.
            toml::Value::Table(sub) => resultado.extend(hojas(sub, &ruta)),
            _ => resultado.push(ruta),
        }
    }
    resultado
}

fn existe_ruta(tabla: &toml::Table, ruta: &str) -> bool {
    let mut actual = tabla;
    let mut partes = ruta.split('.').peekable();
    while let Some(parte) = partes.next() {
        match actual.get(parte) {
            Some(toml::Value::Table(sub)) if partes.peek().is_some() => actual = sub,
            Some(_) => return partes.peek().is_none(),
            None => return false,
        }
    }
    false
}

/// Directorio desde el que se busca la config de proyecto: el del
/// archivo abierto por argumento (o el propio argumento si es una
/// carpeta), o el cwd si no se abrió nada. Absoluto y canonicalizado
/// cuando se puede, para que subir con `ancestors()` no se confunda con
/// un `..` literal en el argumento.
pub fn directorio_inicio_proyecto(ruta_arg: Option<&str>) -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let dir = match ruta_arg {
        None => cwd.clone(),
        Some(ruta) => {
            let ruta = cwd.join(ruta);
            if ruta.is_dir() {
                ruta
            } else {
                ruta.parent().map(Path::to_path_buf).unwrap_or_else(|| cwd.clone())
            }
        }
    };
    std::fs::canonicalize(&dir).unwrap_or(dir)
}

/// Busca `.tcode/config.toml` subiendo desde `desde` (incluido) hasta la
/// raíz del repo git (la primera carpeta con un `.git`, que sí se revisa)
/// o la del filesystem, lo primero que aparezca — no se sale del repo
/// para no levantar la config de un proyecto "padre" que no tiene nada
/// que ver. Nunca devuelve la config global del usuario (puede coincidir
/// si `directorio_config()` cayó en su respaldo relativo `.tcode`).
pub fn buscar_config_proyecto(desde: &Path) -> Option<PathBuf> {
    buscar_excluyendo(desde, &ruta_config())
}

fn buscar_excluyendo(desde: &Path, ruta_global: &Path) -> Option<PathBuf> {
    let global = std::fs::canonicalize(ruta_global).ok();
    for dir in desde.ancestors() {
        let candidata = dir.join(CARPETA_PROYECTO).join(ARCHIVO_PROYECTO);
        if candidata.is_file() {
            let es_la_global = global.is_some() && std::fs::canonicalize(&candidata).ok() == global;
            if !es_la_global {
                return Some(candidata);
            }
        }
        if dir.join(".git").exists() {
            return None;
        }
    }
    None
}

/// Atajo de [`buscar_config_proyecto`] + [`ConfigProyecto::cargar`].
pub fn cargar_config_proyecto(desde: &Path) -> Option<ConfigProyecto> {
    buscar_config_proyecto(desde).map(|ruta| ConfigProyecto::cargar(&ruta))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{guardar_en, ComandoLsp};
    use crate::panel_admin::CampoEditor;

    fn proyecto(texto: &str) -> ConfigProyecto {
        ConfigProyecto::desde_texto(Path::new("/p/.tcode/config.toml"), texto)
    }

    #[test]
    fn mezclar_toml_mezcla_tablas_anidadas_y_reemplaza_escalares_y_arrays() {
        let mut base: toml::Table =
            toml::from_str("a = 1\nlista = [1, 2]\n[t]\nx = 1\ny = 2\n[t.sub]\nz = 3\nw = 4\n").unwrap();
        let encima: toml::Table = toml::from_str("lista = [9]\n[t]\ny = 20\n[t.sub]\nw = 40\n").unwrap();
        mezclar_toml(&mut base, &encima);
        let esperado: toml::Table =
            toml::from_str("a = 1\nlista = [9]\n[t]\nx = 1\ny = 20\n[t.sub]\nz = 3\nw = 40\n").unwrap();
        assert_eq!(base, esperado);
    }

    #[test]
    fn mezclar_toml_una_tabla_reemplaza_a_un_valor_que_no_era_tabla() {
        let mut base: toml::Table = toml::from_str("t = 1\n").unwrap();
        let encima: toml::Table = toml::from_str("[t]\nx = 1\n").unwrap();
        mezclar_toml(&mut base, &encima);
        assert_eq!(base, encima);
    }

    #[test]
    fn proyecto_parcial_pisa_solo_lo_que_define() {
        let mut global = Config::default();
        global.editor.tamano_tabulacion = 2;
        global.editor.modo_vim = true;
        global.interfaz.tema = "claro".to_string();

        let p = proyecto("[editor]\nnumeros_de_linea = false\n");
        let efectiva = p.aplicar_sobre(&global);
        assert!(!efectiva.editor.numeros_de_linea);
        // El resto de `[editor]` y las demás secciones siguen siendo las
        // de la global, no los valores por defecto.
        assert_eq!(efectiva.editor.tamano_tabulacion, 2);
        assert!(efectiva.editor.modo_vim);
        assert_eq!(efectiva.interfaz.tema, "claro");
        assert_eq!(p.claves_pisadas(), ["editor.numeros_de_linea"]);
        assert!(p.pisa("editor", "numeros_de_linea"));
        assert!(!p.pisa("editor", "tamano_tabulacion"));
    }

    #[test]
    fn proyecto_puede_fijar_un_option_que_la_global_tiene_apagado() {
        let p = proyecto("[editor]\ncolumna_regla = 100\n");
        assert_eq!(p.aplicar_sobre(&Config::default()).editor.columna_regla, Some(100));
        assert!(p.claves_desconocidas().is_empty());
    }

    #[test]
    fn claves_desconocidas_se_ignoran_sin_romper_el_resto() {
        let p = proyecto("[editor]\nnumeros_de_linea = false\ninventada = 3\n[seccion_nueva]\nx = 1\n");
        assert!(p.error().is_none());
        let efectiva = p.aplicar_sobre(&Config::default());
        assert!(!efectiva.editor.numeros_de_linea);
        assert_eq!(p.claves_desconocidas(), ["editor.inventada", "seccion_nueva.x"]);
        assert_eq!(p.claves_pisadas(), ["editor.numeros_de_linea"]);
    }

    #[test]
    fn toml_invalido_deja_la_global_tal_cual_y_registra_el_error() {
        let mut global = Config::default();
        global.editor.tamano_tabulacion = 3;
        let p = proyecto("[editor\nnumeros_de_linea = false\n");
        assert!(p.error().unwrap().starts_with("TOML inválido"));
        assert_eq!(p.aplicar_sobre(&global), global);
        assert!(p.claves_pisadas().is_empty());
    }

    #[test]
    fn tipo_equivocado_descarta_el_proyecto_entero() {
        let p = proyecto("[editor]\nnumeros_de_linea = false\ntamano_tabulacion = \"cuatro\"\n");
        assert!(p.error().unwrap().starts_with("valor inválido"));
        // Ni siquiera la clave válida se aplica: todo-o-nada ante un
        // error, para que el aviso del panel sea inequívoco.
        assert!(p.aplicar_sobre(&Config::default()).editor.numeros_de_linea);
    }

    #[test]
    fn comandos_lsp_del_proyecto_se_ignoran_por_seguridad() {
        let mut global = Config::default();
        global.lenguajes.fijar_comando_desde_linea("rust", "rust-analyzer").unwrap();
        let p = proyecto(
            "[lenguajes.lsp_comando.python]\ncomando = \"./malicioso.sh\"\n\
             [lenguajes.lsp_comando.rust]\ncomando = \"otro\"\nenv = { LD_PRELOAD = \"x.so\" }\n",
        );
        assert!(p.error().is_none());
        assert_eq!(p.claves_ignoradas(), ["lenguajes.lsp_comando"]);
        assert!(p.claves_pisadas().is_empty(), "la tabla vacía que queda no cuenta como pisada");
        let efectiva = p.aplicar_sobre(&global);
        assert!(efectiva.lenguajes.comando_configurado("python").is_none());
        assert_eq!(
            efectiva.lenguajes.comando_configurado("rust"),
            Some(&ComandoLsp { comando: "rust-analyzer".to_string(), ..Default::default() })
        );
    }

    #[test]
    fn lsp_deshabilitado_se_une_con_el_de_la_global() {
        let mut global = Config::default();
        global.lenguajes.alternar_lsp("python");
        let p = proyecto("[lenguajes]\nlsp_deshabilitado = [\"rust\"]\n");
        let efectiva = p.aplicar_sobre(&global);
        assert!(!efectiva.lenguajes.lsp_habilitado("rust"));
        // El proyecto no puede volver a prender lo que la global apagó.
        assert!(!efectiva.lenguajes.lsp_habilitado("python"));
        assert!(!proyecto("[lenguajes]\nlsp_deshabilitado = []\n").aplicar_sobre(&global).lenguajes.lsp_habilitado("python"));
    }

    #[test]
    fn guardar_la_global_tras_editar_desde_el_panel_no_filtra_valores_del_proyecto() {
        // Simula el flujo del panel de administración (`app`): la
        // efectiva se usa para dibujar el editor, pero el panel edita y
        // guarda SOLO la global.
        let dir = tempfile::tempdir().unwrap();
        let ruta_global = dir.path().join("config.toml");
        let mut global = Config::default();
        guardar_en(&global, &ruta_global).unwrap();

        let p = proyecto("[editor]\nnumeros_de_linea = false\nmodo_vim = true\n[interfaz]\ntema = \"claro\"\n");
        let efectiva = p.aplicar_sobre(&global);
        assert!(efectiva.editor.modo_vim);

        CampoEditor::TamanoTabulacion.aplicar(&mut global, 1);
        guardar_en(&global, &ruta_global).unwrap();

        let en_disco: Config = toml::from_str(&std::fs::read_to_string(&ruta_global).unwrap()).unwrap();
        assert_eq!(en_disco, global);
        assert_eq!(en_disco.editor.tamano_tabulacion, 5);
        assert!(en_disco.editor.numeros_de_linea);
        assert!(!en_disco.editor.modo_vim);
        assert_eq!(en_disco.interfaz.tema, Config::default().interfaz.tema);
        // Y la efectiva recalculada refleja el cambio de la global sin
        // perder los del proyecto.
        let efectiva = p.aplicar_sobre(&global);
        assert_eq!(efectiva.editor.tamano_tabulacion, 5);
        assert!(!efectiva.editor.numeros_de_linea);
    }

    #[test]
    fn busqueda_hacia_arriba_encuentra_la_config_mas_cercana() {
        let dir = tempfile::tempdir().unwrap();
        let raiz = dir.path();
        std::fs::create_dir_all(raiz.join("repo/.git")).unwrap();
        std::fs::create_dir_all(raiz.join("repo/.tcode")).unwrap();
        std::fs::write(raiz.join("repo/.tcode/config.toml"), "").unwrap();
        std::fs::create_dir_all(raiz.join("repo/src/profundo")).unwrap();
        let ninguna = raiz.join("no-existe.toml");

        let encontrada = buscar_excluyendo(&raiz.join("repo/src/profundo"), &ninguna).unwrap();
        assert_eq!(encontrada, raiz.join("repo/.tcode/config.toml"));

        // Una subcarpeta con su propia `.tcode/` gana sobre la del repo.
        std::fs::create_dir_all(raiz.join("repo/src/.tcode")).unwrap();
        std::fs::write(raiz.join("repo/src/.tcode/config.toml"), "").unwrap();
        let encontrada = buscar_excluyendo(&raiz.join("repo/src/profundo"), &ninguna).unwrap();
        assert_eq!(encontrada, raiz.join("repo/src/.tcode/config.toml"));
    }

    #[test]
    fn busqueda_hacia_arriba_se_detiene_en_la_raiz_git() {
        let dir = tempfile::tempdir().unwrap();
        let raiz = dir.path();
        // Una `.tcode/` por ENCIMA del repo no se levanta.
        std::fs::create_dir_all(raiz.join(".tcode")).unwrap();
        std::fs::write(raiz.join(".tcode/config.toml"), "").unwrap();
        std::fs::create_dir_all(raiz.join("repo/src")).unwrap();
        // `.git` como archivo (worktrees/submódulos) también cuenta.
        std::fs::write(raiz.join("repo/.git"), "gitdir: ../otro").unwrap();
        let ninguna = raiz.join("no-existe.toml");

        assert_eq!(buscar_excluyendo(&raiz.join("repo/src"), &ninguna), None);
        // Fuera de cualquier repo sí sube hasta encontrarla.
        std::fs::create_dir_all(raiz.join("suelto/sub")).unwrap();
        assert_eq!(buscar_excluyendo(&raiz.join("suelto/sub"), &ninguna), Some(raiz.join(".tcode/config.toml")));
    }

    #[test]
    fn busqueda_nunca_devuelve_la_config_global() {
        let dir = tempfile::tempdir().unwrap();
        let raiz = dir.path();
        std::fs::create_dir_all(raiz.join(".git")).unwrap();
        std::fs::create_dir_all(raiz.join(".tcode")).unwrap();
        std::fs::write(raiz.join(".tcode/config.toml"), "").unwrap();
        assert_eq!(buscar_excluyendo(raiz, &raiz.join(".tcode/config.toml")), None);
    }

    #[test]
    fn cargar_archivo_inexistente_registra_error_sin_fallar() {
        let p = ConfigProyecto::cargar(Path::new("/no/existe/.tcode/config.toml"));
        assert!(p.error().unwrap().starts_with("no se pudo leer"));
    }
}
