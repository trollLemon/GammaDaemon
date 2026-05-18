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

fn encode(request: &payloads::Request) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(request).expect("request must serialize to JSON");
    bytes.push(b'\n');
    bytes
}

async fn run_request(
    local_ex: &LocalExecutor<'_>,
    socket_path: &PathBuf,
    dmn: GammaDaemon,
    request: &[u8],
) -> payloads::Response {
    let _ = std::fs::remove_file(socket_path);
    let listener = UnixListener::bind(socket_path).expect("failed to bind test unix socket");

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
        serde_json::from_slice(&buf).expect("response should be valid Response JSON")
    };

    smol::future::or(client, async {
        let _ = server.await;
        unreachable!("listen_and_serve should not return during the test")
    })
    .await
}

#[test]
fn status_request_returns_current_state() {
    let local_ex = LocalExecutor::new();
    let socket_path = temp_socket_path("status-ok");

    let response = smol::block_on(local_ex.run(async {
        let (s, _r) = async_channel::bounded(1);
        let dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);
        run_request(
            &local_ex,
            &socket_path,
            dmn,
            &encode(&payloads::Request::Status),
        )
        .await
    }));

    let _ = std::fs::remove_file(&socket_path);

    match response {
        payloads::Response::Status(status) => {
            assert!(status.enabled);
            assert_eq!(status.gamma, 1.0);
            assert_eq!(status.gamma_state, GammaLevel::Unknown.to_string());
        }
        other => panic!("expected status response, got {other:?}"),
    }
}

#[test]
fn invalid_request_returns_error() {
    let local_ex = LocalExecutor::new();
    let socket_path = temp_socket_path("invalid");

    let response = smol::block_on(local_ex.run(async {
        let (s, _r) = async_channel::bounded(1);
        let dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);
        run_request(&local_ex, &socket_path, dmn, b"not valid json\n").await
    }));

    let _ = std::fs::remove_file(&socket_path);

    assert!(matches!(response, payloads::Response::Error { .. }));
}
