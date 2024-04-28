/*
ampack, to unpack and pack Aml burning images: progress module
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

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use crate::Result;

fn progress_style_with_templace<S: AsRef<str>>(template: S) 
    -> Result<ProgressStyle> 
{
    let template = template.as_ref();
    match ProgressStyle::with_template(template) {
        Ok(style) => Ok(style),
        Err(e) => {
            eprintln!(
                "Failed to create progress bar style from template '{}': {}",
                template, e
            );
            Err(e.into())
        }
    }
}

pub(crate) fn progress_bar_with_template<S>(length: u64, template: S) 
    -> Result<ProgressBar>
where
    S: AsRef<str>,
{
    let style = progress_style_with_templace(template)?;
    let bar = ProgressBar::new(length);
    bar.set_style(style);
    Ok(bar)
}

pub(crate) fn progress_bar_with_template_multi<S>(
    multi_progress: &MultiProgress, length: u64, template: S
) 
    -> Result<ProgressBar>
where
    S: AsRef<str>,
{
    Ok(multi_progress.add(progress_bar_with_template(length, template)?))
}