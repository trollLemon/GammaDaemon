mod daemon;
use bulbb::monitor::MonitorDevice;
use std::env;

fn main() {
    //find monitor device
    let monitors = MonitorDevice::get_all_monitor_devices().unwrap();
    
    let mut args: Vec<String> = env::args().collect();
    //get a ref to the first monitor in the list of monitors
    let main_monitor: &MonitorDevice = &monitors[0];
    
    if args.len() == 1 {
        args.push("NAN".to_string());
    }
    //start the daemon
    daemon::run(main_monitor, &args[1]).unwrap();
}
