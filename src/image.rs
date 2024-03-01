use std::{ffi::CStr, fmt::Display, fs::File, io::{Read, Seek}, path::Path};

use crate::{sha1sum::Sha1sum, Error, Result};

/* These values are always the same for any images */

const MAGIC: u32 = 0x27b51956;
const FILE_TYPE: u32 = 0;
const CURRENT_OFFSET_IN_ITEM: u64 = 0;

#[derive(Debug)]
pub(crate) enum ImageError {
    InvalidMagic(u32),
    IllegalVerify,
    InvalidVersion (u32),
    UnmatchedVerify
}

impl Into<Error> for ImageError {
    fn into(self) -> Error {
        Error::ImageError(self)
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
pub(crate) enum ImageVersion {
    V1,
    #[default]
    V2,
    // V3,
}

type RawImageVersion = u32;

impl TryFrom<RawImageVersion> for ImageVersion {
    type Error = Error;

    fn try_from(value: RawImageVersion) -> Result<Self> {
        match value {
            1 => Ok(Self::V1),
            2 => Ok(Self::V2),
            // 3 => Ok(Self::V3),
            _ => Err(ImageError::InvalidVersion(value).into()),
        }
    }
}

impl Display for ImageVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}",
            match self {
                ImageVersion::V1 => "v1",
                ImageVersion::V2 => "v2",
                // ImageVersion::V3 => "v3",
            }
        )
    }
}

impl ImageVersion {
    fn size_raw_info(&self) -> usize {
        match self {
            ImageVersion::V1 => SIZE_RAW_ITEM_INFO_V1,
            ImageVersion::V2 => SIZE_RAW_ITEM_INFO_V2,
            // ImageVersion::V3 => SIZE_RAW_ITEM_INFO_V3,
        }
    }
}

#[repr(packed)]
struct RawVersionHead {
    crc: u32,
    version: u32,
}

const SIZE_RAW_VERSION: usize = std::mem::size_of::<RawVersionHead>();

#[repr(packed)]
struct RawImageHead {
    version_head: RawVersionHead,
    magic: u32,
    image_size: u64,
    item_align_size: u32,
    item_count: u32,
    _reserve: [u8; 36],
}

const SIZE_RAW_IMAGE_HEAD: usize = std::mem::size_of::<RawImageHead>();

#[repr(packed)]
struct RawItemInfo<const LEN: usize> {
    _item_id: u32,
    _file_type: u32,
    _current_offset_in_item: u64,
    offset_in_image: u64,
    item_size: u64,
    item_main_type: [u8; LEN],
    item_sub_type: [u8; LEN],
    verify: u32,
    _is_backup_item: u16,
    _backup_item_id: u16,
    _reserve: [u8; 24],
}

type RawItemInfoV1 = RawItemInfo<32>;
type RawItemInfoV2 = RawItemInfo<256>;
// type RawItemInfoV3 = RawItemInfo<256>;

const SIZE_RAW_ITEM_INFO_V1: usize = std::mem::size_of::<RawItemInfoV1>();
const SIZE_RAW_ITEM_INFO_V2: usize = std::mem::size_of::<RawItemInfoV2>();
// const SIZE_RAW_ITEM_INFO_V3: usize = std::mem::size_of::<RawItemInfoV3>();

fn cstr_from_slice_u8_c_string(slice: &[u8]) -> &CStr {
    unsafe {CStr::from_ptr(slice.as_ptr() as *const i8)}
}

fn string_from_slice_u8_c_string(slice: &[u8]) -> String {
    cstr_from_slice_u8_c_string(slice).to_string_lossy().into()
}

#[derive(Default)]
struct Item {
    data: Vec<u8>,
    extension: String, // main type
    stem: String, // sub type
    sha1sum: Option<Sha1sum>,
}

#[derive(Default)]
pub(crate) struct Image {
    version: ImageVersion,
    align: u32,
    items: Vec<Item>,
}

impl Display for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Amlogic image {}, align {} bytes, {} entries: [", 
            self.version, self.align, self.items.len())?;
        let mut start = false;
        for item in self.items.iter() {
            if start {
                write!(f, ", ")?;
            } else {
                start = true
            }
            write!(f, "{{ {}.{}, 0x{} bytes, ",
                item.stem, item.extension, item.data.len())?;
            if let Some(sha1sum) = &item.sha1sum {
                write!(f, "sha1sum: {}}}", sha1sum)?
            } else {
                write!(f, "no verify")?
            }
        }
        write!(f, "]")?;
        Ok(())
    }
}

impl TryFrom<&Path> for Image {
    type Error = Error;

    fn try_from(value: &Path) -> Result<Self> {
        let mut file = File::open(value)?;
        let mut buffer = [0; 0x10000];
        file.read_exact(&mut buffer[0..SIZE_RAW_IMAGE_HEAD])?;
        let header = unsafe {
            (buffer.as_ptr() as *const RawImageHead).read()};
        if header.magic != MAGIC {
            eprintln!("Image magic invalid: expected 0x{}, found 0x{}", 
                MAGIC, {header.magic});
            return Err(ImageError::InvalidMagic(header.magic).into())
        }
        let version = 
            ImageVersion::try_from(header.version_head.version)?;
        let size_info = version.size_raw_info();
        let buffer_info = &mut buffer[0..size_info];
        let mut items = Vec::new();
        let mut need_verify: Option<Item> = None;
        for item_id in 0..header.item_count {
            file.seek(std::io::SeekFrom::Start(
                SIZE_RAW_IMAGE_HEAD as u64 + 
                    size_info as u64 * item_id as u64))?;
            file.read_exact(buffer_info)?;
            let (offset, size, verify, 
                main_type, sub_type
            ) = match version 
            {
                ImageVersion::V1 => {
                    let info = unsafe {
                        (buffer_info.as_ptr() as *const RawItemInfoV1).read()};
                    (
                        info.offset_in_image,
                        info.item_size,
                        info.verify,
                        string_from_slice_u8_c_string(
                            &info.item_main_type),
                        string_from_slice_u8_c_string(
                            &info.item_sub_type)
                    )
                },
                ImageVersion::V2 => {
                    let info = unsafe {
                        (buffer_info.as_ptr() as *const RawItemInfoV2).read()};
                    (
                        info.offset_in_image,
                        info.item_size,
                        info.verify, 
                        string_from_slice_u8_c_string(
                            &info.item_main_type),
                        string_from_slice_u8_c_string(
                            &info.item_sub_type)
                    )
                },
            };
            file.seek(std::io::SeekFrom::Start(offset))?;
            let mut data = vec![0; size as usize];
            file.read_exact(&mut data)?;
            if main_type == "VERIFY" {
                let sha1sum = 
                    if size == 48 && data.starts_with(b"sha1sum ") && 
                        verify == 0 
                    {
                        Sha1sum::from_hex(&data[8..48])?
                    } else {
                        return Err(ImageError::IllegalVerify.into())
                    };
                if let Some(mut item) = need_verify {
                    item.sha1sum = Some(sha1sum);
                    items.push(item);
                    need_verify = None;
                } else {
                    eprintln!("Could not find next VERIFY");
                    return Err(ImageError::UnmatchedVerify.into())
                }
            } else {
                let item = Item {
                    data,
                    extension: main_type,
                    stem: sub_type,
                    sha1sum: None,
                };
                if verify == 0 {
                    items.push(item)
                } else {
                    need_verify = Some(item)
                }
            }
        }
        if need_verify.is_some() {
            eprintln!("Could not found last VERIFY");
            return Err(ImageError::UnmatchedVerify.into())
        }
        Ok(Self {
            version,
            align: header.item_align_size,
            items,
        })
    }
}

impl Image {
    pub(crate) fn try_read<P: AsRef<Path>>(file: P) -> Result<Self> {
        file.as_ref().try_into()
    }

    // pub(crate) fn verify(&self) {
    //     for item in self.items.iter() {
    //         if let Some(sha1sum) = item.sha1sum {
                

    //         }
    //     }
    // }

    // pub(crate) fn try_write<P: AsRef<Path>>(&self, dir: P) -> Result<()> {
    //     let parent = dir.as_ref();
    //     if parent.exists() {
    //         println!(" => Removing existing '{}'", parent.display());
    //         if parent.is_dir() {
    //             remove_dir_all(parent)?
    //         } else {
    //             remove_file(parent)?
    //         }
    //     }
    //     println!(" => Creating parent '{}'", parent.display());
    //     create_dir_all(parent)?;
    //     let mut offset = std::mem::size_of::<AmlCImageHead>();
    //     let item_info_size = match image_head.version_head.version {
    //         ImageVersion::V1 => std::mem::size_of::<AmlCItemInfoV1>(),
    //         ImageVersion::V2 => std::mem::size_of::<AmlCItemInfoV2>(),
    //         ImageVersion::V3 => 0,
    //     };
    //     let mut need_verify = None;
    //     for id in 1..image_head.item_count+1 {
    //         let item_info_ptr = unsafe {data.as_ptr().byte_add(offset)};
    //         let item_info: ItemInfo = match image_head.version_head.version {
    //             ImageVersion::V1 => (item_info_ptr as *const AmlCItemInfoV1).into(),
    //             ImageVersion::V2 => (item_info_ptr as *const AmlCItemInfoV2).into(),
    //             ImageVersion::V3 => ItemInfo::default(),
    //         };
    //         println!(" => Item {:02}/{:02}: item id {:02}, main type '{}', \
    //             sub type '{}', type {}, offset in item 0x{:x}, \
    //             offset in image 0x{:x}, verify {}, backup {} (id {})", 
    //             id, image_head.item_count, item_info.item_id, 
    //             item_info.item_main_type, item_info.item_sub_type, 
    //             item_info.file_type, item_info.current_offset_in_item,
    //             item_info.offset_in_image, item_info.verify,
    //             item_info.is_backup_item, item_info.backup_item_id
    //         );
    //         let start  = item_info.offset_in_image as usize;
    //         let end = start + item_info.item_size as usize;
    //         let item = &data[start..end];
    //         match item_info.verify {
    //             0 => if let Some((name, last_item)) = need_verify {
    //                 if item_info.item_main_type != "VERIFY" {
    //                     println!("  -> Last item expects a 'VERIFY' item");
    //                     panic!("Unmatched verify");
    //                 }
    //                 if item_info.item_size != 48 ||
    //                     ! item.starts_with(b"sha1sum ") ||
    //                     item_info.item_sub_type != name
    //                 {
    //                     println!("  -> Item is not a valid 'VERIFY' item");
    //                     panic!("Invalid verify");
    //                 }
    //                 println!("  -> Verifying last item...");
    //                 let sha1sum_expected = <[u8; 20]>::from_hex(&item[8..48])?;
    //                 let sha1sum_actual: [u8; 20] = sha1::Sha1::digest(last_item).into();
    //                 if sha1sum_expected == sha1sum_actual {
    //                     println!("  -> Last item was OK");
    //                 } else {
    //                     println!("  -> Last item was corrupted! Expected {:?}, found {:?}", sha1sum_expected, sha1sum_actual);

    //                 }
    //                 need_verify = None;
    //             } else {
    //                 let item_path = parent.join(
    //                     format!("{}.{}", item_info.item_sub_type, item_info.item_main_type));
    //                 let mut item_file = File::create(item_path)?;
    //                 item_file.write_all(item)?;
    //             },
    //             1 => if let Some(name) = need_verify {
    //                 println!("  -> Another 'need verify' item encoutered before \
    //                     the VERIFY item needed by last item was found");
    //                 panic!("Unmatched verify")
    //             } else {
    //                 println!("  -> This item expects the next item to be 'VERIFY'");
    //                 need_verify = Some(
    //                     (item_info.item_sub_type.clone(), item));
    //             },
    //             _ => panic!("Invalid value for verify"),
    //         }
    //         offset += item_info_size;
    //     }
    //     Ok(())
        
    // }
}