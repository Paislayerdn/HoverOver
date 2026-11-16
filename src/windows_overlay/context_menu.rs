//! Show the native image context menu and return the chosen command.

use crate::app::MenuAction;
use windows::{
	Win32::{
		Foundation::{HWND, POINT},
		Graphics::Gdi::ClientToScreen,
		UI::WindowsAndMessaging::{
			AppendMenuW, CreatePopupMenu, DestroyMenu, SetForegroundWindow,
			TrackPopupMenu, HMENU, MF_CHECKED, MF_SEPARATOR, MF_STRING, MF_UNCHECKED,
			TPM_RETURNCMD, TPM_RIGHTBUTTON,
		},
	},
	core::{PCWSTR, Result, w},
};

const COMMAND_DELETE: usize = 1;
const COMMAND_FLIP_HORIZONTAL: usize = 2;
const COMMAND_FLIP_VERTICAL: usize = 3;
const COMMAND_SET_OPACITY: usize = 4;
const COMMAND_SET_POSITION: usize = 5;

struct PopupMenu(HMENU);

impl Drop for PopupMenu {
	fn drop(&mut self) {
		unsafe {
			let _ = DestroyMenu(self.0);
		}
	}
}

pub(super) fn show(
	window: HWND,
	client_x: f32,
	client_y: f32,
	flip_x: bool,
	flip_y: bool,
) -> Result<MenuAction> {
	let menu = PopupMenu(unsafe { CreatePopupMenu()? });
	unsafe {
		AppendMenuW(menu.0, MF_STRING, COMMAND_DELETE, w!("Delete"))?;
		AppendMenuW(menu.0, MF_SEPARATOR, 0, PCWSTR::null())?;
		let horizontal_flags = MF_STRING | if flip_x { MF_CHECKED } else { MF_UNCHECKED };
		let vertical_flags = MF_STRING | if flip_y { MF_CHECKED } else { MF_UNCHECKED };
		AppendMenuW(
			menu.0,
			horizontal_flags,
			COMMAND_FLIP_HORIZONTAL,
			w!("Flip horizontal"),
		)?;
		AppendMenuW(
			menu.0,
			vertical_flags,
			COMMAND_FLIP_VERTICAL,
			w!("Flip vertical"),
		)?;
		AppendMenuW(menu.0, MF_SEPARATOR, 0, PCWSTR::null())?;
		AppendMenuW(
			menu.0,
			MF_STRING,
			COMMAND_SET_OPACITY,
			w!("Set opacity..."),
		)?;
		AppendMenuW(
			menu.0,
			MF_STRING,
			COMMAND_SET_POSITION,
			w!("Set position..."),
		)?;

		let mut cursor = POINT {
			x: client_x.round() as i32,
			y: client_y.round() as i32,
		};
		if ClientToScreen(window, &mut cursor).0 == 0 {
			return Err(windows::core::Error::from_win32());
		}
		let _ = SetForegroundWindow(window);
		let command = TrackPopupMenu(
			menu.0,
			TPM_RETURNCMD | TPM_RIGHTBUTTON,
			cursor.x,
			cursor.y,
			0,
			window,
			None,
		)
		.0 as usize;
		Ok(menu_action(command))
	}
}

fn menu_action(command: usize) -> MenuAction {
	match command {
		COMMAND_DELETE => MenuAction::Delete,
		COMMAND_FLIP_HORIZONTAL => MenuAction::FlipHorizontal,
		COMMAND_FLIP_VERTICAL => MenuAction::FlipVertical,
		COMMAND_SET_OPACITY => MenuAction::SetOpacity,
		COMMAND_SET_POSITION => MenuAction::SetPosition,
		_ => MenuAction::Cancel,
	}
}
