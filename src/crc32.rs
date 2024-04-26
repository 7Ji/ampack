use std::{fs::File, io::{Read, Seek}, path::Path};

use indicatif::ProgressBar;

use crate::Result;

#[derive(Clone, Copy)]
struct Crc32Table {
    table: [u32; 0x100]
}

impl Default for Crc32Table {
    fn default() -> Self {
        let mut table = [0; 0x100];
        for id in 0..0x100 {
            let mut byte = id;
            for _ in 0..8 {
                let int = byte >> 1;
                if byte & 1 == 0 {
                    byte = int;
                } else {
                    byte = int ^ 0xedb88320;
                }
            }
            table[id as usize] = byte
        }
        Self { table }
    }
}

pub(crate) struct Crc32Hasher {
    pub(crate) value: u32,
    table: Crc32Table,
}

impl Default for Crc32Hasher {
    fn default() -> Self {
        Self { 
            value: 0xffffffff,
            table: Crc32Table::default()
        }
    }
}

impl Crc32Hasher {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn update(&mut self, data: &[u8]) {
        for byte in data.iter() {
            let lookup_id = (self.value ^ *byte as u32) & 0xff;
            let lookup_value = self.table.table[lookup_id as usize];
            self.value = lookup_value ^ self.value >> 8;
        }
    }

    pub(crate) fn udpate_with_bar(&mut self, data: &[u8], bar: &ProgressBar) {
        for chunk in data.chunks(0x100000) {
            for byte in chunk.iter() {
                let lookup_id = (self.value ^ *byte as u32) & 0xff;
                let lookup_value = self.table.table[lookup_id as usize];
                self.value = lookup_value ^ self.value >> 8;
            }
            bar.inc(1)
        }
    }

    pub(crate) fn from_reader<R: Read>(mut reader: R) -> Self {
        let mut crc32 = Self::new();
        let mut buffer = [0; 0x100000];
        while let Ok(size) = reader.read(&mut buffer) {
            if size == 0 { break };
            let data = &buffer[0..size];
            crc32.update(data)
        }
        crc32
    }

    pub(crate) fn try_hash_image_file<P: AsRef<Path>>(file: P) -> Result<Self> {
        let mut file = File::open(file)?;
        file.seek(std::io::SeekFrom::Start(4))?;
        Ok(Self::from_reader(file))
    }
}
