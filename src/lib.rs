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

use verandah_plugin::api::prelude::*;
use verandah_plugin::utils::prelude::*;

use std::path::PathBuf;

pub mod cli;
mod config;
mod error;
mod render;
pub mod socket;
mod sound;
mod timer;

use config::{Config, ConfigBuilder, DEFAULT_INTERVAL_MS};
use render::{FillDirection, PhaseIndicatorDisplay, RenderMode};
use socket::{Command, SocketListener};
use timer::{Phase, Timer, Transition};

const WIDGET_TYPE: &str = "pomodoro";

// Default colors used when parsing fails
const DEFAULT_FG: Rgba<u8> = rgb("#FFFFFF");
const DEFAULT_WORK_BG: Rgba<u8> = rgb("#E57373");
const DEFAULT_BREAK_BG: Rgba<u8> = rgb("#81C784");
const DEFAULT_PAUSED_BG: Rgba<u8> = rgb("#7F8C8D");
const DEFAULT_EMPTY_BG: Rgba<u8> = rgb("#2C3E50");
const DEFAULT_DOT_RUNNING: Rgba<u8> = rgb("#008000");
const DEFAULT_DOT_PAUSED: Rgba<u8> = rgb("#808080");

struct PomodoroWidget {
    timer: Timer,
    config: PluginConfig,
    interval: PluginDuration,
    last_tick: Option<Instant>,
    // Parsed colors (keys: fg, work_bg, break_bg, paused_bg)
    colors: HashMap<String, Rgba<u8>>,
    padding: f32,
    // Render mode and fill direction
    render_mode: RenderMode,
    fill_direction: FillDirection,
    phase_indicator_display: PhaseIndicatorDisplay,
    pulse_on_pause: bool,
    phases: HashMap<String, String>,
    // Labels/fallback text (keys: work, short_break, long_break, paused)
    labels: HashMap<String, String>,
    // Sounds to play on phase transitions (keys: work, short_break, long_break)
    // Sound indicates the STARTING phase, not the ending one
    sounds: HashMap<String, PathBuf>,
    // Socket control
    command_rx: Option<Receiver<Command>>,
    socket_listener: Option<SocketListener>,
}

impl PomodoroWidget {
    fn new() -> Self {
        let cfg = ConfigBuilder::default().build();
        PomodoroWidget {
            timer: Timer::new(&cfg),
            config: PluginConfig::new(),
            interval: PluginDuration::from_millis(DEFAULT_INTERVAL_MS),
            last_tick: None,
            colors: parse_colors(&cfg.colors),
            padding: cfg.padding,
            render_mode: cfg.render_mode,
            fill_direction: cfg.fill_direction,
            phase_indicator_display: cfg.phase_indicator_display,
            pulse_on_pause: cfg.pulse_on_pause,
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

        let cfg: Config = match parse_config::<ConfigBuilder>(&config) {
            PluginResult::ROk(builder) => builder.build(),
            PluginResult::RErr(e) => return PluginResult::RErr(e),
        };

        self.timer = Timer::new(&cfg);
        self.interval = PluginDuration::from_millis(cfg.interval);
        self.colors = parse_colors(&cfg.colors);
        self.padding = cfg.padding.clamp(0.0, 0.4);
        self.render_mode = cfg.render_mode;
        self.fill_direction = cfg.fill_direction;
        self.phase_indicator_display = cfg.phase_indicator_display;
        self.pulse_on_pause = cfg.pulse_on_pause;
        self.phases = cfg.phases;
        self.labels = cfg.labels;

        // Resolve sound paths
        self.sounds.clear();
        for (key, sound_name) in &cfg.sounds {
            match sound::resolve_sound(sound_name) {
                Some(path) => {
                    tracing::info!(key, path = %path.display(), "Sound configured");
                    self.sounds.insert(key.clone(), path);
                }
                None => {
                    tracing::warn!(key, sound = %sound_name, "Configured sound not found");
                }
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

    fn poll_state(&mut self) -> PluginResult<PluginPollResponse> {
        // Process any pending commands from socket
        self.process_commands();

        let now = Instant::now();

        // Tick the timer for each elapsed second to prevent drift under load
        if let Some(last) = self.last_tick {
            let elapsed_secs = now.duration_since(last).as_secs();
            if elapsed_secs >= 1 {
                self.last_tick = Some(now);

                // Play at most one sound per poll, for the phase that is now
                // current. Catching up after a suspend can cross many phase
                // boundaries in one poll; a sound (and its playback thread)
                // per stale transition would burst all at once.
                if catch_up_timer(&mut self.timer, elapsed_secs) {
                    let sound_key = match self.timer.phase() {
                        Phase::Work => "work",
                        Phase::ShortBreak => "short_break",
                        Phase::LongBreak => "long_break",
                    };
                    if let Some(path) = self.sounds.get(sound_key) {
                        sound::play_sound(path);
                    }
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
        let state = PluginWidgetState::Text(text.into());

        // Use fast interval for smooth pulse animation when paused
        let should_pulse =
            self.pulse_on_pause && !self.timer.is_running() && !self.timer.at_phase_boundary();

        tracing::trace!(
            pulse_on_pause = self.pulse_on_pause,
            is_running = self.timer.is_running(),
            at_phase_boundary = self.timer.at_phase_boundary(),
            should_pulse,
            "poll_state"
        );

        if should_pulse {
            PluginResult::ROk(PluginPollResponse::with_interval(
                state,
                PluginDuration::from_millis(100),
            ))
        } else {
            PluginResult::ROk(PluginPollResponse::state(state))
        }
    }

    fn render(
        &self,
        images: RHashMap<RString, PluginImage>,
        _state: &PluginWidgetState,
        image_size: PluginImageSize,
    ) -> PluginResult<PluginImage> {
        let phase = self.timer.phase();
        let icon_key = match phase {
            Phase::Work => "work",
            Phase::ShortBreak => "short_break",
            Phase::LongBreak => "long_break",
        };

        // Get phase icon (used for fill_icon mode and paused display)
        let phase_icon = images.get(&RString::from(icon_key));

        // Get icon and fallback text for phase boundary display
        let (paused_icon, fallback_text): (Option<&PluginImage>, Option<&str>) =
            if !self.timer.is_running() && self.timer.at_phase_boundary() {
                let fallback = self
                    .labels
                    .get(icon_key)
                    .map(|s: &String| s.as_str())
                    .unwrap_or(match phase {
                        Phase::Work => "Work",
                        Phase::ShortBreak => "Short\nBreak",
                        Phase::LongBreak => "Long\nBreak",
                    });
                (phase_icon, Some(fallback))
            } else {
                (None, None)
            };

        let rgb_img = render::render_button(
            &self.timer,
            image_size.width,
            image_size.height,
            get_color(&self.colors, "fg", DEFAULT_FG),
            get_color(&self.colors, "work_bg", DEFAULT_WORK_BG),
            get_color(&self.colors, "break_bg", DEFAULT_BREAK_BG),
            get_color(&self.colors, "paused_bg", DEFAULT_PAUSED_BG),
            get_color(&self.colors, "empty_bg", DEFAULT_EMPTY_BG),
            get_color(&self.colors, "dot_running", DEFAULT_DOT_RUNNING),
            get_color(&self.colors, "dot_paused", DEFAULT_DOT_PAUSED),
            self.padding,
            paused_icon,
            phase_icon,
            fallback_text,
            self.labels
                .get("paused")
                .map(|s: &String| s.as_str())
                .unwrap_or("PAUSED"),
            &self.phases,
            self.render_mode,
            self.fill_direction,
            self.phase_indicator_display,
            self.pulse_on_pause,
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

    fn supported_actions(&self) -> RVec<PluginActionSpec> {
        vec![
            PluginActionSpec::new("toggle", "Toggle the timer between running and paused")
                .with_default(),
            PluginActionSpec::new("start", "Start the timer"),
            PluginActionSpec::new("stop", "Stop/pause the timer"),
            PluginActionSpec::new("reset", "Reset the timer to the beginning"),
            PluginActionSpec::new("skip", "Skip to the next phase"),
        ]
        .into()
    }

    fn handle_action(&mut self, verb: RStr<'_>) -> PluginResult<()> {
        match Command::parse(verb.as_str()) {
            Some(cmd) => {
                tracing::info!(verb = verb.as_str(), "Applying plugin action");
                cmd.apply(&mut self.timer);
                PluginResult::ROk(())
            }
            None => PluginResult::RErr(PluginError::new(format!("Unsupported action: {verb}"))),
        }
    }
}

/// Advance the timer by `elapsed_secs` one-second ticks, reporting whether
/// any phase transition occurred along the way.
fn catch_up_timer(timer: &mut Timer, elapsed_secs: u64) -> bool {
    let mut transitioned = false;
    for _ in 0..elapsed_secs {
        if timer.tick() != Transition::None {
            transitioned = true;
        }
    }
    transitioned
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_actions_declares_five_verbs_with_toggle_default() -> error::Result<()> {
        let widget = PomodoroWidget::new();
        let actions = widget.supported_actions();
        let names: Vec<&str> = actions.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["toggle", "start", "stop", "reset", "skip"]);
        let defaults: Vec<&str> = actions
            .iter()
            .filter(|a| a.is_default)
            .map(|a| a.name.as_str())
            .collect();
        assert_eq!(defaults, vec!["toggle"]);
        Ok(())
    }

    #[test]
    fn handle_action_toggle_flips_timer_running() -> error::Result<()> {
        let mut widget = PomodoroWidget::new();
        assert!(!widget.timer.is_running());
        assert!(widget.handle_action("toggle".into()).is_ok());
        assert!(widget.timer.is_running());
        assert!(widget.handle_action("toggle".into()).is_ok());
        assert!(!widget.timer.is_running());
        Ok(())
    }

    #[test]
    fn handle_action_unknown_verb_errors() -> error::Result<()> {
        let mut widget = PomodoroWidget::new();
        let result = widget.handle_action("bogus".into());
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn catch_up_reports_one_transition_flag_across_many_boundaries() -> error::Result<()> {
        use config::ConfigBuilder;
        let toml_str = r#"
work = 1
short_break = 1
long_break = 1
auto_start_work = true
auto_start_break = true
"#;
        let cfg = verandah_plugin::api::toml::from_str::<ConfigBuilder>(toml_str)?.build();
        let mut timer = Timer::new(&cfg);
        timer.start();

        // A long suspend: 480 seconds crosses a full 8-phase cycle, but the
        // caller gets a single flag, so only one sound plays per poll.
        assert!(catch_up_timer(&mut timer, 480));
        assert_eq!(timer.sessions_completed(), 1);

        // No boundary crossed: no sound.
        assert!(!catch_up_timer(&mut timer, 5));
        Ok(())
    }
}
