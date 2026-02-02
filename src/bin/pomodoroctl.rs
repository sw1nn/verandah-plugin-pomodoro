use clap::Parser;
use verandah_plugin_pomodoro::{cli::Cli, socket};

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
