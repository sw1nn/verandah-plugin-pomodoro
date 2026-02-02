use std::collections::HashMap;

use serde::Deserialize;

pub const DEFAULT_WORK_MINS: u64 = 25;
pub const DEFAULT_SHORT_BREAK_MINS: u64 = 5;
pub const DEFAULT_LONG_BREAK_MINS: u64 = 15;
pub const DEFAULT_INTERVAL_MS: u64 = 1000;
pub const DEFAULT_PADDING: f32 = 0.05;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Work duration in minutes
    pub work: u64,
    /// Short break duration in minutes
    pub short_break: u64,
    /// Long break duration in minutes
    pub long_break: u64,
    /// Auto-start work after break
    pub auto_start_work: bool,
    /// Auto-start break after work
    pub auto_start_break: bool,
    /// Polling interval in milliseconds
    pub interval: u64,
    /// Text padding as fraction of button size (0.0 to 0.4)
    pub padding: f32,
    /// Sound files to play on phase transitions (keys: work, break)
    #[serde(default)]
    pub sounds: HashMap<String, String>,
    /// Phase indicator text (keys: work, short_break, long_break)
    #[serde(default)]
    pub phases: HashMap<String, String>,
    /// Labels/fallback text (keys: work, short_break, long_break, paused)
    #[serde(default)]
    pub labels: HashMap<String, String>,
    /// Colors (keys: fg, work_bg, break_bg, paused_bg) - format: #RRGGBB or #RGB
    #[serde(default, alias = "colours")]
    pub colors: HashMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            work: DEFAULT_WORK_MINS,
            short_break: DEFAULT_SHORT_BREAK_MINS,
            long_break: DEFAULT_LONG_BREAK_MINS,
            auto_start_work: false,
            auto_start_break: false,
            interval: DEFAULT_INTERVAL_MS,
            padding: DEFAULT_PADDING,
            sounds: HashMap::new(),
            phases: HashMap::new(),
            labels: Self::default_labels(),
            colors: Self::default_colors(),
        }
    }
}

impl Config {
    fn default_colors() -> HashMap<String, String> {
        let mut colors = HashMap::new();
        colors.insert("fg".to_string(), "#ffffff".to_string()); // White
        colors.insert("work_bg".to_string(), "#e57373".to_string()); // Soft coral
        colors.insert("break_bg".to_string(), "#81c784".to_string()); // Soft mint
        colors.insert("paused_bg".to_string(), "#7f8c8d".to_string()); // Gray
        colors
    }

    fn default_labels() -> HashMap<String, String> {
        let mut labels = HashMap::new();
        labels.insert("paused".to_string(), "PAUSED".to_string());
        labels
    }

    /// Merge HashMap fields with defaults for any missing keys.
    /// Call this after deserializing to ensure all expected keys are present.
    pub fn with_defaults(mut self) -> Self {
        for (key, value) in Self::default_colors() {
            self.colors.entry(key).or_insert(value);
        }
        for (key, value) in Self::default_labels() {
            self.labels.entry(key).or_insert(value);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() -> crate::error::Result<()> {
        let cfg = Config::default();
        assert_eq!(cfg.work, 25);
        assert_eq!(cfg.short_break, 5);
        assert_eq!(cfg.long_break, 15);
        assert!(!cfg.auto_start_work);
        assert!(!cfg.auto_start_break);
        assert_eq!(cfg.colors.get("fg"), Some(&"#ffffff".to_string()));
        assert_eq!(cfg.colors.get("work_bg"), Some(&"#e57373".to_string()));
        assert_eq!(cfg.colors.get("break_bg"), Some(&"#81c784".to_string()));
        assert_eq!(cfg.colors.get("paused_bg"), Some(&"#7f8c8d".to_string()));
        Ok(())
    }

    #[test]
    fn test_config_parse_toml() -> crate::error::Result<()> {
        let toml_str = r##"
work = 30
short_break = 10
auto_start_work = true

[colors]
fg = "#000000"
work_bg = "#ff0000"

[sounds]
work = "bell.wav"
"##;
        let cfg: Config = toml::from_str::<Config>(toml_str).unwrap().with_defaults();
        assert_eq!(cfg.work, 30);
        assert_eq!(cfg.short_break, 10);
        assert!(cfg.auto_start_work);
        assert_eq!(cfg.colors.get("fg"), Some(&"#000000".to_string()));
        assert_eq!(cfg.colors.get("work_bg"), Some(&"#ff0000".to_string()));
        // defaults should still be present for unspecified fields
        assert_eq!(cfg.long_break, DEFAULT_LONG_BREAK_MINS);
        assert_eq!(cfg.sounds.get("work"), Some(&"bell.wav".to_string()));
        Ok(())
    }

    #[test]
    fn test_config_parse_inline_tables() -> crate::error::Result<()> {
        let toml_str = r##"
work = 25
colors = { fg = "#ffffff", work_bg = "#e57373" }
"##;
        let cfg: Config = toml::from_str::<Config>(toml_str).unwrap().with_defaults();
        assert_eq!(cfg.work, 25);
        assert_eq!(cfg.colors.get("fg"), Some(&"#ffffff".to_string()));
        assert_eq!(cfg.colors.get("work_bg"), Some(&"#e57373".to_string()));
        Ok(())
    }

    #[test]
    fn test_config_partial_colors_merged_with_defaults() -> crate::error::Result<()> {
        let toml_str = r##"
[colors]
fg = "#000000"
"##;
        let cfg: Config = toml::from_str::<Config>(toml_str).unwrap().with_defaults();
        // User-specified value should be preserved
        assert_eq!(cfg.colors.get("fg"), Some(&"#000000".to_string()));
        // Defaults should be filled in for unspecified keys
        assert_eq!(cfg.colors.get("work_bg"), Some(&"#e57373".to_string()));
        assert_eq!(cfg.colors.get("break_bg"), Some(&"#81c784".to_string()));
        assert_eq!(cfg.colors.get("paused_bg"), Some(&"#7f8c8d".to_string()));
        Ok(())
    }

    #[test]
    fn test_config_partial_labels_merged_with_defaults() -> crate::error::Result<()> {
        let toml_str = r##"
[labels]
work = "WORK"
"##;
        let cfg: Config = toml::from_str::<Config>(toml_str).unwrap().with_defaults();
        // User-specified value should be preserved
        assert_eq!(cfg.labels.get("work"), Some(&"WORK".to_string()));
        // Default should be filled in
        assert_eq!(cfg.labels.get("paused"), Some(&"PAUSED".to_string()));
        Ok(())
    }
}
