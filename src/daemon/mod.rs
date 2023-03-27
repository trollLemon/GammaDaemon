use bulbb::monitor::MonitorDevice;
use std::thread;
use std::time::Duration;
use battery::State;

mod read_file;
mod file_paths;
pub struct Daemon<'a> {

    device: &'a MonitorDevice,
}

struct BatteryInfo {
  old_status: State,
  new_status: State, 
  old_ac_status: char,
  new_ac_status: char,

}


impl BatteryInfo  {


pub fn new ()-> Self{ 
Self {old_status: State::Unknown, new_status: State::Unknown, old_ac_status: '0', new_ac_status: '0'  }

}

pub fn update(&mut self) {

    self.old_status=self.new_status;
    self.old_ac_status=self.new_ac_status;

}

}

impl<'a> Daemon<'a> {





fn calc_new_brightness( &'a self, state: &battery::State) -> u32 {

    let gamma = match state {
       State::Full => {3},
       State::__Nonexhaustive=> {128},
       State::Charging => {128},
       State::Discharging => {80},
       State::Empty=> {10},
       State::Unknown => {64},
    };


gamma
}



fn status_changed(&'a self, old: &battery::State, new: &battery::State) -> bool {
 old==new
}


fn change_brightness( &'a self, gamma: u32) -> Result<(),bulbb::error::Error> {
    self.device.set_brightness(gamma)
}




//creates a new Daemon Instace
//Assumes there is a valid MonitorDevice so we can change screen
//brightness
pub fn new (main_disp: &'a MonitorDevice  )-> Self{

    
Self { device: (main_disp) }

}


pub fn run(&'a mut self) -> Result< (), battery::Error> {
  let delay: u64 = 1;
  let sleep_duration = Duration::from_secs(delay);

          let manager = battery::Manager::new()?;
          let  mut battery = manager.batteries()?.next().unwrap()?; 
          let  old_status = battery.state();   
          
          let mut  battery_info = BatteryInfo::new();
          let  old_ac_status : String = read_file::ReadFile::get_contents(file_paths::AC_STATUS_FILE).unwrap();
          battery_info.old_status = old_status;
          battery_info.old_ac_status= old_ac_status.chars().next().unwrap_or('0');

          loop {
            let new_ac_status : String = read_file::ReadFile::get_contents(file_paths::AC_STATUS_FILE).unwrap();
            let status = battery.state();  
            battery_info.new_status = status;
            battery_info.new_ac_status = new_ac_status.chars().next().unwrap_or('0');
            thread::sleep(sleep_duration);
            self.perform_screen_change(&battery_info)?; 
            manager.refresh(&mut battery).ok();
            battery_info.update();

  }

}

 fn perform_screen_change (&'a self, info: &BatteryInfo) -> Result<(), battery::Error> {
     
            let old_status = info.old_status;
            let status = info.new_status;
            dbg!(&old_status);
            dbg!(&status);
          if self.status_changed(&old_status, &status) {

          let gamma: u32 = self.calc_new_brightness(&status);
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
