use std::{error::Error, rc::Rc};

use async_channel::{Receiver, Sender};
use async_lock::Mutex;
use log::{error, info};
use std::time::Duration;

use crate::config::GammaDaemonConfig;
use crate::core::dbus::{BatteryProvider, BatteryState};

/// Outcome of waiting for a shutdown request,
/// or a trigger fired carrying an optional new gamma value.
enum Wakeup {
    Shutdown,
    Trigger(Option<f32>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Describes the current state of the daemon w.r.t the battery.
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

/// Keeps track of the state of the system (current screen gamma, battery state, and daemon
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
    #[allow(async_fn_in_trait)]
    async fn tick(&mut self) -> Option<f32>;
    /// Returns a copy of the daemon config.
    fn get_config(&mut self) -> GammaDaemonConfig;
}

pub struct GammaDaemon<B: BatteryProvider> {
    enabled: bool,
    current_gamma: f32,
    current_gamma_level: GammaLevel,
    channel_send: Sender<f32>,
    battery: B,
    cfg: GammaDaemonConfig,
}

impl<B: BatteryProvider> GammaDaemon<B> {
    /// Constructs a new `GammaDaemon` with the given config and battery source.
    pub fn new(cfg: GammaDaemonConfig, sender_fn: Sender<f32>, battery: B) -> Self {
        Self {
            enabled: true,
            current_gamma: 1.0,
            current_gamma_level: GammaLevel::Unknown,
            channel_send: sender_fn,
            battery,
            cfg,
        }
    }
}

impl<B: BatteryProvider> Daemon for GammaDaemon<B> {
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

    async fn tick(&mut self) -> Option<f32> {
        if !self.enabled {
            return None;
        }

        let battery_state_result = self.battery.info().await;

        let battery_state = match battery_state_result {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to get battery state from dbus: {}", e);
                return None;
            }
        };

        let state = battery_state.state;
        let soc = battery_state.soc;

        let bc = &self.cfg.backlight_config;
        let (new_gamma, level) = match (state, soc <= bc.gamma_low_percentage) {
            (BatteryState::Unknown, _) => (bc.gamma_unknown, GammaLevel::Unknown),
            (BatteryState::FullyCharged, _) => (bc.gamma_full, GammaLevel::Full),
            (BatteryState::Charging, _) => (bc.gamma_charging, GammaLevel::Charging),
            (BatteryState::Discharging, true) => (bc.gamma_low, GammaLevel::Low),
            (BatteryState::Discharging, false) => (bc.gamma_discharging, GammaLevel::Discharging),
            (BatteryState::Empty, _)
            | (BatteryState::PendingCharge, _)
            | (BatteryState::PendingDischarge, _) => (bc.gamma_plugged, GammaLevel::Plugged),
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

/// Awaits for either:
/// - a message received from the Daemon initializing a set option.
/// - a tick timer firing.
///
/// If the former happens first we return the f32 sent over the channel, or None.
async fn await_trigger(recv_chan: &Receiver<f32>, refresh_interval: u64) -> Option<f32> {
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
}

/// start an async loop to continuously check the Daemon for a state change, then apply the change
/// to the monitor device.
pub async fn update_loop<D: Daemon>(
    dmn: Rc<Mutex<D>>,
    update_fn: GammaUpdateFn,
    recv_chan: Receiver<f32>,
    shutdown: Receiver<()>,
) {
    let refresh_interval = {
        let mut dmn_lock = dmn.lock().await;
        dmn_lock.get_config().backlight_config.poll_interval
    };

    loop {
        let gamma_update = {
            let mut dmn_lock = dmn.lock().await;
            dmn_lock.tick().await
        };

        let wakeup = {
            let on_shutdown = async {
                let _ = shutdown.recv().await;
                Wakeup::Shutdown
            };
            let on_trigger =
                async { Wakeup::Trigger(await_trigger(&recv_chan, refresh_interval).await) };
            smol::future::race(on_shutdown, on_trigger).await
        };

        let trigger = match wakeup {
            Wakeup::Shutdown => {
                info!("shutdown signal received, stopping update loop");
                return;
            }
            Wakeup::Trigger(trigger) => trigger,
        };

        if let Some(gamma) = trigger.or(gamma_update) {
            if let Err(e) = update_fn(gamma) {
                error!("Failed to change the gamma: {}", e);
            } else {
                info!("Set gamma to {}", gamma);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::dbus::BatteryInfo;
    use std::cell::{Cell, RefCell};

    /// A test battery source for unit tests.
    #[derive(Clone)]
    struct MockBattery {
        info: Rc<Cell<BatteryInfo>>,
    }

    impl MockBattery {
        fn new(state: BatteryState, soc: f64) -> Self {
            Self {
                info: Rc::new(Cell::new(BatteryInfo { state, soc })),
            }
        }

        /// Updates the reading returned by the next `state()` call.
        fn set(&self, state: BatteryState, soc: f64) {
            self.info.set(BatteryInfo { state, soc });
        }
    }

    impl BatteryProvider for MockBattery {
        async fn info(&self) -> Result<BatteryInfo, Box<dyn Error>> {
            Ok(self.info.get())
        }
    }

    /// Builds a daemon over the given sender with a mock battery, returning the
    /// daemon and a handle to mutate the battery during tests.
    fn make(
        s: Sender<f32>,
        state: BatteryState,
        soc: f64,
    ) -> (GammaDaemon<MockBattery>, MockBattery) {
        let battery = MockBattery::new(state, soc);
        let dmn = GammaDaemon::new(GammaDaemonConfig::default(), s, battery.clone());
        (dmn, battery)
    }

    fn tick(dmn: &mut GammaDaemon<MockBattery>) -> Option<f32> {
        smol::block_on(dmn.tick())
    }

    /// A battery source whose `info()` always fails, used to exercise the
    /// error path in `tick`.
    struct ErrBattery;

    impl BatteryProvider for ErrBattery {
        async fn info(&self) -> Result<BatteryInfo, Box<dyn Error>> {
            Err("dbus unavailable".into())
        }
    }

    /// Returns the default config with `poll_interval` overridden, so loop tests
    /// can either fire the tick timer immediately (0) or effectively never (large).
    fn config_with_poll(poll_interval: u64) -> GammaDaemonConfig {
        let mut cfg = GammaDaemonConfig::default();
        cfg.backlight_config.poll_interval = poll_interval;
        cfg
    }

    thread_local! {
        static APPLIED: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
        static UPDATE_SHOULD_FAIL: Cell<bool> = const { Cell::new(false) };
    }

    fn reset_recorder() {
        APPLIED.with(|a| a.borrow_mut().clear());
        UPDATE_SHOULD_FAIL.with(|f| f.set(false));
    }

    /// Records every applied gamma value, optionally failing to exercise the
    /// loop's error-handling branch.
    fn recording_update_fn(gamma: f32) -> Result<(), Box<dyn Error>> {
        APPLIED.with(|a| a.borrow_mut().push(gamma));
        if UPDATE_SHOULD_FAIL.with(|f| f.get()) {
            Err("forced update failure".into())
        } else {
            Ok(())
        }
    }

    fn applied() -> Vec<f32> {
        APPLIED.with(|a| a.borrow().clone())
    }

    fn loop_daemon(
        cfg: GammaDaemonConfig,
        sender: Sender<f32>,
        state: BatteryState,
        soc: f64,
    ) -> Rc<Mutex<GammaDaemon<MockBattery>>> {
        let battery = MockBattery::new(state, soc);
        Rc::new(Mutex::new(GammaDaemon::new(cfg, sender, battery)))
    }

    #[test]
    fn test_enable_when_disabled_succeeds() {
        let (s, _r) = async_channel::bounded(1);
        let (mut test_dmn, _b) = make(s, BatteryState::Unknown, 0.5);
        test_dmn.enabled = false;
        let result = test_dmn.enable();
        assert!(result.is_ok());
    }

    #[test]
    fn test_enable_when_already_enabled_errors() {
        let (s, _r) = async_channel::bounded(1);
        let (mut test_dmn, _b) = make(s, BatteryState::Unknown, 0.5);
        let result = test_dmn.enable();
        assert!(result.is_err());
    }

    #[test]
    fn test_disable_when_enabled_succeeds() {
        let (s, _r) = async_channel::bounded(1);
        let (mut test_dmn, _b) = make(s, BatteryState::Unknown, 0.5);
        let result = test_dmn.disable();
        assert!(result.is_ok());
    }

    #[test]
    fn test_disable_when_already_disabled_errors() {
        let (s, _r) = async_channel::bounded(1);
        let (mut test_dmn, _b) = make(s, BatteryState::Unknown, 0.5);
        test_dmn.enabled = false;
        let result = test_dmn.disable();
        assert!(result.is_err());
    }

    #[test]
    fn test_set_updates_current_gamma() {
        let (s, _r) = async_channel::bounded(1);
        let (mut test_dmn, _b) = make(s, BatteryState::Unknown, 0.5);
        let result = test_dmn.set(0.5);
        assert!(result.is_ok());
        assert_eq!(test_dmn.current_gamma, 0.5);
    }

    #[test]
    fn test_status_reflects_current_state() {
        let (s, _r) = async_channel::bounded(1);
        let (mut test_dmn, _b) = make(s, BatteryState::Unknown, 0.5);
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
        let (mut test_dmn, _b) = make(s, BatteryState::Charging, 0.5);
        test_dmn.enabled = false;
        assert!(tick(&mut test_dmn).is_none());
    }

    #[test]
    fn test_tick_returns_none_when_state_unchanged() {
        let (s, _r) = async_channel::bounded(1);
        let (mut test_dmn, b) = make(s, BatteryState::Unknown, 0.5);
        assert!(tick(&mut test_dmn).is_none());
        b.set(BatteryState::Charging, 0.5);
        assert!(tick(&mut test_dmn).is_some());
        assert!(tick(&mut test_dmn).is_none());
    }

    #[test]
    fn test_tick_full_state_returns_full_gamma() {
        let (s, _r) = async_channel::bounded(1);
        let (mut test_dmn, _b) = make(s, BatteryState::FullyCharged, 0.5);
        let result = tick(&mut test_dmn);
        assert_eq!(result, Some(test_dmn.cfg.backlight_config.gamma_full));
    }

    #[test]
    fn test_tick_charging_state_returns_charging_gamma() {
        let (s, _r) = async_channel::bounded(1);
        let (mut test_dmn, _b) = make(s, BatteryState::Charging, 0.5);
        let result = tick(&mut test_dmn);
        assert_eq!(result, Some(test_dmn.cfg.backlight_config.gamma_charging));
    }

    #[test]
    fn test_tick_discharging_above_low_threshold_returns_discharging_gamma() {
        let (s, _r) = async_channel::bounded(1);
        let soc = GammaDaemonConfig::default()
            .backlight_config
            .gamma_low_percentage
            + 0.1;
        let (mut test_dmn, _b) = make(s, BatteryState::Discharging, soc);
        let result = tick(&mut test_dmn);
        assert_eq!(
            result,
            Some(test_dmn.cfg.backlight_config.gamma_discharging)
        );
    }

    #[test]
    fn test_tick_discharging_at_or_below_low_threshold_returns_low_gamma() {
        let threshold = GammaDaemonConfig::default()
            .backlight_config
            .gamma_low_percentage;

        let (s, _r) = async_channel::bounded(1);
        let (mut test_dmn, _b) = make(s, BatteryState::Discharging, threshold - 0.05);
        let result = tick(&mut test_dmn);
        assert_eq!(result, Some(test_dmn.cfg.backlight_config.gamma_low));

        let (s, _r) = async_channel::bounded(1);
        let (mut test_dmn, _b) = make(s, BatteryState::Discharging, threshold);
        let result = tick(&mut test_dmn);
        assert_eq!(result, Some(test_dmn.cfg.backlight_config.gamma_low));
    }

    #[test]
    fn test_tick_empty_state_returns_plugged_gamma() {
        let (s, _r) = async_channel::bounded(1);
        let (mut test_dmn, _b) = make(s, BatteryState::Empty, 0.5);
        let result = tick(&mut test_dmn);
        assert_eq!(result, Some(test_dmn.cfg.backlight_config.gamma_plugged));
    }

    #[test]
    fn test_tick_unknown_state_returns_unknown_gamma() {
        let (s, _r) = async_channel::bounded(1);
        let (mut test_dmn, _b) = make(s, BatteryState::Unknown, 0.5);
        test_dmn.current_gamma_level = GammaLevel::Full;
        let result = tick(&mut test_dmn);
        assert_eq!(result, Some(test_dmn.cfg.backlight_config.gamma_unknown));
    }

    #[test]
    fn test_set_still_updates_current_gamma_when_send_fails() {
        let (s, _r) = async_channel::bounded(1);
        let (mut test_dmn, _b) = make(s, BatteryState::Unknown, 0.5);

        assert!(test_dmn.set(0.3).is_ok());
        let result = test_dmn.set(0.6);
        assert!(result.is_err());
        assert_eq!(test_dmn.current_gamma, 0.6);
    }

    #[test]
    fn test_set_errors_when_receiver_dropped() {
        let (s, r) = async_channel::bounded(1);
        drop(r);
        let (mut test_dmn, _b) = make(s, BatteryState::Unknown, 0.5);

        let result = test_dmn.set(0.5);
        assert!(result.is_err());
        assert_eq!(test_dmn.current_gamma, 0.5);
    }

    #[test]
    fn test_set_at_range_bounds_succeeds() {
        let (s, _r) = async_channel::unbounded();
        let (mut test_dmn, _b) = make(s, BatteryState::Unknown, 0.5);

        assert!(test_dmn.set(0.0).is_ok());
        assert_eq!(test_dmn.current_gamma, 0.0);
        assert!(test_dmn.set(1.0).is_ok());
        assert_eq!(test_dmn.current_gamma, 1.0);
    }

    #[test]
    fn test_tick_notcharging_state_returns_plugged_gamma() {
        let (s, _r) = async_channel::bounded(1);
        let (mut test_dmn, _b) = make(s, BatteryState::PendingCharge, 0.5);
        let result = tick(&mut test_dmn);
        assert_eq!(result, Some(test_dmn.cfg.backlight_config.gamma_plugged));
    }

    #[test]
    fn test_tick_disabled_does_not_mutate_gamma_level() {
        let (s, _r) = async_channel::bounded(1);
        let (mut test_dmn, _b) = make(s, BatteryState::FullyCharged, 0.9);
        test_dmn.enabled = false;

        assert!(tick(&mut test_dmn).is_none());
        assert_eq!(test_dmn.current_gamma_level, GammaLevel::Unknown);
        test_dmn.enabled = true;
        assert_eq!(
            tick(&mut test_dmn),
            Some(test_dmn.cfg.backlight_config.gamma_full)
        );
    }

    #[test]
    fn test_tick_low_to_discharging_transition_re_triggers() {
        let (s, _r) = async_channel::bounded(1);
        let threshold = GammaDaemonConfig::default()
            .backlight_config
            .gamma_low_percentage;
        let (mut test_dmn, b) = make(s, BatteryState::Discharging, threshold + 0.1);

        assert_eq!(
            tick(&mut test_dmn),
            Some(test_dmn.cfg.backlight_config.gamma_discharging)
        );
        b.set(BatteryState::Discharging, threshold - 0.05);
        assert_eq!(
            tick(&mut test_dmn),
            Some(test_dmn.cfg.backlight_config.gamma_low)
        );
        b.set(BatteryState::Discharging, threshold + 0.1);
        assert_eq!(
            tick(&mut test_dmn),
            Some(test_dmn.cfg.backlight_config.gamma_discharging)
        );
    }

    #[test]
    fn test_tick_repeated_same_state_only_triggers_once() {
        let (s, _r) = async_channel::bounded(1);
        let (mut test_dmn, b) = make(s, BatteryState::FullyCharged, 0.9);

        assert!(tick(&mut test_dmn).is_some());
        b.set(BatteryState::FullyCharged, 0.5);
        assert!(tick(&mut test_dmn).is_none());
        b.set(BatteryState::FullyCharged, 1.0);
        assert!(tick(&mut test_dmn).is_none());
    }

    #[test]
    fn test_tick_charging_ignores_low_soc() {
        let (s, _r) = async_channel::bounded(1);
        let (mut test_dmn, _b) = make(s, BatteryState::Charging, 0.01);
        let result = tick(&mut test_dmn);
        assert_eq!(result, Some(test_dmn.cfg.backlight_config.gamma_charging));
    }

    #[test]
    fn test_tick_does_not_overwrite_manually_set_gamma_without_state_change() {
        let (s, _r) = async_channel::unbounded();
        let (mut test_dmn, _b) = make(s, BatteryState::Charging, 0.5);

        assert!(tick(&mut test_dmn).is_some());
        assert!(test_dmn.set(0.123).is_ok());
        assert!(tick(&mut test_dmn).is_none());
        assert_eq!(test_dmn.current_gamma, 0.123);
    }

    #[test]
    fn test_get_config_returns_configured_values() {
        let (s, _r) = async_channel::bounded(1);
        let (mut test_dmn, _b) = make(s, BatteryState::Unknown, 0.5);
        let cfg = test_dmn.get_config();
        assert_eq!(
            cfg.backlight_config.poll_interval,
            GammaDaemonConfig::default().backlight_config.poll_interval
        );
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

    #[test]
    fn test_await_trigger_returns_sent_gamma_before_timer() {
        let (s, r) = async_channel::bounded(1);
        s.try_send(0.42).expect("send should succeed");
        let result = smol::block_on(await_trigger(&r, 3600));
        assert_eq!(result, Some(0.42));
    }

    #[test]
    fn test_await_trigger_returns_none_when_timer_fires_first() {
        let (_s, r) = async_channel::bounded::<f32>(1);
        let result = smol::block_on(await_trigger(&r, 0));
        assert!(result.is_none());
    }

    #[test]
    fn test_await_trigger_returns_none_when_sender_dropped() {
        let (s, r) = async_channel::bounded::<f32>(1);
        drop(s);
        let result = smol::block_on(await_trigger(&r, 3600));
        assert!(result.is_none());
    }

    #[test]
    fn test_await_trigger_drains_value_from_channel() {
        let (s, r) = async_channel::bounded(1);
        s.try_send(0.75).expect("send should succeed");
        let result = smol::block_on(await_trigger(&r, 3600));
        assert_eq!(result, Some(0.75));
        assert!(r.try_recv().is_err());
    }

    #[test]
    fn test_tick_returns_none_when_battery_info_errors() {
        let (s, _r) = async_channel::bounded(1);
        let mut test_dmn = GammaDaemon::new(GammaDaemonConfig::default(), s, ErrBattery);
        test_dmn.current_gamma_level = GammaLevel::Full;

        assert!(smol::block_on(test_dmn.tick()).is_none());
        assert_eq!(test_dmn.current_gamma_level, GammaLevel::Full);
    }

    /// Spins until `cond` holds, yielding to the executor so the spawned
    /// `update_loop` task can make progress in between checks.
    async fn wait_until(mut cond: impl FnMut() -> bool) {
        while !cond() {
            smol::future::yield_now().await;
        }
    }

    #[test]
    fn test_update_loop_applies_triggered_gamma() {
        reset_recorder();
        let ex = smol::LocalExecutor::new();
        let (s, recv) = async_channel::unbounded::<f32>();
        let (sd, shutdown) = async_channel::bounded::<()>(1);
        let dmn = loop_daemon(config_with_poll(3600), s.clone(), BatteryState::Unknown, 0.5);

        smol::block_on(ex.run(async {
            let task = ex.spawn(update_loop(dmn, recording_update_fn, recv, shutdown));
            s.send(0.42).await.expect("send trigger");
            wait_until(|| applied().contains(&0.42)).await;
            sd.send(()).await.expect("send shutdown");
            task.await;
        }));

        assert_eq!(applied(), vec![0.42]);
    }

    #[test]
    fn test_update_loop_returns_on_shutdown() {
        reset_recorder();
        let ex = smol::LocalExecutor::new();
        let (s, recv) = async_channel::unbounded::<f32>();
        let (sd, shutdown) = async_channel::bounded::<()>(1);
        let dmn = loop_daemon(config_with_poll(3600), s, BatteryState::Unknown, 0.5);

        smol::block_on(ex.run(async {
            let task = ex.spawn(update_loop(dmn, recording_update_fn, recv, shutdown));
            sd.send(()).await.expect("send shutdown");
            task.await;
        }));

        assert!(applied().is_empty());
    }

    #[test]
    fn test_update_loop_applies_tick_gamma_when_no_trigger() {
        reset_recorder();
        let ex = smol::LocalExecutor::new();
        let (_s, recv) = async_channel::unbounded::<f32>();
        let (sd, shutdown) = async_channel::bounded::<()>(1);
        let cfg = config_with_poll(1);
        let expected = cfg.backlight_config.gamma_charging;
        let dmn = loop_daemon(cfg, _s, BatteryState::Charging, 0.5);

        smol::block_on(ex.run(async {
            let task = ex.spawn(update_loop(dmn, recording_update_fn, recv, shutdown));
            wait_until(|| applied().contains(&expected)).await;
            sd.send(()).await.expect("send shutdown");
            task.await;
        }));

        assert!(applied().contains(&expected));
    }

    #[test]
    fn test_update_loop_trigger_takes_precedence_over_tick() {
        reset_recorder();
        let ex = smol::LocalExecutor::new();
        let (s, recv) = async_channel::unbounded::<f32>();
        let (sd, shutdown) = async_channel::bounded::<()>(1);
        let cfg = config_with_poll(3600);
        let tick_gamma = cfg.backlight_config.gamma_charging;
        let dmn = loop_daemon(cfg, s.clone(), BatteryState::Charging, 0.5);

        smol::block_on(ex.run(async {
            let task = ex.spawn(update_loop(dmn, recording_update_fn, recv, shutdown));
            s.send(0.99).await.expect("send trigger");
            wait_until(|| !applied().is_empty()).await;
            sd.send(()).await.expect("send shutdown");
            task.await;
        }));

        assert_eq!(applied(), vec![0.99]);
        assert_ne!(0.99, tick_gamma);
    }

    #[test]
    fn test_update_loop_continues_when_update_fn_errors() {
        reset_recorder();
        UPDATE_SHOULD_FAIL.with(|f| f.set(true));
        let ex = smol::LocalExecutor::new();
        let (s, recv) = async_channel::unbounded::<f32>();
        let (sd, shutdown) = async_channel::bounded::<()>(1);
        let dmn = loop_daemon(config_with_poll(3600), s.clone(), BatteryState::Unknown, 0.5);

        smol::block_on(ex.run(async {
            let task = ex.spawn(update_loop(dmn, recording_update_fn, recv, shutdown));
            s.send(0.5).await.expect("send first trigger");
            wait_until(|| !applied().is_empty()).await;
            s.send(0.6).await.expect("send second trigger");
            wait_until(|| applied().len() >= 2).await;
            sd.send(()).await.expect("send shutdown");
            task.await;
        }));

        assert_eq!(applied(), vec![0.5, 0.6]);
    }
}
