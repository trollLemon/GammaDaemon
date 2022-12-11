use std::process::Command;

fn main()->Result<(), ()> {

    let output =
        Command::new ("xrandr").status().expect("failed to execute process");

    Ok(())
}
