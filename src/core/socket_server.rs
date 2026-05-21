use crate::core::daemon;
use crate::payloads;

use log::error;
use smol::lock::Mutex;
use smol::{
    net::unix::{UnixListener, UnixStream},
    prelude::*,
    LocalExecutor,
};

use std::rc::Rc;

const MAX_REQUEST_SIZE: usize = 8 * 1024;

/// Reads a single newline-delimited message from the stream, returning the
/// bytes preceding the delimiter. Fails if the message exceeds the size cap.
async fn read_message(stream: &mut UnixStream) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut buf = Vec::new();
    let mut temp = [0u8; 512];

    loop {
        let n = stream.read(&mut temp).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&temp[..n]);

        if let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            buf.truncate(pos);
            return Ok(buf);
        }
        if buf.len() > MAX_REQUEST_SIZE {
            return Err("request exceeds maximum allowed size".into());
        }
    }

    if buf.is_empty() {
        return Err("connection closed before a request was received".into());
    }
    Ok(buf)
}

/// Applies a parsed request to the daemon and produces the response to send back.
fn dispatch<D>(request: payloads::Request, mut dmn: async_lock::MutexGuard<D>) -> payloads::Response
where
    D: daemon::Daemon + 'static,
{
    match request {
        payloads::Request::Status => {
            let status = dmn.status();
            payloads::Response::Status(payloads::StatusPayload {
                enabled: status.enabled,
                gamma: status.gamma,
                gamma_state: status.current_gamma_level.to_string(),
            })
        }
        payloads::Request::Set { gamma } => match dmn.set(gamma) {
            Ok(()) => payloads::Response::Ok {
                message: format!("Set gamma to {gamma}"),
            },
            Err(e) => payloads::Response::Error {
                message: format!("failed to set gamma: {e}"),
            },
        },
        payloads::Request::Enable => match dmn.enable() {
            Ok(()) => payloads::Response::Ok {
                message: "GammaDaemon is now enabled".to_string(),
            },
            Err(_) => payloads::Response::Error {
                message: "GammaDaemon is already enabled".to_string(),
            },
        },
        payloads::Request::Disable => match dmn.disable() {
            Ok(()) => payloads::Response::Ok {
                message: "GammaDaemon is now disabled".to_string(),
            },
            Err(_) => payloads::Response::Error {
                message: "GammaDaemon is already disabled".to_string(),
            },
        },
    }
}

/// Handles a single client connection: reads one JSON request, dispatches it
/// against the shared daemon, and writes one JSON response back.
async fn handle_connection<D>(
    mut stream: UnixStream,
    dmn_mutex: &Rc<Mutex<D>>,
) -> Result<(), Box<dyn std::error::Error>>
where
    D: daemon::Daemon + 'static,
{
    let raw = read_message(&mut stream).await?;

    let response = match serde_json::from_slice::<payloads::Request>(&raw) {
        Ok(request) => {
            let dmn = dmn_mutex.lock().await;
            dispatch(request, dmn)
        }
        Err(e) => payloads::Response::Error {
            message: format!("invalid request: {e}"),
        },
    };

    let mut bytes = serde_json::to_vec(&response)?;
    bytes.push(b'\n');

    stream.write_all(&bytes).await?;
    stream.flush().await?;
    stream.close().await?;
    Ok(())
}

/// Accepts incoming connections on the given Unix listener, spawning a
/// task per connection to handle the request against the shared daemon.
pub async fn listen_and_serve<D>(
    ex: &LocalExecutor<'_>,
    listener: UnixListener,
    dmn: Rc<Mutex<D>>,
) -> std::io::Result<()>
where
    D: daemon::Daemon + 'static,
{
    loop {
        let (stream, _) = listener.accept().await?;
        let dmn_clone = Rc::clone(&dmn);
        ex.spawn(async move {
            if let Err(e) = handle_connection(stream, &dmn_clone).await {
                error!("failed to handle connection: {e}");
            }
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::GammaDaemonConfig, core::daemon::GammaLevel};
    use async_lock::Mutex;
    use std::error::Error;

    pub struct MockDaemon {
        pub enabled: bool,
        pub level: GammaLevel,
        pub gamma: f32,
        pub fail_toggle: bool,
    }

    impl daemon::Daemon for MockDaemon {
        fn status(&mut self) -> daemon::Status {
            daemon::Status {
                enabled: self.enabled,
                gamma: self.gamma,
                current_gamma_level: self.level,
            }
        }

        fn set(&mut self, gamma: f32) -> Result<(), Box<dyn Error>> {
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

        fn tick(&mut self, _: battery::State, _: f32) -> Option<f32> {
            Some(67.0)
        }

        fn get_config(&mut self) -> crate::config::GammaDaemonConfig {
            GammaDaemonConfig::default()
        }
    }

    fn mock(enabled: bool, fail_toggle: bool) -> MockDaemon {
        MockDaemon {
            enabled,
            level: GammaLevel::Discharging,
            gamma: 1.0,
            fail_toggle,
        }
    }

    #[test]
    fn dispatch_status_returns_state() {
        smol::block_on(async {
            let mtx = Mutex::new(mock(true, false));
            let response = dispatch(payloads::Request::Status, mtx.lock().await);

            match response {
                payloads::Response::Status(status) => {
                    assert!(status.enabled);
                    assert_eq!(status.gamma, 1.0);
                }
                other => panic!("expected status response, got {other:?}"),
            }
        });
    }

    #[test]
    fn dispatch_set_updates_gamma() {
        smol::block_on(async {
            let mtx = Mutex::new(mock(true, false));
            let response = dispatch(payloads::Request::Set { gamma: 0.85 }, mtx.lock().await);

            assert!(matches!(response, payloads::Response::Ok { .. }));
            assert_eq!(mtx.lock().await.gamma, 0.85);
        });
    }

    #[test]
    fn dispatch_enable_success() {
        smol::block_on(async {
            let mtx = Mutex::new(mock(false, false));
            let response = dispatch(payloads::Request::Enable, mtx.lock().await);

            assert!(matches!(response, payloads::Response::Ok { .. }));
            assert!(mtx.lock().await.enabled);
        });
    }

    #[test]
    fn dispatch_enable_already_enabled() {
        smol::block_on(async {
            let mtx = Mutex::new(mock(true, true));
            let response = dispatch(payloads::Request::Enable, mtx.lock().await);

            match response {
                payloads::Response::Error { message } => {
                    assert!(message.contains("already enabled"))
                }
                other => panic!("expected error response, got {other:?}"),
            }
        });
    }

    #[test]
    fn dispatch_disable_success() {
        smol::block_on(async {
            let mtx = Mutex::new(mock(true, false));
            let response = dispatch(payloads::Request::Disable, mtx.lock().await);

            assert!(matches!(response, payloads::Response::Ok { .. }));
            assert!(!mtx.lock().await.enabled);
        });
    }

    #[test]
    fn dispatch_disable_already_disabled() {
        smol::block_on(async {
            let mtx = Mutex::new(mock(false, true));
            let response = dispatch(payloads::Request::Disable, mtx.lock().await);

            match response {
                payloads::Response::Error { message } => {
                    assert!(message.contains("already disabled"))
                }
                other => panic!("expected error response, got {other:?}"),
            }
        });
    }
}
