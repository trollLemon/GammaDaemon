use async_lock::Mutex;
use gamma_daemon::config::GammaDaemonConfig;
use gamma_daemon::core::daemon::{GammaDaemon, GammaLevel};
use gamma_daemon::core::socket_server::listen_and_serve;
use gamma_lib::payloads;
use smol::net::unix::{UnixListener, UnixStream};
use smol::prelude::*;
use smol::LocalExecutor;
use std::path::PathBuf;
use std::rc::Rc;

fn temp_socket_path(tag: &str) -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("gamma-daemon-it-{tag}-{pid}-{nanos}.sock"))
}

fn split_response(response: &[u8]) -> (String, Vec<u8>) {
    let header_end = response
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("response missing header/body delimiter");
    let header = std::str::from_utf8(&response[..header_end])
        .expect("non-utf8 header")
        .to_string();
    let status_line = header
        .lines()
        .next()
        .expect("response missing status line")
        .to_string();
    let body = response[(header_end + 4)..].to_vec();
    (status_line, body)
}

async fn run_request(
    local_ex: &LocalExecutor<'_>,
    socket_path: &PathBuf,
    dmn: GammaDaemon,
    request: &[u8],
) -> Vec<u8> {
    let _ = std::fs::remove_file(socket_path);
    let listener =
        UnixListener::bind(socket_path).expect("failed to bind test unix socket");

    let dmn_mtx = Rc::new(Mutex::new(dmn));
    let server = listen_and_serve(local_ex, listener, Rc::clone(&dmn_mtx));

    let client = async {
        let mut stream = UnixStream::connect(socket_path)
            .await
            .expect("client failed to connect");
        stream.write_all(request).await.expect("write request");
        stream.flush().await.expect("flush request");

        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.expect("read response");
        buf
    };

    smol::future::or(client, async {
        let _ = server.await;
        unreachable!("listen_and_serve should not return during the test")
    })
    .await
}

#[test]
fn status_endpoint_returns_current_state() {
    let local_ex = LocalExecutor::new();
    let socket_path = temp_socket_path("status-ok");

    let response = smol::block_on(local_ex.run(async {
        let dmn = GammaDaemon::new(GammaDaemonConfig::default());
        run_request(
            &local_ex,
            &socket_path,
            dmn,
            b"GET /status HTTP/1.1\r\nContent-Length: 0\r\n\r\n",
        )
        .await
    }));

    let _ = std::fs::remove_file(&socket_path);

    let (status_line, body) = split_response(&response);
    assert_eq!(status_line, "HTTP/1.1 200 OK");

    let payload: payloads::StatusPayload =
        serde_json::from_slice(&body).expect("body should be valid StatusPayload JSON");
    assert!(payload.enabled);
    assert_eq!(payload.gamma, 1.0);
    assert_eq!(payload.gamma_state, GammaLevel::Unknown.to_string());
}

#[test]
fn status_endpoint_reflects_enabled_daemon() {
    let local_ex = LocalExecutor::new();
    let socket_path = temp_socket_path("status-disabled");

    let response = smol::block_on(local_ex.run(async {
        let dmn = GammaDaemon::new(GammaDaemonConfig::default());
        run_request(
            &local_ex,
            &socket_path,
            dmn,
            b"GET /status HTTP/1.1\r\nContent-Length: 0\r\n\r\n",
        )
        .await
    }));
    
    let _ = std::fs::remove_file(&socket_path);

    let (status_line, body) = split_response(&response);
    assert_eq!(status_line, "HTTP/1.1 200 OK");

    let payload: payloads::StatusPayload = serde_json::from_slice(&body).unwrap();
    assert!(payload.enabled);
    assert_eq!(payload.gamma, 1.0);
    assert_eq!(payload.gamma_state, GammaLevel::Unknown.to_string());
}

#[test]
fn unknown_endpoint_returns_404() {
    let local_ex = LocalExecutor::new();
    let socket_path = temp_socket_path("not-found");

    let response = smol::block_on(local_ex.run(async {
        let dmn = GammaDaemon::new(GammaDaemonConfig::default());
        run_request(
            &local_ex,
            &socket_path,
            dmn,
            b"GET /nonsense HTTP/1.1\r\nContent-Length: 0\r\n\r\n",
        )
        .await
    }));

    let _ = std::fs::remove_file(&socket_path);

    let (status_line, _body) = split_response(&response);
    assert_eq!(status_line, "HTTP/1.1 404 Not Found");
}
