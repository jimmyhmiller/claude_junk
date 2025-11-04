use std::io;
use std::fmt;
use std::error::Error;

#[derive(Debug)]
pub enum HprofError {
    Io(io::Error),
    InvalidHeader(String),
    InvalidTag(u8),
    InvalidRecord(String),
    UnexpectedEof,
    InvalidUtf8,
    InvalidIdSize(u32),
}

impl fmt::Display for HprofError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HprofError::Io(err) => write!(f, "IO error: {}", err),
            HprofError::InvalidHeader(msg) => write!(f, "Invalid header: {}", msg),
            HprofError::InvalidTag(tag) => write!(f, "Invalid tag: {:#x}", tag),
            HprofError::InvalidRecord(msg) => write!(f, "Invalid record: {}", msg),
            HprofError::UnexpectedEof => write!(f, "Unexpected end of file"),
            HprofError::InvalidUtf8 => write!(f, "Invalid UTF-8 string"),
            HprofError::InvalidIdSize(size) => write!(f, "Invalid identifier size: {}", size),
        }
    }
}

impl Error for HprofError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            HprofError::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for HprofError {
    fn from(err: io::Error) -> Self {
        HprofError::Io(err)
    }
}

pub type Result<T> = std::result::Result<T, HprofError>;
