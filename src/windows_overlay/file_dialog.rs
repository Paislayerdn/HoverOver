//! Native Windows image picker.

use std::{mem::size_of, os::windows::ffi::OsStringExt, path::PathBuf};

use windows::{
	Win32::{
		Foundation::HWND,
		UI::Controls::Dialogs::{
			CommDlgExtendedError, GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR,
			OFN_PATHMUSTEXIST, OPENFILENAMEW,
		},
	},
	core::{PCWSTR, PWSTR},
};

pub unsafe fn choose_image(owner: HWND) -> Result<Option<PathBuf>, String> {
	let mut file_name = vec![0u16; 32_768];
	let filter: Vec<u16> = "Image files\0*.png;*.jpg;*.jpeg;*.gif;*.webp\0PNG images\0*.png\0JPEG images\0*.jpg;*.jpeg\0GIF images\0*.gif\0WebP images\0*.webp\0\0"
        .encode_utf16()
        .collect();

	let mut dialog = OPENFILENAMEW {
		lStructSize: size_of::<OPENFILENAMEW>() as u32,
		hwndOwner: owner,
		lpstrFilter: PCWSTR(filter.as_ptr()),
		lpstrFile: PWSTR(file_name.as_mut_ptr()),
		nMaxFile: file_name.len() as u32,
		Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
		..Default::default()
	};

	if unsafe { GetOpenFileNameW(&mut dialog) }.as_bool() {
		let path_end = file_name
			.iter()
			.position(|character| *character == 0)
			.unwrap_or(file_name.len());
		return Ok(Some(PathBuf::from(std::ffi::OsString::from_wide(
			&file_name[..path_end],
		))));
	}

	let dialog_error = unsafe { CommDlgExtendedError() }.0;
	if dialog_error == 0 {
		Ok(None)
	} else {
		Err(format!("Windows file picker error 0x{dialog_error:08X}"))
	}
}
