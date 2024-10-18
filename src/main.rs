/*
ampack, to unpack and pack Aml burning images: main cli module
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

use std::path::Path;

use clap::Parser;

mod crc32;
mod error;
mod image;
mod progress;
mod sha1sum;

use error::{Error, Result};
use image::ImageVersion;

use crate::image::Image;


#[derive(clap::Subcommand, Debug, Clone)]
enum Action {
    /// Read and verify and image without unpacking it
    Verify {
        /// Path of image to verify
        in_file: String
    },
    /// Unpack an image to get partition files
    Unpack {
        /// Path of image to unpack
        in_file: String,
        /// Path of dir to output, would be deleted if exists, and then created
        out_dir: String,
        #[arg(long)]
        /// Do not verify items
        no_verify: bool,
    },
    /// Convert an image to another image
    Convert {
        /// Path of the input file
        in_file: String,
        /// Path of the output file
        out_file: String,
        #[arg(long)]
        /// Do not verify input image
        no_verify: bool,
        /// Version of the output image
        #[arg(long, default_value_t)]
        out_ver: ImageVersion,
        /// Alignment of the output image, multiply of 4, 8 for Android >= 11
        #[arg(long, default_value_t = 4)]
        out_align: u8,
        /// Verify the output image after conversion
        #[arg(long)]
        verify: bool,
    },
    /// (Re)pack partition files into an image
    Pack {
        /// Path of image to pack into
        in_dir: String,
        /// Path of dir that contains files
        out_file: String,
        /// Version of the output image
        #[arg(long, default_value_t)]
        out_ver: ImageVersion,
        /// Alignment of the output image, multiply of 4, 8 for Android >= 11
        #[arg(long, default_value_t = 4)]
        out_align: u8,
        /// Verify the output image after packing
        #[arg(long)]
        verify: bool,
    },
    /// Calculate the CRC32 checksum of an image
    Crc32 {
        in_file: String
    }
}

#[derive(Parser, Debug)]
#[command(version)]
struct Arg {
    #[command(subcommand)]
    action: Action
}

fn verify<P: AsRef<Path>>(in_file: P) -> Result<()> {
    let in_file = in_file.as_ref();
    println!("Verifying image at '{}'", in_file.display());
    let image = Image::try_read_file(in_file)?;
    image.verify()?;
    image.print_table_stdout()?;
    println!("Verified image at '{}'", in_file.display());
    Ok(())
}

fn unpack<P1, P2>(in_file: P1, out_dir: P2, no_verify: bool) -> Result<()>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>
{
    let in_file = in_file.as_ref();
    let out_dir = out_dir.as_ref();
    println!("Unpacking image '{}' to '{}'", in_file.display(), out_dir.display());
    let image = Image::try_read_file(in_file)?;
    if ! no_verify {
        image.verify()?
    }
    image.print_table_stdout()?;
    image.try_write_dir(out_dir)?;
    println!("Unpacked image '{}' to '{}'", in_file.display(), out_dir.display());
    Ok(())
}

fn convert<P1, P2>(in_file: P1, out_file: P2, no_verify: bool,
                    out_ver: ImageVersion, out_align: u8, do_verify: bool)
-> Result<()>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>
{
    let in_file = in_file.as_ref();
    let out_file = out_file.as_ref();
    println!("Converting image '{}' to '{}'", in_file.display(), out_file.display());
    let mut image = Image::try_read_file(in_file)?;
    if no_verify {
        image.print_table_stdout()?;
        image.clear_verify()
    } else {
        image.verify()?;
        image.print_table_stdout()?
    }
    image.fill_verify()?;
    image.print_table_stdout()?;
    image.set_ver_align(out_ver, out_align);
    image.try_write_file(out_file)?;
    println!("Converted image '{}' to '{}'", in_file.display(), out_file.display());
    if do_verify {
        verify(out_file)?
    }
    Ok(())
}

fn pack<P1, P2>(in_dir: P1, out_file: P2, out_ver: ImageVersion,
    out_align: u8, do_verify: bool)
-> Result<()>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>
{
    let in_dir = in_dir.as_ref();
    let out_file = out_file.as_ref();
    println!("Packing '{}' to '{}'", in_dir.display(), out_file.display());
    let mut image = Image::try_read_dir(&in_dir)?;
    image.print_table_stdout()?;
    image.fill_verify()?;
    image.print_table_stdout()?;
    image.set_ver_align(out_ver, out_align);
    image.try_write_file(out_file)?;
    println!("Packed '{}' to '{}'", in_dir.display(), out_file.display());
    if do_verify {
        verify(out_file)?
    }
    Ok(())
}

fn do_crc32<P: AsRef<Path>>(in_file: P) -> Result<()> {
    let in_file = in_file.as_ref();
    println!("Calculating CRC32 checksum of '{}'", in_file.display());
    let crc32 = crc32::Crc32Hasher::try_hash_image_file(in_file)?;
    println!("CRC32 checksum of '{}' is 0x{:08x}", in_file.display(), crc32.value);
    Ok(())
}

fn main() -> Result<()> {
    let arg = Arg::parse();
    match arg.action {
        Action::Verify { in_file } => verify(in_file),
        Action::Unpack { in_file, out_dir , no_verify} => unpack(in_file, out_dir, no_verify),
        Action::Convert { in_file, out_file, no_verify, out_ver, out_align, verify } => convert(in_file, out_file, no_verify, out_ver, out_align, verify),
        Action::Pack { in_dir, out_file, out_ver, out_align, verify } => pack(in_dir, out_file, out_ver, out_align, verify),
        Action::Crc32 { in_file } => do_crc32(in_file),
    }
}
