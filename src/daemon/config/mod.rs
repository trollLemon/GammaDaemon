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

/* Returns a config struct with the user config values
 * If there is no config file, a default config is supplied to serde *
 *
 * */
pub fn load_config()
    ->Config {
    let config_env: String = env::var("USER").expect("ENV Var not set");
    let config_file =
        "/home/".to_owned() + &config_env + "/.config/GammaDaemon/conf.toml";
    let contents = fs::read_to_string(config_file).unwrap_or("full = 255\ncharging = 255\ndischarching = 155\nunknown = 200\nac_in = 255".to_string());

    let gamma_values:
        Config = toml::from_str(&contents).expect("Error Reading Config File");

    gamma_values
}
