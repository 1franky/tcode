use anyhow::{Context, Result};

/// Una fila de una tabla CSV/TSV ya analizada: sus celdas como texto, y el
/// rango de bytes `[inicio, fin)` que ocupaba en el texto original
/// (incluyendo su salto de línea) — permite reemplazarla entera al
/// confirmar la edición de una celda, en vez de tener que reserializar el
/// archivo completo (ver [`serializar_fila`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilaCsv {
    pub celdas: Vec<String>,
    pub inicio_byte: usize,
    pub fin_byte: usize,
}

/// Una tabla CSV/TSV (PLAN.md §9). `filas[0]` es siempre el encabezado
/// (la fila que se dibuja congelada en la vista) — un archivo vacío da
/// una tabla sin filas, ni siquiera encabezado.
#[derive(Debug, Clone, Default)]
pub struct TablaCsv {
    pub delimitador: u8,
    pub filas: Vec<FilaCsv>,
}

impl TablaCsv {
    pub fn num_columnas(&self) -> usize {
        self.filas.iter().map(|f| f.celdas.len()).max().unwrap_or(0)
    }

    pub fn num_filas(&self) -> usize {
        self.filas.len()
    }
}

/// Delimitador por extensión de archivo (PLAN.md §9: detección automática
/// por `.csv`/`.tsv`): tabulador para `.tsv`, coma para cualquier otra
/// cosa (incluido `.csv`).
pub fn delimitador_por_extension(ruta: &str) -> u8 {
    let extension = ruta.rsplit('.').next().unwrap_or("");
    if extension.eq_ignore_ascii_case("tsv") {
        b'\t'
    } else {
        b','
    }
}

/// Analiza `texto` como CSV/TSV con el delimitador dado, calculando
/// además el rango de bytes original de cada fila. `flexible` (número de
/// celdas distinto entre filas no aborta el parseo) porque un archivo a
/// medio editar es exactamente el caso más común en un editor de texto —
/// preferible mostrar filas irregulares a negarse a dibujar la tabla.
pub fn analizar(texto: &str, delimitador: u8) -> Result<TablaCsv> {
    let mut lector = ::csv::ReaderBuilder::new()
        .delimiter(delimitador)
        .has_headers(false)
        .flexible(true)
        .from_reader(texto.as_bytes());

    let mut filas = Vec::new();
    let mut registro = ::csv::StringRecord::new();
    loop {
        let inicio_byte = lector.position().byte() as usize;
        if !lector.read_record(&mut registro).context("CSV/TSV inválido")? {
            break;
        }
        let fin_byte = lector.position().byte() as usize;
        filas.push(FilaCsv { celdas: registro.iter().map(str::to_string).collect(), inicio_byte, fin_byte });
    }
    Ok(TablaCsv { delimitador, filas })
}

/// Serializa una fila con el delimitador dado, en el mismo estilo que
/// escribiría cualquier archivo CSV real: solo cita los campos que lo
/// necesitan (contienen el delimitador, una comilla o un salto de línea).
/// Siempre con `\n` como terminador — el `rope` de [`crate::Buffer`] está
/// siempre normalizado a `\n` puertas adentro sin importar el fin de línea
/// real del archivo (`Eol`, reconstruido solo al guardar), así que una fila
/// reemplazada a mitad de archivo debe usar ese mismo `\n` interno para
/// quedar consistente con el resto del texto. El valor por defecto del
/// propio crate `csv` es CRLF, así que hay que pedir `\n` explícito.
///
/// No garantiza reproducir el quoting exacto que tenía esa fila en el
/// archivo original si no hacía falta citar lo que el usuario sí había
/// citado — compromiso deliberado: reemplazar la fila completa con un
/// quoting mínimo y correcto es mucho más simple y robusto que intentar
/// parchear un único campo preservando cada detalle de formato del resto.
pub fn serializar_fila(celdas: &[String], delimitador: u8) -> Result<String> {
    let mut escritor = ::csv::WriterBuilder::new()
        .delimiter(delimitador)
        .terminator(::csv::Terminator::Any(b'\n'))
        .from_writer(Vec::new());
    escritor.write_record(celdas).context("no se pudo serializar la fila CSV/TSV")?;
    let bytes = escritor.into_inner().map_err(|e| anyhow::anyhow!("no se pudo cerrar el escritor CSV/TSV: {e}"))?;
    String::from_utf8(bytes).context("la fila serializada no es UTF-8 válido")
}

/// Una edición de la vista de tabla ya traducida a texto: reemplazar los
/// bytes `[inicio_byte, fin_byte)` del texto original por `reemplazo`.
/// Ordenar, insertar y eliminar filas/columnas (BACKLOG.md P2 #9) se
/// expresan TODAS así, como un único rango contiguo, para que `app` las
/// aplique con una sola llamada a `Editor::reemplazar_rango_bytes` — un
/// solo snapshot en el historial, así que cada operación se deshace con
/// un único `Ctrl+Z`, igual que confirmar la edición de una celda.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdicionCsv {
    pub inicio_byte: usize,
    pub fin_byte: usize,
    pub reemplazo: String,
}

impl EdicionCsv {
    /// Aplica la edición sobre `texto` — solo para tests y para quien
    /// necesite el resultado sin pasar por un `Editor`.
    pub fn aplicar(&self, texto: &str) -> String {
        format!("{}{}{}", &texto[..self.inicio_byte], self.reemplazo, &texto[self.fin_byte..])
    }
}

/// Texto original de una fila (`[inicio_byte, fin_byte)`), incluido su
/// salto de línea y cualquier línea en blanco que el lector de `csv`
/// se haya salteado justo antes de ella.
fn texto_fila<'a>(texto: &'a str, fila: &FilaCsv) -> &'a str {
    &texto[fila.inicio_byte..fila.fin_byte]
}

/// Clave de orden de un texto: minúsculas y vocales sin tilde, para que
/// "Ángel" no quede después de "Zoe" (en Unicode las letras acentuadas
/// van después de toda la `a-z`). La `ñ` se pliega a `n~`: `~` es mayor
/// que cualquier letra ASCII, así que "ñ" queda después de "nz" y antes
/// de "o" — el orden del alfabeto español — sin necesitar una
/// dependencia de collation completa (ICU) para un caso tan acotado.
fn clave_texto(valor: &str) -> String {
    let mut clave = String::with_capacity(valor.len());
    for c in valor.chars().flat_map(char::to_lowercase) {
        match c {
            'á' | 'à' | 'ä' | 'â' => clave.push('a'),
            'é' | 'è' | 'ë' | 'ê' => clave.push('e'),
            'í' | 'ì' | 'ï' | 'î' => clave.push('i'),
            'ó' | 'ò' | 'ö' | 'ô' => clave.push('o'),
            'ú' | 'ù' | 'ü' | 'û' => clave.push('u'),
            'ñ' => clave.push_str("n~"),
            otro => clave.push(otro),
        }
    }
    clave
}

/// Ordena las filas de datos (todas menos `filas[0]`, el encabezado, que
/// no se mueve) por la celda de `columna`. Si TODAS las celdas no vacías
/// de esa columna son números (`f64`, tras recortar espacios), compara
/// numéricamente — si no, como texto con [`clave_texto`]. Las celdas
/// vacías (o filas más cortas que no llegan a esa columna) van siempre al
/// final, tanto en orden ascendente como descendente, igual que en una
/// hoja de cálculo. El orden es estable: filas con el mismo valor quedan
/// en el orden en que estaban.
///
/// No re-serializa nada: reordena los bytes originales de cada fila tal
/// cual, así que el quoting de cada una se conserva exacto. Devuelve
/// `None` si no hay nada que ordenar (menos de dos filas de datos) o si
/// el orden resultante es el mismo que el actual (así no se registra un
/// paso de deshacer vacío).
pub fn ordenar_por_columna(texto: &str, tabla: &TablaCsv, columna: usize, ascendente: bool) -> Option<EdicionCsv> {
    let datos = tabla.filas.get(1..)?;
    if datos.len() < 2 {
        return None;
    }

    fn celda_de(fila: &FilaCsv, columna: usize) -> &str {
        fila.celdas.get(columna).map(|c| c.trim()).unwrap_or("")
    }
    let no_vacias = || datos.iter().map(|f| celda_de(f, columna)).filter(|c| !c.is_empty());
    let numerica = no_vacias().next().is_some() && no_vacias().all(|c| c.parse::<f64>().is_ok());

    let mut orden: Vec<usize> = (0..datos.len()).collect();
    // `sort_by` es estable: es lo que garantiza el orden original entre
    // iguales (también entre todas las celdas vacías del final).
    orden.sort_by(|&a, &b| {
        let (va, vb) = (celda_de(&datos[a], columna), celda_de(&datos[b], columna));
        match (va.is_empty(), vb.is_empty()) {
            (true, true) => return std::cmp::Ordering::Equal,
            (true, false) => return std::cmp::Ordering::Greater,
            (false, true) => return std::cmp::Ordering::Less,
            (false, false) => {}
        }
        let resultado = if numerica {
            let (na, nb) = (va.parse::<f64>().unwrap_or(0.0), vb.parse::<f64>().unwrap_or(0.0));
            na.total_cmp(&nb)
        } else {
            clave_texto(va).cmp(&clave_texto(vb))
        };
        if ascendente {
            resultado
        } else {
            resultado.reverse()
        }
    });
    if orden.iter().enumerate().all(|(i, &j)| i == j) {
        return None;
    }

    let inicio_byte = datos[0].inicio_byte;
    let fin_byte = datos[datos.len() - 1].fin_byte;
    Some(EdicionCsv { inicio_byte, fin_byte, reemplazo: concatenar_filas(texto, orden.iter().map(|&i| &datos[i]), fin_byte) })
}

/// Concatena el texto original de `filas` (en el orden dado) asegurando
/// que cada una termine en `\n` — la última fila de un archivo sin salto
/// de línea final no lo tiene, y al moverla al medio se pegaría con la
/// siguiente. Si el rango original (que termina en `fin_original`) no
/// terminaba en `\n`, el resultado tampoco: el archivo conserva si tenía
/// o no salto de línea al final.
fn concatenar_filas<'a>(texto: &str, filas: impl Iterator<Item = &'a FilaCsv>, fin_original: usize) -> String {
    let mut resultado = String::new();
    for fila in filas {
        resultado.push_str(texto_fila(texto, fila));
        if !resultado.ends_with('\n') {
            resultado.push('\n');
        }
    }
    if !texto[..fin_original].ends_with('\n') {
        resultado.pop();
    }
    resultado
}

/// Inserta una fila vacía (con tantas celdas vacías como columnas tiene
/// la tabla) para que quede en la posición `indice` (`0..=num_filas`):
/// `indice == num_filas` la agrega al final. Una fila de una sola celda
/// vacía se escribe `""` (lo hace el propio crate `csv`), no una línea en
/// blanco — el lector se saltea las líneas en blanco y la fila nueva
/// desaparecería al volver a analizar el archivo.
pub fn insertar_fila(texto: &str, tabla: &TablaCsv, indice: usize) -> Result<EdicionCsv> {
    let celdas = vec![String::new(); tabla.num_columnas().max(1)];
    let mut nueva = serializar_fila(&celdas, tabla.delimitador)?;

    let posicion = if let Some(fila) = tabla.filas.get(indice) {
        fila.inicio_byte
    } else if let Some(ultima) = tabla.filas.last() {
        // Al final: si la última fila no tenía salto de línea (archivo
        // sin `\n` final), la nueva va en su propia línea y hereda esa
        // ausencia de `\n` final.
        if !texto_fila(texto, ultima).ends_with('\n') {
            nueva.pop();
            nueva.insert(0, '\n');
        }
        ultima.fin_byte
    } else {
        0
    };
    Ok(EdicionCsv { inicio_byte: posicion, fin_byte: posicion, reemplazo: nueva })
}

/// Elimina la fila `indice` (cualquiera, encabezado incluido: la
/// siguiente pasa a ser el encabezado). Si es la última fila y no tenía
/// salto de línea final, también se come el `\n` de la anterior — así el
/// archivo sigue sin `\n` final en vez de ganar uno de regalo.
pub fn eliminar_fila(texto: &str, tabla: &TablaCsv, indice: usize) -> Option<EdicionCsv> {
    let fila = tabla.filas.get(indice)?;
    let mut inicio_byte = fila.inicio_byte;
    let es_ultima = indice + 1 == tabla.filas.len();
    if es_ultima && indice > 0 && !texto_fila(texto, fila).ends_with('\n') && texto[..inicio_byte].ends_with('\n') {
        inicio_byte -= 1;
    }
    Some(EdicionCsv { inicio_byte, fin_byte: fila.fin_byte, reemplazo: String::new() })
}

/// Re-serializa TODAS las filas con `transformar` aplicado a las celdas
/// de cada una. Insertar/eliminar una columna cambia todas las filas, así
/// que acá no hay forma de conservar el texto original de cada fila como
/// al ordenar: se usa el mismo quoting mínimo y correcto de
/// [`serializar_fila`] (compromiso ya aceptado al editar una celda). Las
/// líneas en blanco entre filas se pierden (el lector de `csv` ya las
/// ignoraba); si el archivo no terminaba en `\n`, sigue sin terminar.
fn reserializar_todas(texto: &str, tabla: &TablaCsv, transformar: impl Fn(&mut Vec<String>)) -> Result<Option<EdicionCsv>> {
    let (Some(primera), Some(ultima)) = (tabla.filas.first(), tabla.filas.last()) else { return Ok(None) };
    let mut reemplazo = String::new();
    for fila in &tabla.filas {
        let mut celdas = fila.celdas.clone();
        transformar(&mut celdas);
        reemplazo.push_str(&serializar_fila(&celdas, tabla.delimitador)?);
    }
    if !texto[..ultima.fin_byte].ends_with('\n') {
        reemplazo.pop();
    }
    Ok(Some(EdicionCsv { inicio_byte: primera.inicio_byte, fin_byte: ultima.fin_byte, reemplazo }))
}

/// Inserta una columna vacía para que quede en la posición `indice`
/// (`0..=num_columnas`) en todas las filas. Una fila más corta que
/// `indice` (CSV irregular) se deja como está: su celda en esa columna ya
/// era implícitamente vacía, y rellenarla cambiaría filas que el usuario
/// no tocó.
pub fn insertar_columna(texto: &str, tabla: &TablaCsv, indice: usize) -> Result<Option<EdicionCsv>> {
    reserializar_todas(texto, tabla, |celdas| {
        if indice <= celdas.len() {
            celdas.insert(indice, String::new());
        }
    })
}

/// Elimina la columna `indice` de todas las filas que la tengan. Con una
/// sola columna no hace nada (`Ok(None)`): el resultado serían filas sin
/// ninguna celda, que el lector de `csv` ya no puede distinguir de líneas
/// en blanco — para vaciar el archivo está el modo texto (`Ctrl+K T`).
pub fn eliminar_columna(texto: &str, tabla: &TablaCsv, indice: usize) -> Result<Option<EdicionCsv>> {
    if tabla.num_columnas() <= 1 || indice >= tabla.num_columnas() {
        return Ok(None);
    }
    reserializar_todas(texto, tabla, |celdas| {
        if indice < celdas.len() {
            celdas.remove(indice);
        }
    })
}

/// Filtro de la vista de tabla (BACKLOG.md P2 #9): solo se muestran las
/// filas cuya celda en `columna` contiene `texto`, sin distinguir
/// mayúsculas ni tildes (misma [`clave_texto`] que al ordenar). Es solo de
/// vista — no toca el archivo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiltroCsv {
    pub columna: usize,
    pub texto: String,
}

/// Índices REALES (en `tabla.filas`) de las filas visibles con `filtro`
/// aplicado, en orden. El encabezado (`0`) siempre está, filtre lo que
/// filtre: la vista lo dibuja congelado arriba. Este es el mapeo "fila
/// visible → fila real" que usan la navegación (`EstadoCsv::fila` es un
/// índice en esta lista) y la edición de celdas, para que con un filtro
/// activo se edite la fila correcta del archivo.
pub fn filas_visibles(tabla: &TablaCsv, filtro: Option<&FiltroCsv>) -> Vec<usize> {
    let Some(filtro) = filtro else { return (0..tabla.filas.len()).collect() };
    let aguja = clave_texto(&filtro.texto);
    tabla
        .filas
        .iter()
        .enumerate()
        .filter(|(i, fila)| *i == 0 || clave_texto(fila.celdas.get(filtro.columna).map(String::as_str).unwrap_or("")).contains(&aguja))
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analiza_filas_simples_y_calcula_sus_rangos_de_bytes() {
        let texto = "a,b\nc,d\n";
        let tabla = analizar(texto, b',').unwrap();
        assert_eq!(tabla.filas.len(), 2);
        assert_eq!(tabla.filas[0].celdas, vec!["a", "b"]);
        assert_eq!(&texto[tabla.filas[0].inicio_byte..tabla.filas[0].fin_byte], "a,b\n");
        assert_eq!(tabla.filas[1].celdas, vec!["c", "d"]);
        assert_eq!(&texto[tabla.filas[1].inicio_byte..tabla.filas[1].fin_byte], "c,d\n");
    }

    #[test]
    fn respeta_celdas_citadas_con_el_delimitador_adentro() {
        let texto = "nombre,ciudad\n\"Ana, Pérez\",\"Bogotá\"\n";
        let tabla = analizar(texto, b',').unwrap();
        assert_eq!(tabla.filas[1].celdas, vec!["Ana, Pérez", "Bogotá"]);
    }

    #[test]
    fn tsv_usa_tabulador_como_delimitador() {
        assert_eq!(delimitador_por_extension("datos.tsv"), b'\t');
        assert_eq!(delimitador_por_extension("datos.csv"), b',');
        assert_eq!(delimitador_por_extension("datos.CSV"), b',');
        assert_eq!(delimitador_por_extension("sin_extension"), b',');

        let texto = "a\tb\nc\td\n";
        let tabla = analizar(texto, b'\t').unwrap();
        assert_eq!(tabla.filas[0].celdas, vec!["a", "b"]);
    }

    #[test]
    fn filas_de_distinto_largo_no_abortan_el_parseo() {
        let texto = "a,b,c\nd,e\n";
        let tabla = analizar(texto, b',').unwrap();
        assert_eq!(tabla.filas[0].celdas, vec!["a", "b", "c"]);
        assert_eq!(tabla.filas[1].celdas, vec!["d", "e"]);
        assert_eq!(tabla.num_columnas(), 3);
    }

    #[test]
    fn archivo_vacio_da_una_tabla_sin_filas() {
        let tabla = analizar("", b',').unwrap();
        assert_eq!(tabla.num_filas(), 0);
    }

    #[test]
    fn serializar_fila_cita_solo_cuando_hace_falta() {
        let simple = serializar_fila(&["a".to_string(), "b".to_string()], b',').unwrap();
        assert_eq!(simple, "a,b\n");

        let con_coma = serializar_fila(&["a, b".to_string(), "c".to_string()], b',').unwrap();
        assert_eq!(con_coma, "\"a, b\",c\n");
    }

    #[test]
    fn round_trip_analizar_y_serializar_conserva_el_contenido() {
        let texto = "nombre,edad\nAna,30\n";
        let tabla = analizar(texto, b',').unwrap();
        let fila_reserializada = serializar_fila(&tabla.filas[1].celdas, b',').unwrap();
        let tabla_reanalizada = analizar(&fila_reserializada, b',').unwrap();
        assert_eq!(tabla_reanalizada.filas[0].celdas, tabla.filas[1].celdas);
    }

    /// Aplica una operación de la vista (que devuelve una `EdicionCsv`)
    /// sobre `texto` — atajo para los tests de abajo.
    fn aplicar(texto: &str, edicion: Option<EdicionCsv>) -> String {
        edicion.expect("la operación debería producir una edición").aplicar(texto)
    }

    fn columna(texto: &str, delimitador: u8, col: usize) -> Vec<String> {
        analizar(texto, delimitador).unwrap().filas.iter().map(|f| f.celdas.get(col).cloned().unwrap_or_default()).collect()
    }

    #[test]
    fn ordenar_numerico_compara_como_numero_y_no_como_texto() {
        let texto = "n\n10\n9\n-2.5\n100\n";
        let tabla = analizar(texto, b',').unwrap();
        let asc = aplicar(texto, ordenar_por_columna(texto, &tabla, 0, true));
        assert_eq!(asc, "n\n-2.5\n9\n10\n100\n"); // como texto sería 10 < 100 < 9
        let desc = aplicar(texto, ordenar_por_columna(texto, &tabla, 0, false));
        assert_eq!(desc, "n\n100\n10\n9\n-2.5\n");
    }

    #[test]
    fn ordenar_texto_ignora_mayusculas_y_tildes_y_respeta_la_enie() {
        let texto = "nombre\nzoe\nÁngel\nñandú\nnube\nOscar\nbeto\n";
        let tabla = analizar(texto, b',').unwrap();
        let asc = aplicar(texto, ordenar_por_columna(texto, &tabla, 0, true));
        assert_eq!(columna(&asc, b',', 0), vec!["nombre", "Ángel", "beto", "nube", "ñandú", "Oscar", "zoe"]);
    }

    #[test]
    fn ordenar_deja_las_celdas_vacias_al_final_en_ambos_sentidos() {
        let texto = "k,v\na,2\nb,\nc,1\nd\n";
        let tabla = analizar(texto, b',').unwrap();
        let asc = aplicar(texto, ordenar_por_columna(texto, &tabla, 1, true));
        assert_eq!(columna(&asc, b',', 0), vec!["k", "c", "a", "b", "d"]);
        let desc = aplicar(texto, ordenar_por_columna(texto, &tabla, 1, false));
        assert_eq!(columna(&desc, b',', 0), vec!["k", "a", "c", "b", "d"]);
    }

    #[test]
    fn ordenar_es_estable_entre_valores_iguales() {
        let texto = "k,v\nx,1\na,0\ny,1\nb,0\nz,1\n";
        let tabla = analizar(texto, b',').unwrap();
        let asc = aplicar(texto, ordenar_por_columna(texto, &tabla, 1, true));
        assert_eq!(columna(&asc, b',', 0), vec!["k", "a", "b", "x", "y", "z"]);
        let desc = aplicar(texto, ordenar_por_columna(texto, &tabla, 1, false));
        assert_eq!(columna(&desc, b',', 0), vec!["k", "x", "y", "z", "a", "b"]);
    }

    #[test]
    fn ordenar_conserva_el_quoting_original_y_el_fin_de_archivo_sin_salto() {
        // La última fila no tiene `\n`: al moverla al medio no debe
        // pegarse con la siguiente, y el archivo sigue sin `\n` final.
        let texto = "ciudad,pais\n\"Lima, centro\",Perú\n\"México, \"\"DF\"\"\",México\nBogotá,Colombia";
        let tabla = analizar(texto, b',').unwrap();
        let asc = aplicar(texto, ordenar_por_columna(texto, &tabla, 0, true));
        assert_eq!(asc, "ciudad,pais\nBogotá,Colombia\n\"Lima, centro\",Perú\n\"México, \"\"DF\"\"\",México");
    }

    #[test]
    fn ordenar_no_produce_edicion_si_ya_esta_ordenado_o_no_hay_filas() {
        let texto = "n\n1\n2\n";
        let tabla = analizar(texto, b',').unwrap();
        assert!(ordenar_por_columna(texto, &tabla, 0, true).is_none());
        let solo_encabezado = analizar("n\n", b',').unwrap();
        assert!(ordenar_por_columna("n\n", &solo_encabezado, 0, true).is_none());
    }

    #[test]
    fn insertar_fila_en_medio_al_final_y_en_archivo_sin_salto_final() {
        let texto = "a,b\n\"x, y\",z\n";
        let tabla = analizar(texto, b',').unwrap();
        assert_eq!(aplicar(texto, insertar_fila(texto, &tabla, 1).ok()), "a,b\n,\n\"x, y\",z\n");
        assert_eq!(aplicar(texto, insertar_fila(texto, &tabla, 2).ok()), "a,b\n\"x, y\",z\n,\n");

        let sin_salto = "a,b\nc,d";
        let tabla = analizar(sin_salto, b',').unwrap();
        let resultado = aplicar(sin_salto, insertar_fila(sin_salto, &tabla, 2).ok());
        assert_eq!(resultado, "a,b\nc,d\n,");
        assert_eq!(analizar(&resultado, b',').unwrap().num_filas(), 3);
    }

    #[test]
    fn insertar_fila_de_una_columna_no_queda_como_linea_en_blanco() {
        let texto = "a\nb\n";
        let tabla = analizar(texto, b',').unwrap();
        let resultado = aplicar(texto, insertar_fila(texto, &tabla, 1).ok());
        assert_eq!(analizar(&resultado, b',').unwrap().num_filas(), 3);
    }

    #[test]
    fn eliminar_fila_conserva_el_resto_byte_a_byte() {
        let texto = "a,b\n\"x, y\",z\nc,d\n";
        let tabla = analizar(texto, b',').unwrap();
        assert_eq!(aplicar(texto, eliminar_fila(texto, &tabla, 1)), "a,b\nc,d\n");
        assert_eq!(aplicar(texto, eliminar_fila(texto, &tabla, 0)), "\"x, y\",z\nc,d\n");

        let sin_salto = "a,b\nc,d";
        let tabla = analizar(sin_salto, b',').unwrap();
        assert_eq!(aplicar(sin_salto, eliminar_fila(sin_salto, &tabla, 1)), "a,b");
        assert!(eliminar_fila(sin_salto, &tabla, 5).is_none());
    }

    #[test]
    fn insertar_y_eliminar_columna_reserializan_con_quoting_correcto() {
        let texto = "nombre,ciudad\n\"Ana, Pérez\",\"Bogotá \"\"centro\"\"\"\n";
        let tabla = analizar(texto, b',').unwrap();

        let con_columna = aplicar(texto, insertar_columna(texto, &tabla, 1).unwrap());
        assert_eq!(con_columna, "nombre,,ciudad\n\"Ana, Pérez\",,\"Bogotá \"\"centro\"\"\"\n");
        let reanalizada = analizar(&con_columna, b',').unwrap();
        assert_eq!(reanalizada.filas[1].celdas, vec!["Ana, Pérez", "", "Bogotá \"centro\""]);

        let sin_columna = aplicar(texto, eliminar_columna(texto, &tabla, 0).unwrap());
        assert_eq!(sin_columna, "ciudad\n\"Bogotá \"\"centro\"\"\"\n");
    }

    #[test]
    fn columnas_en_tsv_y_sin_salto_final() {
        let texto = "a\tb\nc, d\te";
        let tabla = analizar(texto, b'\t').unwrap();
        // En TSV la coma no obliga a citar.
        assert_eq!(aplicar(texto, insertar_columna(texto, &tabla, 2).unwrap()), "a\tb\t\nc, d\te\t");
        assert_eq!(aplicar(texto, eliminar_columna(texto, &tabla, 1).unwrap()), "a\nc, d");
    }

    #[test]
    fn eliminar_la_unica_columna_no_hace_nada() {
        let texto = "a\nb\n";
        let tabla = analizar(texto, b',').unwrap();
        assert!(eliminar_columna(texto, &tabla, 0).unwrap().is_none());
    }

    #[test]
    fn filas_visibles_sin_filtro_son_todas_y_con_filtro_siempre_incluyen_el_encabezado() {
        let texto = "ciudad,pais\nLima,Perú\n\"Ciudad de México\",México\nBogotá,Colombia\n";
        let tabla = analizar(texto, b',').unwrap();
        assert_eq!(filas_visibles(&tabla, None), vec![0, 1, 2, 3]);

        // Sin distinguir mayúsculas ni tildes: "MEXICO" encuentra "México".
        let filtro = FiltroCsv { columna: 1, texto: "MEXICO".to_string() };
        assert_eq!(filas_visibles(&tabla, Some(&filtro)), vec![0, 2]);

        let ninguna = FiltroCsv { columna: 0, texto: "zzz".to_string() };
        assert_eq!(filas_visibles(&tabla, Some(&ninguna)), vec![0]);
    }
}
