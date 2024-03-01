use std::fmt::Display;

use hex::FromHex;

use crate::{Error, Result};

type Sha1sumByteArray = [u8; 20];

pub(crate) struct Sha1sum(Sha1sumByteArray);

// impl TryFrom<&[u8]> for Sha1sum {
//     type Error = Error;

//     fn try_from(value: &[u8]) -> Result<Self> {
//         Ok(Self(Sha1sumByteArray::from_hex(value)?))
//     }
// }

impl Sha1sum {
    pub(crate) fn from_hex(slice: &[u8]) -> Result<Self> {
        Ok(Self(Sha1sumByteArray::from_hex(slice)?))
    }
}

impl Display for Sha1sum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.0.iter() {
            write!(f, "{:02x}", byte)?
        }
        Ok(())
    }
}