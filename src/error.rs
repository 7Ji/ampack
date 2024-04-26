use std::fmt::Display;

#[derive(Debug)]
pub(crate) enum Error {
    IOError (std::io::Error),
    NulError (std::ffi::NulError),
    FromHexError (hex::FromHexError),
    ImageError (crate::image::ImageError),
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::IOError(value)
    }
}

impl From<std::ffi::NulError> for Error {
    fn from(value: std::ffi::NulError) -> Self {
        Self::NulError(value)
    }
}

impl From<hex::FromHexError> for Error {
    fn from(value: hex::FromHexError) -> Self {
        Self::FromHexError(value)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error: ")?;
        match self {
            Error::IOError(e) => 
                write!(f, "{}", e),
            Error::NulError(e) => 
                write!(f, "{}", e),
            Error::FromHexError(e) => 
                write!(f, "{}", e),
            Error::ImageError(e) => 
                write!(f, "{}", e),
        }
    }
}

pub(crate) type Result<T> = std::result::Result<T, Error>;