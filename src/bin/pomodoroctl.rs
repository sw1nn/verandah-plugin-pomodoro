use clap::{Parser, Subcommand};
use verandah_plugin_pomodoro::socket;

#[derive(Parser)]
#[command(name = "verandah-pomodoroctl")]
#[command(about = "Control the verandah pomodoro timer")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
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
    fn as_str(&self) -> &'static str {
        match self {
            Commands::Toggle => "toggle",
            Commands::Start => "start",
            Commands::Stop => "stop",
            Commands::Reset => "reset",
            Commands::Skip => "skip",
        }
    }
}

fn main() {
    let cli = Cli::parse();

    let command = cli.command.as_str();

    match socket::send_command(command) {
        Ok(()) => {
            eprintln!("Sent: {command}");
        }
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}
