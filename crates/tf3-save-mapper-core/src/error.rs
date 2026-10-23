use std::fmt;
use std::io;

/// What can go wrong reading a save.
#[derive(Debug)]
pub enum Error {
    /// Reading the file or decompressing it failed.
    Io(io::Error),
    /// The stream does not start with `tf**`.
    NotASave,
    /// The stream is larger than the reader accepts.
    TooLarge { limit: u64 },
    /// A field ran past the end of the stream.
    Truncated { at: usize },
    /// A Lua value had a tag the format does not define, or nested too deeply.
    BadLuaValue { at: usize },
    /// No street-edge table was found.
    NoEdges,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::NotASave => write!(f, "not a Transport Fever 3 save (no `tf**` magic)"),
            Error::TooLarge { limit } => {
                write!(f, "the decompressed stream is larger than {limit} bytes")
            }
            Error::Truncated { at } => write!(f, "the stream ends early (reading at byte {at})"),
            Error::BadLuaValue { at } => write!(f, "unreadable Lua value at byte {at}"),
            Error::NoEdges => write!(f, "no street edges found"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
