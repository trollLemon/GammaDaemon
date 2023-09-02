/* This contains the daemon functionality
 *
 * Only the run function is public, since the client code doesn't need to
 * do any of the gamma calculations or changes, that's all abstracted away in this file
 *
 */

use battery::{Battery, State};
use bulbb::error::Error;
use bulbb::monitor::MonitorDevice;
use daemonize::Daemonize;
use std::fs::File;
use std::thread;
use std::time::Duration;
mod config;
mod read_file;
use crate::daemon::config::Config;
use std::env;

pub const AC_STATUS_FILE: &str = "/sys/class/power_supply/AC/online"; //this is the AC status file

pub trait Backlight {
    fn change_gamma(&self, gamma: u32) -> Result<(), Error>;
}

impl Backlight for MonitorDevice {
    fn change_gamma(&self, gamma: u32) -> Result<(), Error> {
        self.set_brightness(gamma)
    }
}

/* struct to bundle useful information about the notebooks battery
 * and AC charger.
 *
 * We'll use this to check if the battery status changed or if the AC is plugged in
 */
#[derive(Debug)]
pub struct BatteryInfo {
    soc: f32,
    old_status: State,
    new_status: State,
    old_ac_status: char,
    new_ac_status: char,
    gamma_values: Box<Config>,
}

// Make a struct for our Battery Info
// Initially sets all values to either unknown and 0 for the state and AC status
// These will be updated during the Daemons run time
fn new_battery_info(gamma_values: Config, battery: &mut Battery) -> BatteryInfo {
    BatteryInfo {
        soc: battery.state_of_charge().value,
        old_status: State::Unknown,
        new_status: State::Unknown,
        old_ac_status: '0',
        new_ac_status: '0',
        gamma_values: Box::new(gamma_values),
    }
}
// updates old status variables so we can compare them in the next iteration of the program loop
// Assumes new_battery_info() has been called by the client code.
fn update(info: &mut BatteryInfo) {
    info.old_status = info.new_status;
    info.old_ac_status = info.new_ac_status;
}

/* Helper function to determine the gamma if the battery is discharging and/or is low.
 *
 * If the battery is discharging and isnt below the threshold set by the user, then
 * the function retunrs the user's 'discharging' gamma setting. Otherwise the function returns
 * the 'low' gamma setting.
 *
 * */
fn low_gamma_or_charging(info: &BatteryInfo) -> u32 {
    let config = &info.gamma_values;
    if info.soc <= (config.low_perc as f32) / 100.0 {
        return config.low;
    }
    config.discharging
}

/*  Returns a u32 for the new brightness we shall set for the MonitorDevice
 *
 *  This function requires  reference to the battery's state, and a reference to a battery info struct
 *
 *  Depending on the state of the battery and the AC, this function will set the gamma to the matching value mapped to the state in
 *  the config file
 *
 * */
fn calc_new_brightness(info: &BatteryInfo) -> u32 {
    let config = &info.gamma_values; // user config values
    let state = info.new_status;
    let plugged = info.new_ac_status == '1';

    // calculate gamma based on the battery state
    match (state, plugged) {
        (State::Full, false) => config.full,
        (State::Full, true) => config.full,
        (State::__Nonexhaustive, _) => 128, // not implemented in the battery crate yet, we'll ignore it
        (State::Charging, _) => config.charging,
        (State::Discharging, _) => low_gamma_or_charging(info),
        (State::Empty, _) => 10,
        (State::Unknown, true) => config.ac_in,
        (State::Unknown, false) => low_gamma_or_charging(info),
    }
}

/* Returns a bool showing if the battery has changed states.
 * I.E: From State::Charging to State::Discharging
 *
 * This function requires references to an old battery state and a new one
 */
fn status_changed(status: &BatteryInfo) -> bool {
    let old_status = status.old_status;
    let new_status = status.new_status;

    let old_ac_status = status.old_ac_status;
    let new_ac_status = status.new_ac_status;

    old_status != new_status || old_ac_status != new_ac_status
}

fn daemonize() {
    let stdout = File::create("/tmp/gamma_daemon.out").unwrap();
    let stderr = File::create("/tmp/gamma_daemon.err").unwrap();

    let daemonize = Daemonize::new()
        .pid_file("/tmp/gamma_daemon.pid")
        .working_directory("/tmp")
        .group("video") // Group name
        .stdout(stdout) // Redirect stdout to `/tmp/daemon.out`.
        .stderr(stderr) // Redirect stderr to `/tmp/daemon.err`.
        .privileged_action(|| "Executed before drop privileges");

    match daemonize.start() {
        Ok(_) => println!("gamma_daemon started"),
        Err(e) => eprintln!("{}", e),
    }
}

fn loop_update(
    manager: &battery::Manager,
    battery_info: &mut BatteryInfo,
    battery: &mut Battery,
    sleep_duration: Duration,
) -> Result<(), battery::Error> {
    update(battery_info);
    manager.refresh(battery)?;
    thread::sleep(sleep_duration);
    Ok(())
}

/* This function will run the perform_screen_change function, and direct
 * success or error messages to std::out or std::err based on the result of
 * perform_screen_change.
 * */
fn try_change<T: Backlight>(device: &T, info: &BatteryInfo) -> u8 {
    match perform_screen_change(device, &info) {
        Ok(g) => {
            println!("Changed gamma to {}", g);
            return 0;
        }
        //If there is an error changing the gamma, print an error
        Err(e) => {
            println!("Error changing gamma: {}", e);
            return 1;
        }
    };
}

/* Run the Daemon.
 * Returns a result with a () success type, and a battery::Error if there is any issue reading from
 * the notebook battery
 *
 * This function requires a reference to a laptop monitor
 */
pub fn run(device: &MonitorDevice) -> Result<(), battery::Error> {
    let delay: u64 = 1; // check for changes every second
    let sleep_duration = Duration::from_secs(delay);

    let env: String = match env::var("USER") {
        Ok(s) => s,
        Err(_) => "NAN".to_string(),
    };
    let config_file = "/home/".to_owned() + &env + "/.config/GammaDaemon/conf.toml";
    let config: Config = config::load_config(config_file);

    let manager = battery::Manager::new()?;
    let mut battery = manager.batteries()?.next().unwrap()?;
    let mut battery_info = Box::new(new_battery_info(config, &mut battery));

    let old_status = battery.state();
    let old_ac_status: String = read_file::get_contents(AC_STATUS_FILE).unwrap();

    daemonize();

    battery_info.old_status = old_status;
    battery_info.old_ac_status = old_ac_status.chars().next().unwrap_or('0');

    update(&mut battery_info);

    loop {
        let new_ac_status: String = read_file::get_contents(AC_STATUS_FILE).unwrap();

        let status = battery.state();

        // Put the new data into the battery info
        battery_info.new_status = status;
        battery_info.new_ac_status = new_ac_status.chars().next().unwrap_or('0');

        if status_changed(&battery_info) {
            try_change(device, &mut battery_info);
        }
        loop_update(&manager, &mut battery_info, &mut battery, sleep_duration)?;
    }
}

/* Performs checks to determine if we need to change the screen gamma
 * Returns a Result with a success value of (), and a battery::Error if there was an error changing
 * the screen Gamma
 */
fn perform_screen_change<T: Backlight>(device: &T, info: &BatteryInfo) -> Result<u32, Error> {
    let gamma: u32 = calc_new_brightness(info);

    match device.change_gamma(gamma) {
        Ok(_) => Ok(gamma),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::config::Config;
    use battery::State;

    struct MockMonitorDevice;

    impl Backlight for MockMonitorDevice {
        fn change_gamma(&self, gamma: u32) -> Result<(), Error> {
            if gamma > 255 {
                return Err(Error::InvalidBrightnessLevel {
                    given: gamma,
                    max: 255,
                });
            }

            Ok(())
        }
    }

    impl MockMonitorDevice {
        fn new() -> Self {
            MockMonitorDevice
        }
    }

    #[test]
    fn test_successful_brightness_change() {
        let device = MockMonitorDevice::new();

        let battery_info1 = BatteryInfo {
            soc: 0.75,
            old_status: State::Charging,
            new_status: State::Discharging,
            old_ac_status: 'C',
            new_ac_status: 'D',
            gamma_values: Box::new(Config {
                full: 225,
                low: 100,
                low_perc: 25,
                charging: 255,
                discharging: 155,
                unknown: 155,
                ac_in: 225,
            }),
        };

        let result = perform_screen_change(&device, &battery_info1);

        assert!(result.is_ok());
    }

    #[test]
    fn test_brightness_change_failure() {
        let device = MockMonitorDevice::new();

        let battery_info1 = BatteryInfo {
            soc: 0.75,
            old_status: State::Charging,
            new_status: State::Discharging,
            old_ac_status: 'C',
            new_ac_status: 'D',
            gamma_values: Box::new(Config {
                full: 225,
                low: 100,
                low_perc: 25,
                charging: 255,
                discharging: 1155,
                unknown: 155,
                ac_in: 225,
            }),
        };

        let result = perform_screen_change(&device, &battery_info1);

        assert!(!result.is_ok());
    }

    #[test]
    fn test_try_change_success() {
        let device = MockMonitorDevice::new();

        let battery_info1 = BatteryInfo {
            soc: 0.75,
            old_status: State::Charging,
            new_status: State::Discharging,
            old_ac_status: 'C',
            new_ac_status: 'D',
            gamma_values: Box::new(Config {
                full: 225,
                low: 100,
                low_perc: 25,
                charging: 255,
                discharging: 155,
                unknown: 155,
                ac_in: 225,
            }),
        };

        let result = try_change(&device, &battery_info1);

        assert!(result == 0);
    }

    #[test]
    fn test_try_change_failure() {
        let device = MockMonitorDevice::new();

        let battery_info1 = BatteryInfo {
            soc: 0.75,
            old_status: State::Charging,
            new_status: State::Charging,
            old_ac_status: 'C',
            new_ac_status: 'C',
            gamma_values: Box::new(Config {
                full: 225,
                low: 100,
                low_perc: 25,
                charging: 2355,
                discharging: 155,
                unknown: 155,
                ac_in: 225,
            }),
        };

        let result = try_change(&device, &battery_info1);

        assert!(result == 1);
    }

    #[test]
    fn test_change() {
        let mut battery_info1 = BatteryInfo {
            soc: 75.0,
            old_status: State::Charging,
            new_status: State::Discharging,
            old_ac_status: 'C',
            new_ac_status: 'D',
            gamma_values: Box::new(Config {
                full: 225,
                low: 100,
                low_perc: 25,
                charging: 255,
                discharging: 155,
                unknown: 155,
                ac_in: 225,
            }),
        };

        assert_eq!(true, status_changed(&battery_info1));
        battery_info1.old_status = State::Discharging;
        battery_info1.old_ac_status = 'D';
        assert_eq!(false, status_changed(&battery_info1));
    }

    #[test]
    fn test_update() {
        let mut battery_info1 = BatteryInfo {
            soc: 75.0,
            old_status: State::Charging,
            new_status: State::Discharging,
            old_ac_status: 'C',
            new_ac_status: 'D',
            gamma_values: Box::new(Config {
                full: 225,
                low: 100,
                low_perc: 25,
                charging: 255,
                discharging: 155,
                unknown: 155,
                ac_in: 225,
            }),
        };

        update(&mut &mut battery_info1);
        assert_eq!(
            true,
            battery_info1.old_ac_status == battery_info1.new_ac_status
        );
        assert_eq!(true, battery_info1.old_status == battery_info1.new_status);
    }

    #[test]
    fn test_new_gamma_unknown() {
        let gamma_values: Config = Config {
            full: 200,
            low: 100,
            low_perc: 20,
            charging: 200,
            discharging: 155,
            unknown: 155,
            ac_in: 200,
        };

        let test_info: BatteryInfo = BatteryInfo {
            soc: 0.5,
            old_status: State::Unknown,
            new_status: State::Unknown,
            old_ac_status: '0',
            new_ac_status: '0',
            gamma_values: Box::new(gamma_values),
        };

        let gamma = calc_new_brightness(&test_info);

        assert_eq!(gamma, 155);
        let gamma_values: Config = Config {
            full: 200,
            low: 100,
            low_perc: 20,
            charging: 200,
            discharging: 155,
            unknown: 155,
            ac_in: 200,
        };

        let test_info: BatteryInfo = BatteryInfo {
            soc: 50.0,
            old_status: State::Full,
            new_status: State::Unknown,
            old_ac_status: '1',
            new_ac_status: '0',
            gamma_values: Box::new(gamma_values),
        };

        let gamma = calc_new_brightness(&test_info);

        assert_eq!(gamma, 155);
    }

    #[test]
    fn test_new_gamma_charging() {
        let gamma_values: Config = Config {
            full: 200,
            low: 100,
            low_perc: 20,
            charging: 255,
            discharging: 155,
            unknown: 155,
            ac_in: 200,
        };

        let test_info: BatteryInfo = BatteryInfo {
            soc: 0.5,
            old_status: State::Unknown,
            new_status: State::Charging,
            old_ac_status: '0',
            new_ac_status: '1',
            gamma_values: Box::new(gamma_values),
        };

        let gamma = calc_new_brightness(&test_info);

        assert_eq!(gamma, 255);

        let gamma_values: Config = Config {
            full: 200,
            low: 100,
            low_perc: 20,
            charging: 255,
            discharging: 155,
            unknown: 155,
            ac_in: 200,
        };

        let test_info: BatteryInfo = BatteryInfo {
            soc: 0.5,
            old_status: State::Discharging,
            new_status: State::Charging,
            old_ac_status: '1',
            new_ac_status: '1',
            gamma_values: Box::new(gamma_values),
        };

        let gamma = calc_new_brightness(&test_info);

        assert_eq!(gamma, 255);
    }
    #[test]
    fn test_new_gamma_full() {
        let gamma_values: Config = Config {
            full: 220,
            low: 100,
            low_perc: 20,
            charging: 200,
            discharging: 155,
            unknown: 155,
            ac_in: 200,
        };

        let test_info: BatteryInfo = BatteryInfo {
            soc: 1.0,
            old_status: State::Unknown,
            new_status: State::Full,
            old_ac_status: '0',
            new_ac_status: '0',
            gamma_values: Box::new(gamma_values),
        };

        let gamma = calc_new_brightness(&test_info);

        assert_eq!(gamma, 220);

        let gamma_values: Config = Config {
            full: 220,
            low: 100,
            low_perc: 20,
            charging: 200,
            discharging: 155,
            unknown: 155,
            ac_in: 200,
        };

        let test_info: BatteryInfo = BatteryInfo {
            soc: 1.0,
            old_status: State::Discharging,
            new_status: State::Full,
            old_ac_status: '1',
            new_ac_status: '1',
            gamma_values: Box::new(gamma_values),
        };

        let gamma = calc_new_brightness(&test_info);

        assert_eq!(gamma, 220);
    }

    #[test]
    fn test_new_gamma_low() {
        let gamma_values: Config = Config {
            full: 200,
            low: 100,
            low_perc: 20,
            charging: 200,
            discharging: 155,
            unknown: 155,
            ac_in: 200,
        };

        let test_info: BatteryInfo = BatteryInfo {
            soc: 0.2,
            old_status: State::Unknown,
            new_status: State::Discharging,
            old_ac_status: '0',
            new_ac_status: '0',
            gamma_values: Box::new(gamma_values),
        };

        let gamma = calc_new_brightness(&test_info);

        assert_eq!(gamma, 100);
    }

    #[test]
    fn test_new_gamma_discharging() {
        let gamma_values: Config = Config {
            full: 200,
            low: 100,
            low_perc: 20,
            charging: 200,
            discharging: 155,
            unknown: 155,
            ac_in: 200,
        };

        let test_info: BatteryInfo = BatteryInfo {
            soc: 50.0,
            old_status: State::Unknown,
            new_status: State::Unknown,
            old_ac_status: '0',
            new_ac_status: '0',
            gamma_values: Box::new(gamma_values),
        };

        let gamma = calc_new_brightness(&test_info);

        assert_eq!(gamma, 155);
    }

    #[test]
    fn test_new_gamma_unknown_no_ac() {
        let gamma_values: Config = Config {
            full: 200,
            low: 100,
            low_perc: 20,
            charging: 200,
            discharging: 155,
            unknown: 155,
            ac_in: 200,
        };

        let test_info: BatteryInfo = BatteryInfo {
            soc: 50.0,
            old_status: State::Unknown,
            new_status: State::Unknown,
            old_ac_status: '0',
            new_ac_status: '0',
            gamma_values: Box::new(gamma_values),
        };

        let gamma = calc_new_brightness(&test_info);

        assert_eq!(gamma, 155);
    }

    #[test]
    fn test_new_gamma_unknown_ac() {
        let gamma_values: Config = Config {
            full: 200,
            low: 100,
            low_perc: 20,
            charging: 200,
            discharging: 155,
            unknown: 155,
            ac_in: 200,
        };

        let test_info: BatteryInfo = BatteryInfo {
            soc: 50.0,
            old_status: State::Unknown,
            new_status: State::Unknown,
            old_ac_status: '0',
            new_ac_status: '1',
            gamma_values: Box::new(gamma_values),
        };

        let gamma = calc_new_brightness(&test_info);

        assert_eq!(gamma, 200);
    }

    #[test]
    fn test_new_gamma_discharging_low_soc() {
        let gamma_values: Config = Config {
            full: 200,
            low: 100,
            low_perc: 24,
            charging: 200,
            discharging: 155,
            unknown: 155,
            ac_in: 200,
        };

        let test_info: BatteryInfo = BatteryInfo {
            soc: 0.0,
            old_status: State::Unknown,
            new_status: State::Unknown,
            old_ac_status: '0',
            new_ac_status: '0',
            gamma_values: Box::new(gamma_values),
        };

        let gamma = calc_new_brightness(&test_info);

        assert_eq!(gamma, 100);
    }
    #[test]
    fn test_new_gamma_empty() {
        let gamma_values: Config = Config {
            full: 200,
            low: 100,
            low_perc: 24,
            charging: 200,
            discharging: 155,
            unknown: 155,
            ac_in: 200,
        };

        let test_info: BatteryInfo = BatteryInfo {
            soc: 0.0,
            old_status: State::Unknown,
            new_status: State::Empty,
            old_ac_status: '0',
            new_ac_status: '0',
            gamma_values: Box::new(gamma_values),
        };

        let gamma = calc_new_brightness(&test_info);

        assert_eq!(gamma, 10);
    }
}
