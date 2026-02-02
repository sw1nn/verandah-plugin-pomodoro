use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "verandah-pomodoroctl")]
#[command(about = "Control the verandah pomodoro timer")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Toggle the timer between running and paused
    Toggle,
    /// Start the timer
    Start,
    /// Stop/pause the timer
    Stop,
    /// Reset the timer to the beginning
    Reset,
    /// Skip to the next phase
    Skip,
}

impl Commands {
    pub fn as_str(&self) -> &'static str {
        match self {
            Commands::Toggle => "toggle",
            Commands::Start => "start",
            Commands::Stop => "stop",
            Commands::Reset => "reset",
            Commands::Skip => "skip",
        }
    }
}
