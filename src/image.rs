use std::{cmp::min, ffi::{CStr, CString}, fmt::Display, fs::{create_dir_all, remove_dir_all, remove_file, File}, io::{Read, Seek, Write}, ops::{Deref, DerefMut}, path::Path, time::Duration};

use cli_table::{Cell, Style, Table, Row, format::Justify};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::{Serialize, Deserialize};

use crate::{sha1sum::Sha1sum, Error, Result};

/* These values are always the same for any images */

const MAGIC: u32 = 0x27b51956;
const FILE_TYPE_GENERIC: u32 = 0;
const FILE_TYPE_SPARSE: u32 = 254;
const CURRENT_OFFSET_IN_ITEM: u64 = 0;
const ANDROID_SPARSE_IMAGE_MAGIC: u64 = 0xed26ff3a;
const ANDROID_SPARSE_IMAGE_MAGIC_BYTES: [u8; 4] = [0x3a, 0xff, 0x26, 0xed];

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
    SizeMismatch {
        exptected: usize,
        actual: usize
    }
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

impl Into<RawImageVersion> for &ImageVersion {
    fn into(self) -> RawImageVersion {
        match self {
            ImageVersion::V1 => 1,
            ImageVersion::V2 => 2,
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

    fn size_item_type(&self) -> usize {
        match self {
            ImageVersion::V1 => SIZE_ITEM_TYPE_V1,
            ImageVersion::V2 => SIZE_ITEM_TYPE_V2,
        }
    }
}

#[repr(packed)]
struct RawImageHead {
    crc: u32,
    version: u32,
    magic: u32,
    image_size: u64,
    item_align_size: u32,
    item_count: u32,
    _reserve: [u8; 36],
}

impl RawImageHead {
    fn new(version: &ImageVersion) -> Self {
        Self {
            crc: 0,
            version: version.into(),
            magic: MAGIC,
            image_size: 0,
            item_align_size: 4,
            item_count: 0,
            _reserve: [0; 36],
        }
    }
}

impl From<&ImageVersion> for RawImageHead {
    fn from(value: &ImageVersion) -> Self {
        Self::new(value)
    }
}

const SIZE_RAW_IMAGE_HEAD: usize = std::mem::size_of::<RawImageHead>();
const SIZE_ITEM_TYPE_V1: usize = 32;
const SIZE_ITEM_TYPE_V2: usize = 256;


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

type RawItemInfoV1 = RawItemInfoVariableLength<SIZE_ITEM_TYPE_V1>;
type RawItemInfoV2 = RawItemInfoVariableLength<SIZE_ITEM_TYPE_V2>;
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

fn bytes_fill_from_str(dest: &mut [u8], src: &str) {
    let src = src.as_bytes();
    let len = min(dest.len() - 1, src.len());
    dest[0..len].copy_from_slice(&src[0..len])
}


impl<const LEN: usize> Into<RawItemInfoVariableLength<LEN>> for &RawItemInfo {
    fn into(self) -> RawItemInfoVariableLength<LEN> {
        let mut item_main_type = [0; LEN];
        bytes_fill_from_str(&mut item_main_type, &self.item_main_type);
        let mut item_sub_type = [0; LEN];
        bytes_fill_from_str(&mut item_sub_type, &self.item_sub_type);
        RawItemInfoVariableLength { 
            item_id: self.item_id,
            file_type: self.file_type,
            current_offset_in_item: self.current_offset_in_item,
            offset_in_image: self.offset_in_image,
            item_size: self.item_size,
            item_main_type,
            item_sub_type,
            verify: self.verify,
            is_backup_item: self.is_backup_item,
            backup_item_id: self.backup_item_id, 
            reserve: [0; 24]
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
            ImageVersion::try_from(header.version)?;
        let size_info = version.size_raw_info();
        let buffer_info = &mut buffer[0..size_info];
        let mut items = Vec::new();
        let mut need_verify: Option<Item> = None;
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
                if item_info.item_sub_type != item_need_verify.stem {
                    eprintln!("Partition {} does not have its verify right \
                        after it, but {}.{}", item_need_verify.stem,
                        item_info.item_sub_type, item_info.item_main_type);
                    return Err(ImageError::UnmatchedVerify.into())
                }
                if item_info.item_main_type != "VERIFY" {
                    eprintln!("Item after {}.{} that needs verify is not a \
                        verify item but a non-verify item {}.{}",
                        item_need_verify.stem, item_need_verify.extension,
                        item_info.item_sub_type, item_info.item_main_type);
                    return Err(ImageError::UnmatchedVerify.into())
                }
                if ! (item_info.item_size == 48 && 
                        data.starts_with(b"sha1sum ") && 
                        item_info.verify == 0) 
                {
                    eprintln!("Verify item content for {} is not sha1sum",
                        item_need_verify.stem);
                    return Err(ImageError::IllegalVerify.into())
                }
                let sha1sum = Sha1sum::from_hex(&data[8..48])?;
                item_need_verify.sha1sum = Some(sha1sum);
                items.push(item_need_verify);
                need_verify = None;
            } else {
                let item = Item {
                    data,
                    extension: item_info.item_main_type.clone(),
                    stem: item_info.item_sub_type.clone(),
                    sha1sum: None,
                };
                if item.extension == "PARTITION" {
                    if item_info.verify == 0 {
                        eprintln!("Partition {} does not have verify",
                            item.stem);
                        return Err(ImageError::UnmatchedVerify.into())
                    }
                    need_verify = Some(item)
                } else {
                    if item_info.verify != 0 {
                        eprintln!("Item {}.{} has verify", item.stem, item.extension);
                        return Err(ImageError::IllegalVerify.into())
                    }
                    items.push(item)
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

struct Essentials<'a> {
    ddr_usb: &'a Item,
    uboot_usb: &'a Item,
    aml_sdc_burn_init: &'a Item,
    meson1_dtb: &'a Item,
    platform_conf: &'a Item
}

impl<'a> TryFrom<&'a Image> for Essentials<'a> {
    type Error = Error;

    fn try_from(image: &'a Image) -> Result<Self> {
        Ok(Self {
            ddr_usb: image.find_item("DDR", "USB")?,
            uboot_usb: image.find_item("UBOOT", "USB")?,
            aml_sdc_burn_init: image.find_item("aml_sdc_burn", "ini")?,
            meson1_dtb: image.find_item("meson1", "dtb")?,
            platform_conf: image.find_item("platform", "conf")?,
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

    fn partitions(&self) -> Vec<&Item> {
        self.items.iter().filter(
            |item|item.extension == "PARTITION").collect()
    }

    pub(crate) fn verify(&self) -> Result<()> {
        let essentials = Essentials::try_from(self)?;
        let need_verifies: Vec<&Item> = self.items.iter().filter(
            |item|item.sha1sum.is_some()).collect();
        let multiprogress = MultiProgress::new();
        let mut mapped: Vec<(&Item, String, ProgressBar)> = need_verifies.iter().map(
        |item|
        {
            let name = format!("{}.{}", item.stem, item.extension);
            let progress_bar = multiprogress.add(ProgressBar::new(item.data.len() as u64 / 0x100000));
            progress_bar.set_style(ProgressStyle::with_template(&format!("Verifying item => {} {}", "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>5}/{len:>5} MiB", name)).unwrap());
            progress_bar.set_message("Waiting for start...");
            (*item, name, progress_bar)
        }).collect();
        use rayon::prelude::*;
        let result = mapped.par_iter_mut().map(|(item, name, ref mut progress_bar)| {
            let sha1sum_record = item.sha1sum.as_ref().expect("Sha1sum not recorded");
            let sha1sum_calculated = Sha1sum::from_data_with_bar(&item.data, progress_bar);
            if sha1sum_record != &sha1sum_calculated {
                eprintln!("Recorded SHA1sum ({}) different from calculated \
                    SHA1sum ({}) for item '{}'", sha1sum_record, 
                    sha1sum_calculated, name);
                return Err(Error::ImageError(ImageError::IllegalVerify));
            }
            Ok(())
        }).find_first(|r|r.is_err());
        multiprogress.clear().unwrap();
        if let Some(r) = result {
            if let Err(e) = r {
                return Err(e)
            } else {
                eprintln!("Unexpected: Filtered error still results in OK");
            }
        }
        Ok(())
    }

    pub(crate) fn clear_verify(&mut self) {
        for item in self.items.iter_mut() {
            item.sha1sum = None
        }
    }

    pub(crate) fn fill_verify(&mut self) -> Result<()> {
        let mut need_verifies: Vec<&mut Item> = self.items.iter_mut().filter(
            |item|item.sha1sum.is_none()).collect();
        let multiprogress = MultiProgress::new();
        let mut mapped: Vec<(&Item, ProgressBar)> = need_verifies.iter().map(
        |item|
        {
            let name = format!("{}.{}", item.stem, item.extension);
            let progress_bar = multiprogress.add(ProgressBar::new(item.data.len() as u64 / 0x100000));
            progress_bar.set_style(ProgressStyle::with_template(&format!("Generating verify => {} {}", "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>5}/{len:>5} MiB", name)).unwrap());
            progress_bar.set_message("Waiting for start...");
            (item as &Item, progress_bar)
        }).collect();
        use rayon::prelude::*;
        let sha1sums: Vec<Sha1sum> = mapped.par_iter_mut().map(|(item, ref mut progress_bar)| {
            Sha1sum::from_data_with_bar(&item.data, progress_bar)
        }).collect();
        multiprogress.clear().unwrap();
        for (item, sha1sum) in need_verifies.iter_mut().zip(sha1sums.into_iter()) {
            item.sha1sum = Some(sha1sum)
        }
        Ok(())
    }

    pub(crate) fn try_read_file<P: AsRef<Path>>(file: P) -> Result<Self> {
        file.as_ref().try_into()
    }

    pub(crate) fn print_table_stdout(&self) -> () {
        println!("Items in image:");
        let mut rows = Vec::new();
        for (id, item) in self.items.iter().enumerate() {
            rows.push([
                cell_right!(id),
                cell_right!(&item.stem),
                cell_right!(&item.extension),
                cell_right!(format!("0x{:x}", item.data.len())),
                if let Some(sha1sum) = &item.sha1sum {
                    cell_right!(format!("{}", sha1sum))
                } else {
                    cell_right!("None")
                }
            ])
        }
        let table = rows.table().title([
            cell_bold_center!("ID"),
            cell_bold_center!("stem"),
            cell_bold_center!("extension"),
            cell_bold_center!("size"),
            cell_bold_center!("sha1sum")
        ]).bold(true);
        cli_table::print_stdout(table).unwrap();
    }

    pub(crate) fn try_write_dir<P: AsRef<Path>>(&self, dir: P) -> Result<()> {
        let parent = dir.as_ref();
        if parent.exists() {
            if parent.is_dir() {
                remove_dir_all(parent)?
            } else {
                remove_file(parent)?
            }
        }
        create_dir_all(parent)?;
        let progress_bar = ProgressBar::new(self.items.len() as u64);
        progress_bar.set_style(ProgressStyle::with_template(
            "Writing items => [{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}").unwrap());
        progress_bar.enable_steady_tick(Duration::from_secs(1));
        for item in self.items.iter() {
            let name = format!("{}.{}", item.stem, item.extension);
            let mut file = File::create(parent.join(&name))?;
            progress_bar.set_message(name);
            file.write_all(&item.data)?;
            progress_bar.inc(1);
        }
        Ok(())
    }

    pub(crate) fn try_write_file<P: AsRef<Path>>(&self, file: P) -> Result<()> {
        let image_to_write = ImageToWrite::try_from(self)?;
        let mut out_file = File::create(file.as_ref())?;
        let progress_bar = ProgressBar::new(
            (image_to_write.data_head_infos.len() + 
                    image_to_write.data_body.len()) as u64);
        progress_bar.set_style(ProgressStyle::with_template(
            "Writing image => \
                [{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7}"
            ).unwrap());
        // progress_bar.enable_steady_tick(Duration::from_secs(1));
        for chunk in 
            image_to_write.data_head_infos.chunks(0x100000).chain(
                image_to_write.data_body.chunks(0x100000)) 
        {
            out_file.write_all(chunk)?;
            progress_bar.inc(chunk.len() as u64)
        }
        progress_bar.finish_and_clear();
        Ok(())
    }
}



struct ImageToWrite {
    head: RawImageHead,
    infos: Vec<RawItemInfo>,
    sha1sums: Vec<Sha1sum>,
    data_head_infos: Vec<u8>,
    data_body: Vec<u8>,
}

impl ImageToWrite {
    fn find_backup(&self, sha1sum: &Sha1sum) -> (u16, u16, u64) {
        for (id, (item_sha1sum, item_info)) in 
            self.sha1sums.iter().zip(self.infos.iter()).enumerate() 
        {
            if sha1sum == item_sha1sum {
                return (1, id as u16, item_info.offset_in_image)
            }
        }
        (0, 0, 0)
    }

    fn append_item(&mut self, item: &Item) -> Result<()>{
        // println!("Appending item to ")
        let sha1sum = if let Some(sha1sum) = &item.sha1sum {
            sha1sum
        } else {
            eprintln!("Sha1sum for item {}.{} does not exist", 
                item.stem, item.extension);
            return Err(ImageError::IllegalVerify.into());
        };
        let (is_backup_item, backup_item_id, offset) 
            = self.find_backup(sha1sum);
        let mut offset = offset as usize;
        let align_size = self.head.item_align_size as usize;
        
        // let remaining = self.data_body.len() % self.head.item_align_size as usize
        if is_backup_item == 0 { // Not a backup item
            offset = self.data_body.len();
            self.data_body.extend_from_slice(&item.data);
            let remaining =  align_size - self.data_body.len() % align_size;
            for _ in remaining..4 {
                self.data_body.push(0)
            }
        }
        let end = (offset as usize + item.data.len() + 3) / align_size * align_size;
        let info = RawItemInfo {
            item_id: self.infos.len() as u32,
            file_type: 
                if item.data.starts_with(
                    &ANDROID_SPARSE_IMAGE_MAGIC_BYTES
                ) {
                    FILE_TYPE_SPARSE
                } else {
                    FILE_TYPE_GENERIC
                },
            current_offset_in_item: 0,
            offset_in_image: offset as u64,
            item_size: item.data.len() as u64,
            item_main_type: item.extension.clone(),
            item_sub_type: item.stem.clone(),
            verify: if item.extension == "PARTITION" {1} else {0},
            is_backup_item,
            backup_item_id,
        };
        self.infos.push(info);
        self.sha1sums.push(sha1sum.clone());
        self.head.item_count += 1;
        if item.extension == "PARTITION" {
            offset = end;
            if is_backup_item == 0 {
                let content = format!("sha1sum {}", sha1sum);
                let bytes = content.as_bytes();
                if bytes.len() != 48 {
                    eprintln!("sha1sum content length != 40");
                    return Err(ImageError::SizeMismatch { 
                        exptected: 48, actual: bytes.len() }.into());
                }
                self.data_body.extend_from_slice(bytes);
                self.sha1sums.push(Sha1sum::from_data(bytes));
            } else {
                self.sha1sums.push(self.sha1sums[backup_item_id as usize].clone());
            }
            self.infos.push(RawItemInfo { 
                item_id: self.infos.len() as u32, 
                file_type: 0, 
                current_offset_in_item: 0,
                offset_in_image: offset as u64,
                item_size: 48,
                item_main_type: "VERIFY".into(),
                item_sub_type: item.stem.clone(),
                verify: 0,
                is_backup_item, 
                backup_item_id: backup_item_id + 1
            });
            self.head.item_count += 1;
        }
        Ok(())
    }

    fn finalize(&mut self, version: &ImageVersion) -> Result<()> {
        let size_info = version.size_raw_info();
        let offset = (
            SIZE_RAW_IMAGE_HEAD + size_info * self.head.item_count as usize
        ) as u64;
        self.head.image_size = self.data_body.len() as u64 + offset;
        self.head.version = version.into();
        let pointer_head = &self.head as *const RawImageHead as *const u8;
        let len_head = SIZE_RAW_IMAGE_HEAD;
        use std::slice::from_raw_parts;
        let raw_head = unsafe {from_raw_parts(pointer_head, len_head)};
        self.data_head_infos.clear();
        self.data_head_infos.extend_from_slice(raw_head);

        for info in self.infos.iter_mut() {
            info.offset_in_image += offset;
        }
        match version {
            ImageVersion::V1 => 
                for info in self.infos.iter() {
                    let raw_item_info: RawItemInfoV1 = info.into();
                    let pointer_info = 
                        &raw_item_info as *const RawItemInfoV1 as *const u8;
                    let raw_info = unsafe {
                        from_raw_parts(
                            pointer_info, SIZE_RAW_ITEM_INFO_V1)};
                    self.data_head_infos.extend_from_slice(raw_info)
                },
            ImageVersion::V2 => 
                for info in self.infos.iter() {
                    let raw_item_info: RawItemInfoV2 = info.into();
                    let pointer_info = 
                        &raw_item_info as *const RawItemInfoV2 as *const u8;
                    let raw_info = unsafe {
                        from_raw_parts(
                            pointer_info, SIZE_RAW_ITEM_INFO_V2)};
                    self.data_head_infos.extend_from_slice(raw_info)
                }
        }
        let offset_actual = self.data_head_infos.len();
        if offset != offset_actual as u64 {
            eprintln!("Actual head + infos size ({}) != expected ({})",
                offset_actual, offset);
            return Err(ImageError::SizeMismatch { 
                exptected: offset as usize, actual: offset_actual as usize 
            }.into());
        }
        Ok(())
    }
}

impl TryFrom<&Image> for ImageToWrite {
    type Error = Error;

    fn try_from(image: &Image) -> Result<Self> {
        let mut image_to_write = Self {
            head: RawImageHead::from(&image.version),
            infos: Vec::new(),
            sha1sums: Vec::new(),
            data_head_infos: Vec::new(),
            data_body: Vec::new(),
        };
        let mut ddr_usb = None;
        let mut uboot_usb = None;
        let mut generic_items = Vec::new();
        for item in image.items.iter() {
            if item.extension == "USB" {
                match item.stem.as_str() {
                    "DDR" => {
                        if ddr_usb.is_some() {
                            eprintln!("Duplicated DDR.USB, refuse to write");
                            return Err(ImageError::DuplicatedItem { 
                                stem: "DDR".into(), 
                                extension: "USB".into() }.into())
                        }
                        ddr_usb = Some(item);
                    },
                    "UBOOT" => {
                        if uboot_usb.is_some() {
                            eprintln!("Duplicated UBOOT.USB, refuse to write");
                            return Err(ImageError::DuplicatedItem { 
                                stem: "UBOOT".into(), 
                                extension: "USB".into() }.into())
                        }
                        uboot_usb = Some(item);
                    },
                    _ => {
                        println!("Warning: unexpected {}.USB", item.stem);
                        generic_items.push(item)
                    }
                }
            } else {
                generic_items.push(item)
            }
        }
        let ddr_usb = match ddr_usb {
            Some(ddr_usb) => ddr_usb,
            None => {
                eprintln!("DDR.USB does not exist, refuse to write");
                return Err(ImageError::MissingItem { 
                    stem: "DDR".into(), extension: "USB".into() }.into())
            },
        };
        let uboot_usb = match uboot_usb {
            Some(uboot_usb) => uboot_usb,
            None => {
                eprintln!("UBOOT.USB does not exist, refuse to write");
                return Err(ImageError::MissingItem { 
                    stem: "UBOOT".into(), extension: "USB".into() }.into())
            },
        };
        generic_items.sort_by(|some, other| {
            let order_stem = some.stem.cmp(&other.stem);
            if order_stem == std::cmp::Ordering::Equal {
                some.extension.cmp(&other.extension)
            } else {
                order_stem
            }
        });
        println!("Combining image...");
        image_to_write.append_item(ddr_usb)?;
        image_to_write.append_item(uboot_usb)?;
        for item in generic_items.iter_mut() {
            image_to_write.append_item(item)?
        }
        image_to_write.finalize(&image.version)?;
        println!("Cauculating CRC32 of image...");
        let mut crc32_hasher = crate::crc32::Crc32Hasher::new();
        crc32_hasher.update(&image_to_write.data_head_infos[4..]);
        crc32_hasher.update(&image_to_write.data_body);
        image_to_write.head.crc = crc32_hasher.value;
        let pointer = 
            image_to_write.data_head_infos.as_ptr() as *mut u32;
        unsafe {*pointer = crc32_hasher.value};
        println!("CRC32 of image is {:x}", crc32_hasher.value);
        Ok(image_to_write)
    }
}