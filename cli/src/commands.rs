use gamma_lib::constants;
use gamma_lib::payloads;
use std::error::Error;
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;

/// Sends a single JSON request to the daemon and returns its JSON response.
fn send_request(
    socket_path: &str,
    request: &payloads::Request,
) -> Result<payloads::Response, Box<dyn Error>> {
    let mut stream = UnixStream::connect(socket_path)?;

    let mut bytes = serde_json::to_vec(request)?;
    bytes.push(b'\n');
    stream.write_all(&bytes)?;
    stream.shutdown(Shutdown::Write)?;

    let mut buffer = Vec::new();
    stream.read_to_end(&mut buffer)?;

    let response = serde_json::from_slice::<payloads::Response>(&buffer)?;
    Ok(response)
}

pub fn set_gamma(value: f32) -> Result<String, Box<dyn Error>> {
    set_gamma_on_socket(constants::DEFAULT_SOCKET_PATH, value)
}

fn set_gamma_on_socket(socket_path: &str, value: f32) -> Result<String, Box<dyn Error>> {
    match send_request(socket_path, &payloads::Request::Set { gamma: value })? {
        payloads::Response::Ok { .. } => Ok("Gamma set successfully".to_string()),
        payloads::Response::Error { message } => {
            Err(format!("Failed to set gamma: {message}").into())
        }
        payloads::Response::Status(_) => {
            Err("Failed to set gamma: received unexpected response from daemon".into())
        }
    }
}

pub fn enable_gamma_daemon(socket_path: &str) -> Result<String, Box<dyn Error>> {
    match send_request(socket_path, &payloads::Request::Enable)? {
        payloads::Response::Ok { .. } => Ok("GammaDaemon is enabled".to_string()),
        payloads::Response::Error { message } => {
            Err(format!("Failed to enable GammaDaemon: {message}").into())
        }
        payloads::Response::Status(_) => {
            Err("Failed to enable GammaDaemon: received unexpected response from daemon".into())
        }
    }
}

pub fn disable_gamma_daemon(socket_path: &str) -> Result<String, Box<dyn Error>> {
    match send_request(socket_path, &payloads::Request::Disable)? {
        payloads::Response::Ok { .. } => Ok("GammaDaemon is now disabled".to_string()),
        payloads::Response::Error { message } => {
            Err(format!("Failed to disable GammaDaemon: {message}").into())
        }
        payloads::Response::Status(_) => {
            Err("Failed to disable GammaDaemon: received unexpected response from daemon".into())
        }
    }
}

pub fn show_status(socket_path: &str) -> Result<String, Box<dyn Error>> {
    match send_request(socket_path, &payloads::Request::Status)? {
        payloads::Response::Status(status) => {
            let is_enabled = if status.enabled { "Yes" } else { "No" };
            Ok(format!(
                "Enabled: {}\nGamma State: {}\n",
                is_enabled, status.gamma_state
            ))
        }
        payloads::Response::Error { message } => {
            Err(format!("Failed to get status: {message}").into())
        }
        payloads::Response::Ok { .. } => {
            Err("Failed to get status: received unexpected response from daemon".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::{
        disable_gamma_daemon, enable_gamma_daemon, set_gamma_on_socket, show_status,
    };
    use gamma_lib::payloads;
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

    /// Spawns a one-shot Unix server that replies to the next connection with
    /// the given response, encoded as a newline-delimited JSON message.
    fn spawn_temp_unix_server(
        response: payloads::Response,
    ) -> Result<TestUnixServer, Box<dyn Error>> {
        let dir = tempfile::tempdir().expect("tempdir");
        let socket_path = dir.path().join("socket");
        let mut body = serde_json::to_vec(&response).expect("response must serialize to JSON");
        body.push(b'\n');
        let listener = UnixListener::bind(&socket_path)?;

        thread::spawn(move || {
            if let Ok((mut stream, _addr)) = listener.accept() {
                let mut buf = Vec::new();
                let _ = stream.read_to_end(&mut buf);
                let _ = stream.write_all(&body);
            }
        });

        Ok(TestUnixServer {
            _dir: dir,
            socket_path,
        })
    }

    #[test]
    fn test_show_status_enabled() {
        let server = spawn_temp_unix_server(payloads::Response::Status(payloads::StatusPayload {
            enabled: true,
            gamma_state: "state".to_string(),
            gamma: 0.85,
        }))
        .unwrap();

        let rslt = show_status(server.socket_path.to_str().unwrap());
        assert!(rslt.is_ok());
        assert_eq!(rslt.unwrap(), "Enabled: Yes\nGamma State: state\n");
    }

    #[test]
    fn test_show_status_disabled() {
        let server = spawn_temp_unix_server(payloads::Response::Status(payloads::StatusPayload {
            enabled: false,
            gamma_state: "state".to_string(),
            gamma: 0.0,
        }))
        .unwrap();

        let rslt = show_status(server.socket_path.to_str().unwrap());
        assert!(rslt.is_ok());
        assert_eq!(rslt.unwrap(), "Enabled: No\nGamma State: state\n");
    }

    #[test]
    fn test_show_status_bad_socket() {
        let rslt = show_status("a.socket");
        assert!(rslt.is_err());
        assert!(rslt
            .unwrap_err()
            .to_string()
            .contains("No such file or directory"));
    }

    #[test]
    fn test_show_status_error_response() {
        let server = spawn_temp_unix_server(payloads::Response::Error {
            message: "a bad request happened".to_string(),
        })
        .unwrap();

        let rslt = show_status(server.socket_path.to_str().unwrap());
        assert!(rslt.is_err());
        let err = rslt.unwrap_err().to_string();
        assert!(err.contains("Failed to get status") && err.contains("bad request"));
    }

    #[test]
    fn test_enable_gamma_daemon_ok() {
        let server = spawn_temp_unix_server(payloads::Response::Ok {
            message: "GammaDaemon is now enabled".to_string(),
        })
        .unwrap();

        let rslt = enable_gamma_daemon(server.socket_path.to_str().unwrap());
        assert!(rslt.is_ok());
        assert_eq!(rslt.unwrap(), "GammaDaemon is enabled");
    }

    #[test]
    fn test_enable_gamma_daemon_bad_socket() {
        let rslt = enable_gamma_daemon("a.socket");
        assert!(rslt.is_err());
        assert!(rslt
            .unwrap_err()
            .to_string()
            .contains("No such file or directory"));
    }

    #[test]
    fn test_enable_gamma_daemon_error_response() {
        let server = spawn_temp_unix_server(payloads::Response::Error {
            message: "a bad request happened".to_string(),
        })
        .unwrap();

        let rslt = enable_gamma_daemon(server.socket_path.to_str().unwrap());
        assert!(rslt.is_err());
        let err = rslt.unwrap_err().to_string();
        assert!(err.contains("Failed to enable GammaDaemon") && err.contains("bad request"));
    }

    #[test]
    fn test_disable_gamma_daemon_ok() {
        let server = spawn_temp_unix_server(payloads::Response::Ok {
            message: "GammaDaemon is now disabled".to_string(),
        })
        .unwrap();

        let rslt = disable_gamma_daemon(server.socket_path.to_str().unwrap());
        assert!(rslt.is_ok());
        assert_eq!(rslt.unwrap(), "GammaDaemon is now disabled");
    }

    #[test]
    fn test_disable_gamma_daemon_bad_socket() {
        let rslt = disable_gamma_daemon("a.socket");
        assert!(rslt.is_err());
        assert!(rslt
            .unwrap_err()
            .to_string()
            .contains("No such file or directory"));
    }

    #[test]
    fn test_disable_gamma_daemon_error_response() {
        let server = spawn_temp_unix_server(payloads::Response::Error {
            message: "a bad request happened".to_string(),
        })
        .unwrap();

        let rslt = disable_gamma_daemon(server.socket_path.to_str().unwrap());
        assert!(rslt.is_err());
        let err = rslt.unwrap_err().to_string();
        assert!(err.contains("Failed to disable GammaDaemon") && err.contains("bad request"));
    }

    #[test]
    fn test_set_gamma_ok() {
        let server = spawn_temp_unix_server(payloads::Response::Ok {
            message: "Set gamma to 0.5".to_string(),
        })
        .unwrap();

        let rslt = set_gamma_on_socket(server.socket_path.to_str().unwrap(), 0.5);
        assert!(rslt.is_ok());
        assert_eq!(rslt.unwrap(), "Gamma set successfully");
    }

    #[test]
    fn test_set_gamma_bad_socket() {
        let rslt = set_gamma_on_socket("a.socket", 0.5);
        assert!(rslt.is_err());
        assert!(rslt
            .unwrap_err()
            .to_string()
            .contains("No such file or directory"));
    }

    #[test]
    fn test_set_gamma_error_response() {
        let server = spawn_temp_unix_server(payloads::Response::Error {
            message: "a bad request happened".to_string(),
        })
        .unwrap();

        let rslt = set_gamma_on_socket(server.socket_path.to_str().unwrap(), 0.5);
        assert!(rslt.is_err());
        let err = rslt.unwrap_err().to_string();
        assert!(err.contains("Failed to set gamma") && err.contains("bad request"));
    }
}
