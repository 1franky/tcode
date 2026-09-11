use anyhow::{bail, Context, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

/// Envía un mensaje JSON-RPC por `writer`, con el framing `Content-Length`
/// que exige LSP (<https://microsoft.github.io/language-server-protocol/>).
pub async fn escribir_mensaje(writer: &mut (impl AsyncWriteExt + Unpin), valor: &Value) -> Result<()> {
    let cuerpo = serde_json::to_vec(valor)?;
    let encabezado = format!("Content-Length: {}\r\n\r\n", cuerpo.len());
    writer.write_all(encabezado.as_bytes()).await?;
    writer.write_all(&cuerpo).await?;
    writer.flush().await?;
    Ok(())
}

/// Lee un mensaje JSON-RPC de `reader`, parseando el encabezado
/// `Content-Length` (otros encabezados, como `Content-Type`, se ignoran).
pub async fn leer_mensaje(reader: &mut (impl AsyncBufReadExt + Unpin)) -> Result<Value> {
    let mut content_length: Option<usize> = None;
    loop {
        let mut linea = String::new();
        let bytes = reader.read_line(&mut linea).await?;
        if bytes == 0 {
            bail!("el servidor LSP cerró la conexión");
        }
        let linea = linea.trim_end();
        if linea.is_empty() {
            break;
        }
        if let Some(valor) = linea.strip_prefix("Content-Length:") {
            content_length = Some(valor.trim().parse().context("Content-Length inválido")?);
        }
    }

    let content_length = content_length.context("falta el encabezado Content-Length")?;
    let mut buffer = vec![0u8; content_length];
    reader.read_exact(&mut buffer).await?;
    serde_json::from_slice(&buffer).context("el cuerpo del mensaje LSP no es JSON válido")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn escribir_y_leer_un_mensaje_hacen_round_trip() {
        let (mut escritor, lector) = tokio::io::duplex(4096);
        let mut lector = BufReader::new(lector);

        let mensaje = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "processId": null } });
        escribir_mensaje(&mut escritor, &mensaje).await.unwrap();

        let leido = leer_mensaje(&mut lector).await.unwrap();
        assert_eq!(leido, mensaje);
    }

    #[tokio::test]
    async fn escribe_dos_mensajes_seguidos_y_los_lee_en_orden() {
        let (mut escritor, lector) = tokio::io::duplex(4096);
        let mut lector = BufReader::new(lector);

        let primero = json!({ "jsonrpc": "2.0", "method": "a" });
        let segundo = json!({ "jsonrpc": "2.0", "method": "b" });
        escribir_mensaje(&mut escritor, &primero).await.unwrap();
        escribir_mensaje(&mut escritor, &segundo).await.unwrap();

        assert_eq!(leer_mensaje(&mut lector).await.unwrap(), primero);
        assert_eq!(leer_mensaje(&mut lector).await.unwrap(), segundo);
    }

    #[tokio::test]
    async fn leer_sin_content_length_falla() {
        let (mut escritor, lector) = tokio::io::duplex(4096);
        let mut lector = BufReader::new(lector);

        escritor.write_all(b"Content-Type: application/json\r\n\r\n{}").await.unwrap();
        drop(escritor);

        assert!(leer_mensaje(&mut lector).await.is_err());
    }
}
