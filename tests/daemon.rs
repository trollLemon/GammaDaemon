use async_lock::Mutex;
use gamma_daemon::config::GammaDaemonConfig;
use gamma_daemon::core::daemon::{GammaDaemon, GammaLevel};
use gamma_daemon::core::socket_server::listen_and_serve;
use gamma_daemon::payloads;
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

async fn run_requests(
    local_ex: &LocalExecutor<'_>,
    socket_path: &PathBuf,
    dmn: GammaDaemon,
    requests: &[Vec<u8>],
) -> Vec<payloads::Response> {
    let _ = std::fs::remove_file(socket_path);
    let listener = UnixListener::bind(socket_path).expect("failed to bind test unix socket");

    let dmn_mtx = Rc::new(Mutex::new(dmn));
    let server = listen_and_serve(local_ex, listener, Rc::clone(&dmn_mtx));

    let client = async {
        let mut responses = Vec::new();
        for request in requests {
            let mut stream = UnixStream::connect(socket_path)
                .await
                .expect("client failed to connect");
            stream.write_all(request).await.expect("write request");
            stream.flush().await.expect("flush request");

            let mut buf = Vec::new();
            stream.read_to_end(&mut buf).await.expect("read response");
            responses.push(
                serde_json::from_slice(&buf).expect("response should be valid Response JSON"),
            );
        }
        responses
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

#[test]
fn set_request_returns_ok() {
    let local_ex = LocalExecutor::new();
    let socket_path = temp_socket_path("set-ok");

    let response = smol::block_on(local_ex.run(async {
        let (s, _r) = async_channel::bounded(1);
        let dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);
        run_request(
            &local_ex,
            &socket_path,
            dmn,
            &encode(&payloads::Request::Set { gamma: 0.5 }),
        )
        .await
    }));

    let _ = std::fs::remove_file(&socket_path);

    assert!(matches!(response, payloads::Response::Ok { .. }));
}

#[test]
fn enable_request_on_already_enabled_daemon_returns_error() {
    let local_ex = LocalExecutor::new();
    let socket_path = temp_socket_path("enable-twice");

    let response = smol::block_on(local_ex.run(async {
        let (s, _r) = async_channel::bounded(1);
        let dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);
        run_request(
            &local_ex,
            &socket_path,
            dmn,
            &encode(&payloads::Request::Enable),
        )
        .await
    }));

    let _ = std::fs::remove_file(&socket_path);

    match response {
        payloads::Response::Error { message } => assert!(message.contains("already enabled")),
        other => panic!("expected error response, got {other:?}"),
    }
}

#[test]
fn empty_request_returns_error() {
    let local_ex = LocalExecutor::new();
    let socket_path = temp_socket_path("empty");

    let response = smol::block_on(local_ex.run(async {
        let (s, _r) = async_channel::bounded(1);
        let dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);
        run_request(&local_ex, &socket_path, dmn, b"\n").await
    }));

    let _ = std::fs::remove_file(&socket_path);

    assert!(matches!(response, payloads::Response::Error { .. }));
}

#[test]
fn trailing_data_after_newline_is_ignored() {
    let local_ex = LocalExecutor::new();
    let socket_path = temp_socket_path("trailing");

    let mut request = encode(&payloads::Request::Status);
    request.extend_from_slice(b"garbage past the delimiter");

    let response = smol::block_on(local_ex.run(async {
        let (s, _r) = async_channel::bounded(1);
        let dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);
        run_request(&local_ex, &socket_path, dmn, &request).await
    }));

    let _ = std::fs::remove_file(&socket_path);

    assert!(matches!(response, payloads::Response::Status(_)));
}

#[test]
fn disable_then_status_reports_disabled() {
    let local_ex = LocalExecutor::new();
    let socket_path = temp_socket_path("disable-status");

    let responses = smol::block_on(local_ex.run(async {
        let (s, _r) = async_channel::bounded(1);
        let dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);
        run_requests(
            &local_ex,
            &socket_path,
            dmn,
            &[
                encode(&payloads::Request::Disable),
                encode(&payloads::Request::Status),
            ],
        )
        .await
    }));

    let _ = std::fs::remove_file(&socket_path);

    assert!(matches!(responses[0], payloads::Response::Ok { .. }));
    match &responses[1] {
        payloads::Response::Status(status) => assert!(!status.enabled),
        other => panic!("expected status response, got {other:?}"),
    }
}

#[test]
fn disable_twice_returns_error_on_second_call() {
    let local_ex = LocalExecutor::new();
    let socket_path = temp_socket_path("disable-twice");

    let responses = smol::block_on(local_ex.run(async {
        let (s, _r) = async_channel::bounded(1);
        let dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);
        run_requests(
            &local_ex,
            &socket_path,
            dmn,
            &[
                encode(&payloads::Request::Disable),
                encode(&payloads::Request::Disable),
            ],
        )
        .await
    }));

    let _ = std::fs::remove_file(&socket_path);

    assert!(matches!(responses[0], payloads::Response::Ok { .. }));
    match &responses[1] {
        payloads::Response::Error { message } => assert!(message.contains("already disabled")),
        other => panic!("expected error response, got {other:?}"),
    }
}

#[test]
fn set_then_status_reflects_new_gamma() {
    let local_ex = LocalExecutor::new();
    let socket_path = temp_socket_path("set-status");

    let responses = smol::block_on(local_ex.run(async {
        let (s, _r) = async_channel::unbounded();
        let dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);
        run_requests(
            &local_ex,
            &socket_path,
            dmn,
            &[
                encode(&payloads::Request::Set { gamma: 0.33 }),
                encode(&payloads::Request::Status),
            ],
        )
        .await
    }));

    let _ = std::fs::remove_file(&socket_path);

    assert!(matches!(responses[0], payloads::Response::Ok { .. }));
    match &responses[1] {
        payloads::Response::Status(status) => assert_eq!(status.gamma, 0.33),
        other => panic!("expected status response, got {other:?}"),
    }
}

#[test]
fn oversized_request_closes_connection_without_response() {
    let local_ex = LocalExecutor::new();
    let socket_path = temp_socket_path("oversized");

    let got_response = smol::block_on(local_ex.run(async {
        let (s, _r) = async_channel::bounded(1);
        let dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);

        let _ = std::fs::remove_file(&socket_path);
        let listener = UnixListener::bind(&socket_path).expect("failed to bind test unix socket");
        let dmn_mtx = Rc::new(Mutex::new(dmn));
        let server = listen_and_serve(&local_ex, listener, Rc::clone(&dmn_mtx));

        let mut payload = vec![b'a'; 9 * 1024];
        payload.push(b'\n');

        let client = async {
            let mut stream = UnixStream::connect(&socket_path)
                .await
                .expect("client failed to connect");
            let _ = stream.write_all(&payload).await;
            let _ = stream.flush().await;

            let mut buf = Vec::new();
            match stream.read_to_end(&mut buf).await {
                Ok(_) => serde_json::from_slice::<payloads::Response>(&buf).is_ok(),
                Err(_) => false,
            }
        };

        smol::future::or(client, async {
            let _ = server.await;
            unreachable!("listen_and_serve should not return during the test")
        })
        .await
    }));

    let _ = std::fs::remove_file(&socket_path);

    assert!(!got_response);
}
