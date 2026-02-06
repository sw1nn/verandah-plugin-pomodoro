use std::{
    fs,
    io::Read,
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread::{self, JoinHandle},
};

use xdg::BaseDirectories;

use crate::timer::Timer;

const SOCKET_DIR: &str = "verandah-plugin-pomodoro";
const SOCKET_NAME: &str = "pomodoro.socket";

/// Commands that can be sent to the timer
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Toggle,
    Start,
    Stop,
    Reset,
    Skip,
}

impl Command {
    pub fn parse<S>(s: S) -> Option<Self>
    where
        S: AsRef<str>,
    {
        match s.as_ref().trim().to_lowercase().as_str() {
            "toggle" => Some(Command::Toggle),
            "start" => Some(Command::Start),
            "stop" => Some(Command::Stop),
            "reset" => Some(Command::Reset),
            "skip" => Some(Command::Skip),
            _ => None,
        }
    }

    pub fn apply(&self, timer: &mut Timer) {
        match self {
            Command::Toggle => timer.toggle(),
            Command::Start => timer.start(),
            Command::Stop => timer.pause(),
            Command::Reset => timer.reset(),
            Command::Skip => {
                let _ = timer.skip();
            }
        }
    }
}

/// Get the socket path using XDG runtime directory
pub fn get_socket_path() -> Option<PathBuf> {
    let xdg = BaseDirectories::with_prefix(SOCKET_DIR).ok()?;
    xdg.place_runtime_file(SOCKET_NAME).ok()
}

/// Find existing socket for the control client
pub fn find_socket() -> Option<PathBuf> {
    let xdg = BaseDirectories::with_prefix(SOCKET_DIR).ok()?;

    xdg.list_runtime_files(".")
        .into_iter()
        .find(|path| path.file_name().map(|n| n == SOCKET_NAME).unwrap_or(false))
}

/// Send a command to a running pomodoro instance
pub fn send_command<S>(command: S) -> std::io::Result<()>
where
    S: AsRef<str>,
{
    let socket_path = find_socket().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "No running pomodoro instance found",
        )
    })?;

    let mut stream = UnixStream::connect(&socket_path)?;
    std::io::Write::write_all(&mut stream, command.as_ref().as_bytes())?;
    Ok(())
}

/// Socket listener that receives commands and sends them through a channel
pub struct SocketListener {
    socket_path: PathBuf,
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl SocketListener {
    pub fn new(command_tx: Sender<Command>) -> std::io::Result<Self> {
        let socket_path = get_socket_path()
            .ok_or_else(|| std::io::Error::other("Failed to determine XDG runtime directory"))?;

        // Check if an existing socket is in use by another instance
        if socket_path.exists() {
            if UnixStream::connect(&socket_path).is_ok() {
                return Err(std::io::Error::other(
                    "Another pomodoro instance is already running",
                ));
            }
            // Stale socket from a crashed process, safe to remove
            fs::remove_file(&socket_path)?;
        }

        let listener = UnixListener::bind(&socket_path)?;
        listener.set_nonblocking(true)?;

        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = Arc::clone(&shutdown);
        let socket_path_clone = socket_path.clone();

        let handle = thread::spawn(move || {
            Self::listen_loop(listener, command_tx, shutdown_clone, &socket_path_clone);
        });

        tracing::info!(path = %socket_path.display(), "Socket listener started");

        Ok(SocketListener {
            socket_path,
            shutdown,
            handle: Some(handle),
        })
    }

    fn listen_loop(
        listener: UnixListener,
        tx: Sender<Command>,
        shutdown: Arc<AtomicBool>,
        socket_path: &Path,
    ) {
        while !shutdown.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut message = String::new();
                    if let Err(e) = stream.read_to_string(&mut message) {
                        tracing::warn!(error = %e, "Failed to read from socket");
                        continue;
                    }

                    tracing::debug!(message = %message.trim(), "Received command");

                    if let Some(cmd) = Command::parse(&message) {
                        if tx.send(cmd).is_err() {
                            tracing::warn!("Command channel closed");
                            break;
                        }
                    } else {
                        tracing::warn!(message = %message.trim(), "Unknown command");
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Socket accept error");
                    thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }

        // Cleanup socket on exit
        if socket_path.exists() {
            let _ = fs::remove_file(socket_path);
        }
        tracing::info!("Socket listener stopped");
    }

    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        if self.socket_path.exists() {
            let _ = fs::remove_file(&self.socket_path);
        }
    }
}

impl Drop for SocketListener {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Create a command channel
pub fn command_channel() -> (Sender<Command>, Receiver<Command>) {
    mpsc::channel()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_parse() -> crate::error::Result<()> {
        assert_eq!(Command::parse("toggle"), Some(Command::Toggle));
        assert_eq!(Command::parse("TOGGLE"), Some(Command::Toggle));
        assert_eq!(Command::parse("  start  "), Some(Command::Start));
        assert_eq!(Command::parse("stop"), Some(Command::Stop));
        assert_eq!(Command::parse("reset"), Some(Command::Reset));
        assert_eq!(Command::parse("skip"), Some(Command::Skip));
        assert_eq!(Command::parse("unknown"), None);
        Ok(())
    }

    #[test]
    fn test_command_apply_toggle() -> crate::error::Result<()> {
        let config = crate::config::ConfigBuilder::default().build();
        let mut timer = Timer::new(&config);

        assert!(!timer.is_running());
        Command::Toggle.apply(&mut timer);
        assert!(timer.is_running());
        Command::Toggle.apply(&mut timer);
        assert!(!timer.is_running());
        Ok(())
    }

    #[test]
    fn test_command_apply_start_stop() -> crate::error::Result<()> {
        let config = crate::config::ConfigBuilder::default().build();
        let mut timer = Timer::new(&config);

        assert!(!timer.is_running());
        Command::Start.apply(&mut timer);
        assert!(timer.is_running());
        Command::Stop.apply(&mut timer);
        assert!(!timer.is_running());
        Ok(())
    }
}
