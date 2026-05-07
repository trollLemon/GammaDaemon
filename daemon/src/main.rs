mod config;
mod core;
mod http;

use crate::config::GammaDaemonConfig;
use crate::core::daemon::GammaDaemon;
use crate::core::socket_server::listen_and_serve;
use async_lock::Mutex;
use gamma_lib::constants;
use log::info;
use smol::net::unix::UnixListener;
use std::env;
use std::sync::Arc;

/// Entry point for the GammaDaemon binary. Loads the config (falling back to defaults),
/// constructs the daemon, binds the Unix socket, and runs the listen-and-serve loop.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let config_path = env::args()
        .nth(1)
        .unwrap_or_else(|| constants::DEFAULT_CONFIG_PATH.to_string());

    let cfg = config::read_config_from_file(&config_path).unwrap_or_else(|e| {
        info!("failed to read config ({e}), using defaults");
        GammaDaemonConfig::default()
    });

    let dmn = Arc::new(Mutex::new(GammaDaemon::new(cfg)));

    let socket_path = constants::DEFAULT_SOCKET_PATH;

    info!("Removing old socket file {} if it exists.", socket_path);

    let _ = std::fs::remove_file(socket_path);
    let listener = UnixListener::bind(socket_path)?;

    info!("Binding to socket file at {}", socket_path);

    smol::block_on(listen_and_serve(listener, dmn))?;
    Ok(())
}
