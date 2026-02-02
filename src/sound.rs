//! Audio playback for phase transition sounds

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::thread;

use rodio::{Decoder, OutputStream, Sink};
use xdg::BaseDirectories;

const SOUND_EXTENSIONS: &[&str] = &["oga", "ogg", "wav", "mp3"];

/// Play a sound file in a background thread
pub fn play_sound<P>(path: P)
where
    P: AsRef<Path>,
{
    let path = path.as_ref().to_path_buf();
    if !path.exists() {
        tracing::warn!(path = %path.display(), "Sound file not found");
        return;
    }

    thread::spawn(move || {
        if let Err(e) = play_audio_file(&path) {
            tracing::warn!(path = %path.display(), error = %e, "Failed to play sound");
        }
    });
}

/// Play an audio file synchronously
fn play_audio_file(path: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (_stream, stream_handle) = OutputStream::try_default()?;
    let file = File::open(path)?;
    let source = Decoder::new(BufReader::new(file))?;
    let sink = Sink::try_new(&stream_handle)?;
    sink.append(source);
    sink.sleep_until_end();
    Ok(())
}

/// Resolve a sound name to a full path
///
/// Accepts:
/// - Absolute paths: `/path/to/sound.ogg`
/// - Relative paths: `./sounds/bell.wav`
/// - Sound theme names: `alarm-clock-elapsed` (searches XDG sound dirs)
pub fn resolve_sound<S>(name: S) -> Option<PathBuf>
where
    S: AsRef<str>,
{
    let name = name.as_ref();
    if name.is_empty() {
        return None;
    }

    let path = Path::new(name);

    // Absolute or relative path
    if path.is_absolute() || name.starts_with("./") || name.starts_with("../") {
        if path.exists() {
            return Some(path.to_path_buf());
        }
        return None;
    }

    // Search XDG sound directories
    resolve_sound_in_xdg(name)
}

/// Search for a sound in XDG data directories
fn resolve_sound_in_xdg(name: &str) -> Option<PathBuf> {
    let xdg = BaseDirectories::new().ok()?;

    // Search in sounds subdirectory of data dirs
    for data_dir in xdg.get_data_dirs() {
        let sounds_dir = data_dir.join("sounds");
        if let Some(path) = find_sound_in_dir(&sounds_dir, name) {
            return Some(path);
        }
    }

    // Also check user data home
    let sounds_dir = xdg.get_data_home().join("sounds");
    if let Some(path) = find_sound_in_dir(&sounds_dir, name) {
        return Some(path);
    }

    None
}

/// Find a sound file in a directory, trying various extensions
fn find_sound_in_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    if !dir.exists() {
        return None;
    }

    // Try with each extension
    for ext in SOUND_EXTENSIONS {
        let path = dir.join(format!("{name}.{ext}"));
        if path.exists() {
            return Some(path);
        }
    }

    // Try in freedesktop theme structure (theme/category/sound)
    // Common categories: stereo, 5.1
    for category in &["stereo", ""] {
        for ext in SOUND_EXTENSIONS {
            let path = if category.is_empty() {
                dir.join(format!("{name}.{ext}"))
            } else {
                dir.join(category).join(format!("{name}.{ext}"))
            };
            if path.exists() {
                return Some(path);
            }
        }
    }

    // Recursively search subdirectories (sound themes)
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir()
                && let Some(found) = find_sound_in_dir(&path, name)
            {
                return Some(found);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_empty_sound() {
        assert!(resolve_sound("").is_none());
    }

    #[test]
    fn test_resolve_nonexistent_absolute() {
        assert!(resolve_sound("/nonexistent/path/sound.ogg").is_none());
    }
}
