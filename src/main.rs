use std::process::Command;
use xrandr::{XHandle, XrandrError, Monitor};
fn main()->Result<(), XrandrError> {
    let output =
        Command::new ("xrandr").status().expect("failed to execute process");
    let monitors = XHandle::open() ?.monitors() ? ;
    let mut main_display: Option<&Monitor> = None;

   for i in &monitors {
            if i .is_primary { main_display = Some(i); }
       }

        panic!("Cannot find any displays!");
    

    Ok(())
}
