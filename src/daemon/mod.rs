/* This contains the daemon functionality
 *
 * Only the run function is public, since the client code doesn't need to
 * do any of the gamma calculations or changes, that's all abstracted away in this file
 *
 */

use battery::State;
use bulbb::monitor::MonitorDevice;
use bulbb::error::Error;
use daemonize::Daemonize;
use std::fs::File;
use std::thread;
use std::time::Duration;
mod config;
mod read_file;

use crate::daemon::config::Config;

pub const AC_STATUS_FILE: &str = "/sys/class/power_supply/AC/online"; //this is the AC status file

/* struct to bundle useful information about the notebooks battery
 * and AC charger.
 *
 * We'll use this to check if the battery status changed or if the AC is plugged in
 */
#[derive(Debug)]
pub struct BatteryInfo {
    old_status: State,
    new_status: State,
    old_ac_status: char,
    new_ac_status: char,
    gamma_values: Box<Config>,
}

// Make a struct for our Battery Info
// Initially sets all values to either unknown and 0 for the state and AC status
// These will be updated during the Daemons run time
fn new_battery_info(gamma_values: Config) -> BatteryInfo {
    BatteryInfo {
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
dbg!(&info);

}

/*  Returns a u32 for the new brightness we shall set for the MonitorDevice
 *
 *  This function requires  reference to the battery's state, and a reference to a battery info struct
 *
 *  Depending on the state of the battery and the AC, this function will set the gamma to the matching value mapped to the state in
 *  the config file
 *
 * */
fn calc_new_brightness(state: &battery::State, info: &BatteryInfo) -> u32 {
    let config = &info.gamma_values; // user config values

    // calculate gamma based on the battery state
    match (state, info.new_ac_status) {
        (State::Full, _) => config.full,
        (State::__Nonexhaustive, _) => 128, // not implemented in the battery crate yet, we'll ignore it
        (State::Charging, _) => config.charging,
        (State::Discharging, _) => config.discharging,
        (State::Empty, _) => 10,
        (State::Unknown, '1') => config.ac_in,
        (State::Unknown, _) => config.discharging,
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


  dbg!(&old_status, &new_status, &old_ac_status, &new_ac_status);

   old_status != new_status || old_ac_status != new_ac_status



    
}


fn daemonize() {
    let stdout = File::create("/tmp/gamma_daemon.out").unwrap();
    let stderr = File::create("/tmp/gamma_daemon.err").unwrap();

    let daemonize = Daemonize::new()
        .pid_file("/tmp/gamma_daemon.pid") // Every method except `new` and `start`
        .working_directory("/tmp") // for default behaviour.
        .group("video") // Group name
        .stdout(stdout) // Redirect stdout to `/tmp/daemon.out`.
        .stderr(stderr) // Redirect stderr to `/tmp/daemon.err`.
        .privileged_action(|| "Executed before drop privileges");

    match daemonize.start() {
        Ok(_) => println!("gamma_daemon started"),
        Err(e) => eprintln!("{}", e),
    }
}

/* Run the Daemon.
 * Returns a result with a () success type, and a battery::Error if there is any issue reading from
 * the notebook battery
 *
 * This function requires a reference to a laptop monitor
 */
pub fn run(device: &MonitorDevice) -> Result<(), battery::Error> {
    let delay: u64 = 1; // check for changes every second
    let sleep_duration = Duration::from_secs(delay); // Duration for the delay

    // Set up required variables
    let manager = battery::Manager::new()?;
    let mut battery = manager.batteries()?.next().unwrap()?;
    let old_status = battery.state();
    let config: Config = config::load_config();
    let mut battery_info = Box::new(new_battery_info(config));

    let old_ac_status: String = read_file::get_contents(AC_STATUS_FILE).unwrap(); //Read from the AC status file on Linux

//    daemonize();

    battery_info.old_status = old_status;
    // set the ac status to what is currently in the ac status file
    // if there is any issue reading the file, just have it be 0
    battery_info.old_ac_status = old_ac_status.chars().next().unwrap_or('0');

    match perform_screen_change(&device, &battery_info, &old_status) {
     Ok(g) => {
                println!("Changed gamma to {}", g);
                update(&mut battery_info);
                manager.refresh(&mut battery)?;
            }
            //If there is an error changing the gamma, print an error
            Err(e) => {
                println!("Error changing gamma: {}", e);
            }
        };


    update(&mut battery_info);
    loop {
        let new_ac_status: String = read_file::get_contents(AC_STATUS_FILE).unwrap(); // Get updated AC status

        let status = battery.state();  // Get a new battery state
        
        // Put the new data into the battery info
        battery_info.new_status = status;
        battery_info.new_ac_status = new_ac_status.chars().next().unwrap_or('0');
        
        if status_changed(&mut battery_info){
        
        // Change gamma
        match perform_screen_change(device, &battery_info, &status) {
            // Update variables to current data
            Ok(g) => {
                  
                               println!("Changed gamma to {}", g);
                }
            //If there is an error changing the gamma, print an error
            Err(e) => {
                println!("Error changing gamma: {}", e);
            }
        };

        }
            update(&mut battery_info);
                manager.refresh(&mut battery)?;
        

        thread::sleep(sleep_duration);
    }
}

/* Performs checks to determine if we need to change the screen gamma
 * Returns a Result with a success value of (), and a battery::Error if there was an error changing
 * the screen Gamma
 */
fn perform_screen_change(device: &MonitorDevice, info: &BatteryInfo, status: &State)-> Result<u32, Error> {
     
        let gamma: u32 = calc_new_brightness(status, info);
            
         match device.set_brightness(gamma)  {
        Ok(_)=> Ok(gamma),
        Err(e)=> Err(e),

         }

}

#[cfg(test)]
mod tests {
    use super::calc_new_brightness;
    use super::BatteryInfo;
    use crate::daemon::config::Config;
    use battery::State;

    #[test]
    fn test_new_gamma_charging() {
        let gamma_values: Config = Config {
            full: 200,
            charging: 200,
            discharging: 155,
            unknown: 155,
            ac_in: 200,
        };

        let test_info: BatteryInfo = BatteryInfo {
            old_status: State::Unknown,
            new_status: State::Unknown,
            old_ac_status: '0',
            new_ac_status: '0',
            gamma_values: Box::new(gamma_values),
        };

        let gamma = calc_new_brightness(&State::Charging, &test_info);

        assert_eq!(gamma, 200);
    }

    #[test]
    fn test_new_gamma_disharging() {
        let gamma_values: Config = Config {
            full: 200,
            charging: 200,
            discharging: 155,
            unknown: 155,
            ac_in: 200,
        };

        let test_info: BatteryInfo = BatteryInfo {
            old_status: State::Unknown,
            new_status: State::Unknown,
            old_ac_status: '0',
            new_ac_status: '0',
            gamma_values: Box::new(gamma_values),
        };

        let gamma = calc_new_brightness(&State::Discharging, &test_info);

        assert_eq!(gamma, 155);
    }

    #[test]
    fn test_new_gamma_unknown_no_ac() {
        let gamma_values: Config = Config {
            full: 200,
            charging: 200,
            discharging: 155,
            unknown: 155,
            ac_in: 200,
        };

        let test_info: BatteryInfo = BatteryInfo {
            old_status: State::Unknown,
            new_status: State::Unknown,
            old_ac_status: '0',
            new_ac_status: '0',
            gamma_values: Box::new(gamma_values),
        };

        let gamma = calc_new_brightness(&State::Unknown, &test_info);

        assert_eq!(gamma, 155);
    }

    #[test]
    fn test_new_gamma_unknown_ac() {
        let gamma_values: Config = Config {
            full: 200,
            charging: 200,
            discharging: 155,
            unknown: 155,
            ac_in: 200,
        };

        let test_info: BatteryInfo = BatteryInfo {
            old_status: State::Unknown,
            new_status: State::Unknown,
            old_ac_status: '0',
            new_ac_status: '1',
            gamma_values: Box::new(gamma_values),
        };

        let gamma = calc_new_brightness(&State::Unknown, &test_info);

        assert_eq!(gamma, 200);
    }
}
