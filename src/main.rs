use std::{ffi::{c_char, CStr, CString}, fmt::Display, fs::{create_dir_all, remove_dir_all, remove_file, File}, io::{Read, Write}, path::Path};

use clap::Parser;

use hex::FromHex;
use sha1::Digest;

#[derive(Debug)]
enum Error {
    IOError (std::io::Error),
    NulError (std::ffi::NulError),
    FromHexError (hex::FromHexError),
}

// macro_rules! from_error{
//     ($external:ty, $internal:ty) => {
//         impl From<$external> for Error {
//             fn from(value: $external) -> Self {
//                 $internal
//             }
//         }
//     };
// }

// from_error!(std::io::Error, Error::NulError);

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

type Result<T> = std::result::Result<T, Error>;

#[derive(clap::Subcommand, Debug, Clone)]
enum Action {
    /// Unpack an image
    Unpack {
        /// Path of image to unpack
        image: String,
        /// Path of dir to output, would be deleted if exists, and then created
        outdir: String,
    },
    /// Pack partition files into an image
    Pack {
        /// Path of image to pack into
        image: String,
        /// Path of dir that contains files
        indir: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
enum ImageVersion {
    V1,
    V2,
    V3,
}

impl Display for ImageVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}",
            match self {
                ImageVersion::V1 => "v1",
                ImageVersion::V2 => "v2",
                ImageVersion::V3 => "v3",
            }
        )
    }
}

#[derive(Parser, Debug)]
#[command(version)]
struct Arg {
    #[command(subcommand)]
    action: Action,

    #[arg(short = 'v', long, value_enum)]
    /// Force version of the image, disables auto detection for unpack, required
    /// for pack
    imgver: Option<ImageVersion>,
}

fn try_into_human_readble<N: Into<u64>>(original: N) -> (f64, char) {
    let mut number = original.into() as f64;
    const SUFFIXES: [char; 8] = ['B', 'K', 'M', 'G', 'T', 'P', 'E', 'Z' ];
    let mut suffix_id = 0;
    while number >= 1024.0 && suffix_id < 8 {
        number /= 1024.0;
        suffix_id += 1;
    }
    if suffix_id >= 8 {
        return (f64::NAN, '-')
    }
    (number, SUFFIXES[suffix_id])
}

#[repr(packed)]
struct AmlCVersionHead {
    crc: u32,
    version: u32,
}

struct VersionHead {
    crc: u32,
    version: ImageVersion,
}

impl From<&AmlCVersionHead> for VersionHead {
    fn from(value: &AmlCVersionHead) -> Self {
        let version = match value.version {
            1 => ImageVersion::V1,
            2 => ImageVersion::V2,
            3 => ImageVersion::V3,
            _ => panic!("Unknown version")
        };
        Self {
            crc: value.crc,
            version,
        }
    }
}

macro_rules! impl_struct_from_ptr {
    ($stype: ident, $ptype: ident) => {
        impl From<*const $ptype> for $stype {
            fn from(value: *const $ptype) -> Self {
                (&(unsafe {value.read()})).into()
            }
        }
        
    };
}

impl_struct_from_ptr!(VersionHead, AmlCVersionHead);

#[repr(packed)]
struct AmlCImageHead {
    version_head: AmlCVersionHead,
    magic: u32,
    image_size: u64,
    item_align_size: u32,
    item_count: u32,
    _reserve: [u8; 36],
}

struct ImageHead {
    version_head: VersionHead,
    magic: u32,
    image_size: u64,
    item_align_size: u32,
    item_count: u32,
}

impl From<&AmlCImageHead> for ImageHead {
    fn from(value: &AmlCImageHead) -> Self {
        Self {
            version_head: (&value.version_head).into(),
            magic: value.magic,
            image_size: value.image_size,
            item_align_size: value.item_align_size,
            item_count: value.item_count,
        }
    }
}

impl_struct_from_ptr!(ImageHead, AmlCImageHead);

#[repr(packed)]
struct AmlCItemInfoV1 {
    item_id: u32,
    file_type: u32,
    current_offset_in_item: u64,
    offset_in_image: u64,
    item_size: u64,
    item_main_type: [u8; 32],
    item_sub_type: [u8; 32],
    verify: u32,
    is_backup_item: u16,
    backup_item_id: u16,
    _reserve: [u8; 24],
}

#[repr(packed)]
struct AmlCItemInfoV2 {
    item_id: u32,
    file_type: u32,
    current_offset_in_item: u64,
    offset_in_image: u64,
    item_size: u64,
    item_main_type: [u8; 256],
    item_sub_type: [u8; 256],
    verify: u32,
    is_backup_item: u16,
    backup_item_id: u16,
    _reserve: [u8; 24],
}

#[derive(Default)]
struct ItemInfo {
    item_id: u32,
    file_type: u32,
    current_offset_in_item: u64,
    offset_in_image: u64,
    item_size: u64,
    item_main_type: String,
    item_sub_type: String,
    verify: u32,
    is_backup_item: u16,
    backup_item_id: u16,
}

impl From<&AmlCItemInfoV1> for ItemInfo {
    fn from(value: &AmlCItemInfoV1) -> Self {
        ItemInfo {
            item_id: value.item_id,
            file_type: value.file_type,
            current_offset_in_item: value.current_offset_in_item,
            offset_in_image: value.offset_in_image,
            item_size: value.item_size,
            item_main_type: string_from_slice_u8_c_string(
                &value.item_main_type),
            item_sub_type: string_from_slice_u8_c_string(
                &value.item_sub_type),
            verify: value.verify,
            is_backup_item: value.is_backup_item,
            backup_item_id: value.backup_item_id,
        }
    }
}

impl_struct_from_ptr!(ItemInfo, AmlCItemInfoV1);

impl From<&AmlCItemInfoV2> for ItemInfo {
    fn from(value: &AmlCItemInfoV2) -> Self {
        ItemInfo {
            item_id: value.item_id,
            file_type: value.file_type,
            current_offset_in_item: value.current_offset_in_item,
            offset_in_image: value.offset_in_image,
            item_size: value.item_size,
            item_main_type: string_from_slice_u8_c_string(
                &value.item_main_type),
            item_sub_type: string_from_slice_u8_c_string(
                &value.item_sub_type),
            verify: value.verify,
            is_backup_item: value.is_backup_item,
            backup_item_id: value.backup_item_id,
        }
    }
}

impl_struct_from_ptr!(ItemInfo, AmlCItemInfoV2);

fn cstr_from_slice_u8_c_string(slice: &[u8]) -> &CStr {
    unsafe {CStr::from_ptr(slice.as_ptr() as *const i8)}
}

fn string_from_slice_u8_c_string(slice: &[u8]) -> String {
    cstr_from_slice_u8_c_string(slice).to_string_lossy().into()
}

fn unpack<P1, P2>(image: P1, outdir: P2) -> Result<()>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>
{
    let mut file = File::open(image.as_ref())?;
    let meatadata = file.metadata()?;
    let len = meatadata.len();
    let (len_readable, suffix) = try_into_human_readble(len);
    println!("Extracting '{}'", image.as_ref().display());
    println!(" => File length is {:.2}{} (0x{:x})", len_readable, suffix, len);
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;
    // let records_count = data[0x18];
    let image_head: ImageHead = (data.as_ptr() as *const AmlCImageHead).into();
    println!(" => Image crc 0x{:x}, version {}, magic 0x{:x}, \
        image size 0x{:x}, alignment 0x{:x}, count {}", 
        image_head.version_head.crc, image_head.version_head.version,
        image_head.magic, image_head.image_size, image_head.item_align_size,
        image_head.item_count
    );
    let parent = outdir.as_ref();
    if parent.exists() {
        println!(" => Removing existing '{}'", parent.display());
        if parent.is_dir() {
            remove_dir_all(parent)?
        } else {
            remove_file(parent)?
        }
    }
    println!(" => Creating parent '{}'", parent.display());
    create_dir_all(parent)?;
    let mut offset = std::mem::size_of::<AmlCImageHead>();
    let item_info_size = match image_head.version_head.version {
        ImageVersion::V1 => std::mem::size_of::<AmlCItemInfoV1>(),
        ImageVersion::V2 => std::mem::size_of::<AmlCItemInfoV2>(),
        ImageVersion::V3 => 0,
    };
    let mut need_verify = None;
    for id in 1..image_head.item_count+1 {
        let item_info_ptr = unsafe {data.as_ptr().byte_add(offset)};
        let item_info: ItemInfo = match image_head.version_head.version {
            ImageVersion::V1 => (item_info_ptr as *const AmlCItemInfoV1).into(),
            ImageVersion::V2 => (item_info_ptr as *const AmlCItemInfoV2).into(),
            ImageVersion::V3 => ItemInfo::default(),
        };
        println!(" => Item {:02}/{:02}: item id {:02}, main type '{}', \
            sub type '{}', type {}, offset in item 0x{:x}, \
            offset in image 0x{:x}, verify {}, backup {} (id {})", 
            id, image_head.item_count, item_info.item_id, 
            item_info.item_main_type, item_info.item_sub_type, 
            item_info.file_type, item_info.current_offset_in_item,
            item_info.offset_in_image, item_info.verify,
            item_info.is_backup_item, item_info.backup_item_id
        );
        let start  = item_info.offset_in_image as usize;
        let end = start + item_info.item_size as usize;
        let item = &data[start..end];
        match item_info.verify {
            0 => if let Some((name, last_item)) = need_verify {
                if item_info.item_main_type != "VERIFY" {
                    println!("  -> Last item expects a 'VERIFY' item");
                    panic!("Unmatched verify");
                }
                if item_info.item_size != 48 ||
                    ! item.starts_with(b"sha1sum ") ||
                    item_info.item_sub_type != name
                {
                    println!("  -> Item is not a valid 'VERIFY' item");
                    panic!("Invalid verify");
                }
                println!("  -> Verifying last item...");
                let sha1sum_expected = <[u8; 20]>::from_hex(&item[8..48])?;
                let sha1sum_actual: [u8; 20] = sha1::Sha1::digest(last_item).into();
                if sha1sum_expected == sha1sum_actual {
                    println!("  -> Last item was OK");
                } else {
                    println!("  -> Last item was corrupted! Expected {:?}, found {:?}", sha1sum_expected, sha1sum_actual);

                }
                need_verify = None;
            } else {
                let item_path = parent.join(
                    format!("{}.{}", item_info.item_sub_type, item_info.item_main_type));
                let mut item_file = File::create(item_path)?;
                item_file.write_all(item)?;
            },
            1 => if let Some(name) = need_verify {
                println!("  -> Another 'need verify' item encoutered before \
                    the VERIFY item needed by last item was found");
                panic!("Unmatched verify")
            } else {
                println!("  -> This item expects the next item to be 'VERIFY'");
                need_verify = Some(
                    (item_info.item_sub_type.clone(), item));
            },
            _ => panic!("Invalid value for verify"),
        }
        offset += item_info_size;
    }
    Ok(())
}

fn main() -> Result<()> {
    println!("{}, {}, {}, {}", 
        std::mem::size_of::<AmlCVersionHead>(),
        std::mem::size_of::<AmlCImageHead>(),
        std::mem::size_of::<AmlCItemInfoV1>(),
        std::mem::size_of::<AmlCItemInfoV2>()
    );
    // return Ok(());
    let arg = Arg::parse();
    match arg.action {
        Action::Unpack { image, outdir } => unpack(image, outdir),
        Action::Pack { image, indir } => todo!(),
    }
}
