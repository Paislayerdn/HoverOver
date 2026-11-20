//! Show the native image context menu and return the chosen command.

use crate::app::{ContextMenuState, MenuAction};
use windows::{
	Win32::{
		Foundation::{HWND, POINT},
		Graphics::Gdi::ClientToScreen,
		UI::WindowsAndMessaging::{
			AppendMenuW, CreatePopupMenu, DestroyMenu, HMENU, MF_CHECKED, MF_GRAYED, MF_SEPARATOR,
			MF_STRING, MF_UNCHECKED, SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON,
			TrackPopupMenu,
		},
	},
	core::{PCWSTR, Result, w},
};

const COMMAND_DELETE: usize = 1;
const COMMAND_FLIP_HORIZONTAL: usize = 2;
const COMMAND_FLIP_VERTICAL: usize = 3;
const COMMAND_SET_PROPERTIES: usize = 4;
const COMMAND_BRING_TO_FRONT: usize = 5;
const COMMAND_BRING_FORWARD: usize = 6;
const COMMAND_SEND_BACKWARD: usize = 7;
const COMMAND_SEND_TO_BACK: usize = 8;

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
	state: ContextMenuState,
) -> Result<MenuAction> {
	let menu = PopupMenu(unsafe { CreatePopupMenu()? });
	unsafe {
		AppendMenuW(menu.0, MF_STRING, COMMAND_DELETE, w!("Delete"))?;
		AppendMenuW(menu.0, MF_SEPARATOR, 0, PCWSTR::null())?;
		let horizontal_flags = MF_STRING
			| if state.flip_x {
				MF_CHECKED
			} else {
				MF_UNCHECKED
			};
		let vertical_flags = MF_STRING
			| if state.flip_y {
				MF_CHECKED
			} else {
				MF_UNCHECKED
			};
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
		append_arrange_item(
			menu.0,
			COMMAND_BRING_FORWARD,
			w!("Bring Forward"),
			state.arrange.bring_forward,
		)?;
		append_arrange_item(
			menu.0,
			COMMAND_SEND_BACKWARD,
			w!("Send Backward"),
			state.arrange.send_backward,
		)?;
		append_arrange_item(
			menu.0,
			COMMAND_BRING_TO_FRONT,
			w!("Bring to Front"),
			state.arrange.bring_to_front,
		)?;
		append_arrange_item(
			menu.0,
			COMMAND_SEND_TO_BACK,
			w!("Send to Back"),
			state.arrange.send_to_back,
		)?;
		AppendMenuW(menu.0, MF_SEPARATOR, 0, PCWSTR::null())?;
		AppendMenuW(
			menu.0,
			MF_STRING,
			COMMAND_SET_PROPERTIES,
			w!("Edit properties..."),
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

unsafe fn append_arrange_item(
	menu: HMENU,
	command: usize,
	text: windows::core::PCWSTR,
	enabled: bool,
) -> windows::core::Result<()> {
	unsafe {
		AppendMenuW(
			menu,
			MF_STRING
				| if enabled {
					Default::default()
				} else {
					MF_GRAYED
				},
			command,
			text,
		)
	}
}

fn menu_action(command: usize) -> MenuAction {
	match command {
		COMMAND_DELETE => MenuAction::Delete,
		COMMAND_FLIP_HORIZONTAL => MenuAction::FlipHorizontal,
		COMMAND_FLIP_VERTICAL => MenuAction::FlipVertical,
		COMMAND_SET_PROPERTIES => MenuAction::SetProperties,
		COMMAND_BRING_TO_FRONT => MenuAction::BringToFront,
		COMMAND_BRING_FORWARD => MenuAction::BringForward,
		COMMAND_SEND_BACKWARD => MenuAction::SendBackward,
		COMMAND_SEND_TO_BACK => MenuAction::SendToBack,
		_ => MenuAction::Cancel,
	}
}
