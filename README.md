# AMpack, a tool to unpack / (re)pack AMLogic burning images

This aims to replace `aml_image_v2_packer` and addtionally properly support v3 image format.

As this work is based on heavy reverse-engineering without an open document to look up to, the result is not guaranteed. If you encounter any issue, please open an issue or a PR.

## Build
```
cargo build --release
```
The result binary would be `target/release/ampack`

## Usage
```
Usage: ampack [OPTIONS] <COMMAND>

Commands:
  verify   Read and verify and image without unpacking it
  unpack   Unpack an image to get partition files
  convert  Convert an image to another image
  pack     (Re)pack partition files into an image
  crc32    Calculate the CRC32 checksum of an image
  help     Print this message or the help of the given subcommand(s)

Options:
  -v, --imgver <IMGVER>  Force version of the image, disables auto detection for unpack, needed by 'convert' and 'pack' [possible values: v1, v2]
  -h, --help             Print help
  -V, --version          Print version
```

### Verify
```
ampack verify [in file]
```
Verifying an image file at `[in file]`, without unpacking it, this is useful to check a packed image or verify a downloaded image

## Unpack
```
ampack unpack [in file] [out dir]
```
Unpack an image file at `[in file]` into folder `[out dir]`, **the output folder would be removed if it exsits**, and then created.

Unlike `aml_image_v2_packer`, `ampack` would not create `image.cfg` file, see below for the info of `pack` mode.

### Convert
```
ampack convert [in file] [out file]
```
Convert an image file at `[in file]` into another image file at `[out file]`, mostly  useful to convert images between different versions, also useful to check the accuracy of `ampack`: the `[out file]` should be a byte-to-byte re-created clone of `[in file]` if they share the same version.

### Pack
```
ampack pack [in dir] [out file]
```
Pack files and partitions under folder `[in dir]` into an image file at `[out file]`.

Unlike `aml_image_v2_packer`, `ampack` does not expect an `image.cfg` file, rather, it automatically identifies file types under the folder, and check and sort them to guarantee a working image.

### Crc32
```
ampack crc32 [in file]
```
Calculate the crc32 checksum value of an image file at `[in file]`, mostly for debugging purpose when checking `ampack`'s accuracy.

## License
**AMpack**, a tool to unpack / (re)pack AMLogic burning images

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
