use std::fs;
use std::path::PathBuf;

use clap::CommandFactory;
use clap_complete::{Shell, generate_to};

mod cli {
    include!("src/cli.rs");
}

fn main() {
    println!("cargo:rerun-if-changed=src/cli.rs");

    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let manifest_dir =
                std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
            PathBuf::from(manifest_dir).join("target")
        });

    let profile = std::env::var("PROFILE").expect("PROFILE not set");
    let completions_dir = target_dir.join(profile).join("completions");

    fs::create_dir_all(&completions_dir).expect("failed to create completions dir");

    let mut cmd = cli::Cli::command();
    for shell in [Shell::Zsh, Shell::Bash, Shell::Fish] {
        generate_to(shell, &mut cmd, "verandah-pomodoroctl", &completions_dir)
            .expect("failed to generate completions");
    }
}
