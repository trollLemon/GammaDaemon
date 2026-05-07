//  constants for default values and paths
pub const DEFAULT_CONFIG_PATH: &str = "/etc/gamma_daemon/config.toml";
pub const DEFAULT_LOG_LEVEL: &str = "info";
pub const DEFAULT_LOG_FILE: &str = "/var/log/gamma_daemon.log";
pub const DEFAULT_SOCKET_PATH: &str = "/var/run/gamma_daemon.sock";

// HTTP endpoints for socket communication
pub const ENDPOINT_SET_GAMMA: &str = "/set_gamma";
pub const ENDPOINT_TOGGLE: &str = "/toggle";
pub const ENDPOINT_STATUS: &str = "/status";
