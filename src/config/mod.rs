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
    pub gamma_full: f32,     // gamma fraction (0.0-1.0) to set when the battery is full
    pub gamma_charging: f32, // gamma fraction (0.0-1.0) to set when the battery is charging but not full
    pub gamma_discharging: f32, // gamma fraction (0.0-1.0) to set when the battery is discharging
    pub gamma_plugged: f32, // gamma fraction (0.0-1.0) to set when the battery is plugged in but not charging
    pub gamma_unknown: f32, // gamma fraction (0.0-1.0) to set when the battery status is unknown
    pub gamma_low: f32,     // gamma fraction (0.0-1.0) to set when the battery is low
    pub gamma_low_percentage: f32, // battery state-of-charge fraction (0.0-1.0) below which to apply gamma_low
}

impl Default for BacklightConfig {
    fn default() -> Self {
        Self {
            poll_interval: 5,
            gamma_full: 1.0,
            gamma_charging: 0.8,
            gamma_discharging: 0.6,
            gamma_plugged: 0.9,
            gamma_unknown: 0.7,
            gamma_low: 0.4,
            gamma_low_percentage: 0.2,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = BacklightConfig::default();
        assert_eq!(config.poll_interval, 5);
        assert_eq!(config.gamma_full, 1.0);
        assert_eq!(config.gamma_charging, 0.8);
        assert_eq!(config.gamma_discharging, 0.6);
        assert_eq!(config.gamma_plugged, 0.9);
        assert_eq!(config.gamma_unknown, 0.7);
        assert_eq!(config.gamma_low, 0.4);
        assert_eq!(config.gamma_low_percentage, 0.2);
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
gamma_full = 0.95
gamma_charging = 0.9
gamma_discharging = 0.7
gamma_plugged = 0.85
gamma_unknown = 0.75
gamma_low = 0.25
gamma_low_percentage = 0.15
        "#;
        let temp_file_path = "temp_config.toml";
        fs::write(temp_file_path, config_content).expect("Failed to write temp config file");

        let result = read_config_from_file(temp_file_path);
        assert!(result.is_ok());
        let config = result.unwrap().backlight_config;
        assert_eq!(config.poll_interval, 30);
        assert_eq!(config.gamma_full, 0.95);
        assert_eq!(config.gamma_charging, 0.9);
        assert_eq!(config.gamma_discharging, 0.7);
        assert_eq!(config.gamma_plugged, 0.85);
        assert_eq!(config.gamma_unknown, 0.75);
        assert_eq!(config.gamma_low, 0.25);
        assert_eq!(config.gamma_low_percentage, 0.15);
        fs::remove_file(temp_file_path).expect("Failed to remove temp config file");
    }

    fn read_config_from_contents(
        tag: &str,
        content: &str,
    ) -> Result<GammaDaemonConfig, Box<dyn error::Error>> {
        let path = std::env::temp_dir().join(format!(
            "gamma-daemon-cfg-{}-{}.toml",
            std::process::id(),
            tag
        ));
        fs::write(&path, content).expect("Failed to write temp config file");
        let result = read_config_from_file(path.to_str().expect("utf-8 path"));
        let _ = fs::remove_file(&path);
        result
    }

    #[test]
    fn test_gamma_daemon_config_default_matches_backlight_default() {
        let config = GammaDaemonConfig::default();
        let expected = BacklightConfig::default();
        assert_eq!(
            config.backlight_config.poll_interval,
            expected.poll_interval
        );
        assert_eq!(config.backlight_config.gamma_full, expected.gamma_full);
        assert_eq!(config.backlight_config.gamma_low, expected.gamma_low);
    }

    #[test]
    fn test_load_empty_file_errors() {
        let result = read_config_from_contents("empty", "");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_malformed_toml_errors() {
        let result = read_config_from_contents("malformed", "this is not = = valid toml");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_partial_config_errors() {
        let result =
            read_config_from_contents("partial", "[backlight_config]\npoll_interval = 10\n");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_config_with_wrong_field_type_errors() {
        let content = r#"
[backlight_config]
poll_interval = "not a number"
gamma_full = 1.0
gamma_charging = 0.8
gamma_discharging = 0.6
gamma_plugged = 0.9
gamma_unknown = 0.7
gamma_low = 0.4
gamma_low_percentage = 0.2
        "#;
        let result = read_config_from_contents("wrong-type", content);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_config_ignores_unknown_fields() {
        let content = r#"
[backlight_config]
poll_interval = 5
gamma_full = 1.0
gamma_charging = 0.8
gamma_discharging = 0.6
gamma_plugged = 0.9
gamma_unknown = 0.7
gamma_low = 0.4
gamma_low_percentage = 0.2
some_future_field = 123
        "#;
        let result = read_config_from_contents("unknown-fields", content);
        assert!(result.is_ok());
    }

    #[test]
    fn test_load_config_does_not_validate_value_ranges() {
        let content = r#"
[backlight_config]
poll_interval = 0
gamma_full = 9.0
gamma_charging = -1.0
gamma_discharging = 0.6
gamma_plugged = 0.9
gamma_unknown = 0.7
gamma_low = 0.4
gamma_low_percentage = 2.0
        "#;
        let result = read_config_from_contents("out-of-range", content);
        assert!(result.is_ok());
        let config = result.unwrap().backlight_config;
        assert_eq!(config.poll_interval, 0);
        assert_eq!(config.gamma_full, 9.0);
        assert_eq!(config.gamma_charging, -1.0);
        assert_eq!(config.gamma_low_percentage, 2.0);
    }
}
