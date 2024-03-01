


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

pub(crate) type Result<T> = std::result::Result<T, Error>;