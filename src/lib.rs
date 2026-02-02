//! Pomodoro timer widget plugin for verandah
//!
//! Displays a pomodoro timer on a Stream Deck button with:
//! - Remaining time countdown
//! - Phase indicator (Work/Break)
//! - Iteration progress dots
//! - Color-coded backgrounds

use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use verandah_plugin_api::prelude::*;

use std::path::PathBuf;

pub mod cli;
mod config;
mod error;
mod render;
pub mod socket;
mod sound;
mod timer;

use config::{Colour, Config, DEFAULT_INTERVAL_MS};
use socket::{Command, SocketListener};
use timer::{Phase, Timer, Transition};

const WIDGET_TYPE: &str = "pomodoro";

// Default colors used when parsing fails
const DEFAULT_FG: Colour = Colour {
    r: 255,
    g: 255,
    b: 255,
}; // White
const DEFAULT_WORK_BG: Colour = Colour {
    r: 229,
    g: 115,
    b: 115,
}; // Soft coral
const DEFAULT_BREAK_BG: Colour = Colour {
    r: 129,
    g: 199,
    b: 132,
}; // Soft mint
const DEFAULT_PAUSED_BG: Colour = Colour {
    r: 127,
    g: 140,
    b: 141,
}; // Gray

fn parse_colors(colors: &HashMap<String, String>) -> HashMap<String, Colour> {
    let mut parsed = HashMap::new();
    for (key, value) in colors {
        if let Some(colour) = Colour::parse(value) {
            parsed.insert(key.clone(), colour);
        } else {
            tracing::warn!(
                key,
                value,
                "Invalid color format, expected '#RRGGBB' or '#RGB'"
            );
        }
    }
    parsed
}

fn get_color<'a>(
    colors: &'a HashMap<String, Colour>,
    key: &str,
    default: &'a Colour,
) -> &'a Colour {
    colors.get(key).unwrap_or(default)
}

struct PomodoroWidget {
    timer: Timer,
    config: PluginConfig,
    interval: PluginDuration,
    last_tick: Option<Instant>,
    // Parsed colors (keys: fg, work_bg, break_bg, paused_bg)
    colors: HashMap<String, Colour>,
    padding: f32,
    phases: HashMap<String, String>,
    // Labels/fallback text (keys: work, short_break, long_break, paused)
    labels: HashMap<String, String>,
    // Sounds to play on phase transitions (keys: work, break)
    sounds: HashMap<String, PathBuf>,
    // Socket control
    command_rx: Option<Receiver<Command>>,
    socket_listener: Option<SocketListener>,
}

impl PomodoroWidget {
    fn new() -> Self {
        let cfg = Config::default();
        PomodoroWidget {
            timer: Timer::new(&cfg),
            config: PluginConfig::new(),
            interval: PluginDuration::from_millis(DEFAULT_INTERVAL_MS),
            last_tick: None,
            colors: parse_colors(&cfg.colors),
            padding: cfg.padding,
            phases: cfg.phases,
            labels: cfg.labels,
            sounds: HashMap::new(),
            command_rx: None,
            socket_listener: None,
        }
    }

    fn start_socket_listener(&mut self) {
        let (tx, rx) = socket::command_channel();

        match SocketListener::new(tx) {
            Ok(listener) => {
                self.socket_listener = Some(listener);
                self.command_rx = Some(rx);
                tracing::info!("Socket control enabled");
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to start socket listener, control disabled");
            }
        }
    }

    fn process_commands(&mut self) {
        let Some(rx) = &self.command_rx else {
            return;
        };

        while let Ok(cmd) = rx.try_recv() {
            tracing::debug!(command = ?cmd, "Processing command");
            cmd.apply(&mut self.timer);
        }
    }
}

impl WidgetPlugin for PomodoroWidget {
    fn widget_type(&self) -> abi_stable::std_types::RStr<'_> {
        WIDGET_TYPE.into()
    }

    fn default_interval(&self) -> PluginDuration {
        self.interval
    }

    fn init(&mut self, config: PluginConfig) -> PluginResult<()> {
        self.config = config.clone();

        let cfg: Config = match parse_config::<Config>(&config) {
            PluginResult::ROk(c) => c.with_defaults(),
            PluginResult::RErr(e) => return PluginResult::RErr(e),
        };

        self.timer = Timer::new(&cfg);
        self.interval = PluginDuration::from_millis(cfg.interval);
        self.colors = parse_colors(&cfg.colors);
        self.padding = cfg.padding.clamp(0.0, 0.4);
        self.phases = cfg.phases;
        self.labels = cfg.labels;

        // Resolve sound paths
        self.sounds.clear();
        for (key, sound_name) in &cfg.sounds {
            if let Some(path) = sound::resolve_sound(sound_name) {
                tracing::info!(key, path = %path.display(), "Sound configured");
                self.sounds.insert(key.clone(), path);
            }
        }

        // Start the socket listener for external control
        self.start_socket_listener();

        tracing::info!(
            work_mins = cfg.work,
            short_break_mins = cfg.short_break,
            long_break_mins = cfg.long_break,
            "Pomodoro widget initialized"
        );

        PluginResult::ROk(())
    }

    fn config(&self) -> PluginConfig {
        self.config.clone()
    }

    fn poll_state(&mut self) -> PluginResult<PluginWidgetState> {
        // Process any pending commands from socket
        self.process_commands();

        let now = Instant::now();

        // Tick the timer if running and enough time has passed
        if let Some(last) = self.last_tick {
            let elapsed = now.duration_since(last);
            // Tick once per second
            if elapsed.as_secs() >= 1 {
                let transition = self.timer.tick();
                self.last_tick = Some(now);

                // Play sound on phase transition
                match transition {
                    Transition::WorkComplete => {
                        if let Some(path) = self.sounds.get("work") {
                            sound::play_sound(path);
                        }
                    }
                    Transition::BreakComplete => {
                        if let Some(path) = self.sounds.get("break") {
                            sound::play_sound(path);
                        }
                    }
                    Transition::None => {}
                }
            }
        } else {
            self.last_tick = Some(now);
        }

        // Return the formatted time and running state
        // Include running state so UI updates when paused/resumed
        let text = format!(
            "{}|{}",
            self.timer.remaining_formatted(),
            if self.timer.is_running() { "R" } else { "P" }
        );
        PluginResult::ROk(PluginWidgetState::Text(text.into()))
    }

    fn render(
        &self,
        images: RHashMap<RString, PluginImage>,
        _state: &PluginWidgetState,
        image_size: PluginImageSize,
    ) -> PluginResult<PluginImage> {
        // Get icon and fallback text for phase boundary display
        let (icon, fallback_text): (Option<&PluginImage>, Option<&str>) =
            if !self.timer.is_running() && self.timer.at_phase_boundary() {
                let phase = self.timer.phase();
                let icon_key = match phase {
                    Phase::Work => "work",
                    Phase::ShortBreak => "short_break",
                    Phase::LongBreak => "long_break",
                };
                let fallback =
                    self.labels
                        .get(icon_key)
                        .map(|s| s.as_str())
                        .unwrap_or(match phase {
                            Phase::Work => "Work",
                            Phase::ShortBreak => "Short\nBreak",
                            Phase::LongBreak => "Long\nBreak",
                        });
                (images.get(&RString::from(icon_key)), Some(fallback))
            } else {
                (None, None)
            };

        let rgb_img = render::render_button(
            &self.timer,
            image_size.width,
            image_size.height,
            get_color(&self.colors, "fg", &DEFAULT_FG),
            get_color(&self.colors, "work_bg", &DEFAULT_WORK_BG),
            get_color(&self.colors, "break_bg", &DEFAULT_BREAK_BG),
            get_color(&self.colors, "paused_bg", &DEFAULT_PAUSED_BG),
            self.padding,
            icon,
            fallback_text,
            self.labels
                .get("paused")
                .map(|s| s.as_str())
                .unwrap_or("PAUSED"),
            &self.phases,
        );

        PluginResult::ROk(PluginImage::from_rgb(
            rgb_img.width(),
            rgb_img.height(),
            rgb_img.into_raw(),
        ))
    }

    fn shutdown(&mut self) {
        tracing::info!("Pomodoro widget shutting down");
        if let Some(mut listener) = self.socket_listener.take() {
            listener.shutdown();
        }
    }
}

#[sabi_extern_fn]
fn new_widget() -> WidgetPlugin_TO<'static, RBox<()>> {
    WidgetPlugin_TO::from_value(PomodoroWidget::new(), abi_stable::sabi_trait::TD_Opaque)
}

#[export_root_module]
fn get_library() -> PluginModRef {
    PluginMod {
        new: new_widget,
        plugin_api_version: PLUGIN_API_VERSION.into(),
        set_logger: set_logger_impl,
    }
    .leak_into_prefix()
}
