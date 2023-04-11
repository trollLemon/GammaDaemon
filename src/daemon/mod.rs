use bulbb::monitor::MonitorDevice;
use std::thread;
use std::time::Duration;
use battery::State;

mod read_file;
mod file_paths;
mod config;

use crate::daemon::config::Config;



// struct to bundle useful information about the notebooks battery 
// and AC charger.
//
// We'll use this to check if the battery status changed or if the AC is plugged in
pub struct BatteryInfo{
  old_status: State,
  new_status: State, 
  old_ac_status: char,
  new_ac_status: char,
  gamma_values : Config,

}

    // Constructor for the Battery Info
    // Initially sets all values to either unknown and 0 for the state and AC status
    // These will be updated during the Daemons run time
    fn new_battery_info(gamma_values : Config)->BatteryInfo {
        BatteryInfo{
        old_status:
            State::Unknown,
                new_status:
                State::Unknown,
                old_ac_status:
                '0',
                new_ac_status:
                '0',
                gamma_values
        }
    }

    // updates old status variables so we can compare them in the next iteration of the program loop
    // Assumes new() has been called by the client code.
    fn update(info: &mut BatteryInfo) {
        info.old_status = info.new_status;
        info.old_ac_status = info.new_ac_status;
    }



    /*  Returns a u32 for the new brightness we shall set for the MonitorDevice
 *  
 *  This method requires  reference to the battery's state, and a reference to a battery info struct
 *  
 *  Note: If the battery state is unknown, the AC may still be plugged in. This is the case for my
 *  Lenovo Thinkpad.
 *
 *  If this is the case, and the AC is plugged in, the screen brightness will be the same as if the
 *  laptop was Charging
 *
 *
 * */
fn calc_new_brightness( state: &battery::State, info: &BatteryInfo) -> u32 {
    
    let config = &info.gamma_values;

    let gamma = match state {
       State::Full => {config.full},
       State::__Nonexhaustive=> {128},
       State::Charging => {config.charging},
       State::Discharging => {config.discharching},
       State::Empty=> {10},
       State::Unknown => {

           if info.new_ac_status == '1' {
               return  config.ac_in;
                }
            return config.unknown;
            },
        };

gamma
}

// Returns a bool showing if the battery has changed states.
// I.E: From State::Charging to State::Discharging
//
// This method requires references to an old battery state and a new one
fn status_changed( old: &battery::State, new: &battery::State) -> bool {
 old==new
}

// Performs the brightness change on the MonitorDevice.
// Returns a result containing with a () success type, and a bulbb::error if there is an error
// changing the brightness on the hardware.
// A gamma value must be passed into the method
//
fn change_brightness( device : &MonitorDevice, gamma: u32) -> Result<(),bulbb::error::Error> {
    device.set_brightness(gamma)
}


// Run the Daemon.
// Returns a result with a () success type, and a battery::Error if there is any issue reading from
// the notebook battery
pub fn run(device: &MonitorDevice) -> Result< (), battery::Error> {
  let delay: u64 = 1;// check for changes every second
  let sleep_duration = Duration::from_secs(delay);// Duration for the delay

        // Set up required variables
        let manager = battery::Manager::new()?;
        let mut battery = manager.batteries()?.next().unwrap()?;
        let old_status = battery.state();
        let config : Config = config::load_config();  
        let mut battery_info = new_battery_info(config);

        let  old_ac_status : String = read_file::get_contents(file_paths::AC_STATUS_FILE).unwrap();//Read from the AC status file on Linux
        battery_info.old_status = old_status; 
        battery_info.old_ac_status= old_ac_status.chars()
                                                 .next()
                                                 .unwrap_or('0');    // Set the old ac
                                                                     // status. If there
                                                                     // was a problem
                                                                     // reading the AC
                                                                     // data, default to
                                                                     // 0 or 'unplugged'

    loop {
        let new_ac_status:
            String = read_file::get_contents(file_paths::AC_STATUS_FILE)
                     .unwrap();  // Get updated AC status
        let status = battery.state();  // Get a new battery state

        // Put the new data into the battery info
        battery_info.new_status = status;
        battery_info.new_ac_status = new_ac_status.chars()
                                                  .next()
                                                  .unwrap_or('0');

        // Change brightness
        let process = perform_screen_change(device,&battery_info)
                .and_then(|()| { 
                // Update variables to current data 
                    update(&mut battery_info);
                    manager.refresh(&mut battery)
              }).and_then(|()| {
                    thread::sleep(sleep_duration);
                    Ok(())
              });

        match process {
        Ok(_) =>{},
        Err(e)=>{
        println!("Error during gamma change:\n {}",e);
        },

        }
    }
}

// Performs the checks to determine if we need to change the screen brightness
// Returns a Result with a success value of (), and a battery::Error if there was an error changing
// the screen brightness
//
// This method will do nothing if the status in the BatteryInfo has not changed
 fn perform_screen_change (device : &MonitorDevice, info: &BatteryInfo) -> Result<(), battery::Error> {
     
    let old_status = info.old_status;
    let status = info.new_status;
    if status_changed(&old_status, &status) {
        let gamma: u32 = calc_new_brightness(&status, info);
        match change_brightness(device,gamma) {
        Ok(_) => {}, Err(e) => {
            println !("Error changing gamma  {}", e);
        }
        ,
    }
          }
      
Ok(())
 }
 
