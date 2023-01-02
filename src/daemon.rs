use battery::{Battery, Manager, State};
use std::process::Command;
use std::thread;
use std::time::Duration;

#[derive(Debug)]
enum BatteryStatus {
    CRITICAL,
    LOW,
    MEDIUM,
    HIGH,
    CHARGING,
}

//run a thread:
//this thread will run a function to check the battery status of the device
//it will then call change_brightness to change the screen brighness to a certain
//value.
//this is done every 5 seconds
//TODO:config file in the users .config directory should dictate how many seconds the thread
//will wait before checking
pub fn run(display: &String) -> Result<(), battery::Error> {
    let display_name: String = String::from(display);

    thread::spawn(move || -> Result<(), battery::Error> {
        let delay: u64 = 1;
        let sleep = Duration::from_secs(delay); //the thread should sleep for this period of time so we
                                                //arent checking the battery status every milisecond

        let manager = Manager::new()?;
        let mut battery = manager.batteries()?.next().unwrap()?;

        //program loop
        loop {
            manager.refresh(&mut battery).unwrap();
            let status: BatteryStatus = get_battery_status(&mut battery);
            change_brightness(&display_name, determine_brightness(&status));

            thread::sleep(sleep);
        }
    })
    .join()
    .ok();

    Ok(())
}

//returns an value for the gamma flag for xrandr based on the battery status
fn determine_brightness(status: &BatteryStatus) -> f32 {
    let gamma_values: Vec<f32> = vec![1.0, 0.8, 0.5, 0.3];
    match status {
        BatteryStatus::LOW => gamma_values[2],
        BatteryStatus::CRITICAL => gamma_values[3],
        BatteryStatus::CHARGING => gamma_values[0],
        _ => gamma_values[1],
    }
}

/* Returns an enum that says if the battery is:
 * Charging : On AC power
 * High     : battery has a high charge
 * Medium   : Battery has a decent charge
 * Low      : Battery has a low charge
 * Critical : Battery will die soon
 * */
fn get_battery_status(battery: &mut Battery) -> BatteryStatus {
    let charge = battery.state_of_charge().value;
    let is_charging = battery.state() == State::Charging;

    if is_charging {
        return BatteryStatus::CHARGING;
    }

    if charge > 0.8 {
        return BatteryStatus::HIGH;
    }
    if charge > 0.5 {
        return BatteryStatus::MEDIUM;
    }

    if charge > 0.2 {
        return BatteryStatus::LOW;
    }

    BatteryStatus::CRITICAL
}

//runs xrandr on the system to change screen brighness. Takes in a reference to a string for the
//display name, and a f32 for the gamma value.
fn change_brightness(display_name: &String, gamma: f32) {
    let gamma = gamma.to_string(); //need to pass as a string

    match Command::new("xrandr")
        .arg("--output")
        .arg(display_name)
        .arg("--brightness")
        .arg(gamma)
        .output()
    {
        Err(e) => {
            eprintln!("failed to run xrandr: {}", e)
        }
        _ => {}
    };
}
