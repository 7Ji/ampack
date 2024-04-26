use std::fmt::Display;

use hex::FromHex;

use indicatif::ProgressBar;
use serde::{Serialize, Deserialize};
use sha1::{Digest, Sha1};

use crate::{Error, Result};

type Sha1sumByteArray = [u8; 20];

#[derive(Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Clone)]
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

    pub(crate) fn from_data(data: &[u8]) -> Self {
        Self(Sha1::digest(data).into())
    }

    pub(crate) fn from_data_with_bar(data: &[u8], bar: &mut ProgressBar) -> Self {
        const STEP: usize = 0x100000;
        let mut hasher = Sha1::new();
        for chunk in data.chunks(STEP) {
            // bar.set_message(format!("{}/{}", id, suffix));
            hasher.update(chunk);
            bar.inc(1);
        }
        bar.finish_and_clear();
        Self(hasher.finalize().into())
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