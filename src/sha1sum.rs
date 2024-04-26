/*
ampack, to unpack and pack Aml burning images: sha1 checksum module
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

use hex::FromHex;

use indicatif::ProgressBar;
use serde::{Serialize, Deserialize};
use sha1::{Digest, Sha1};

use crate::Result;

type Sha1sumByteArray = [u8; 20];

#[derive(Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub(crate) struct Sha1sum(Sha1sumByteArray);

impl Sha1sum {
    pub(crate) fn from_hex(slice: &[u8]) -> Result<Self> {
        Ok(Self(Sha1sumByteArray::from_hex(slice)?))
    }

    pub(crate) fn from_data(data: &[u8]) -> Self {
        Self(Sha1::digest(data).into())
    }

    pub(crate) fn from_data_with_bar(data: &[u8], bar: &ProgressBar) -> Self {
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