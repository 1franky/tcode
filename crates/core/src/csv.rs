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
}
