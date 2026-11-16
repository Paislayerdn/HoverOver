//! Register the global editor hotkey and read the current Shift-key state.

use std::io;

use windows::Win32::{
	Foundation::HWND,
	UI::{
		Input::KeyboardAndMouse::{
			GetAsyncKeyState, RegisterHotKey, UnregisterHotKey, MOD_CONTROL, MOD_NOREPEAT,
			MOD_SHIFT, VK_F1,
		},
	},
};

pub const TOGGLE_HOTKEY_ID: i32 = 1;

const VK_SHIFT: i32 = 0x10;
const VK_LSHIFT: i32 = 0xA0;
const VK_RSHIFT: i32 = 0xA1;

pub struct ToggleHotkey {
	window: HWND,
}

impl ToggleHotkey {
	pub unsafe fn register(window: HWND) -> io::Result<Self> {
		unsafe {
			RegisterHotKey(
				window,
				TOGGLE_HOTKEY_ID,
				MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT,
				VK_F1.0 as u32,
			)
		}
		.map_err(|_| {
			io::Error::new(
				io::ErrorKind::AddrInUse,
				"Ctrl+Shift+F1 is already used by another program.",
			)
		})?;

		Ok(Self { window })
	}
}

impl Drop for ToggleHotkey {
	fn drop(&mut self) {
		unsafe {
			let _ = UnregisterHotKey(self.window, TOGGLE_HOTKEY_ID);
		}
	}
}

pub fn shift_is_down() -> bool {
	unsafe {
		GetAsyncKeyState(VK_LSHIFT) < 0
			|| GetAsyncKeyState(VK_RSHIFT) < 0
			|| GetAsyncKeyState(VK_SHIFT) < 0
	}
}
