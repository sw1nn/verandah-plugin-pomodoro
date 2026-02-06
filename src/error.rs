use derive_more::From;

#[cfg(test)]
pub type Result<T = ()> = std::result::Result<T, Error>;

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, From)]
pub enum Error {
    #[from]
    Io(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "IO error: {e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
        }
    }
}
