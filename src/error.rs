/*
ampack, to unpack and pack Aml burning images: error handling module
Copyright (C) 2024-present Guoxin "7Ji" Pu

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as
published by the Free Software Foundation, either version 3 of the
License, or (at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

use std::fmt::Display;

#[derive(Debug)]
pub(crate) enum Error {
    IOError (std::io::Error),
    NulError (std::ffi::NulError),
    FromHexError (hex::FromHexError),
    TemplateError (indicatif::style::TemplateError),
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

impl From<indicatif::style::TemplateError> for Error {
    fn from(value: indicatif::style::TemplateError) -> Self {
        Self::TemplateError(value)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::IOError(e) => 
                write!(f, "IO Error: {}", e),
            Error::NulError(e) => 
                write!(f, "Nul Error: {}", e),
            Error::FromHexError(e) => 
                write!(f, "From Hex Error: {}", e),
            Error::TemplateError(e) =>
                write!(f, "Progress Error: {}", e),
            Error::ImageError(e) =>
                write!(f, "Image Error: {}", e),
        }
    }
}

pub(crate) type Result<T> = std::result::Result<T, Error>;