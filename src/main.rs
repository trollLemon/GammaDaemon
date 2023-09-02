mod daemon;
use bulbb::monitor::MonitorDevice;
fn main() {
    //find monitor device
    let monitors = MonitorDevice::get_all_monitor_devices().unwrap();

    //get a ref to the first monitor in the list of monitors
    let main_monitor: &MonitorDevice = &monitors[0];

    //start the daemon
    daemon::run(main_monitor).unwrap();
}
