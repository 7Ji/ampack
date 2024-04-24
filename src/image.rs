use std::{ffi::CStr, fmt::Display, fs::File, io::{Read, Seek}, path::Path, time::Duration};

use indicatif::{ProgressBar, ProgressStyle};
use serde::{Serialize, Deserialize};

use crate::{sha1sum::Sha1sum, Error, Result};

/* These values are always the same for any images */

const MAGIC: u32 = 0x27b51956;
const FILE_TYPE: u32 = 0;
const CURRENT_OFFSET_IN_ITEM: u64 = 0;

#[derive(Debug)]
pub(crate) enum ImageError {
    InvalidMagic {
        magic: u32
    },
    IllegalVerify,
    InvalidVersion {
        version: u32
    },
    UnmatchedVerify,
    DuplicatedItem {
        stem: String,
        extension: String,
    },
    MissingItem {
        stem: String,
        extension: String,
    },
}

impl Into<Error> for ImageError {
    fn into(self) -> Error {
        Error::ImageError(self)
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, 
    clap::ValueEnum, Serialize, Deserialize)]
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
            _ => Err(ImageError::InvalidVersion {version: value}.into()),
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
struct RawItemInfoVariableLength<const LEN: usize> {
    item_id: u32,
    file_type: u32,
    current_offset_in_item: u64,
    offset_in_image: u64,
    item_size: u64,
    item_main_type: [u8; LEN],
    item_sub_type: [u8; LEN],
    verify: u32,
    is_backup_item: u16,
    backup_item_id: u16,
    reserve: [u8; 24],
}

type RawItemInfoV1 = RawItemInfoVariableLength<32>;
type RawItemInfoV2 = RawItemInfoVariableLength<256>;
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

struct RawItemInfo {
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

impl<const LEN: usize> From<RawItemInfoVariableLength<LEN>> for RawItemInfo {
    fn from(value: RawItemInfoVariableLength<LEN>) -> Self {
        let main_type = value.item_main_type;
        let sub_type = value.item_sub_type;
        Self {
            item_id: value.item_id,
            file_type: value.file_type,
            current_offset_in_item: value.current_offset_in_item,
            offset_in_image: value.offset_in_image,
            item_size: value.item_size,
            item_main_type: string_from_slice_u8_c_string(&main_type),
            item_sub_type: string_from_slice_u8_c_string(&sub_type),
            verify: value.verify,
            is_backup_item: value.is_backup_item,
            backup_item_id: value.backup_item_id,
        }
    }
}


#[derive(Default, Serialize, Deserialize)]
struct Item {
    data: Vec<u8>,
    extension: String, // main type
    stem: String, // sub type
    sha1sum: Option<Sha1sum>,
}

#[derive(Default, Serialize, Deserialize)]
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
                write!(f, "no verify }}")?
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
            return Err(ImageError::InvalidMagic{magic: header.magic}.into())
        }
        let version = 
            ImageVersion::try_from(header.version_head.version)?;
        let size_info = version.size_raw_info();
        let buffer_info = &mut buffer[0..size_info];
        let mut items = Vec::new();
        let mut need_verify: Option<Item> = None;
        macro_rules! cell_right {
            ($raw: expr) => {
                $raw.cell().justify(Justify::Right)
            };
        }
        macro_rules! cell_bold_center {
            ($raw: expr) => {
                $raw.cell().bold(true).justify(Justify::Center)
            };
        }
        use cli_table::{Cell, Style, Table, Row, format::Justify};
        let mut rows = Vec::new();
        // println!("Reading image...");
        let progress_bar = ProgressBar::new(header.item_count.into());
        progress_bar.set_style(ProgressStyle::with_template(
            "Reading image => [{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}").unwrap());
        progress_bar.enable_steady_tick(Duration::from_secs(1));
        for item_id in 0..header.item_count {
            file.seek(std::io::SeekFrom::Start(
                SIZE_RAW_IMAGE_HEAD as u64 + 
                    size_info as u64 * item_id as u64))?;
            file.read_exact(buffer_info)?;
            let pointer = buffer_info.as_ptr();
            let item_info: RawItemInfo = match version {
                ImageVersion::V1 => unsafe {(pointer as *const RawItemInfoV1).read()}.into(),
                ImageVersion::V2 => unsafe {(pointer as *const RawItemInfoV2).read()}.into(),
            };
            // progress_bar.message(item_info.item_main_type);
            progress_bar.set_message(format!("{}.{}", 
                item_info.item_sub_type, item_info.item_main_type));
            file.seek(std::io::SeekFrom::Start(item_info.offset_in_image))?;
            let mut data = vec![0; item_info.item_size as usize];
            file.read_exact(&mut data)?;
            if let Some(mut item_need_verify) = need_verify {
                if item_info.item_main_type == "VERIFY" {
                    let sha1sum = 
                        if item_info.item_size == 48 && 
                            data.starts_with(b"sha1sum ") && 
                            item_info.verify == 0 
                        {
                            Sha1sum::from_hex(&data[8..48])?
                        } else {
                            eprintln!("Verify item content is not sha1sum");
                            return Err(ImageError::IllegalVerify.into())
                        };
                    item_need_verify.sha1sum = Some(sha1sum);
                    items.push(item_need_verify);
                    need_verify = None;
                } else {
                    eprintln!("Item after {}.{} that needs verify is not a \
                        verify item but a non-verify item {}.{}",
                        item_need_verify.stem, item_need_verify.extension,
                        item_info.item_sub_type, item_info.item_main_type);
                    return Err(ImageError::UnmatchedVerify.into())
                }
            } else {
                let item = Item {
                    data,
                    extension: item_info.item_main_type.clone(),
                    stem: item_info.item_sub_type.clone(),
                    sha1sum: None,
                };
                if item_info.verify == 0 {
                    items.push(item)
                } else {
                    need_verify = Some(item)
                }
            }
            rows.push([
                cell_right!(item_info.item_id),
                cell_right!(item_info.file_type),
                cell_right!(format!("0x{:x}", item_info.current_offset_in_item)),
                cell_right!(format!("0x{:x}", item_info.offset_in_image)),
                cell_right!(format!("0x{:x}", item_info.item_size)),
                cell_right!(item_info.item_main_type),
                cell_right!(item_info.item_sub_type),
                cell_right!(item_info.verify),
                if item_info.is_backup_item == 0 {
                    format!("no ({})", item_info.backup_item_id).cell()
                } else {
                    format!("yes ({})", item_info.backup_item_id).cell()
                }.justify(Justify::Right)
            ]);
            progress_bar.inc(1);
        }
        progress_bar.finish_and_clear();
        let table = rows.table().title([
            cell_bold_center!("ID"),
            cell_bold_center!("type"),
            cell_bold_center!("item off"),
            cell_bold_center!("image off"),
            cell_bold_center!("size"),
            cell_bold_center!("main type"),
            cell_bold_center!("sub type"),
            cell_bold_center!("verify"),
            cell_bold_center!("backup (id)")
        ]).bold(true);
        if need_verify.is_some() {
            eprintln!("Could not found last VERIFY");
            return Err(ImageError::UnmatchedVerify.into())
        }
        println!("Item infos in raw image:");
        cli_table::print_stdout(table).unwrap();
        Ok(Self {
            version,
            align: header.item_align_size,
            items,
        })
    }
}

impl Image {
    fn find_item(&self, stem: &str, extension: &str) -> Result<&Item> {
        let mut result = None;

        for item in self.items.iter() {
            if item.stem == stem && item.extension == extension {
                if result.is_some() {
                    eprintln!("Duplicated image item: {}.{}", stem, extension);
                    return Err(ImageError::DuplicatedItem { 
                        stem: stem.into(), extension: extension.into()}.into());
                }
                result = Some(item);
            }
        }
        match result {
            Some(item) => Ok(item),
            None => {
                eprintln!("Missing image item: {}.{}", stem, extension);
                Err(ImageError::MissingItem { 
                    stem: stem.into(), extension: extension.into() }.into())
            }
        }
    }

    fn find_item_verified(&self, stem: &str, extension: &str) -> Result<&Item> {
        match self.find_item(stem, extension) {
            Ok(item) => 
                if item.sha1sum.is_some() {
                    Ok(item)
                } else {
                    eprintln!("Item {}.{} is not verified", stem, extension);
                    Err(ImageError::UnmatchedVerify.into())
                },
            Err(e) => Err(e),
        }
    }

    fn find_ddr_usb(&self) -> Result<&Item> {
        self.find_item("DDR", "USB")
    }

    fn find_uboot_usb(&self) -> Result<&Item> {
        self.find_item("UBOOT", "USB")
    }

    fn find_aml_sdc_burn_ini(&self) -> Result<&Item> {
        self.find_item("aml_sdc_burn", "ini")
    }

    fn find_platform_conf(&self) -> Result<&Item> {
        self.find_item("platform", "conf")
    }

    pub(crate) fn verify(&self) -> Result<()> {
        let need_verifies: Vec<&Item> = self.items.iter().filter(
            |item|item.sha1sum.is_some()).collect();
        let progress_bar = ProgressBar::new(need_verifies.len() as u64);
        progress_bar.set_style(ProgressStyle::with_template(
            "Verifying items => [{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}").unwrap());
        progress_bar.enable_steady_tick(Duration::from_secs(1));
        use rayon::prelude::*;
        let result = need_verifies.par_iter().map(|item| {
            let name = format!("{}.{}", item.stem, item.extension);
            progress_bar.set_message(name.clone());
            // sha1
            let sha1sum_record = item.sha1sum.as_ref().expect("Sha1sum not recorded");
            let sha1sum_calculated = Sha1sum::from_data(&item.data);
            if sha1sum_record != &sha1sum_calculated {
                eprintln!("Recorded SHA1sum ({}) different from calculated \
                    SHA1sum ({}) for item '{}'", sha1sum_record, 
                    sha1sum_calculated, name);
                return Err(Error::ImageError(ImageError::IllegalVerify));
            }
            progress_bar.inc(1);
            Ok(())
        }).find_first(|r|r.is_err());
        progress_bar.finish_and_clear();
        if let Some(r) = result {
            if let Err(e) = r {
                return Err(e)
            } else {
                eprintln!("Unexpected: Filtered error still results in OK");
            }
        }
        Ok(())
    }

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