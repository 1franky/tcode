use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::nodo::Nodo;

/// Etiquetas de una sola tecla para "salto rápido" (`Ctrl+K J`): dígitos
/// primero (más cómodos para pocos archivos, como en una lista corta),
/// después letras — 36 en total. Si hay más filas visibles que letras del
/// alfabeto, las que sobran simplemente no reciben etiqueta (siguen
/// navegables con las flechas de siempre, nada se rompe). No hace falta
/// evitar ninguna tecla en particular: el modo salto captura el teclado
/// por completo mientras está activo (como la barra de búsqueda o
/// "Guardar como"), así que no compite con ningún otro atajo.
const ALFABETO_ETIQUETAS_SALTO: &str = "1234567890abcdefghijklmnopqrstuvwxyz";

/// Explorador de archivos lateral (`Ctrl+B`, PLAN.md §4/§11 M1): un árbol
/// navegable con una fila seleccionada, aplanado en el orden en que se
/// dibuja (respetando qué carpetas están expandidas).
pub struct Explorador {
    raiz: Nodo,
    seleccion: usize,
    visible: bool,
    /// "Salto rápido" (`Ctrl+K J`): mientras está activo, `panel_archivos`
    /// dibuja una etiqueta junto a cada fila (`Explorador::
    /// etiqueta_para_fila`) y la siguiente tecla que llegue se interpreta
    /// como esa etiqueta (`saltar_a_etiqueta`), no como navegación normal.
    modo_salto: bool,
}

impl Explorador {
    /// Abre el explorador con `ruta_raiz` como carpeta de proyecto. Oculto
    /// por defecto: se muestra explícitamente con `Ctrl+B`.
    pub fn nuevo(ruta_raiz: impl Into<PathBuf>) -> Result<Self> {
        Ok(Self { raiz: Nodo::raiz(ruta_raiz)?, seleccion: 0, visible: false, modo_salto: false })
    }

    /// Explorador sin contenido, para cuando `nuevo` falla (p. ej. la
    /// carpeta no se puede leer) y no se quiere hacer fallar el arranque
    /// del editor por eso.
    pub fn vacio() -> Self {
        Self { raiz: Nodo::vacio(), seleccion: 0, visible: false, modo_salto: false }
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn mostrar(&mut self) {
        self.visible = true;
    }

    /// Oculta el explorador y, de paso, sale del modo salto si estaba
    /// activo — no tendría sentido quedar "esperando una etiqueta" de un
    /// panel que ya no se ve.
    pub fn ocultar(&mut self) {
        self.visible = false;
        self.modo_salto = false;
    }

    pub fn alternar_visibilidad(&mut self) {
        if self.visible {
            self.ocultar();
        } else {
            self.mostrar();
        }
    }

    pub fn modo_salto(&self) -> bool {
        self.modo_salto
    }

    pub fn activar_modo_salto(&mut self) {
        self.modo_salto = true;
    }

    pub fn salir_modo_salto(&mut self) {
        self.modo_salto = false;
    }

    /// Etiqueta que le toca a la fila visible en `indice` (mismo orden que
    /// [`Explorador::lista_visible`]), o `None` si se acabó el alfabeto —
    /// esa fila queda sin etiqueta, pero sigue ahí, navegable como
    /// siempre. Función asociada, no depende de una instancia: la usa
    /// tanto `saltar_a_etiqueta` acá abajo como `panel_archivos` para
    /// dibujar las etiquetas.
    pub fn etiqueta_para_fila(indice: usize) -> Option<char> {
        ALFABETO_ETIQUETAS_SALTO.chars().nth(indice)
    }

    /// Resuelve `etiqueta` a la fila que le corresponde y la activa
    /// directamente (abre el archivo o expande/colapsa la carpeta, igual
    /// que [`Explorador::activar_seleccion`]) — sin tener que navegar ahí
    /// primero con las flechas. Sale del modo salto al acertar; una
    /// etiqueta que no le toca a ninguna fila visible no hace nada y lo
    /// deja activo, a la espera de una válida (mismo criterio que un dedo
    /// equivocado no debería cancelar todo el gesto).
    pub fn saltar_a_etiqueta(&mut self, etiqueta: char) -> Result<Option<PathBuf>> {
        let etiqueta = etiqueta.to_ascii_lowercase();
        let Some(indice) = ALFABETO_ETIQUETAS_SALTO.chars().position(|e| e == etiqueta) else {
            return Ok(None);
        };
        if indice >= self.lista_visible().len() {
            return Ok(None);
        }
        self.seleccion = indice;
        self.modo_salto = false;
        self.activar_seleccion()
    }

    pub fn nombre_raiz(&self) -> &str {
        &self.raiz.nombre
    }

    pub fn seleccion(&self) -> usize {
        self.seleccion
    }

    /// Recorrido en profundidad de las filas visibles (respetando qué
    /// carpetas están expandidas), como `(profundidad, nodo)`. La raíz
    /// misma no aparece como fila: solo su contenido.
    pub fn lista_visible(&self) -> Vec<(usize, &Nodo)> {
        let mut salida = Vec::new();
        for hijo in &self.raiz.hijos {
            aplanar(hijo, 0, &mut salida);
        }
        salida
    }

    pub fn mover_abajo(&mut self) {
        let total = self.lista_visible().len();
        if total == 0 {
            return;
        }
        self.seleccion = (self.seleccion + 1).min(total - 1);
    }

    pub fn mover_arriba(&mut self) {
        self.seleccion = self.seleccion.saturating_sub(1);
    }

    /// Activa la fila seleccionada: si es una carpeta, la expande o
    /// colapsa (y no devuelve nada); si es un archivo, devuelve su ruta
    /// para que quien llama lo abra en el editor.
    pub fn activar_seleccion(&mut self) -> Result<Option<PathBuf>> {
        let mut restante = self.seleccion;
        let Some(nodo) = nodo_mut_en_indice(&mut self.raiz.hijos, &mut restante) else {
            return Ok(None);
        };

        let resultado = if nodo.es_carpeta {
            nodo.alternar()?;
            None
        } else {
            Some(nodo.ruta.clone())
        };

        let total = self.lista_visible().len();
        self.seleccion = self.seleccion.min(total.saturating_sub(1));

        Ok(resultado)
    }

    /// Nodo bajo la fila seleccionada ahora mismo, si el árbol no está
    /// vacío — lo que necesitan crear/renombrar/borrar para saber sobre
    /// qué archivo/carpeta operar sin duplicar el recorrido de
    /// `activar_seleccion`.
    pub fn seleccion_actual(&self) -> Option<&Nodo> {
        let mut restante = self.seleccion;
        nodo_en_indice(&self.raiz.hijos, &mut restante)
    }

    /// Carpeta donde debería crearse un archivo/carpeta nuevo (`Ctrl+K
    /// N`/`Ctrl+K C`): la seleccionada si es una carpeta, la que la
    /// contiene si es un archivo, o la raíz del proyecto si no hay nada
    /// seleccionado (árbol vacío).
    pub fn carpeta_destino_para_nuevo(&self) -> &Path {
        match self.seleccion_actual() {
            Some(nodo) if nodo.es_carpeta => &nodo.ruta,
            Some(nodo) => nodo.ruta.parent().unwrap_or(&self.raiz.ruta),
            None => &self.raiz.ruta,
        }
    }

    /// Crea un archivo vacío llamado `nombre` dentro de
    /// [`Explorador::carpeta_destino_para_nuevo`] y refresca esa carpeta
    /// en el árbol para que aparezca sin tener que colapsar/reexpandirla
    /// a mano. Falla (sin tocar el disco) si ya existe algo con ese
    /// nombre ahí — mismo criterio que "Guardar como" nunca sobreescribe
    /// en silencio.
    pub fn crear_archivo(&mut self, nombre: &str) -> Result<()> {
        let destino = self.carpeta_destino_para_nuevo().join(nombre);
        if destino.exists() {
            anyhow::bail!("ya existe '{}'", destino.display());
        }
        std::fs::File::create(&destino).with_context(|| format!("no se pudo crear '{}'", destino.display()))?;
        self.refrescar_carpeta(destino.parent().unwrap_or(&self.raiz.ruta).to_path_buf())
    }

    /// Análogo a [`Explorador::crear_archivo`] pero para una carpeta
    /// nueva (`std::fs::create_dir`, no recursivo — mismo criterio que
    /// cualquier explorador: si la carpeta padre no existe, es un error,
    /// no algo para crear en cascada sin confirmación).
    pub fn crear_carpeta(&mut self, nombre: &str) -> Result<()> {
        let destino = self.carpeta_destino_para_nuevo().join(nombre);
        if destino.exists() {
            anyhow::bail!("ya existe '{}'", destino.display());
        }
        std::fs::create_dir(&destino).with_context(|| format!("no se pudo crear '{}'", destino.display()))?;
        self.refrescar_carpeta(destino.parent().unwrap_or(&self.raiz.ruta).to_path_buf())
    }

    /// Renombra la fila seleccionada a `nuevo_nombre` (dentro de la misma
    /// carpeta contenedora — esto es "renombrar", no "mover" a otra
    /// carpeta) y refresca esa carpeta en el árbol.
    pub fn renombrar_seleccion(&mut self, nuevo_nombre: &str) -> Result<()> {
        let Some(nodo) = self.seleccion_actual() else {
            anyhow::bail!("no hay nada seleccionado para renombrar");
        };
        let origen = nodo.ruta.clone();
        let carpeta_padre = origen.parent().unwrap_or(&self.raiz.ruta).to_path_buf();
        let destino = carpeta_padre.join(nuevo_nombre);
        if destino.exists() {
            anyhow::bail!("ya existe '{}'", destino.display());
        }
        std::fs::rename(&origen, &destino)
            .with_context(|| format!("no se pudo renombrar '{}' a '{}'", origen.display(), destino.display()))?;
        self.refrescar_carpeta(carpeta_padre)
    }

    /// Borra la fila seleccionada del disco (archivo con
    /// `std::fs::remove_file`, carpeta con `std::fs::remove_dir_all` —
    /// recursivo, sin papelera de reciclaje) y refresca la carpeta
    /// contenedora. Quien llama (`app`) es responsable de haber pedido
    /// confirmación antes (`tcode_fs::EstadoConfirmarBorrado`) — este
    /// método no vuelve a preguntar nada, borra directo.
    pub fn borrar_seleccion(&mut self) -> Result<()> {
        let Some(nodo) = self.seleccion_actual() else {
            anyhow::bail!("no hay nada seleccionado para borrar");
        };
        let ruta = nodo.ruta.clone();
        let es_carpeta = nodo.es_carpeta;
        let carpeta_padre = ruta.parent().unwrap_or(&self.raiz.ruta).to_path_buf();

        if es_carpeta {
            std::fs::remove_dir_all(&ruta).with_context(|| format!("no se pudo borrar '{}'", ruta.display()))?;
        } else {
            std::fs::remove_file(&ruta).with_context(|| format!("no se pudo borrar '{}'", ruta.display()))?;
        }
        self.refrescar_carpeta(carpeta_padre)?;

        // La fila borrada (y todas las que le seguían) ya no existen —
        // recortar la selección igual que hace `activar_seleccion` al
        // colapsar una carpeta.
        let total = self.lista_visible().len();
        self.seleccion = self.seleccion.min(total.saturating_sub(1));
        Ok(())
    }

    /// Vuelve a leer del disco los hijos de la carpeta en `ruta` — el
    /// nodo raíz si `ruta` es la raíz del proyecto, o el nodo
    /// correspondiente en el árbol si no (`nodo_mut_por_ruta`). Si esa
    /// carpeta no está en el árbol (poco común: pasó algo raro entre medio)
    /// no hay nada que refrescar visualmente, pero el cambio en disco ya
    /// se hizo de todos modos — no se considera un error.
    fn refrescar_carpeta(&mut self, ruta: PathBuf) -> Result<()> {
        if ruta == self.raiz.ruta {
            return self.raiz.recargar_hijos_si_estaban_cargados();
        }
        let Some(nodo) = nodo_mut_por_ruta(&mut self.raiz.hijos, &ruta) else {
            return Ok(());
        };
        nodo.recargar_hijos_si_estaban_cargados()
    }
}

fn aplanar<'a>(nodo: &'a Nodo, profundidad: usize, salida: &mut Vec<(usize, &'a Nodo)>) {
    salida.push((profundidad, nodo));
    if nodo.es_carpeta && nodo.expandida {
        for hijo in &nodo.hijos {
            aplanar(hijo, profundidad + 1, salida);
        }
    }
}

/// Recorrido en profundidad mutable, análogo a `aplanar`, que devuelve el
/// nodo en la posición `indice` del recorrido (respetando expansión).
/// `indice` se va descontando a medida que se visitan nodos.
fn nodo_mut_en_indice<'a>(nodos: &'a mut [Nodo], indice: &mut usize) -> Option<&'a mut Nodo> {
    for nodo in nodos.iter_mut() {
        if *indice == 0 {
            return Some(nodo);
        }
        *indice -= 1;
        if nodo.es_carpeta && nodo.expandida {
            if let Some(encontrado) = nodo_mut_en_indice(&mut nodo.hijos, indice) {
                return Some(encontrado);
            }
        }
    }
    None
}

/// Versión de solo lectura de [`nodo_mut_en_indice`] — `Explorador::
/// seleccion_actual` no necesita mutar nada, solo consultar.
fn nodo_en_indice<'a>(nodos: &'a [Nodo], indice: &mut usize) -> Option<&'a Nodo> {
    for nodo in nodos {
        if *indice == 0 {
            return Some(nodo);
        }
        *indice -= 1;
        if nodo.es_carpeta && nodo.expandida {
            if let Some(encontrado) = nodo_en_indice(&nodo.hijos, indice) {
                return Some(encontrado);
            }
        }
    }
    None
}

/// Busca el nodo cuya `ruta` coincide exactamente con `objetivo`, en
/// cualquier parte del árbol (esté o no expandido — no importa para
/// refrescar: una carpeta nunca expandida no tiene hijos cargados que
/// refrescar, `Nodo::recargar_hijos_si_estaban_cargados` ya lo resuelve
/// sola). Usado por `Explorador::refrescar_carpeta` tras crear/renombrar/
/// borrar algo.
fn nodo_mut_por_ruta<'a>(nodos: &'a mut [Nodo], objetivo: &Path) -> Option<&'a mut Nodo> {
    for nodo in nodos {
        if nodo.ruta == objetivo {
            return Some(nodo);
        }
        if nodo.es_carpeta {
            if let Some(encontrado) = nodo_mut_por_ruta(&mut nodo.hijos, objetivo) {
                return Some(encontrado);
            }
        }
    }
    None
}

/// Ruta por defecto para abrir el explorador: la carpeta que contiene
/// `ruta_archivo` si se pasó una, si no el directorio de trabajo actual.
pub fn raiz_por_defecto(ruta_archivo: Option<&str>) -> PathBuf {
    ruta_archivo
        .map(Path::new)
        .and_then(|p| p.parent())
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crear_arbol_de_prueba() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("carpeta")).unwrap();
        std::fs::write(dir.path().join("carpeta").join("dentro.txt"), "").unwrap();
        std::fs::write(dir.path().join("archivo.txt"), "").unwrap();
        dir
    }

    #[test]
    fn arranca_oculto_y_alterna_visibilidad() {
        let dir = crear_arbol_de_prueba();
        let mut explorador = Explorador::nuevo(dir.path()).unwrap();
        assert!(!explorador.visible());
        explorador.alternar_visibilidad();
        assert!(explorador.visible());
        explorador.alternar_visibilidad();
        assert!(!explorador.visible());
    }

    #[test]
    fn lista_visible_no_incluye_hijos_de_carpetas_colapsadas() {
        let dir = crear_arbol_de_prueba();
        let explorador = Explorador::nuevo(dir.path()).unwrap();
        // "carpeta" (colapsada) y "archivo.txt": 2 filas, no 3.
        assert_eq!(explorador.lista_visible().len(), 2);
    }

    #[test]
    fn activar_una_carpeta_la_expande_y_aparecen_sus_hijos() {
        let dir = crear_arbol_de_prueba();
        let mut explorador = Explorador::nuevo(dir.path()).unwrap();
        // Fila 0 = "carpeta" (carpetas antes que archivos).
        assert!(explorador.lista_visible()[0].1.es_carpeta);

        let resultado = explorador.activar_seleccion().unwrap();
        assert_eq!(resultado, None, "activar una carpeta no debe devolver una ruta para abrir");
        assert_eq!(explorador.lista_visible().len(), 3, "carpeta + su hijo + archivo.txt");
    }

    #[test]
    fn activar_un_archivo_devuelve_su_ruta() {
        let dir = crear_arbol_de_prueba();
        let mut explorador = Explorador::nuevo(dir.path()).unwrap();
        explorador.mover_abajo(); // fila 1 = "archivo.txt"
        let resultado = explorador.activar_seleccion().unwrap();
        assert_eq!(resultado, Some(dir.path().join("archivo.txt")));
    }

    #[test]
    fn mover_arriba_y_abajo_se_recorta_a_los_limites() {
        let dir = crear_arbol_de_prueba();
        let mut explorador = Explorador::nuevo(dir.path()).unwrap();
        explorador.mover_arriba();
        assert_eq!(explorador.seleccion(), 0);
        explorador.mover_abajo();
        explorador.mover_abajo();
        explorador.mover_abajo();
        assert_eq!(explorador.seleccion(), 1); // solo 2 filas visibles (0 y 1)
    }

    #[test]
    fn raiz_por_defecto_usa_la_carpeta_padre_del_archivo() {
        let raiz = raiz_por_defecto(Some("/tmp/proyecto/archivo.rs"));
        assert_eq!(raiz, PathBuf::from("/tmp/proyecto"));
    }

    #[test]
    fn raiz_por_defecto_sin_archivo_usa_el_directorio_actual() {
        let raiz = raiz_por_defecto(None);
        assert_eq!(raiz, std::env::current_dir().unwrap());
    }

    #[test]
    fn etiqueta_para_fila_usa_digitos_primero_y_despues_letras() {
        assert_eq!(Explorador::etiqueta_para_fila(0), Some('1'));
        assert_eq!(Explorador::etiqueta_para_fila(9), Some('0'));
        assert_eq!(Explorador::etiqueta_para_fila(10), Some('a'));
        assert_eq!(Explorador::etiqueta_para_fila(35), Some('z'));
        assert_eq!(Explorador::etiqueta_para_fila(36), None, "el alfabeto tiene 36 etiquetas");
    }

    #[test]
    fn activar_modo_salto_y_salir_alternan_el_estado() {
        let dir = crear_arbol_de_prueba();
        let mut explorador = Explorador::nuevo(dir.path()).unwrap();
        assert!(!explorador.modo_salto());
        explorador.activar_modo_salto();
        assert!(explorador.modo_salto());
        explorador.salir_modo_salto();
        assert!(!explorador.modo_salto());
    }

    #[test]
    fn ocultar_tambien_sale_del_modo_salto() {
        let dir = crear_arbol_de_prueba();
        let mut explorador = Explorador::nuevo(dir.path()).unwrap();
        explorador.mostrar();
        explorador.activar_modo_salto();
        explorador.ocultar();
        assert!(!explorador.modo_salto());
    }

    #[test]
    fn saltar_a_etiqueta_abre_el_archivo_que_le_toca_sin_navegar() {
        let dir = crear_arbol_de_prueba();
        let mut explorador = Explorador::nuevo(dir.path()).unwrap();
        explorador.activar_modo_salto();
        // Fila 0 = "carpeta" (colapsada), fila 1 = "archivo.txt" -> etiqueta '2'.
        let resultado = explorador.saltar_a_etiqueta('2').unwrap();
        assert_eq!(resultado, Some(dir.path().join("archivo.txt")));
        assert_eq!(explorador.seleccion(), 1);
        assert!(!explorador.modo_salto(), "saltar con éxito sale del modo salto");
    }

    #[test]
    fn saltar_a_etiqueta_sobre_una_carpeta_la_expande_en_vez_de_devolver_una_ruta() {
        let dir = crear_arbol_de_prueba();
        let mut explorador = Explorador::nuevo(dir.path()).unwrap();
        explorador.activar_modo_salto();
        // Fila 0 = "carpeta" -> etiqueta '1'.
        let resultado = explorador.saltar_a_etiqueta('1').unwrap();
        assert_eq!(resultado, None);
        assert_eq!(explorador.lista_visible().len(), 3, "la carpeta quedó expandida");
        assert!(!explorador.modo_salto());
    }

    #[test]
    fn saltar_a_etiqueta_sin_fila_correspondiente_no_hace_nada_y_sigue_activo() {
        let dir = crear_arbol_de_prueba();
        let mut explorador = Explorador::nuevo(dir.path()).unwrap();
        explorador.activar_modo_salto();
        // Solo hay 2 filas visibles ('1' y '2'); 'z' no le toca a ninguna.
        let resultado = explorador.saltar_a_etiqueta('z').unwrap();
        assert_eq!(resultado, None);
        assert!(explorador.modo_salto(), "una etiqueta inválida no cancela el modo salto");
    }

    #[test]
    fn carpeta_destino_para_nuevo_sin_seleccion_es_la_raiz() {
        let dir = crear_arbol_de_prueba();
        let explorador = Explorador::nuevo(dir.path()).unwrap();
        // Fila 0 = "carpeta" (una carpeta) — pero para probar "sin
        // selección" de verdad hace falta un árbol vacío.
        let vacio = tempfile::tempdir().unwrap();
        let explorador_vacio = Explorador::nuevo(vacio.path()).unwrap();
        assert_eq!(explorador_vacio.carpeta_destino_para_nuevo(), vacio.path());
        // Con "carpeta" seleccionada (fila 0, una carpeta): el destino es
        // ella misma.
        assert_eq!(explorador.carpeta_destino_para_nuevo(), dir.path().join("carpeta"));
    }

    #[test]
    fn carpeta_destino_para_nuevo_con_archivo_seleccionado_es_su_carpeta_contenedora() {
        let dir = crear_arbol_de_prueba();
        let mut explorador = Explorador::nuevo(dir.path()).unwrap();
        explorador.mover_abajo(); // fila 1 = "archivo.txt"
        assert_eq!(explorador.carpeta_destino_para_nuevo(), dir.path());
    }

    #[test]
    fn crear_archivo_en_la_raiz_aparece_en_el_arbol_y_en_disco() {
        let dir = crear_arbol_de_prueba();
        let mut explorador = Explorador::nuevo(dir.path()).unwrap();
        explorador.mover_abajo(); // selecciona "archivo.txt" -> destino es la raíz

        explorador.crear_archivo("nuevo.txt").unwrap();

        assert!(dir.path().join("nuevo.txt").is_file());
        assert!(explorador.lista_visible().iter().any(|(_, n)| n.nombre == "nuevo.txt"));
    }

    #[test]
    fn crear_archivo_dentro_de_una_carpeta_seleccionada() {
        let dir = crear_arbol_de_prueba();
        let mut explorador = Explorador::nuevo(dir.path()).unwrap();
        // Fila 0 = "carpeta" (colapsada) — sigue siendo el destino aunque
        // no esté expandida.
        explorador.crear_archivo("adentro2.txt").unwrap();

        assert!(dir.path().join("carpeta").join("adentro2.txt").is_file());
        // Expandir para confirmar que el hijo nuevo aparece en el árbol.
        explorador.activar_seleccion().unwrap();
        assert!(explorador.lista_visible().iter().any(|(_, n)| n.nombre == "adentro2.txt"));
    }

    #[test]
    fn crear_archivo_que_ya_existe_falla_sin_tocar_el_disco() {
        let dir = crear_arbol_de_prueba();
        let mut explorador = Explorador::nuevo(dir.path()).unwrap();
        explorador.mover_abajo(); // destino: raíz
        std::fs::write(dir.path().join("archivo.txt"), "contenido original").unwrap();

        assert!(explorador.crear_archivo("archivo.txt").is_err());
        assert_eq!(std::fs::read_to_string(dir.path().join("archivo.txt")).unwrap(), "contenido original");
    }

    #[test]
    fn crear_carpeta_aparece_en_el_arbol_y_en_disco() {
        let dir = crear_arbol_de_prueba();
        let mut explorador = Explorador::nuevo(dir.path()).unwrap();
        explorador.mover_abajo(); // destino: raíz

        explorador.crear_carpeta("carpeta_nueva").unwrap();

        assert!(dir.path().join("carpeta_nueva").is_dir());
        assert!(explorador.lista_visible().iter().any(|(_, n)| n.nombre == "carpeta_nueva" && n.es_carpeta));
    }

    #[test]
    fn renombrar_seleccion_cambia_el_nombre_conservando_el_contenido() {
        let dir = crear_arbol_de_prueba();
        std::fs::write(dir.path().join("archivo.txt"), "hola").unwrap();
        let mut explorador = Explorador::nuevo(dir.path()).unwrap();
        explorador.mover_abajo(); // fila 1 = "archivo.txt"

        explorador.renombrar_seleccion("renombrado.txt").unwrap();

        assert!(!dir.path().join("archivo.txt").exists());
        assert_eq!(std::fs::read_to_string(dir.path().join("renombrado.txt")).unwrap(), "hola");
        assert!(explorador.lista_visible().iter().any(|(_, n)| n.nombre == "renombrado.txt"));
        assert!(!explorador.lista_visible().iter().any(|(_, n)| n.nombre == "archivo.txt"));
    }

    #[test]
    fn renombrar_a_un_nombre_que_ya_existe_falla() {
        let dir = crear_arbol_de_prueba();
        let mut explorador = Explorador::nuevo(dir.path()).unwrap();
        explorador.mover_abajo(); // fila 1 = "archivo.txt"

        assert!(explorador.renombrar_seleccion("carpeta").is_err());
        assert!(dir.path().join("archivo.txt").exists(), "no debería haberse tocado el original");
    }

    #[test]
    fn borrar_un_archivo_lo_quita_del_arbol_y_del_disco() {
        let dir = crear_arbol_de_prueba();
        let mut explorador = Explorador::nuevo(dir.path()).unwrap();
        explorador.mover_abajo(); // fila 1 = "archivo.txt"

        explorador.borrar_seleccion().unwrap();

        assert!(!dir.path().join("archivo.txt").exists());
        assert!(!explorador.lista_visible().iter().any(|(_, n)| n.nombre == "archivo.txt"));
    }

    #[test]
    fn borrar_una_carpeta_la_borra_recursivamente() {
        let dir = crear_arbol_de_prueba();
        let mut explorador = Explorador::nuevo(dir.path()).unwrap();
        // Fila 0 = "carpeta" (tiene "dentro.txt" adentro).

        explorador.borrar_seleccion().unwrap();

        assert!(!dir.path().join("carpeta").exists());
        assert_eq!(explorador.lista_visible().len(), 1, "solo queda archivo.txt");
    }

    #[test]
    fn seleccion_actual_es_none_en_un_arbol_vacio() {
        let dir = tempfile::tempdir().unwrap();
        let explorador = Explorador::nuevo(dir.path()).unwrap();
        assert!(explorador.seleccion_actual().is_none());
    }
}
