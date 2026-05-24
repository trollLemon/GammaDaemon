use async_channel::bounded;
use async_lock::Mutex;
use async_signal::{Signal, Signals};
use gamma_daemon::config::{self, GammaDaemonConfig};
use gamma_daemon::constants;
use gamma_daemon::core::daemon::{self, update_loop, GammaDaemon};
use gamma_daemon::core::dbus::{find_battery_path, UPower};
use gamma_daemon::core::socket_server::listen_and_serve;
use log::{error, info};
use smol::net::unix::UnixListener;
use smol::stream::StreamExt;
use smol::LocalExecutor;
use std::env;
use std::fs::{DirBuilder, Permissions};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::rc::Rc;
use zbus::Connection;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let config_path = env::args()
        .nth(1)
        .unwrap_or_else(|| constants::DEFAULT_CONFIG_PATH.to_string());

    let cfg = config::read_config_from_file(&config_path).unwrap_or_else(|e| {
        info!("failed to read config ({e}), using defaults");
        GammaDaemonConfig::default()
    });


    let (s, r) = bounded(1);
    let (shutdown_s, shutdown_r) = bounded::<()>(1);

    let connection = smol::block_on(Connection::system())?;
    let battery_path = smol::block_on(find_battery_path(&connection))?;

    let dmn = Rc::new(Mutex::new(GammaDaemon::new(
        cfg,
        s,
        UPower::new(connection, battery_path),
    )));

    let runtime_dir = constants::DEFAULT_RUNTIME_DIR;
    let socket_path = constants::DEFAULT_SOCKET_PATH;

    info!("Ensuring runtime directory {} exists", runtime_dir);
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(runtime_dir)?;
    std::fs::set_permissions(runtime_dir, Permissions::from_mode(0o700))?;

    info!("Removing old socket file {} if it exists.", socket_path);
    let _ = std::fs::remove_file(socket_path);

    info!("Binding to socket file at {}", socket_path);
    let listener = UnixListener::bind(socket_path)?;

    std::fs::set_permissions(socket_path, Permissions::from_mode(0o600))?;

    let local_ex = LocalExecutor::new();

    smol::block_on(local_ex.run(async {
        let tick_dmn = Rc::clone(&dmn);
        let loop_shutdown = shutdown_r.clone();
        local_ex
            .spawn(async move {
                update_loop(
                    tick_dmn,
                    daemon::change_gamma,
                    r,
                    loop_shutdown,
                )
                .await
            })
            .detach();

        // Translate SIGTERM/SIGINT into a shutdown by closing the channel.
        local_ex
            .spawn(async move {
                match Signals::new([Signal::Term, Signal::Int]) {
                    Ok(mut signals) => {
                        if let Some(Ok(sig)) = signals.next().await {
                            info!("received signal {:?}, shutting down", sig);
                        }
                    }
                    Err(e) => error!("failed to register signal handler: {e}"),
                }
                shutdown_s.close();
            })
            .detach();

        let serve = listen_and_serve(&local_ex, listener, dmn);
        let wait_shutdown = async {
            let _ = shutdown_r.recv().await;
            info!("shutdown requested, stopping socket server");
            Ok(())
        };
        smol::future::race(serve, wait_shutdown).await
    }))?;
    Ok(())
}
