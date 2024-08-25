/*
ampack, to unpack and pack Aml burning images: image handling module
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

use std::{cmp::{min, Ordering}, ffi::{c_char, CStr}, fmt::Display, fs::{create_dir_all, read_dir, remove_dir_all, remove_file, File}, io::{Read, Seek, Write}, path::Path, time::Duration};

use cli_table::{Cell, Style, Table, format::Justify};
use indicatif::MultiProgress;
use serde::{Serialize, Deserialize};

use crate::{progress::{progress_bar_with_template, progress_bar_with_template_multi}, sha1sum::Sha1sum, Error, Result};

/* These values are always the same for any images */

const MAGIC: u32 = 0x27b51956;
const FILE_TYPE_GENERIC: u32 = 0;
const FILE_TYPE_SPARSE: u32 = 254;
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
    UnexpectedItem {
        stem: String,
        extension: String,
    },
    SizeMismatch {
        exptected: usize,
        actual: usize
    },
}

impl Into<Error> for ImageError {
    fn into(self) -> Error {
        Error::ImageError(self)
    }
}

impl Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Image Error: ")?;
        match self {
            ImageError::InvalidMagic { magic } => 
                write!(f, "Invalid Magic: 0x{:08x}", magic),
            ImageError::IllegalVerify => 
                write!(f, "Illegal Verify"),
            ImageError::InvalidVersion { version } => 
                write!(f, "Invalid Version: {}", version),
            ImageError::UnmatchedVerify => 
                write!(f, "Unmatched Verify"),
            ImageError::DuplicatedItem { stem, extension } => 
                write!(f, "Duplicated Item '{}.{}'", stem, extension),
            ImageError::MissingItem { stem, extension } => 
                write!(f, "Missing Item '{}.{}'", stem, extension),
            ImageError::UnexpectedItem { stem, extension } =>
                write!(f, "Unexpected Item '{}.{}'", stem, extension),
            ImageError::SizeMismatch { exptected, actual } => 
                write!(f, "Size Mismatch (expected {} != actual {})",
                    exptected, actual),
        }
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
            }
        )
    }
}

impl ImageVersion {
    fn size_raw_info(&self) -> usize {
        match self {
            ImageVersion::V1 => SIZE_RAW_ITEM_INFO_V1,
            ImageVersion::V2 => SIZE_RAW_ITEM_INFO_V2,
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
    fn new(version: &ImageVersion, item_align_size: u32) -> Self {
        Self {
            crc: 0,
            version: version.into(),
            magic: MAGIC,
            image_size: 0,
            item_align_size,
            item_count: 0,
            _reserve: [0; 36],
        }
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
    _reserve: [u8; 24],
}

type RawItemInfoV1 = RawItemInfoVariableLength<SIZE_ITEM_TYPE_V1>;
type RawItemInfoV2 = RawItemInfoVariableLength<SIZE_ITEM_TYPE_V2>;
// type RawItemInfoV3 = RawItemInfo<256>;

const SIZE_RAW_ITEM_INFO_V1: usize = std::mem::size_of::<RawItemInfoV1>();
const SIZE_RAW_ITEM_INFO_V2: usize = std::mem::size_of::<RawItemInfoV2>();
// const SIZE_RAW_ITEM_INFO_V3: usize = std::mem::size_of::<RawItemInfoV3>();

fn cstr_from_slice_u8_c_string(slice: &[u8]) -> &CStr {
    unsafe {CStr::from_ptr(slice.as_ptr() as *const c_char)}
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
            _reserve: [0; 24]
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

fn sort_ref_items_by_name(some: &&Item, other: &&Item) -> Ordering {
    let order_stem = some.stem.cmp(&other.stem);
    if order_stem == std::cmp::Ordering::Equal {
        some.extension.cmp(&other.extension)
    } else {
        order_stem
    }
}

fn sort_items_by_name(some: &Item, other: &Item) -> Ordering {
    let order_stem = some.stem.cmp(&other.stem);
    if order_stem == std::cmp::Ordering::Equal {
        some.extension.cmp(&other.extension)
    } else {
        order_stem
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

    fn find_essentials(&self) -> Result<(&Item, &Item, &Item, &Item, &Item)> {
        Ok((
            self.find_item("DDR", "USB")?,
            self.find_item("UBOOT", "USB")?,
            self.find_item("aml_sdc_burn", "ini")?,
            self.find_item("meson1", "dtb")?,
            self.find_item("platform", "conf")?,
        ))
    }

    pub(crate) fn verify(&self) -> Result<()> {
        let _ = self.find_essentials();
        let need_verifies: Vec<&Item> = self.items.iter().filter(
            |item|item.sha1sum.is_some()).collect();
        let multi_progress = MultiProgress::new();
        let template_prefix = 
            "Verifying item => [{elapsed_precise}] {bar:40.cyan/blue} \
            {pos:>5}/{len:>5} MiB ".to_string();
        let mut mapped = Vec::new();
        for item in need_verifies.iter() {
            let name = format!("{}.{}", item.stem, item.extension);
            let mut template = template_prefix.clone();
            template.push_str(&name);
            let progress_bar = progress_bar_with_template_multi(
                &multi_progress, 
                item.data.len() as u64 / 0x100000, 
                &template)?;
            mapped.push((*item, name, progress_bar))
        }
        use rayon::prelude::*;
        let result = mapped.par_iter_mut().map(|(item, name, progress_bar)| {
            let sha1sum_record = match &item.sha1sum {
                Some(sha1sum) => sha1sum,
                None => {
                    eprintln!("Verify item not found for {}.{}", 
                        &item.stem, &item.extension);
                    return Err(ImageError::MissingItem { 
                        stem: item.stem.clone(), extension: "VERIFY".into()
                    }.into());
                },
            };
            let sha1sum_calculated = Sha1sum::from_data_with_bar(&item.data, progress_bar);
            if sha1sum_record != &sha1sum_calculated {
                eprintln!("Recorded SHA1sum ({}) different from calculated \
                    SHA1sum ({}) for item '{}'", sha1sum_record, 
                    sha1sum_calculated, name);
                return Err(ImageError::IllegalVerify.into());
            }
            Ok(())
        }).find_first(|r|r.is_err());
        multi_progress.clear()?;
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
        let multi_progress = MultiProgress::new();
        let mut mapped = Vec::new();
        let template_prefix = 
            "Generating verify => [{elapsed_precise}] {bar:40.cyan/blue} \
            {pos:>5}/{len:>5} MiB ".to_string();
        for item in need_verifies.iter_mut() {
            let name = format!("{}.{}", item.stem, item.extension);
            let mut template = template_prefix.clone();
            template.push_str(&name);
            let progress_bar = progress_bar_with_template_multi(
                &multi_progress, 
                item.data.len() as u64 / 0x100000,
                &template)?;
            mapped.push((item, progress_bar))
        }
        use rayon::prelude::*;
        let sha1sums: Vec<Sha1sum> = mapped.par_iter_mut().map(|(item, progress_bar)| {
            Sha1sum::from_data_with_bar(&item.data, progress_bar)
        }).collect();
        multi_progress.clear()?;
        for (item, sha1sum) in need_verifies.iter_mut().zip(sha1sums.into_iter()) {
            item.sha1sum = Some(sha1sum)
        }
        Ok(())
    }

    pub(crate) fn try_read_file<P: AsRef<Path>>(file: P) -> Result<Self> {
        let path_file = file.as_ref();
        let mut file = File::open(path_file)?;
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
        let progress_bar = progress_bar_with_template(
            header.item_count.into(), 
            "Reading image => [{elapsed_precise}] {bar:40.cyan/blue} \
                                        {pos:>7}/{len:7} {msg}")?;
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
        cli_table::print_stdout(table)?;
        Ok(Self {
            version,
            align: header.item_align_size,
            items,
        })
        // file.as_ref().try_into()
    }

    pub(crate) fn try_read_dir<P: AsRef<Path>>(dir: P) -> Result<Self> {
        let path_dir = dir.as_ref();
        let mut entries = Vec::new();
        for entry in read_dir(path_dir)? {
            let entry = entry?;
            entries.push(entry)
        }
        let progress_bar = progress_bar_with_template(
            entries.len() as u64, 
            "Reading items => [{elapsed_precise}] {bar:40.cyan/blue} \
                                        {pos:>3}/{len:3} {msg}")?;
        progress_bar.enable_steady_tick(Duration::from_secs(1));
        let mut uboot_usb = None;
        let mut ddr_usb = None;
        let mut aml_sdc_burn_ini = None;
        let mut meson1_dtb = None;
        let mut platform_conf = None;
        let mut generic_items = Vec::new();
        for entry in entries {
            progress_bar.set_message(entry.file_name().to_string_lossy().into_owned());
            let path_entry = entry.path();
            let file_name = match path_entry.file_name() {
                Some(file_name) => file_name.to_string_lossy(),
                None => {
                    return Err(Error::IOError(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "Cannot figure out the file name of part")));
                },
            };
            let (stem, extension) = match 
                file_name.split_once('.') 
            {
                Some((stem, extension)) => (stem, extension),
                None => continue,
            };
            let mut data = Vec::new();
            let mut file = File::open(&path_entry)?;
            file.read_to_end(&mut data)?;
            let item = Item {
                data,
                extension: extension.into(),
                stem: stem.into(),
                sha1sum: None,
            };
            match (item.stem.as_ref(), item.extension.as_ref()) {
                ("DDR", "USB") => ddr_usb = Some(item),
                ("UBOOT", "USB") => uboot_usb = Some(item),
                ("aml_sdc_burn", "ini") => aml_sdc_burn_ini = Some(item),
                ("meson1", "dtb") => meson1_dtb = Some(item),
                ("platform", "conf") => platform_conf = Some(item),
                _ => generic_items.push(item)
            }
            progress_bar.inc(1);
        }
        progress_bar.finish_and_clear();
        let mut items = Vec::new();
        for (item, stem) in [(ddr_usb, "DDR"), (uboot_usb, "UBOOT")] {
            match item {
                Some(item) => items.push(item),
                None => {
                    eprintln!("Essential {}.USB file does not exist", stem);
                    return Err(ImageError::MissingItem { 
                        stem: stem.into(), extension: "USB".into() }.into());
                },
            }
        }
        for (item, stem, extension) in [
            (aml_sdc_burn_ini, "aml_sdc_burn", "ini"),
            (meson1_dtb, "meson1", "dtb"),
            (platform_conf, "platform", "conf")] 
        {
            match item {
                Some(item) => generic_items.push(item),
                None => {
                    eprintln!("Essential {}.{} file does not exist", stem, extension);
                    return Err(ImageError::MissingItem { 
                        stem: stem.into(), extension: extension.into()}.into())
                }
            }
        }
        generic_items.sort_by(sort_items_by_name);
        items.append(&mut generic_items);
        Ok(Self {
            version: ImageVersion::V2,
            align: 4,
            items,
        })
    }

    pub(crate) fn print_table_stdout(&self) -> Result<()> {
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
        cli_table::print_stdout(table)?;
        Ok(())
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
        let progress_bar = progress_bar_with_template(
            self.items.len() as u64, 
            "Writing items => [{elapsed_precise}] {bar:40.cyan/blue} \
                                        {pos:>7}/{len:7} {msg}")?;
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
        let progress_bar = progress_bar_with_template(
            ((image_to_write.data_head_infos.len() + 
                    image_to_write.data_body.len()) / 0x100000) as u64,
            "Writing image => [{elapsed_precise}] {bar:40.cyan/blue} \
                                        {pos:>5}/{len:5} MiB")?;
        for chunk in 
            image_to_write.data_head_infos.chunks(0x100000).chain(
                image_to_write.data_body.chunks(0x100000)) 
        {
            out_file.write_all(chunk)?;
            progress_bar.inc(1)
        }
        progress_bar.finish_and_clear();
        Ok(())
    }

    fn guess_align_size(&self) -> u32 {
        if self.find_item("super", "PARTITION").is_err() {
            return 4
        }
        for item in self.items.iter() {
            if item.extension != "PARTITION" {
                continue
            }
            if item.stem.ends_with("_a") {
                println!("Both super partition and _a partition found, this \
                    image is probably for Android >= 11");
                return 8
            }
        }
        4   
    }

    pub(crate) fn set_ver_align(&mut self, ver: ImageVersion, align: u8) {
        self.version = ver;
        self.align = ((align + 3) >> 2 << 2) as u32;
        println!("Image version set to {}, alignment set to {}", 
            self.version, self.align);
        let guessed_align = self.guess_align_size();
        if guessed_align != self.align {
            println!("Warning: alignment size guessed from image items is {}, \
                but it's set as {}", guessed_align, self.align)
        }
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
            if sha1sum == item_sha1sum && ! (item_info.item_main_type == "USB" && item_info.item_sub_type.ends_with("_ENC")) {
                return (1, id as u16, item_info.offset_in_image)
            }
        }
        (0, 0, 0)
    }

    fn append_item(&mut self, item: &Item) -> Result<()>{
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
        if is_backup_item == 0 { // Not a backup item
            offset = (self.data_body.len() + align_size - 1) / align_size * align_size;
            for _ in self.data_body.len() .. offset {
                self.data_body.push(0)
            }
            self.data_body.extend_from_slice(&item.data);
        }
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
        offset += item.data.len();
        if item.extension == "PARTITION" {
            let content = format!("sha1sum {}", sha1sum);
            let bytes = content.as_bytes();
            if bytes.len() != 48 {
                eprintln!("sha1sum content length != 40");
                return Err(ImageError::SizeMismatch { 
                    exptected: 48, actual: bytes.len() }.into());
            }
            self.data_body.extend_from_slice(bytes);
            self.sha1sums.push(Sha1sum::from_data(bytes));
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
                backup_item_id: if is_backup_item == 0 { 0 } else { backup_item_id + 1 }
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
            head: RawImageHead::new(&image.version, image.align),
            infos: Vec::new(),
            sha1sums: Vec::new(),
            data_head_infos: Vec::new(),
            data_body: Vec::new(),
        };
        let mut ddr_usb = None;
        let mut uboot_usb = None;
        let mut ddr_enc_usb = None;
        let mut uboot_enc_usb = None;
        let mut generic_items = Vec::new();
        for item in image.items.iter() {
            if item.extension == "USB" {
                let stem = item.stem.as_str();
                let item_usb =
                    match stem {
                        "DDR" => &mut ddr_usb,
                        "UBOOT" => &mut uboot_usb,
                        "DDR_ENC" => &mut ddr_enc_usb,
                        "UBOOT_ENC" => &mut uboot_enc_usb,
                        _ => {
                            eprintln!("Unexpected {}.USB, refuse to write", 
                                item.stem);
                            return Err(ImageError::UnexpectedItem { 
                                stem: stem.into(), extension: "USB".into() 
                            }.into())
                        }
                    };
                if item_usb.is_some() {
                    eprintln!("Duplicated {}.USB, refuse to write", stem);
                    return Err(ImageError::DuplicatedItem { 
                        stem: stem.into(), extension: "USB".into() }.into())
                } else {
                    *item_usb = Some(item)
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
        generic_items.sort_by(sort_ref_items_by_name);
        let progress_bar = progress_bar_with_template(
            image.items.len() as u64,
            "Combining image => [{elapsed_precise}] {bar:40.cyan/blue} \
                                            {pos:>3}/{len:3} {msg}")?;

        progress_bar.set_message("DDR.USB");
        image_to_write.append_item(ddr_usb)?;
        progress_bar.inc(1);

        if let Some(ddr_enc_usb) = ddr_enc_usb {
            progress_bar.set_message("DDR_ENC.USB");
            image_to_write.append_item(ddr_enc_usb)?;
            progress_bar.inc(1);
        }

        progress_bar.set_message("UBOOT.USB");
        image_to_write.append_item(uboot_usb)?;
        progress_bar.inc(1);

        if let Some(uboot_enc_usb) = uboot_enc_usb {
            progress_bar.set_message("UBOOT_ENC.USB");
            image_to_write.append_item(uboot_enc_usb)?;
            progress_bar.inc(1);
        }
        for item in generic_items.iter_mut() {
            progress_bar.set_message(format!("{}.{}", item.stem, item.extension));
            image_to_write.append_item(item)?;
            progress_bar.inc(1);
        }
        progress_bar.set_message("finalizing...");
        progress_bar.finish_and_clear();
        image_to_write.finalize(&image.version)?;
        let progress_bar = progress_bar_with_template(
            ((image_to_write.data_head_infos.len() + 
                    image_to_write.data_body.len() - 4) / 0x100000
                ) as u64,
            "Calculating CRC32 => [{elapsed_precise}] {bar:40.cyan/blue} \
                {pos:>5}/{len:5} MiB")?;
        let mut crc32_hasher = crate::crc32::Crc32Hasher::new();
        crc32_hasher.udpate_with_bar(&image_to_write.data_head_infos[4..], &progress_bar);
        crc32_hasher.udpate_with_bar(&image_to_write.data_body, &progress_bar);
        progress_bar.finish_and_clear();
        image_to_write.head.crc = crc32_hasher.value;
        let pointer = 
            image_to_write.data_head_infos.as_ptr() as *mut u32;
        unsafe {*pointer = crc32_hasher.value};
        println!("CRC32 of image is 0x{:08x}", crc32_hasher.value);
        Ok(image_to_write)
    }
}