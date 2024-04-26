/*
ampack, to unpack and pack Aml burning images: pointer handling module
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

#[macro_export] macro_rules! impl_struct_try_from_ptr {
    ($stype: ident, $ptype: ident) => {
        impl TryFrom<*const $ptype> for $stype {
            type Error = Error;
            fn try_from(value: *const $ptype) -> Result<Self> {
                (&(unsafe {value.read()})).try_into()
            }
        }       
    };
}

#[macro_export] macro_rules! impl_struct_from_ptr {
    ($stype: ident, $ptype: ident) => {
        impl From<*const $ptype> for $stype {
            fn from(value: *const $ptype) -> Self {
                (&(unsafe {value.read()})).into()
            }
        }
    };
}