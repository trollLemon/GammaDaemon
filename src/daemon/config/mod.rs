/*  Contains utility function to read the config file for gamma values
 *  
 *  
 * */

use serde::Deserialize;
use std::fs;
use std::env;

#[derive(Deserialize)]
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

pub fn
load_config()
    ->Config {
    let config_env: String = env::var("USER").expect("ENV Var not set");
    let config_file =
        "/home/".to_owned() + &config_env + "/.config/GammaDaemon/conf.toml";
    let contents = fs::read_to_string(config_file).unwrap_or("full = 255\ncharging = 255\ndischarching = 155\nunknown = 200\nac_in = 255".to_string());

    let gamma_values:
        Config = toml::from_str(&contents).expect("Error Reading Config File");

    gamma_values
}
