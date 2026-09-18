use std::path::{Path, PathBuf};

use anyhow::Result;

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
}
