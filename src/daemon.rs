use bulbb::monitor::MonitorDevice;
use bulbb::error::Error;
use std::thread;
use std::time::Duration;
use battery::Manager;
use battery::State;
pub struct Daemon<'a> {

    device: &'a MonitorDevice,
}



impl<'a> Daemon<'a> {





fn calc_new_brightness( &'a self, state: battery::State) -> u32 {

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



fn status_changed(&'a self, old: battery::State, new: battery::State) -> bool {

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


pub fn run(&'a mut self) -> battery::Error {
  let delay: u64 = 1;
  let sleep_duration = Duration::from_secs(delay);



  loop {
      thread::sleep(sleep_duration);
      match self.perform_screen_change() {
          Ok(_) => {
          
              println!("Changed gamma");
          }, 
          Err(e) => {
            return e;
          }
      }
        

  }
}

 fn perform_screen_change (&'a self) -> Result<(), battery::Error> {
     let manager = battery::Manager::new()?;
  let mut battery = manager.batteries()?.next().unwrap()?;
          let old_status = battery.state();   
          manager.refresh(&mut battery).unwrap();
          let status : battery::State = battery.state();
          
          if self.status_changed(old_status, status) {

          let gamma: u32 = self.calc_new_brightness(status);
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
