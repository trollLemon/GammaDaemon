#![allow(dead_code)]

use std::error::Error;

use crate::config::GammaDaemonConfig;

#[derive(Clone, Copy, Debug)]
pub enum GammaLevel {
    Full,
    Charging,
    Discharging,
    Plugged,
    Unknown,
    Low,
}

impl std::fmt::Display for GammaLevel {
    /// Formats the gamma level as a human-readable string (e.g., "Gamma Full").
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            GammaLevel::Full => write!(f, "Gamma Full"),
            GammaLevel::Charging => write!(f, "Gamma Charging"),
            GammaLevel::Discharging => write!(f, "Gamma Discharging"),
            GammaLevel::Plugged => write!(f, "Gamma Plugged"),
            GammaLevel::Unknown => write!(f, "Gamma Unknown"),
            GammaLevel::Low => write!(f, "Gamma Low"),
        }
    }
}
pub struct Status {
    pub enabled: bool,
    pub gamma: u8,
    pub current_gamma_level: GammaLevel,
}

pub trait Daemon {
    /// Enables the daemon's automatic gamma adjustment. Returns an error if already enabled.
    fn enable(&mut self) -> Result<(), Box<dyn Error>>;
    /// Disables the daemon's automatic gamma adjustment. Returns an error if already disabled.
    fn disable(&mut self) -> Result<(), Box<dyn Error>>;
    /// Sets the current gamma value to `gamma`.
    fn set(&mut self, gamma: u8) -> Result<(), Box<dyn Error>>;
    /// Returns a snapshot of the daemon's current state.
    fn status(&mut self) -> Result<Status, Box<dyn Error>>;
}

pub struct GammaDaemon {
    enabled: bool,
    current_gamma: u8,
    current_gamma_level: GammaLevel,
    cfg: GammaDaemonConfig,
}

impl GammaDaemon {
    /// Constructs a new `GammaDaemon` with the given config, disabled by default
    /// and starting at gamma 100 with an unknown gamma level.
    pub fn new(cfg: GammaDaemonConfig) -> Self {
        Self {
            enabled: false,
            current_gamma: 100,
            current_gamma_level: GammaLevel::Unknown,
            cfg,
        }
    }
}

impl Daemon for GammaDaemon {
    /// Enables the daemon. Errors if it is already enabled.
    fn enable(&mut self) -> Result<(), Box<dyn Error>> {
        if self.enabled {
            return Err(Box::from("GammaDaemon already enabled"));
        }
        self.enabled = true;
        Ok(())
    }

    /// Disables the daemon. Errors if it is already disabled.
    fn disable(&mut self) -> Result<(), Box<dyn Error>> {
        if !self.enabled {
            return Err(Box::from("GammaDaemon already disabled"));
        }
        self.enabled = false;
        Ok(())
    }

    /// Sets the current gamma value.
    /// NOTE: this value will be overwritten on the next battery state change.
    fn set(&mut self, gamma: u8) -> Result<(), Box<dyn Error>> {
        self.current_gamma = gamma;
        Ok(())
    }

    /// Returns the daemon's current enabled flag, gamma value, and gamma level.
    fn status(&mut self) -> Result<Status, Box<dyn Error>> {
        let status: Status = Status {
            enabled: self.enabled,
            gamma: self.current_gamma,
            current_gamma_level: self.current_gamma_level,
        };
        Ok(status)
    }
}
