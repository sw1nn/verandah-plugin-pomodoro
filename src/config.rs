use std::collections::HashMap;

use serde::Deserialize;
use toml::Value;

pub const DEFAULT_WORK_MINS: u64 = 25;
pub const DEFAULT_SHORT_BREAK_MINS: u64 = 5;
pub const DEFAULT_LONG_BREAK_MINS: u64 = 15;
pub const DEFAULT_INTERVAL_MS: u64 = 1000;
pub const DEFAULT_PADDING: f32 = 0.05;
pub const DEFAULT_RENDER_MODE: &str = "text";
pub const DEFAULT_FILL_DIRECTION: &str = "empty_to_full";
pub const DEFAULT_PHASE_INDICATOR_DISPLAY: &str = "paused";

/// Final configuration after building from TOML
#[derive(Debug, Clone)]
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
    /// Render mode: "text" (default) or "fill_bg"
    pub render_mode: String,
    /// Fill direction for fill_bg mode: "empty_to_full" (default) or "full_to_empty"
    pub fill_direction: String,
    /// When to display the phase indicator: "none", "running", "paused" (default), or "both"
    pub phase_indicator_display: String,
    /// Pulse brightness when paused (for icon-based render modes)
    pub pulse_on_pause: bool,
    /// Sound files to play on phase transitions (keys: work, short_break, long_break)
    /// Sound indicates the phase that is STARTING, not the one that ended
    pub sounds: HashMap<String, String>,
    /// Phase indicator text (keys: work, short_break, long_break)
    pub phases: HashMap<String, String>,
    /// Labels/fallback text (keys: work, short_break, long_break, paused)
    pub labels: HashMap<String, String>,
    /// Colors (keys: fg, work_bg, break_bg, paused_bg, empty_bg) - format: #RRGGBB or #RGB
    pub colors: HashMap<String, String>,
}

/// Builder for Config that deserializes from TOML and applies defaults
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ConfigBuilder {
    work: u64,
    short_break: u64,
    long_break: u64,
    auto_start_work: bool,
    auto_start_break: bool,
    interval: u64,
    padding: f32,
    render_mode: String,
    fill_direction: String,
    phase_indicator_display: String,
    pulse_on_pause: bool,
    #[serde(default)]
    sounds: HashMap<String, String>,
    #[serde(default)]
    phases: HashMap<String, String>,
    #[serde(default)]
    labels: HashMap<String, String>,
    #[serde(default, alias = "colours")]
    colors: HashMap<String, String>,
    /// Catch-all for unknown fields (logged as warnings in build())
    #[serde(flatten)]
    unknown: HashMap<String, Value>,
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        ConfigBuilder {
            work: DEFAULT_WORK_MINS,
            short_break: DEFAULT_SHORT_BREAK_MINS,
            long_break: DEFAULT_LONG_BREAK_MINS,
            auto_start_work: false,
            auto_start_break: false,
            interval: DEFAULT_INTERVAL_MS,
            padding: DEFAULT_PADDING,
            render_mode: DEFAULT_RENDER_MODE.to_string(),
            fill_direction: DEFAULT_FILL_DIRECTION.to_string(),
            phase_indicator_display: DEFAULT_PHASE_INDICATOR_DISPLAY.to_string(),
            pulse_on_pause: false,
            sounds: HashMap::new(),
            phases: HashMap::new(),
            labels: HashMap::new(),
            colors: HashMap::new(),
            unknown: HashMap::new(),
        }
    }
}

impl ConfigBuilder {
    fn default_colors() -> HashMap<String, String> {
        let mut colors = HashMap::new();
        colors.insert("fg".to_string(), "#ffffff".to_string()); // White
        colors.insert("work_bg".to_string(), "#e57373".to_string()); // Soft coral
        colors.insert("break_bg".to_string(), "#81c784".to_string()); // Soft mint
        colors.insert("paused_bg".to_string(), "#7f8c8d".to_string()); // Gray
        colors.insert("empty_bg".to_string(), "#2c3e50".to_string()); // Dark blue-gray
        colors.insert("dot_running".to_string(), "#008000".to_string()); // Dark green
        colors.insert("dot_paused".to_string(), "#808080".to_string()); // Gray
        colors
    }

    fn default_labels() -> HashMap<String, String> {
        let mut labels = HashMap::new();
        labels.insert("paused".to_string(), "PAUSED".to_string());
        labels
    }

    /// Build the final Config, logging warnings for unknown fields
    /// and merging defaults for colors/labels.
    pub fn build(mut self) -> Config {
        // Log warnings for unknown fields (skip internal fields added by verandah)
        for key in self.unknown.keys() {
            if key == "_widget_id" {
                continue;
            }
            tracing::warn!(field = key, "Unknown config field");
        }

        // Merge defaults for colors
        for (key, value) in Self::default_colors() {
            self.colors.entry(key).or_insert(value);
        }

        // Merge defaults for labels
        for (key, value) in Self::default_labels() {
            self.labels.entry(key).or_insert(value);
        }

        Config {
            work: self.work,
            short_break: self.short_break,
            long_break: self.long_break,
            auto_start_work: self.auto_start_work,
            auto_start_break: self.auto_start_break,
            interval: self.interval,
            padding: self.padding,
            render_mode: self.render_mode,
            fill_direction: self.fill_direction,
            phase_indicator_display: self.phase_indicator_display,
            pulse_on_pause: self.pulse_on_pause,
            sounds: self.sounds,
            phases: self.phases,
            labels: self.labels,
            colors: self.colors,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder_defaults() -> crate::error::Result<()> {
        let cfg = ConfigBuilder::default().build();
        assert_eq!(cfg.work, 25);
        assert_eq!(cfg.short_break, 5);
        assert_eq!(cfg.long_break, 15);
        assert!(!cfg.auto_start_work);
        assert!(!cfg.auto_start_break);
        assert_eq!(cfg.colors.get("fg"), Some(&"#ffffff".to_string()));
        assert_eq!(cfg.colors.get("work_bg"), Some(&"#e57373".to_string()));
        assert_eq!(cfg.colors.get("break_bg"), Some(&"#81c784".to_string()));
        assert_eq!(cfg.colors.get("paused_bg"), Some(&"#7f8c8d".to_string()));
        assert_eq!(cfg.colors.get("empty_bg"), Some(&"#2c3e50".to_string()));
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
        let cfg: Config = toml::from_str::<ConfigBuilder>(toml_str).unwrap().build();
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
        let cfg: Config = toml::from_str::<ConfigBuilder>(toml_str).unwrap().build();
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
        let cfg: Config = toml::from_str::<ConfigBuilder>(toml_str).unwrap().build();
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
        let cfg: Config = toml::from_str::<ConfigBuilder>(toml_str).unwrap().build();
        // User-specified value should be preserved
        assert_eq!(cfg.labels.get("work"), Some(&"WORK".to_string()));
        // Default should be filled in
        assert_eq!(cfg.labels.get("paused"), Some(&"PAUSED".to_string()));
        Ok(())
    }

    #[test]
    fn test_config_unknown_fields_captured() -> crate::error::Result<()> {
        let toml_str = r##"
work = 25
unknown_field = "value"
another_unknown = 42
"##;
        let builder: ConfigBuilder = toml::from_str(toml_str).unwrap();
        // Unknown fields should be captured in the builder
        assert!(builder.unknown.contains_key("unknown_field"));
        assert!(builder.unknown.contains_key("another_unknown"));
        // Known fields should still work
        assert_eq!(builder.work, 25);
        Ok(())
    }
}
