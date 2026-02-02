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
    /// Background color during work (hex RGB)
    pub work_bg: String,
    /// Background color during break (hex RGB)
    pub break_bg: String,
    /// Background color when paused (hex RGB)
    pub paused_bg: String,
    /// Foreground/text color (hex RGB)
    pub fg_color: String,
    /// Text padding as fraction of button size (0.0 to 0.4)
    pub padding: f32,
    /// Sound file to play when work period ends
    pub work_sound: Option<String>,
    /// Sound file to play when break period ends
    pub break_sound: Option<String>,
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
            work_bg: "#c0392b".to_string(),   // Red
            break_bg: "#27ae60".to_string(),  // Green
            paused_bg: "#7f8c8d".to_string(), // Gray
            fg_color: "#ffffff".to_string(),  // White
            padding: DEFAULT_PADDING,
            work_sound: None,
            break_sound: None,
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
    pub fn parse<S>(s: S) -> Option<Self>
    where
        S: AsRef<str>,
    {
        let s = s.as_ref().strip_prefix('#').unwrap_or(s.as_ref());
        if s.len() < 6 {
            return None;
        }
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some(Colour { r, g, b })
    }
}

impl TryFrom<&str> for Colour {
    type Error = &'static str;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Colour::parse(s).ok_or("invalid colour format, expected '#rrggbb' or 'rrggbb'")
    }
}

impl TryFrom<String> for Colour {
    type Error = &'static str;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Colour::parse(&s).ok_or("invalid colour format, expected '#rrggbb' or 'rrggbb'")
    }
}

impl TryFrom<&String> for Colour {
    type Error = &'static str;

    fn try_from(s: &String) -> Result<Self, Self::Error> {
        Colour::parse(s).ok_or("invalid colour format, expected '#rrggbb' or 'rrggbb'")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_colour_parse_valid() -> crate::error::Result<()> {
        let c = Colour::parse("ff6b35").unwrap();
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 107);
        assert_eq!(c.b, 53);
        Ok(())
    }

    #[test]
    fn test_colour_parse_with_hash() -> crate::error::Result<()> {
        let c = Colour::parse("#a1b2c3").unwrap();
        assert_eq!(c.r, 0xa1);
        assert_eq!(c.g, 0xb2);
        assert_eq!(c.b, 0xc3);
        Ok(())
    }

    #[test]
    fn test_colour_parse_too_short() -> crate::error::Result<()> {
        assert!(Colour::parse("fff").is_none());
        assert!(Colour::parse("#fff").is_none());
        Ok(())
    }

    #[test]
    fn test_colour_try_from() -> crate::error::Result<()> {
        let c: Colour = "#ff0000".try_into().unwrap();
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 0);

        let c: Colour = "00ff00".try_into().unwrap();
        assert_eq!(c.r, 0);
        assert_eq!(c.g, 255);
        assert_eq!(c.b, 0);

        let result: Result<Colour, _> = "bad".try_into();
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
        Ok(())
    }
}
