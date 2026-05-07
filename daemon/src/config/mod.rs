use serde::{Deserialize, Serialize};
use std::error;
use std::fs;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GammaDaemonConfig {
    pub backlight_config: BacklightConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacklightConfig {
    pub poll_interval: u64,  // how often to check the battery status, in seconds
    pub gamma_full: u64,     // the gamma value to set when the battery is full (100%)
    pub gamma_charging: u64, // the gamma value to set when the battery is charging but not full
    pub gamma_discharging: u64, // the gamma value to set when the battery is discharging
    pub gamma_plugged: u64, // the gamma value to set when the battery is plugged in but not charging
    pub gamma_unknown: u64, // the gamma value to set when the battery status is unknown
    pub gamma_low_percentage: f64, // the battery percentage threshold below which to apply the low battery gamma value
}

impl Default for BacklightConfig {
    /// Returns sensible default backlight settings (60s polling, gamma 100/80/60/90/70,
    /// low-battery threshold at 20%).
    fn default() -> Self {
        Self {
            poll_interval: 60,
            gamma_full: 100,
            gamma_charging: 80,
            gamma_discharging: 60,
            gamma_plugged: 90,
            gamma_unknown: 70,
            gamma_low_percentage: 20.0,
        }
    }
}

/// Reads and parses a `GammaDaemonConfig` from the TOML file at `path`.
/// Returns an error if the file can't be read or the contents fail to deserialize.
pub fn read_config_from_file(path: &str) -> Result<GammaDaemonConfig, Box<dyn error::Error>> {
    let config_content = fs::read_to_string(path)?;
    let config: GammaDaemonConfig = toml::from_str(&config_content)?;
    Ok(config)
}

mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = BacklightConfig::default();
        assert_eq!(config.poll_interval, 60);
        assert_eq!(config.gamma_full, 100);
        assert_eq!(config.gamma_charging, 80);
        assert_eq!(config.gamma_discharging, 60);
        assert_eq!(config.gamma_plugged, 90);
        assert_eq!(config.gamma_unknown, 70);
        assert_eq!(config.gamma_low_percentage, 20.0);
    }

    #[test]
    fn test_failed_to_load() {
        let result = read_config_from_file("non_existent_config.toml");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_valid_config() {
        let config_content = r#"
[backlight_config]
poll_interval = 30
gamma_full = 110
gamma_charging = 90
gamma_discharging = 70
gamma_plugged = 95
gamma_unknown = 75
gamma_low_percentage = 15.0
        "#;
        let temp_file_path = "temp_config.toml";
        fs::write(temp_file_path, config_content).expect("Failed to write temp config file");

        let result = read_config_from_file(temp_file_path);
        assert!(result.is_ok());
        let config = result.unwrap().backlight_config;
        assert_eq!(config.poll_interval, 30);
        assert_eq!(config.gamma_full, 110);
        assert_eq!(config.gamma_charging, 90);
        assert_eq!(config.gamma_discharging, 70);
        assert_eq!(config.gamma_plugged, 95);
        assert_eq!(config.gamma_unknown, 75);
        assert_eq!(config.gamma_low_percentage, 15.0);
        fs::remove_file(temp_file_path).expect("Failed to remove temp config file");
    }
}
