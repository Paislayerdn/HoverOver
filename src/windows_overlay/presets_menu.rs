//! Native popup for saving or loading a preset.

use windows::{
	Win32::{
		Foundation::HWND,
		UI::WindowsAndMessaging::{
			AppendMenuW, CreatePopupMenu, DestroyMenu, HMENU, MF_STRING, SetForegroundWindow,
			TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu,
		},
	},
	core::{Result, w},
};

const COMMAND_SAVE: usize = 1;
const COMMAND_LOAD: usize = 2;
const COMMAND_CLEAR_LOCAL_DATA: usize = 3;

#[derive(Clone, Copy)]
pub(super) enum PresetsMenuAction {
	Cancel,
	Save,
	Load,
	ClearLocalData,
}

struct MenuHandle(HMENU);
impl Drop for MenuHandle {
	fn drop(&mut self) {
		unsafe {
			let _ = DestroyMenu(self.0);
		}
	}
}

pub(super) fn show(window: HWND, button_left: i32, button_top: i32) -> Result<PresetsMenuAction> {
	let menu = MenuHandle(unsafe { CreatePopupMenu()? });
	unsafe {
		AppendMenuW(menu.0, MF_STRING, COMMAND_LOAD, w!("Load..."))?;
		AppendMenuW(menu.0, MF_STRING, COMMAND_SAVE, w!("Save..."))?;
		AppendMenuW(
			menu.0,
			MF_STRING,
			COMMAND_CLEAR_LOCAL_DATA,
			w!("Clear All Local Data..."),
		)?;
		let _ = SetForegroundWindow(window);
		let command = TrackPopupMenu(
			menu.0,
			TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_LEFTALIGN,
			button_left,
			button_top,
			0,
			window,
			None,
		)
		.0 as usize;
		Ok(match command {
			COMMAND_SAVE => PresetsMenuAction::Save,
			COMMAND_LOAD => PresetsMenuAction::Load,
			COMMAND_CLEAR_LOCAL_DATA => PresetsMenuAction::ClearLocalData,
			_ => PresetsMenuAction::Cancel,
		})
	}
}
