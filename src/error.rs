use derive_more::From;

#[allow(dead_code)]
pub type Result<T = ()> = std::result::Result<T, Error>;

#[allow(dead_code)]
#[derive(Debug, From)]
pub enum Error {
    #[from]
    Io(std::io::Error),

    InvalidConfig {
        field: &'static str,
        message: String,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "IO error: {e}"),
            Error::InvalidConfig { field, message } => {
                write!(f, "Invalid config for '{field}': {message}")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::InvalidConfig { .. } => None,
        }
    }
}
