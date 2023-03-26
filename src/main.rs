
mod daemon;
use bulbb::monitor::MonitorDevice;
use battery::Error;
fn main() {

    //find monitor device 
    let monitors = MonitorDevice::get_all_monitor_devices().unwrap();
    
    //get a ref to the first monitor in the list of monitors
    let main_Monitor : &MonitorDevice = &monitors[0];

     let mut process: daemon::Daemon = daemon::Daemon::new(main_Monitor);
    
      process.run(); 
     

}
