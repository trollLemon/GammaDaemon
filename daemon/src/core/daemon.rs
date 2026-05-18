#![allow(dead_code)]

use std::{error::Error, rc::Rc};

use async_channel::{Receiver, Sender};
use async_lock::Mutex;
use battery::{Battery, Manager};
use log::{error, info};
use std::time::Duration;

use crate::config::GammaDaemonConfig;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// Stores the current status of the daemon
pub struct Status {
    pub enabled: bool,
    pub gamma: f32,
    pub current_gamma_level: GammaLevel,
}

/// Abstracts updating the backlight with a new gamma value.
pub type GammaUpdateFn = fn(f32) -> Result<(), Box<dyn Error>>;

/// Daemon keeps track of the state of the system (current screen gamma, battery state, and daemon
/// config). It is the source of truth for determining if state mutates, and a screen gamma change
/// should be done by a worker thread.
pub trait Daemon {
    /// Enables the daemon's automatic gamma adjustment. Returns an error if already enabled.
    fn enable(&mut self) -> Result<(), Box<dyn Error>>;
    /// Disables the daemon's automatic gamma adjustment. Returns an error if already disabled.
    fn disable(&mut self) -> Result<(), Box<dyn Error>>;
    /// Sets the current gamma value to `gamma`.
    fn set(&mut self, gamma: f32) -> Result<(), Box<dyn Error>>;
    /// Returns a snapshot of the daemon's current state.
    fn status(&mut self) -> Status;
    /// Given the current state, returns an Option containing the new gamma value if the screen gamma should change, or a
    /// None if no state change occured.
    fn tick(&mut self, state: battery::State, soc: f32) -> Option<f32>;
    /// Returns a copy of the daemon config.
    fn get_config(&mut self) -> GammaDaemonConfig;
}

pub struct GammaDaemon {
    enabled: bool,
    current_gamma: f32,
    current_gamma_level: GammaLevel,
    channel_send: Sender<f32>,
    cfg: GammaDaemonConfig,
}

impl GammaDaemon {
    /// Constructs a new `GammaDaemon` with the given config.
    pub fn new(cfg: GammaDaemonConfig, sender_fn: Sender<f32>) -> Self {
        Self {
            enabled: true,
            current_gamma: 1.0,
            current_gamma_level: GammaLevel::Unknown,
            channel_send: sender_fn,
            cfg,
        }
    }
}

impl Daemon for GammaDaemon {
    fn enable(&mut self) -> Result<(), Box<dyn Error>> {
        if self.enabled {
            return Err(Box::from("GammaDaemon already enabled"));
        }
        self.enabled = true;
        Ok(())
    }

    fn disable(&mut self) -> Result<(), Box<dyn Error>> {
        if !self.enabled {
            return Err(Box::from("GammaDaemon already disabled"));
        }
        self.enabled = false;
        Ok(())
    }

    fn set(&mut self, gamma: f32) -> Result<(), Box<dyn Error>> {
        self.current_gamma = gamma;

        self.channel_send.try_send(gamma)?;

        Ok(())
    }

    fn status(&mut self) -> Status {
        let status: Status = Status {
            enabled: self.enabled,
            gamma: self.current_gamma,
            current_gamma_level: self.current_gamma_level,
        };
        status
    }

    fn tick(&mut self, state: battery::State, soc: f32) -> Option<f32> {
        if !self.enabled {
            return None;
        }
        let bc = &self.cfg.backlight_config;
        let (new_gamma, level) = match (state, soc <= bc.gamma_low_percentage) {
            (battery::State::Unknown, _) => (bc.gamma_unknown, GammaLevel::Unknown),
            (battery::State::Full, _) => (bc.gamma_full, GammaLevel::Full),
            (battery::State::Charging, _) => (bc.gamma_charging, GammaLevel::Charging),
            (battery::State::Discharging, true) => (bc.gamma_low, GammaLevel::Low),
            (battery::State::Discharging, false) => (bc.gamma_discharging, GammaLevel::Discharging),
            (battery::State::Notcharging, _) | (battery::State::Empty, _) => {
                (bc.gamma_plugged, GammaLevel::Plugged)
            }
            _ => return None,
        };

        if level == self.current_gamma_level {
            None
        } else {
            self.current_gamma = new_gamma;
            self.current_gamma_level = level;
            Some(new_gamma)
        }
    }

    fn get_config(&mut self) -> GammaDaemonConfig {
        self.cfg.clone()
    }
}

/// Change the backlight given a gamma fraction in [0.0, 1.0].
pub fn change_gamma(gamma: f32) -> Result<(), Box<dyn Error>> {
    let device = blight::Device::new(None)?;
    let raw = (gamma.clamp(0.0, 1.0) * device.max() as f32).round() as u32;
    blight::set_bl(raw, None)?;
    Ok(())
}

/// start an async loop to continuously check the Daemon for a state change, then apply the change
/// to the monitor device.
pub async fn update_loop<D: Daemon>(
    dmn: Rc<Mutex<D>>,
    mgr: &mut Manager,
    bat: &mut Battery,
    update_fn: GammaUpdateFn,
    recv_chan: Receiver<f32>,
) {
    let mut refresh_interval;

    loop {
        if let Err(e) = mgr.refresh(bat) {
            error!("failed to refresh battery: {}", e);
        }

        let state = bat.state();
        let soc = bat.state_of_charge();

        let gamma_update = {
            let mut dmn_lock = dmn.lock().await;
            let cfg = dmn_lock.get_config();
            refresh_interval = cfg.backlight_config.poll_interval;
            dmn_lock.tick(state, soc.value)
        };

        let trigger = {
            let tick_delay = async {
                smol::Timer::after(Duration::from_secs(refresh_interval)).await;
                None
            };

            let set = async {
                match recv_chan.recv().await {
                    Ok(gamma) => Some(gamma),
                    Err(e) => {
                        error!("failed to read gamma value from receiver: {}", e);
                        None
                    }
                }
            };
            smol::future::or(tick_delay, set).await
        };

        if let Some(gamma) = trigger {
            match update_fn(gamma) {
                Ok(()) => {
                    info!("Set gamma to {}", gamma);
                }
                Err(e) => {
                    error!("Failed to change the gamma: {}", e);
                }
            }
            continue;
        }

        if let Some(gamma) = gamma_update {
            match update_fn(gamma) {
                Ok(()) => {
                    info!("Set gamma to {}", gamma);
                }
                Err(e) => {
                    error!("Failed to change the gamma: {}", e);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enable_when_disabled_succeeds() {
        let (s, _r) = async_channel::bounded(1);
        let mut test_dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);
        test_dmn.enabled = false;
        let result = test_dmn.enable();
        assert!(result.is_ok());
    }

    #[test]
    fn test_enable_when_already_enabled_errors() {
        let (s, _r) = async_channel::bounded(1);
        let mut test_dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);
        let result = test_dmn.enable();
        assert!(result.is_err());
    }

    #[test]
    fn test_disable_when_enabled_succeeds() {
        let (s, _r) = async_channel::bounded(1);
        let mut test_dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);
        let result = test_dmn.disable();
        assert!(result.is_ok());
    }

    #[test]
    fn test_disable_when_already_disabled_errors() {
        let (s, _r) = async_channel::bounded(1);
        let mut test_dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);
        test_dmn.enabled = false;
        let result = test_dmn.disable();
        assert!(result.is_err());
    }

    #[test]
    fn test_set_updates_current_gamma() {
        let (s, _r) = async_channel::bounded(1);
        let mut test_dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);
        let result = test_dmn.set(0.5);
        assert!(result.is_ok());
        assert_eq!(test_dmn.current_gamma, 0.5);
    }

    #[test]
    fn test_status_reflects_current_state() {
        let (s, _r) = async_channel::bounded(1);
        let mut test_dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);
        let result = test_dmn.set(0.5);
        assert!(result.is_ok());

        let status = test_dmn.status();
        assert!(status.enabled);
        assert_eq!(status.gamma, 0.5);
        assert_eq!(status.current_gamma_level, GammaLevel::Unknown);
    }

    #[test]
    fn test_tick_returns_none_when_disabled() {
        let (s, _r) = async_channel::bounded(1);
        let mut test_dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);
        test_dmn.enabled = false;
        let result = test_dmn.tick(battery::State::Charging, 0.5);
        assert!(result.is_none());
    }

    #[test]
    fn test_tick_returns_none_when_state_unchanged() {
        let (s, _r) = async_channel::bounded(1);
        let mut test_dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);
        assert!(test_dmn.tick(battery::State::Unknown, 0.5).is_none());
        assert!(test_dmn.tick(battery::State::Charging, 0.5).is_some());
        assert!(test_dmn.tick(battery::State::Charging, 0.5).is_none());
    }

    #[test]
    fn test_tick_full_state_returns_full_gamma() {
        let (s, _r) = async_channel::bounded(1);
        let mut test_dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);
        let result = test_dmn.tick(battery::State::Full, 0.5);
        assert_eq!(result, Some(test_dmn.cfg.backlight_config.gamma_full));
    }

    #[test]
    fn test_tick_charging_state_returns_charging_gamma() {
        let (s, _r) = async_channel::bounded(1);
        let mut test_dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);
        let result = test_dmn.tick(battery::State::Charging, 0.5);
        assert_eq!(result, Some(test_dmn.cfg.backlight_config.gamma_charging));
    }

    #[test]
    fn test_tick_discharging_above_low_threshold_returns_discharging_gamma() {
        let (s, _r) = async_channel::bounded(1);
        let mut test_dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);
        let soc = test_dmn.cfg.backlight_config.gamma_low_percentage + 0.1;
        let result = test_dmn.tick(battery::State::Discharging, soc);
        assert_eq!(
            result,
            Some(test_dmn.cfg.backlight_config.gamma_discharging)
        );
    }

    #[test]
    fn test_tick_discharging_at_or_below_low_threshold_returns_low_gamma() {
        let (s, _r) = async_channel::bounded(1);
        let mut test_dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);
        let below = test_dmn.cfg.backlight_config.gamma_low_percentage - 0.05;
        let result = test_dmn.tick(battery::State::Discharging, below);
        assert_eq!(result, Some(test_dmn.cfg.backlight_config.gamma_low));

        let (s, _r) = async_channel::bounded(1);
        let mut test_dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);
        let at = test_dmn.cfg.backlight_config.gamma_low_percentage;
        let result = test_dmn.tick(battery::State::Discharging, at);
        assert_eq!(result, Some(test_dmn.cfg.backlight_config.gamma_low));
    }

    #[test]
    fn test_tick_empty_state_returns_plugged_gamma() {
        let (s, _r) = async_channel::bounded(1);
        let mut test_dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);
        let result = test_dmn.tick(battery::State::Empty, 0.5);
        assert_eq!(result, Some(test_dmn.cfg.backlight_config.gamma_plugged));
    }

    #[test]
    fn test_tick_unknown_state_returns_unknown_gamma() {
        let (s, _r) = async_channel::bounded(1);
        let mut test_dmn = GammaDaemon::new(GammaDaemonConfig::default(), s);
        test_dmn.current_gamma_level = GammaLevel::Full;
        let result = test_dmn.tick(battery::State::Unknown, 0.5);
        assert_eq!(result, Some(test_dmn.cfg.backlight_config.gamma_unknown));
    }

    #[test]
    fn test_gamma_level_display_formatting() {
        assert_eq!(format!("{}", GammaLevel::Full), "Gamma Full");
        assert_eq!(format!("{}", GammaLevel::Charging), "Gamma Charging");
        assert_eq!(format!("{}", GammaLevel::Discharging), "Gamma Discharging");
        assert_eq!(format!("{}", GammaLevel::Plugged), "Gamma Plugged");
        assert_eq!(format!("{}", GammaLevel::Unknown), "Gamma Unknown");
        assert_eq!(format!("{}", GammaLevel::Low), "Gamma Low");
    }
}
