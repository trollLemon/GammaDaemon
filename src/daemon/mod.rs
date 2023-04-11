use bulbb::monitor::MonitorDevice;
use std::thread;
use std::time::Duration;
use battery::State;


mod read_file;
mod file_paths;
mod config;

 use crate::daemon::config::Config;



// Daemon struct with the MonitorDevice. 
pub struct Daemon<'a> { 

    device: &'a MonitorDevice,
}


// struct to bundle useful information about the notebooks battery 
// and AC charger.
//
// We'll use this to check if the battery status changed or if the AC is plugged in
struct BatteryInfo{
  old_status: State,
  new_status: State, 
  old_ac_status: char,
  new_ac_status: char,
  gamma_values : Config,

}

impl  BatteryInfo {


// Constructor for the Battery Info
// Initially sets all values to either unknown and 0 for the state and AC status
// These will be updated during the Daemons run time
pub fn new (gamma_values : Config)-> Self{

Self {old_status: State::Unknown, new_status: State::Unknown, old_ac_status: '0', new_ac_status: '0' , gamma_values: gamma_values  }

}

// updates old status variables so we can compare them in the next iteration of the program loop
// Assumes new() has been called by the client code.
pub fn update(&mut self) {

    self.old_status=self.new_status;
    self.old_ac_status=self.new_ac_status;

}

}

impl<'a> Daemon<'a> {




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
fn calc_new_brightness( &'a self, state: &battery::State, info: &BatteryInfo) -> u32 {
    
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
fn status_changed(&'a self, old: &battery::State, new: &battery::State) -> bool {
 old==new
}


// Performs the brightness change on the MonitorDevice.
// Returns a result containing with a () success type, and a bulbb::error if there is an error
// changing the brightness on the hardware.
// A gamma value must be passed into the method 
// 
fn change_brightness( &'a self, gamma: u32) -> Result<(),bulbb::error::Error> {
    self.device.set_brightness(gamma)
}




//creates a new Daemon Instance
//Assumes there is a valid MonitorDevice so we can change screen
//brightness
pub fn new (main_disp: &'a MonitorDevice  )-> Self{

    
Self { device: (main_disp) }

}


// Run the Daemon.
// Returns a result with a () success type, and a battery::Error if there is any issue reading from
// the notebook battery
pub fn run(&'a mut self) -> Result< (), battery::Error> {
  let delay: u64 = 1;// check for changes every second
  let sleep_duration = Duration::from_secs(delay);// Duration for the delay

        // Set up required variables
        let manager = battery::Manager::new()?;//Get a battery Manager so we can read battery
                                                 //data
        let  mut battery = manager.batteries()?.next().unwrap()?; // Get the first battery in the
                                                                    // notebook
        let  old_status = battery.state();   // Get the state of the battery before the program
                                               // loop
        let config : Config = config::load_config();  
        let mut  battery_info = BatteryInfo::new(config); //make a battery info struct with our battery
                                                    //data
        let  old_ac_status : String = read_file::get_contents(file_paths::AC_STATUS_FILE).unwrap();//Read from the AC status file on Linux
        battery_info.old_status = old_status; 
        battery_info.old_ac_status= old_ac_status.chars().next().unwrap_or('0');// Set the old ac
                                                                                // status. If there
                                                                                // was a problem
                                                                                // reading the AC
                                                                                // data, default to
                                                                                // 0 or 'unplugged'

        loop {
           let new_ac_status : String = read_file::get_contents(file_paths::AC_STATUS_FILE).unwrap(); // Get updated AC status
           let status = battery.state();  // Get a new battery state
           
           // Put the new data into the battery info 
           battery_info.new_status = status; 
           battery_info.new_ac_status = new_ac_status.chars().next().unwrap_or('0');
          
           // Change brightness
           self.perform_screen_change(&battery_info)?; 
           
            // Update variables to current data
           manager.refresh(&mut battery).ok();
           battery_info.update();

           
           //wait before looping for a bit
           thread::sleep(sleep_duration);

  }

}


// Performs the checks to determine if we need to change the screen brightness
// Returns a Result with a success value of (), and a battery::Error if there was an error changing
// the screen brightness
//
// This method will do nothing if the status in the BatteryInfo has not changed
 fn perform_screen_change (&'a self, info: &BatteryInfo) -> Result<(), battery::Error> {
     
            let old_status = info.old_status;
            let status = info.new_status;
          if self.status_changed(&old_status, &status) {

          let gamma: u32 = self.calc_new_brightness(&status, info);
            match self.change_brightness(gamma) {
            Ok(_) => {},
            Err(e) => {
                println!("Error changing gamma  {}", e);
            },
            }
          }
      
Ok(())

}





}
