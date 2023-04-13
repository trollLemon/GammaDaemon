/*  Contains utility function to read the config file for gamma values.
 *  We'll deserialize a toml config file in the users .config directory.
 *
 *  If the config file is absent, we'll supply a default config as a String
 *  
 * */

use serde::Deserialize;
use std::fs;
use std::env;


// Config struct
// all values are public so we can access them with the member access operator
#[derive(Deserialize,PartialEq, Eq, Debug)]
pub struct Config {
    pub full:
        u32,
        pub charging:
        u32,
        pub discharching:
        u32,
        pub unknown:
        u32,
        pub ac_in: u32,

}

/* Returns a config struct with the user config values
 * If there is no config file, or the ENV var is not set: a default config is supplied to serde 
 *
 * */
pub fn load_config() ->Config {

 
    //default config
    
    const DEFAULT: Config = Config {full: 225, charging: 255, discharching: 155, unknown: 155, ac_in:225};

    let gamma_values;
    let env: String = match  env::var("USER") {
    Ok(s) => {s},
    Err(_) => {
       "NAN".to_string() 
    }
    };
    


    let config_file = "/home/".to_owned() + &env + "/.config/GammaDaemon/conf.toml";
    let contents = fs::read_to_string(config_file).unwrap_or("full = 255\ncharging = 255\ndischarching = 155\nunknown = 200\nac_in = 255".to_string());
    gamma_values = toml::from_str(&contents).unwrap_or( DEFAULT);

    gamma_values
}





#[cfg(test)]
mod tests{

    use super::*;

// test if we get a default config if there is a problem reading the config file and or env vars
// are not set by the user
#[test]
fn test_default(){
    
    const DEFAULT: Config = Config {full: 225, charging: 255, discharching: 155, unknown: 155, ac_in:225};
    let env: String = match  env::var("USER") {
    Ok(s) => {s},
    Err(_) => {
       "NAN".to_string() 
      }
    };
    

    if env == "NAN".to_string() {
        let test_config : Config = load_config();
        assert_eq!(test_config,DEFAULT);

    } 

}

}
