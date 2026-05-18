use async_channel::bounded;
use async_lock::Mutex;
use gamma_daemon::config::{self, GammaDaemonConfig};
use gamma_daemon::core::daemon::{self, update_loop, GammaDaemon};
use gamma_daemon::core::socket_server::listen_and_serve;
use gamma_lib::constants;
use log::info;
use smol::net::unix::UnixListener;
use smol::LocalExecutor;
use std::env;
use std::fs::{DirBuilder, Permissions};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::rc::Rc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let config_path = env::args()
        .nth(1)
        .unwrap_or_else(|| constants::DEFAULT_CONFIG_PATH.to_string());

    let cfg = config::read_config_from_file(&config_path).unwrap_or_else(|e| {
        info!("failed to read config ({e}), using defaults");
        GammaDaemonConfig::default()
    });

    let mut manager = battery::Manager::new()?;
    let mut battery = manager.batteries()?.next().unwrap()?;

    let (s, r) = bounded(1);

    let dmn = Rc::new(Mutex::new(GammaDaemon::new(cfg, s)));

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
        local_ex
            .spawn(async move {
                update_loop(
                    tick_dmn,
                    &mut manager,
                    &mut battery,
                    daemon::change_gamma,
                    r,
                )
                .await
            })
            .detach();

        listen_and_serve(&local_ex, listener, dmn).await
    }))?;
    Ok(())
}
