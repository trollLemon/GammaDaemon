use xrandr::{XHandle, Monitor};

 mod daemon;

fn main() {
   

    //might move the following code to another function

    let mut main_display: Option<&Monitor> = None;
   

    //get a list of the displays on the system
    let monitors = XHandle::open().unwrap()
    .monitors().unwrap();

    //determine main display
    for i in &monitors {
            if i .is_primary { main_display = Some(i); }
            break;//dont need to look anymore at this point 
   }

    
    
    match main_display{
    Some(disp)=>{daemon::run(&disp.name).expect("Error with Daemon")},
    None => eprintln!("Could not find a display!")
    }
    

}




