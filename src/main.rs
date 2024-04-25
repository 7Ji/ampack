use std::{ffi::{c_char, CStr, CString}, fmt::Display, fs::{create_dir_all, remove_dir_all, remove_file, File}, io::{Read, Write}, path::Path};

use clap::Parser;

use hex::FromHex;
use sha1::Digest;

mod error;
mod image;
mod pointer;
mod pretty;
mod sha1sum;
mod sparse;

use error::{Error, Result};


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
        #[arg(short = 's', long)]
        /// Do not verify items
        no_verify: bool,
    },
    /// Convert an image to another image
    Convert {
        /// Path of the input file
        in_file: String,
        /// Path of the output file
        out_file: String
    },
    /// (Re)pack partition files into an image
    Pack {
        /// Path of image to pack into
        in_dir: String,
        /// Path of dir that contains files
        out_file: String,
    },
}

#[derive(Parser, Debug)]
#[command(version)]
struct Arg {
    #[command(subcommand)]
    action: Action,

    #[arg(short = 'v', long, value_enum)]
    /// Force version of the image, disables auto detection for unpack, needed
    /// by 'convert' and 'pack'
    imgver: Option<image::ImageVersion>,
}

fn verify<P: AsRef<Path>>(image: P) -> Result<()> {
    let image = image::Image::try_read_file(image)?;
    image.verify()?;
    image.print_table_stdout();
    Ok(())
}

fn unpack<P1, P2>(image: P1, outdir: P2, no_verify: bool) -> Result<()>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>
{
    let image = image::Image::try_read_file(image)?;
    if ! no_verify {
        image.verify()?
    }
    image.print_table_stdout();
    image.try_write_dir(&outdir)?;
    Ok(())
}

fn main() -> Result<()> {
    let arg = Arg::parse();
    match arg.action {
        Action::Verify { in_file } => verify(in_file),
        Action::Unpack { in_file, out_dir , no_verify} => unpack(in_file, out_dir, no_verify),
        Action::Pack { in_dir, out_file } => todo!(),
        Action::Convert { in_file, out_file } => todo!(),
    }
}
