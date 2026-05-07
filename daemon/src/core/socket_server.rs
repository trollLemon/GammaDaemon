#![allow(dead_code)]

use crate::core::daemon;
use crate::http;
use gamma_lib::{constants, payloads};
use log::error;
use smol::lock::Mutex;
use smol::{
    net::unix::{UnixListener, UnixStream},
    prelude::*,
};

use std::sync::Arc;


/// Handles the `/status` endpoint: queries the daemon for its current status
/// and returns the JSON-encoded `StatusPayload` as the response body.
fn status_handler<D>(
    mut dmn: async_lock::MutexGuard<D>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>>
where
    D: daemon::Daemon + Send + Sync + 'static,
{
    let status = dmn.status()?;
    let status_json = payloads::StatusPayload {
        enabled: status.enabled,
        gamma: status.gamma,
        gamma_state: status.current_gamma_level.to_string(),
    };
    let resp_body: Vec<u8> = serde_json::to_vec(&status_json)?;

    Ok(resp_body)
}

/// Handles the `/set` endpoint: parses a `SetPayload` from the request body and
/// instructs the daemon to set the gamma value. Returns the response body and status code.
fn set_handler<D>(
    mut dmn: async_lock::MutexGuard<D>,
    body: &[u8],
) -> Result<(Vec<u8>, String), Box<dyn std::error::Error>>
where
    D: daemon::Daemon + Send + Sync + 'static,
{
    let payload: payloads::SetPayload = serde_json::from_slice(body)?;
    let status_code: String = "200 OK".to_string();

    //TODO: check for error returned if bad gama, then return 400.
    dmn.set(payload.gamma)?;
    let resp_body: Vec<u8> =
        Vec::from("Set gamma to ".to_owned() + payload.gamma.to_string().as_str());

    Ok((resp_body, status_code))
}

/// Handles the `/toggle` endpoint: parses a `TogglePayload` and either enables or
/// disables the daemon. Returns a 400 response if the action is unknown or already applied.
fn toggle_handler<D>(
    mut dmn: async_lock::MutexGuard<D>,
    body: &[u8],
) -> Result<(Vec<u8>, String), Box<dyn std::error::Error>>
where
    D: daemon::Daemon + Send + Sync + 'static,
{
    let payload: payloads::TogglePayload = serde_json::from_slice(body)?;
    let resp_body: Vec<u8>;
    let mut status_code: String = "200 OK".to_string();

    if payload.action == "enable" {
        match dmn.enable() {
            Err(_) => {
                status_code = "400 Bad request".to_string();
                let error_payload = http::new_http_error("400", "GammaDaemon is already enabled");
                resp_body = serde_json::to_vec(&error_payload)?;
            }
            _ => {
                resp_body = "GammaDaemon is now enabled".as_bytes().to_vec();
            }
        }
    } else if payload.action == "disable" {
        match dmn.disable() {
            Err(_) => {
                status_code = "400 Bad request".to_string();
                let error_payload = http::new_http_error("400", "GammaDaemon is already disabled");
                resp_body = serde_json::to_vec(&error_payload)?;
            }
            _ => {
                resp_body = "GammaDaemon is now disabled".as_bytes().to_vec();
            }
        }
    } else {
        status_code = "400 Bad request".to_string();
        let error_payload = http::new_http_error("400", "Unknown toggle action");
        resp_body = serde_json::to_vec(&error_payload)?;
    }

    Ok((resp_body, status_code))
}

/// Handles a single client connection: parses the HTTP request, dispatches to the
/// appropriate endpoint handler, and writes the HTTP response back to the stream.
async fn handle_connection<D>(
    mut stream: UnixStream,
    dmn_mutex: &Arc<Mutex<D>>,
) -> Result<(), Box<dyn std::error::Error>>
where
    D: daemon::Daemon + Send + Sync + 'static,
{
    let (header, body) = http::extract_http_request(&mut stream).await?;
    let status_line = header.lines().next().ok_or("No status line")?;
    let parts: Vec<&str> = status_line.split_whitespace().collect();
    // TODO: better error handling here
    let _method = parts.first().unwrap_or(&"GET").to_owned();
    let endpoint = parts.get(1).unwrap_or(&"/").to_owned();

    let dmn = dmn_mutex.lock().await;

    let mut resp_body: Vec<u8> = "".as_bytes().to_vec();
    let mut status_code: String = "200 OK".to_string();

    match endpoint {
        constants::ENDPOINT_STATUS => match status_handler(dmn) {
            Ok(status_resp_body) => {
                resp_body = status_resp_body;
            }
            Err(e) => {
                error!("{}", e);
                status_code = "500 Internal server error".to_string();
            }
        },
        constants::ENDPOINT_TOGGLE => match toggle_handler(dmn, &body) {
            Ok((toggle_resp_body, toggle_status_code)) => {
                resp_body = toggle_resp_body;
                status_code = toggle_status_code;
            }
            Err(e) => {
                error!("{}", e);
                status_code = "500 Internal server error".to_string();
            }
        },
        constants::ENDPOINT_SET_GAMMA => match set_handler(dmn, &body) {
            Ok((set_response_body, set_status_code)) => {
                resp_body = set_response_body;
                status_code = set_status_code;
            }
            Err(e) => {
                error!("{}", e);
                status_code = "500 Internal server error".to_string();
            }
        },
        _ => {
            status_code = "404 Not Found".to_string();
            let error_payload = http::new_http_error("404", "endpoint not found");
            resp_body = serde_json::to_vec(&error_payload)?;
        }
    }

    let response = format!(
        "HTTP/1.1 {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status_code,
        resp_body.len(),
    );

    stream.write_all(response.as_bytes()).await?;
    stream.write_all(resp_body.as_slice()).await?;
    Ok(())
}

/// Accepts incoming connections on the given Unix listener forever, spawning a
/// task per connection to handle the request against the shared daemon.
pub async fn listen_and_serve<D>(listener: UnixListener, dmn: Arc<Mutex<D>>) -> std::io::Result<()>
where
    D: daemon::Daemon + Send + Sync + 'static,
{
    loop {
        let (stream, _) = listener.accept().await?;
        let dmn_clone = Arc::clone(&dmn);
        smol::spawn(async move {
            handle_connection(stream, &dmn_clone)
                .await
                .expect("Failed to handle connection");
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::daemon::GammaLevel;
    use async_lock::Mutex;
    use std::error::Error;

    pub struct MockDaemon {
        pub enabled: bool,
        pub level: GammaLevel,
        pub gamma: u8,
        pub fail_toggle: bool,
    }

    impl daemon::Daemon for MockDaemon {
        fn status(&mut self) -> Result<daemon::Status, Box<dyn Error>> {
            Ok(daemon::Status {
                enabled: self.enabled,
                gamma: self.gamma,
                current_gamma_level: self.level,
            })
        }

        fn set(&mut self, gamma: u8) -> Result<(), Box<dyn Error>> {
            self.gamma = gamma;
            Ok(())
        }

        fn enable(&mut self) -> Result<(), Box<dyn Error>> {
            if self.fail_toggle {
                return Err("Error".into());
            }
            self.enabled = true;
            Ok(())
        }

        fn disable(&mut self) -> Result<(), Box<dyn Error>> {
            if self.fail_toggle {
                return Err("Error".into());
            }
            self.enabled = false;
            Ok(())
        }
    }

    #[test]
    fn test_status_handler() {
        smol::block_on(async {
            let dmn = MockDaemon {
                enabled: true,
                level: GammaLevel::Discharging,
                gamma: 100,
                fail_toggle: false,
            };
            let mtx = Mutex::new(dmn);
            let guard = mtx.lock().await;

            let result = status_handler(guard).unwrap();
            let body: payloads::StatusPayload = serde_json::from_slice(&result).unwrap();

            assert!(body.enabled);
            assert_eq!(body.gamma, 100);
        });
    }

    #[test]
    fn test_set_handler() {
        smol::block_on(async {
            let dmn = MockDaemon {
                enabled: true,
                level: GammaLevel::Discharging,
                gamma: 100,
                fail_toggle: false,
            };
            let mtx = Mutex::new(dmn);
            let guard = mtx.lock().await;

            let input = serde_json::to_vec(&payloads::SetPayload { gamma: 85 }).unwrap();
            let (resp, code) = set_handler(guard, &input).unwrap();

            assert_eq!(code, "200 OK");
            assert!(String::from_utf8(resp).unwrap().contains("85"));

            let final_guard = mtx.lock().await;
            assert_eq!(final_guard.gamma, 85);
        });
    }

    #[test]
    fn test_toggle_handler_enable_success() {
        smol::block_on(async {
            let dmn = MockDaemon {
                enabled: false,
                level: GammaLevel::Discharging,
                gamma: 100,
                fail_toggle: false,
            };
            let mtx = Mutex::new(dmn);
            let guard = mtx.lock().await;

            let input = serde_json::to_vec(&payloads::TogglePayload {
                action: "enable".to_string(),
            })
            .unwrap();
            let (resp, code) = toggle_handler(guard, &input).unwrap();

            assert_eq!(code, "200 OK");
            assert_eq!(
                String::from_utf8(resp).unwrap(),
                "GammaDaemon is now enabled"
            );

            let final_guard = mtx.lock().await;
            assert!(final_guard.enabled);
        });
    }

    #[test]
    fn test_toggle_handler_fail_already_enabled() {
        smol::block_on(async {
            let dmn = MockDaemon {
                enabled: true,
                level: GammaLevel::Discharging,
                gamma: 100,
                fail_toggle: true,
            };
            let mtx = Mutex::new(dmn);
            let guard = mtx.lock().await;

            let input = serde_json::to_vec(&payloads::TogglePayload {
                action: "enable".to_string(),
            })
            .unwrap();
            let (resp, code) = toggle_handler(guard, &input).unwrap();

            assert_eq!(code, "400 Bad request");
            assert!(String::from_utf8(resp).unwrap().contains("already enabled"));
        });
    }

    #[test]
    fn test_toggle_handler_invalid_action() {
        smol::block_on(async {
            let dmn = MockDaemon {
                enabled: true,
                level: GammaLevel::Discharging,
                gamma: 100,
                fail_toggle: false,
            };
            let mtx = Mutex::new(dmn);
            let guard = mtx.lock().await;

            let input = serde_json::to_vec(&payloads::TogglePayload {
                action: "invalid".to_string(),
            })
            .unwrap();
            let (resp, code) = toggle_handler(guard, &input).unwrap();

            assert_eq!(code, "400 Bad request");
            assert!(String::from_utf8(resp)
                .unwrap()
                .contains("Unknown toggle action"));
        });
    }
}
