use gamma_lib::constants;
use gamma_lib::payloads;
use std::error::Error;
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;

fn send_unix_http_request(
    socket_path: &str,
    method: &str,
    endpoint: &str,
    payload: &[u8],
) -> Result<(u16, Vec<u8>), Box<dyn Error>> {
    let mut stream = UnixStream::connect(socket_path)?;
    let request = format!(
        "{} {} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
        method,
        endpoint,
        payload.len()
    );
    stream.write_all(request.as_bytes())?;
    stream.write_all(payload)?;
    stream.shutdown(Shutdown::Write)?;
    let mut buffer = Vec::new();
    stream.read_to_end(&mut buffer)?;

    let header_end = buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or("Invalid HTTP response: missing header separator")?;

    let header_bytes = &buffer[..header_end];
    let body = buffer[(header_end + 4)..].to_vec();

    let status_line = header_bytes
        .split(|&byte| byte == b'\n')
        .next()
        .ok_or("Invalid HTTP response: missing status line")?;
    let status_line = std::str::from_utf8(status_line)?.trim_end_matches('\r');
    let mut parts = status_line.split_whitespace();
    let _http_version = parts
        .next()
        .ok_or("Invalid HTTP response: missing HTTP version")?;
    let status = parts
        .next()
        .ok_or("Invalid HTTP response: missing status code")?
        .parse::<u16>()?;

    Ok((status, body))
}

pub fn set_gamma(value: f32) -> Result<String, Box<dyn Error>> {
    set_gamma_on_socket(constants::DEFAULT_SOCKET_PATH, value)
}

fn set_gamma_on_socket(socket_path: &str, value: f32) -> Result<String, Box<dyn Error>> {
    let payload = payloads::SetPayload { gamma: value };
    let bytes = serde_json::to_vec(&payload)?;
    let (status, body) =
        send_unix_http_request(socket_path, "POST", constants::ENDPOINT_SET_GAMMA, &bytes)?;

    match status {
        200 => Ok("Gamma set successfully".to_string()),
        400 => {
            let payload: payloads::ErrorPayload = serde_json::from_slice(&body)?;
            Err(format!("Failed to set gamma: {}", payload.message).into())
        }
        _ => Err("Failed to set gamma: received unexpected response from daemon".into()),
    }
}

pub fn enable_gamma_daemon(socket_path: &str) -> Result<String, Box<dyn Error>> {
    let payload = payloads::TogglePayload {
        action: "enable".to_string(),
    };

    let bytes = serde_json::to_vec(&payload)?;

    let (status, body) =
        send_unix_http_request(socket_path, "POST", constants::ENDPOINT_TOGGLE, &bytes)?;

    match status {
        200 => Ok("GammaDaemon is enabled".to_string()),
        400 => {
            let payload: payloads::ErrorPayload = serde_json::from_slice(&body)?;
            Err(format!("Failed to enable GammaDaemon: {}", payload.message).into())
        }
        _ => Err("Failed to enable GammaDaemon: received unexpected response from daemon".into()),
    }
}

pub fn disable_gamma_daemon(socket_path: &str) -> Result<String, Box<dyn Error>> {
    let payload = payloads::TogglePayload {
        action: "disable".to_string(),
    };

    let bytes = serde_json::to_vec(&payload)?;

    let (status, body) =
        send_unix_http_request(socket_path, "POST", constants::ENDPOINT_TOGGLE, &bytes)?;

    match status {
        200 => Ok("GammaDaemon is now disabled".to_string()),
        400 => {
            let payload: payloads::ErrorPayload = serde_json::from_slice(&body)?;
            Err(format!("Failed to disable GammaDaemon: {}", payload.message).into())
        }
        _ => Err("Failed to disable GammaDaemon: received unexpected response from daemon".into()),
    }
}

pub fn show_status(socket_path: &str) -> Result<String, Box<dyn Error>> {
    let payload: [u8; 0] = [];
    let (status, body) =
        send_unix_http_request(socket_path, "GET", constants::ENDPOINT_STATUS, &payload)?;

    match status {
        200 => {
            let payload: payloads::StatusPayload = serde_json::from_slice(&body)?;

            let mut is_enabled = "Yes";
            if !payload.enabled {
                is_enabled = "No"
            }
            let result: String = format!(
                "Enabled: {}\nGamma State: {}\n",
                is_enabled, payload.gamma_state
            );
            Ok(result)
        }
        400 => {
            let payload: payloads::ErrorPayload = serde_json::from_slice(&body)?;
            Err(format!("Failed to get status: {}", payload.message).into())
        }
        _ => Err("Failed to get status: received unexpected response from daemon".into()),
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::{
        disable_gamma_daemon, enable_gamma_daemon, set_gamma_on_socket, show_status,
    };
    use gamma_lib::payloads;
    use serde::Serialize;
    use std::error::Error;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;
    use std::thread;
    use tempfile::TempDir;

    struct TestUnixServer {
        _dir: TempDir,
        socket_path: PathBuf,
    }

    fn spawn_temp_unix_server<T>(
        payload: T,
        expected_status: u16,
    ) -> Result<TestUnixServer, Box<dyn Error>>
    where
        T: Serialize + Send + 'static,
    {
        let dir = tempfile::tempdir().expect("tempdir");
        let socket_path = dir.path().join("socket");
        let body = serde_json::to_vec(&payload).expect("payload must serialize to JSON");
        let listener = UnixListener::bind(&socket_path)?;

        thread::spawn(move || {
            if let Ok((mut unix_stream, _socket_addr)) = listener.accept() {
                let mut buf = Vec::new();
                let _ = unix_stream.read_to_end(&mut buf);

                match expected_status {
                    200 => {
                        let headers = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                            body.len()
                        );

                        let _ = unix_stream.write_all(headers.as_bytes());
                        let _ = unix_stream.write_all(&body);
                    }
                    400 => {
                        let error_payload = payloads::ErrorPayload {
                            status: "400".to_string(),
                            message: "a bad request happened".to_string(),
                        };
                        let body = serde_json::to_vec(&error_payload)
                            .expect("payload must serialize to JSON");

                        let headers = format!(
                            "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                            body.len()
                        );

                        let _ = unix_stream.write_all(headers.as_bytes());
                        let _ = unix_stream.write_all(&body);
                    }
                    _ => {}
                }
            }
        });

        Ok(TestUnixServer {
            _dir: dir,
            socket_path,
        })
    }

    #[test]
    fn test_show_status_enabled() {
        let payload = payloads::StatusPayload {
            enabled: true,
            gamma_state: "state".to_string(),
            gamma: 0.85,
        };

        let server = spawn_temp_unix_server(payload, 200);

        assert!(server.is_ok());

        let rslt = show_status(server.unwrap().socket_path.to_str().unwrap());
        assert!(rslt.is_ok());
        let actual_payload = rslt.unwrap();
        assert_eq!(actual_payload, "Enabled: Yes\nGamma State: state\n");
    }

    #[test]
    fn test_show_status_disabled() {
        let payload = payloads::StatusPayload {
            enabled: false,
            gamma_state: "state".to_string(),
            gamma: 0.0,
        };

        let server = spawn_temp_unix_server(payload, 200);

        assert!(server.is_ok());

        let rslt = show_status(server.unwrap().socket_path.to_str().unwrap());
        assert!(rslt.is_ok());
        let actual_payload = rslt.unwrap();
        assert_eq!(actual_payload, "Enabled: No\nGamma State: state\n");
    }

    #[test]
    fn test_show_status_bad_socket() {
        let rslt = show_status("a.socket");
        assert!(rslt.is_err());
        let err = rslt.unwrap_err();

        assert!(err.to_string().contains("No such file or directory"));
    }

    #[test]
    fn test_show_status_bad_request() {
        let payload = payloads::StatusPayload {
            enabled: false,
            gamma_state: "state".to_string(),
            gamma: 0.85,
        };

        let server = spawn_temp_unix_server(payload, 400);

        assert!(server.is_ok());

        let rslt = show_status(server.unwrap().socket_path.to_str().unwrap());
        assert!(rslt.is_err());
        let err = rslt.unwrap_err();

        assert!(
            err.to_string().contains("Failed to get status")
                && err.to_string().contains("bad request")
        );
    }

    #[test]
    fn test_enable_gamma_daemon_ok() {
        let payload = payloads::TogglePayload {
            action: "enable".to_string(),
        };

        let server = spawn_temp_unix_server(payload, 200);

        assert!(server.is_ok());

        let rslt = enable_gamma_daemon(server.unwrap().socket_path.to_str().unwrap());
        assert!(rslt.is_ok());
        let actual = rslt.unwrap();
        assert_eq!(actual, "GammaDaemon is enabled");
    }

    #[test]
    fn test_enable_gamma_daemon_bad_socket() {
        let rslt = enable_gamma_daemon("a.socket");
        assert!(rslt.is_err());
        let err = rslt.unwrap_err();

        assert!(err.to_string().contains("No such file or directory"));
    }

    #[test]
    fn test_enable_gamma_daemon_bad_request() {
        let payload = payloads::TogglePayload {
            action: "enable".to_string(),
        };

        let server = spawn_temp_unix_server(payload, 400);

        assert!(server.is_ok());

        let rslt = enable_gamma_daemon(server.unwrap().socket_path.to_str().unwrap());
        assert!(rslt.is_err());
        let err = rslt.unwrap_err();

        assert!(
            err.to_string().contains("Failed to enable GammaDaemon")
                && err.to_string().contains("bad request")
        );
    }

    #[test]
    fn test_disable_gamma_daemon_ok() {
        let payload = payloads::TogglePayload {
            action: "disable".to_string(),
        };

        let server = spawn_temp_unix_server(payload, 200);

        assert!(server.is_ok());

        let rslt = disable_gamma_daemon(server.unwrap().socket_path.to_str().unwrap());
        assert!(rslt.is_ok());
        let actual = rslt.unwrap();
        assert_eq!(actual, "GammaDaemon is now disabled");
    }

    #[test]
    fn test_disable_gamma_daemon_bad_socket() {
        let rslt = disable_gamma_daemon("a.socket");
        assert!(rslt.is_err());
        let err = rslt.unwrap_err();

        assert!(err.to_string().contains("No such file or directory"));
    }

    #[test]
    fn test_disable_gamma_daemon_bad_request() {
        let payload = payloads::TogglePayload {
            action: "disable".to_string(),
        };

        let server = spawn_temp_unix_server(payload, 400);

        assert!(server.is_ok());

        let rslt = disable_gamma_daemon(server.unwrap().socket_path.to_str().unwrap());
        assert!(rslt.is_err());
        let err = rslt.unwrap_err();

        assert!(
            err.to_string().contains("Failed to disable GammaDaemon")
                && err.to_string().contains("bad request")
        );
    }

    #[test]
    fn test_set_gamma_ok() {
        let payload = payloads::SetPayload { gamma: 0.5 };

        let server = spawn_temp_unix_server(payload, 200);

        assert!(server.is_ok());

        let rslt = set_gamma_on_socket(server.unwrap().socket_path.to_str().unwrap(), 0.5);
        assert!(rslt.is_ok());
        let actual = rslt.unwrap();
        assert_eq!(actual, "Gamma set successfully");
    }

    #[test]
    fn test_set_gamma_bad_socket() {
        let rslt = set_gamma_on_socket("a.socket", 0.5);
        assert!(rslt.is_err());
        let err = rslt.unwrap_err();

        assert!(err.to_string().contains("No such file or directory"));
    }

    #[test]
    fn test_set_gamma_bad_request() {
        let payload = payloads::SetPayload { gamma: 0.5 };

        let server = spawn_temp_unix_server(payload, 400);

        assert!(server.is_ok());

        let rslt = set_gamma_on_socket(server.unwrap().socket_path.to_str().unwrap(), 0.5);
        assert!(rslt.is_err());
        let err = rslt.unwrap_err();

        assert!(
            err.to_string().contains("Failed to set gamma")
                && err.to_string().contains("bad request")
        );
    }
}
