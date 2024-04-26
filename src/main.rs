use std::{ffi::{c_char, CStr, CString}, fmt::Display, fs::{create_dir_all, remove_dir_all, remove_file, File}, io::{Read, Write}, path::Path};

use clap::Parser;

use hex::FromHex;
use sha1::Digest;

mod crc32;
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

fn verify<P: AsRef<Path>>(in_file: P) -> Result<()> {
    let in_file = in_file.as_ref();
    println!("Verifying image at '{}'", in_file.display());
    let image = image::Image::try_read_file(in_file)?;
    image.verify()?;
    image.print_table_stdout();
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
    let image = image::Image::try_read_file(in_file)?;
    if ! no_verify {
        image.verify()?
    }
    image.print_table_stdout();
    image.try_write_dir(&out_dir)?;
    println!("Unpacked image '{}' to '{}'", in_file.display(), out_dir.display());
    Ok(())
}

fn convert<P1, P2>(in_file: P1, out_file: P2, no_verify: bool) -> Result<()>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>
{
    let in_file = in_file.as_ref();
    let out_file = out_file.as_ref();
    println!("Converting image '{}' to '{}'", in_file.display(), out_file.display());
    let mut image = image::Image::try_read_file(&in_file)?;
    if no_verify {
        image.print_table_stdout();
        image.clear_verify()
    } else {
        image.verify()?;
        image.print_table_stdout()
    }
    image.fill_verify()?;
    image.print_table_stdout();
    image.try_write_file(&out_file)?;
    println!("Converted image '{}' to '{}'", in_file.display(), out_file.display());
    Ok(())
}

fn main() -> Result<()> {
    let arg = Arg::parse();
    match arg.action {
        Action::Verify { in_file } => verify(in_file),
        Action::Unpack { in_file, out_dir , no_verify} => unpack(in_file, out_dir, no_verify),
        Action::Convert { in_file, out_file, no_verify } => convert(in_file, out_file, no_verify),
        Action::Pack { in_dir, out_file } => todo!(),
    }
}
