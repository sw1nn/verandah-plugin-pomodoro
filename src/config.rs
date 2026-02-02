use std::collections::HashMap;

use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

pub const DEFAULT_WORK_MINS: u64 = 25;
pub const DEFAULT_SHORT_BREAK_MINS: u64 = 5;
pub const DEFAULT_LONG_BREAK_MINS: u64 = 15;
pub const DEFAULT_INTERVAL_MS: u64 = 1000;
pub const DEFAULT_PADDING: f32 = 0.05;

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
    /// Sound files to play on phase transitions (keys: work, break)
    /// Config format: sounds_work, sounds_break
    pub sounds: HashMap<String, String>,
    /// Phase indicator text (keys: work, short_break, long_break)
    /// Config format: phases_work, phases_short_break, phases_long_break
    pub phases: HashMap<String, String>,
    /// Labels/fallback text (keys: work, short_break, long_break, paused)
    /// Config format: labels_work, labels_short_break, labels_long_break, labels_paused
    pub labels: HashMap<String, String>,
    /// Colors (keys: fg, work_bg, break_bg, paused_bg) - format: #RRGGBB or #RGB
    /// Config format: colors_fg, colors_work_bg, colors_break_bg, colors_paused_bg
    /// Also accepts: colours_* as alias
    pub colors: HashMap<String, String>,
}

impl<'de> Deserialize<'de> for Config {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(ConfigVisitor)
    }
}

struct ConfigVisitor;

impl<'de> Visitor<'de> for ConfigVisitor {
    type Value = Config;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a map of configuration values")
    }

    fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let mut config = Config::default();

        while let Some(key) = map.next_key::<String>()? {
            let value: String = map.next_value()?;

            if let Some(suffix) = key
                .strip_prefix("colors_")
                .or_else(|| key.strip_prefix("colours_"))
            {
                config.colors.insert(suffix.to_string(), value);
            } else if let Some(suffix) = key.strip_prefix("sounds_") {
                config.sounds.insert(suffix.to_string(), value);
            } else if let Some(suffix) = key.strip_prefix("phases_") {
                config.phases.insert(suffix.to_string(), value);
            } else if let Some(suffix) = key.strip_prefix("labels_") {
                config.labels.insert(suffix.to_string(), value);
            } else {
                match key.as_str() {
                    "work" => config.work = value.parse().map_err(de::Error::custom)?,
                    "short_break" => {
                        config.short_break = value.parse().map_err(de::Error::custom)?
                    }
                    "long_break" => config.long_break = value.parse().map_err(de::Error::custom)?,
                    "auto_start_work" => {
                        config.auto_start_work = value.parse().map_err(de::Error::custom)?
                    }
                    "auto_start_break" => {
                        config.auto_start_break = value.parse().map_err(de::Error::custom)?
                    }
                    "interval" => config.interval = value.parse().map_err(de::Error::custom)?,
                    "padding" => config.padding = value.parse().map_err(de::Error::custom)?,
                    _ => {
                        tracing::debug!(key, "Unknown config key, ignoring");
                    }
                }
            }
        }

        Ok(config)
    }
}

impl Default for Config {
    fn default() -> Self {
        let mut colors = HashMap::new();
        colors.insert("fg".to_string(), "#ffffff".to_string()); // White
        colors.insert("work_bg".to_string(), "#e57373".to_string()); // Soft coral
        colors.insert("break_bg".to_string(), "#81c784".to_string()); // Soft mint
        colors.insert("paused_bg".to_string(), "#7f8c8d".to_string()); // Gray

        let mut labels = HashMap::new();
        labels.insert("paused".to_string(), "PAUSED".to_string());

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
            labels,
            colors,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Colour {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Colour {
    /// Parse a color from a hex string.
    /// Requires leading '#' and supports both #RRGGBB and #RGB formats.
    pub fn parse<S>(s: S) -> Option<Self>
    where
        S: AsRef<str>,
    {
        let s = s.as_ref().strip_prefix('#')?;
        match s.len() {
            3 => {
                // #RGB format - each digit is doubled
                let r = u8::from_str_radix(&s[0..1], 16).ok()?;
                let g = u8::from_str_radix(&s[1..2], 16).ok()?;
                let b = u8::from_str_radix(&s[2..3], 16).ok()?;
                Some(Colour {
                    r: r * 17, // 0xF -> 0xFF, 0xA -> 0xAA, etc.
                    g: g * 17,
                    b: b * 17,
                })
            }
            6 => {
                let r = u8::from_str_radix(&s[0..2], 16).ok()?;
                let g = u8::from_str_radix(&s[2..4], 16).ok()?;
                let b = u8::from_str_radix(&s[4..6], 16).ok()?;
                Some(Colour { r, g, b })
            }
            _ => None,
        }
    }
}

impl TryFrom<&str> for Colour {
    type Error = &'static str;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Colour::parse(s).ok_or("invalid colour format, expected '#RRGGBB' or '#RGB'")
    }
}

impl TryFrom<String> for Colour {
    type Error = &'static str;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Colour::parse(&s).ok_or("invalid colour format, expected '#RRGGBB' or '#RGB'")
    }
}

impl TryFrom<&String> for Colour {
    type Error = &'static str;

    fn try_from(s: &String) -> Result<Self, Self::Error> {
        Colour::parse(s).ok_or("invalid colour format, expected '#RRGGBB' or '#RGB'")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_colour_parse_rrggbb() -> crate::error::Result<()> {
        let c = Colour::parse("#ff6b35").unwrap();
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 107);
        assert_eq!(c.b, 53);
        Ok(())
    }

    #[test]
    fn test_colour_parse_rgb() -> crate::error::Result<()> {
        let c = Colour::parse("#fab").unwrap();
        assert_eq!(c.r, 0xff);
        assert_eq!(c.g, 0xaa);
        assert_eq!(c.b, 0xbb);
        Ok(())
    }

    #[test]
    fn test_colour_parse_requires_hash() -> crate::error::Result<()> {
        assert!(Colour::parse("ff6b35").is_none());
        assert!(Colour::parse("fab").is_none());
        Ok(())
    }

    #[test]
    fn test_colour_parse_invalid_length() -> crate::error::Result<()> {
        assert!(Colour::parse("#ff").is_none());
        assert!(Colour::parse("#ffff").is_none());
        assert!(Colour::parse("#fffff").is_none());
        assert!(Colour::parse("#fffffff").is_none());
        Ok(())
    }

    #[test]
    fn test_colour_try_from() -> crate::error::Result<()> {
        let c: Colour = "#ff0000".try_into().unwrap();
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 0);

        let c: Colour = "#0f0".try_into().unwrap();
        assert_eq!(c.r, 0);
        assert_eq!(c.g, 255);
        assert_eq!(c.b, 0);

        let result: Result<Colour, _> = "bad".try_into();
        assert!(result.is_err());

        let result: Result<Colour, _> = "ffffff".try_into();
        assert!(result.is_err());
        Ok(())
    }

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
}
