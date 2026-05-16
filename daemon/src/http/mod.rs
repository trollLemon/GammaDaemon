/// Basic HTTP Module
use gamma_lib::payloads;
use smol::{net::unix::UnixStream, prelude::*};
use std::error::Error;

pub struct HttpRequest {
    pub method: String,
    pub path: String,
    pub body: Vec<u8>,
}

/// Reads an HTTP request from the given Unix stream,
pub async fn extract_http_request(
    stream: &mut UnixStream,
) -> Result<HttpRequest, Box<dyn Error>> {
    let mut buffer = Vec::new();
    let mut temp = [0u8; 512];

    let (header_len, content_length, method, path) = loop {
        let n = stream.read(&mut temp).await?;
        if n == 0 {
            return Err("Connection closed before headers finished".into());
        }
        buffer.extend_from_slice(&temp[..n]);

        let mut headers = [httparse::EMPTY_HEADER; 32];
        let mut req = httparse::Request::new(&mut headers);
        match req.parse(&buffer)? {
            httparse::Status::Complete(len) => {
                let mut content_length: usize = 0;
                for h in req.headers.iter() {
                    if h.name.eq_ignore_ascii_case("content-length") {
                        content_length = std::str::from_utf8(h.value)?.trim().parse::<usize>()?;
                        break;
                    }
                }
                let method = req.method.unwrap_or("GET").to_string();
                let path = req.path.unwrap_or("/").to_string();


                break (len, content_length, method, path);
            }
            httparse::Status::Partial => continue,
        }
    };

    let mut body = buffer[header_len..].to_vec();
    while body.len() < content_length {
        let mut body_temp = [0u8; 512];
        let n = stream.read(&mut body_temp).await?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&body_temp[..n]);
    }

    Ok(HttpRequest {
        method,
        path,
        body,
    })
}

/// Builds an `ErrorPayload` with the provided status string and human-readable message,
/// suitable for serializing as the JSON body of an HTTP error response.
pub fn new_http_error(status: &str, message: &str) -> payloads::ErrorPayload {
    payloads::ErrorPayload {
        status: status.to_string(),
        message: message.to_string(),
    }
}
