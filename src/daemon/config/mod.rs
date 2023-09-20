/*  Contains utility function to read the config file for gamma values.
 *  We'll deserialize a toml config file in the users .config directory.
 *
 *  If the config file is absent, we'll supply a default config as a String
 *
 * */

use serde::Deserialize;
use std::fs;

// Config struct
// all values are public so we can access them with the member access operator
#[derive(Deserialize, PartialEq, Eq, Debug)]
pub struct Config {
    pub full: u32,
    pub low: u32,
    pub low_perc: u32,
    pub charging: u32,
    pub discharging: u32,
    pub unknown: u32,
    pub ac_in: u32,
}

/* Returns a config struct with the user config values
 * If there is no config file, or the ENV var is not set: a default config is supplied to serde
 *
 * */
pub fn load_config(path: String) -> Config {
    const DEFAULT: Config = Config {
        full: 225,
        low: 100,
        low_perc: 25,
        charging: 255,
        discharging: 155,
        unknown: 155,
        ac_in: 225,
    };

    let contents = match fs::read_to_string(path) {
        Ok(stuff) => stuff,
        Err(e) => e.to_string(),
    };

    match toml::from_str(&contents) {
        Ok(conf) => conf,
        Err(e) => {
            eprintln!(
                "Error in config file:\n {} \n gamma_daemon will use the default config",
                e
            );
            DEFAULT
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_default() {
        const DEFAULT: Config = Config {
            full: 225,
            low: 100,
            low_perc: 25,
            charging: 255,
            discharging: 155,
            unknown: 155,
            ac_in: 225,
        };
            let test_config: Config =
                load_config("a path that doesnt have the file in it".to_string());
            assert_eq!(test_config, DEFAULT);
    }

    #[test]
    fn test_valid_config_file() {
        let temp_config = "full = 200\nlow=50\nlow_perc=10\ncharging = 180\ndischarging = 90\nunknown = 90\nac_in = 200".to_string();

        let temp_file_path = "../../test_config.toml".to_string();
        fs::write(&temp_file_path, temp_config).expect("Failed to write temporary config file");

        let test_config: Config = load_config(temp_file_path.clone());
        let expected_config = Config {
            full: 200,
            low: 50,
            low_perc: 10,
            charging: 180,
            discharging: 90,
            unknown: 90,
            ac_in: 200,
        };
        assert_eq!(test_config, expected_config);

        fs::remove_file(temp_file_path).expect("Failed to remove temporary config file");
    }

    #[test]
    fn test_missing_config_file() {
        std::env::remove_var("USER");
        let temp_file_path = "missing_test_config.toml".to_string();
        let test_config: Config = load_config(temp_file_path);
        let expected_config = Config {
            full: 225,
            low: 100,
            low_perc: 25,
            charging: 255,
            discharging: 155,
            unknown: 155,
            ac_in: 225,
        };
        assert_eq!(test_config, expected_config);
    }
}
