use std::{ffi::{c_char, CStr, CString}, fmt::Display, fs::{create_dir_all, remove_dir_all, remove_file, File}, io::{Read, Write}, path::Path};

use clap::Parser;

use hex::FromHex;
use sha1::Digest;

mod error;
mod image;
mod pointer;
mod pretty;
mod sha1sum;

use error::{Error, Result};


#[derive(clap::Subcommand, Debug, Clone)]
enum Action {
    /// Unpack an image
    Unpack {
        /// Path of image to unpack
        image: String,
        /// Path of dir to output, would be deleted if exists, and then created
        outdir: String,
        #[arg(short = 's', long)]
        /// Skip verifying items
        skip_verify: bool,
    },
    /// Pack partition files into an image
    Pack {
        /// Path of image to pack into
        image: String,
        /// Path of dir that contains files
        indir: String,
    },
}

#[derive(Parser, Debug)]
#[command(version)]
struct Arg {
    #[command(subcommand)]
    action: Action,

    #[arg(short = 'v', long, value_enum)]
    /// Force version of the image, disables auto detection for unpack, required
    /// for pack
    imgver: Option<image::ImageVersion>,
}

fn unpack<P1, P2>(image: P1, outdir: P2, skip_verify: bool) -> Result<()>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>
{
    let image = image::Image::try_read(image)?;
    if ! skip_verify {
        image.verify()?;
    }
    println!("=> Read image: {}", &image);
    Ok(())
}

fn main() -> Result<()> {
    let arg = Arg::parse();
    match arg.action {
        Action::Unpack { image, outdir , skip_verify} => unpack(image, outdir, skip_verify),
        Action::Pack { image, indir } => todo!(),
    }
}
