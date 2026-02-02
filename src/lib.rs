//! Pomodoro timer widget plugin for verandah
//!
//! Displays a pomodoro timer on a Stream Deck button with:
//! - Remaining time countdown
//! - Phase indicator (Work/Break)
//! - Iteration progress dots
//! - Color-coded backgrounds

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

struct PomodoroWidget {
    timer: Timer,
    config: PluginConfig,
    interval: PluginDuration,
    last_tick: Option<Instant>,
    // Parsed colors
    fg_color: Colour,
    work_bg: Colour,
    break_bg: Colour,
    paused_bg: Colour,
    padding: f32,
    // Icon key to display when paused during work
    icon: Option<String>,
    // Icon key to display when paused during short break
    short_break_icon: Option<String>,
    // Icon key to display when paused during long break
    long_break_icon: Option<String>,
    // Sound to play when work completes
    work_sound: Option<PathBuf>,
    // Sound to play when break completes
    break_sound: Option<PathBuf>,
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
            fg_color: Colour::parse(&cfg.fg_color).unwrap_or(Colour {
                r: 255,
                g: 255,
                b: 255,
            }),
            work_bg: Colour::parse(&cfg.work_bg).unwrap_or(Colour {
                r: 192,
                g: 57,
                b: 43,
            }),
            break_bg: Colour::parse(&cfg.break_bg).unwrap_or(Colour {
                r: 39,
                g: 174,
                b: 96,
            }),
            paused_bg: Colour::parse(&cfg.paused_bg).unwrap_or(Colour {
                r: 127,
                g: 140,
                b: 141,
            }),
            padding: cfg.padding,
            icon: cfg.icon,
            short_break_icon: cfg.short_break_icon,
            long_break_icon: cfg.long_break_icon,
            work_sound: None,
            break_sound: None,
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

        let cfg: Config = match parse_config(&config) {
            PluginResult::ROk(c) => c,
            PluginResult::RErr(e) => return PluginResult::RErr(e),
        };

        self.timer = Timer::new(&cfg);
        self.interval = PluginDuration::from_millis(cfg.interval);
        self.fg_color = Colour::parse(&cfg.fg_color).unwrap_or(Colour {
            r: 255,
            g: 255,
            b: 255,
        });
        self.work_bg = Colour::parse(&cfg.work_bg).unwrap_or(Colour {
            r: 192,
            g: 57,
            b: 43,
        });
        self.break_bg = Colour::parse(&cfg.break_bg).unwrap_or(Colour {
            r: 39,
            g: 174,
            b: 96,
        });
        self.paused_bg = Colour::parse(&cfg.paused_bg).unwrap_or(Colour {
            r: 127,
            g: 140,
            b: 141,
        });
        self.padding = cfg.padding.clamp(0.0, 0.4);
        self.icon = cfg.icon;
        self.short_break_icon = cfg.short_break_icon;
        self.long_break_icon = cfg.long_break_icon;

        // Resolve sound paths
        self.work_sound = cfg.work_sound.as_deref().and_then(sound::resolve_sound);
        self.break_sound = cfg.break_sound.as_deref().and_then(sound::resolve_sound);

        if let Some(path) = &self.work_sound {
            tracing::info!(path = %path.display(), "Work sound configured");
        }
        if let Some(path) = &self.break_sound {
            tracing::info!(path = %path.display(), "Break sound configured");
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
                        if let Some(path) = &self.work_sound {
                            sound::play_sound(path);
                        }
                    }
                    Transition::BreakComplete => {
                        if let Some(path) = &self.break_sound {
                            sound::play_sound(path);
                        }
                    }
                    Transition::None => {}
                }
            }
        } else {
            self.last_tick = Some(now);
        }

        // Return the formatted time as the state
        let text = self.timer.remaining_formatted();
        PluginResult::ROk(PluginWidgetState::Text(text.into()))
    }

    fn render(
        &self,
        images: RHashMap<RString, PluginImage>,
        _state: &PluginWidgetState,
        image_size: PluginImageSize,
    ) -> PluginResult<PluginImage> {
        // Get the appropriate icon if timer is not running
        let paused_icon = if !self.timer.is_running() {
            // Choose icon based on current phase
            let phase = self.timer.phase();
            let icon_key = match phase {
                Phase::Work => self.icon.as_deref(),
                Phase::ShortBreak => self.short_break_icon.as_deref(),
                Phase::LongBreak => self.long_break_icon.as_deref(),
            };
            tracing::debug!(
                ?phase,
                ?icon_key,
                available_icons = ?images.keys().collect::<Vec<_>>(),
                "Selecting paused icon"
            );
            icon_key.and_then(|key| images.get(&RString::from(key)))
        } else {
            None
        };

        let rgb_img = render::render_button(
            &self.timer,
            image_size.width,
            image_size.height,
            &self.fg_color,
            &self.work_bg,
            &self.break_bg,
            &self.paused_bg,
            self.padding,
            paused_icon,
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
