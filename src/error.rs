use derive_more::From;

#[cfg(test)]
pub type Result<T = ()> = std::result::Result<T, Error>;

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, From)]
pub enum Error {
    #[from]
    Io(std::io::Error),

    #[from]
    Toml(verandah_plugin::api::toml::de::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "IO error: {e}"),
            Error::Toml(e) => write!(f, "TOML error: {e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::Toml(e) => Some(e),
        }
    }
}
