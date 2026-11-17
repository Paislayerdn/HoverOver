//! Native Windows file pickers for images and self-contained presets.

use std::{
	mem::size_of,
	os::windows::ffi::{OsStrExt, OsStringExt},
	path::{Path, PathBuf},
};

use windows::{
	Win32::{
		Foundation::HWND,
		UI::Controls::Dialogs::{
			CommDlgExtendedError, GetOpenFileNameW, GetSaveFileNameW, OFN_FILEMUSTEXIST,
			OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
		},
	},
	core::{PCWSTR, PWSTR},
};

pub unsafe fn choose_image(owner: HWND) -> Result<Option<PathBuf>, String> {
	let filter = wide_filter(
		"Image files\0*.png;*.jpg;*.jpeg;*.gif;*.webp\0PNG images\0*.png\0JPEG images\0*.jpg;*.jpeg\0GIF images\0*.gif\0WebP images\0*.webp\0\0",
	);
	let mut file_name = vec![0u16; 32_768];
	let mut dialog = dialog_template(owner, &filter, &mut file_name);
	dialog.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
	show_open_dialog(&mut dialog, &file_name)
}

pub unsafe fn choose_preset_to_save(
	owner: HWND,
	suggested: Option<&Path>,
) -> Result<Option<PathBuf>, String> {
	let filter = wide_filter("HoverOver presets (*.json)\0*.json\0\0");
	let mut file_name = vec![0u16; 32_768];
	if let Some(path) = suggested {
		copy_path_to_buffer(path, &mut file_name);
	} else if let Ok(folder) = crate::presets::app_presets_folder() {
		// The first navigation directory is set below; an empty file name lets the dialog use it.
		let _ = folder;
	}
	let mut dialog = dialog_template(owner, &filter, &mut file_name);
	let extension = wide_filter("json\0");
	dialog.lpstrDefExt = PCWSTR(extension.as_ptr());
	dialog.Flags = OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
	if suggested.is_none() {
		if let Ok(folder) = crate::presets::app_presets_folder() {
			let folder_text: Vec<u16> = folder.as_os_str().encode_wide().chain(Some(0)).collect();
			dialog.lpstrInitialDir = PCWSTR(folder_text.as_ptr());
			return show_save_dialog(&mut dialog, &file_name, &folder_text);
		}
	}
	show_save_dialog(&mut dialog, &file_name, &[])
}

pub unsafe fn choose_preset_to_open(owner: HWND) -> Result<Option<PathBuf>, String> {
	let filter = wide_filter("HoverOver presets (*.json)\0*.json\0\0");
	let mut file_name = vec![0u16; 32_768];
	let extension = wide_filter("json\0");
	let mut dialog = dialog_template(owner, &filter, &mut file_name);
	dialog.lpstrDefExt = PCWSTR(extension.as_ptr());
	dialog.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
	show_open_dialog(&mut dialog, &file_name)
}

fn show_open_dialog(
	dialog: &mut OPENFILENAMEW,
	file_name: &[u16],
) -> Result<Option<PathBuf>, String> {
	if unsafe { GetOpenFileNameW(dialog) }.as_bool() {
		return Ok(path_from_buffer(file_name));
	}
	let error = unsafe { CommDlgExtendedError() }.0;
	if error == 0 {
		Ok(None)
	} else {
		Err(format!("Windows file picker error 0x{error:08X}"))
	}
}

fn show_save_dialog(
	dialog: &mut OPENFILENAMEW,
	file_name: &[u16],
	_initial_directory: &[u16],
) -> Result<Option<PathBuf>, String> {
	if unsafe { GetSaveFileNameW(dialog) }.as_bool() {
		return Ok(path_from_buffer(file_name));
	}
	let error = unsafe { CommDlgExtendedError() }.0;
	if error == 0 {
		Ok(None)
	} else {
		Err(format!("Windows file picker error 0x{error:08X}"))
	}
}

fn dialog_template(owner: HWND, filter: &[u16], file_name: &mut [u16]) -> OPENFILENAMEW {
	OPENFILENAMEW {
		lStructSize: size_of::<OPENFILENAMEW>() as u32,
		hwndOwner: owner,
		lpstrFilter: PCWSTR(filter.as_ptr()),
		lpstrFile: PWSTR(file_name.as_mut_ptr()),
		nMaxFile: file_name.len() as u32,
		..Default::default()
	}
}

fn wide_filter(text: &str) -> Vec<u16> {
	text.encode_utf16().collect()
}

fn path_from_buffer(buffer: &[u16]) -> Option<PathBuf> {
	let end = buffer
		.iter()
		.position(|character| *character == 0)
		.unwrap_or(buffer.len());
	(end > 0).then(|| PathBuf::from(std::ffi::OsString::from_wide(&buffer[..end])))
}

fn copy_path_to_buffer(path: &Path, buffer: &mut [u16]) {
	for (destination, source) in buffer.iter_mut().zip(path.as_os_str().encode_wide()) {
		*destination = source;
	}
}
