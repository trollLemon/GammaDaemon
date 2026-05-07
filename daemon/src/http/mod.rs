/// Basic HTTP Module
use gamma_lib::payloads;
use smol::{net::unix::UnixStream, prelude::*};
use std::error::Error;

/// Reads an HTTP request from the given Unix stream and splits it into the header
/// string and body bytes. Honors `Content-Length` to know how many body bytes to read.
pub async fn extract_http_request(
    stream: &mut UnixStream,
) -> Result<(String, Vec<u8>), Box<dyn Error>> {
    let mut buffer = Vec::new();
    let mut temp = [0u8; 512];

    let header_end = loop {
        let n = stream.read(&mut temp).await?;
        if n == 0 {
            return Err("Connection closed before headers finished".into());
        }
        buffer.extend_from_slice(&temp[..n]);
        if let Some(pos) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos;
        }
    };

    let header_bytes = &buffer[..header_end];
    let leftover_body = &buffer[(header_end + 4)..];

    let header_str = std::str::from_utf8(header_bytes)?.to_string();
    let mut content_length = 0;
    for line in header_str.lines() {
        if line.to_lowercase().starts_with("content-length:") {
            content_length = line
                .split(':')
                .nth(1)
                .unwrap_or("0")
                .trim()
                .parse::<usize>()?;
        }
    }

    let mut body = leftover_body.to_vec();
    while body.len() < content_length {
        let mut body_temp = [0u8; 512];
        let n = stream.read(&mut body_temp).await?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&body_temp[..n]);
    }

    Ok((header_str, body))
}

/// Builds an `ErrorPayload` with the provided status string and human-readable message,
/// suitable for serializing as the JSON body of an HTTP error response.
pub fn new_http_error(status: &str, message: &str) -> payloads::ErrorPayload {
    payloads::ErrorPayload {
        status: status.to_string(),
        message: message.to_string(),
    }
}
