//! Global editor shortcut and physical left/right modifier-key state.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use windows::{
	Win32::{
		Foundation::{HWND, LPARAM, LRESULT, WPARAM},
		UI::{
			Input::KeyboardAndMouse::GetAsyncKeyState,
			WindowsAndMessaging::{
				CallNextHookEx, HHOOK, KBDLLHOOKSTRUCT, PostMessageW, SetWindowsHookExW,
				UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_APP, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN,
				WM_SYSKEYUP,
			},
		},
	},
	core::Result,
};

pub const TOGGLE_EDITOR_MESSAGE: u32 = WM_APP + 1;

const VK_F1: u32 = 0x70;
const VK_CONTROL: i32 = 0x11;
const VK_LCONTROL: i32 = 0xA2;
const VK_RCONTROL: i32 = 0xA3;
const VK_SHIFT: i32 = 0x10;
const VK_LSHIFT: i32 = 0xA0;
const VK_RSHIFT: i32 = 0xA1;

static OVERLAY_WINDOW: AtomicIsize = AtomicIsize::new(0);
static F1_IS_DOWN: AtomicBool = AtomicBool::new(false);

pub struct GlobalShortcut {
	hook: HHOOK,
}

impl GlobalShortcut {
	pub unsafe fn install(
		window: HWND,
		instance: windows::Win32::Foundation::HMODULE,
	) -> Result<Self> {
		OVERLAY_WINDOW.store(window.0 as isize, Ordering::SeqCst);
		F1_IS_DOWN.store(false, Ordering::SeqCst);

		match unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), instance, 0) } {
			Ok(hook) => Ok(Self { hook }),
			Err(error) => {
				OVERLAY_WINDOW.store(0, Ordering::SeqCst);
				Err(error)
			}
		}
	}
}

pub fn clear_active_window() {
	OVERLAY_WINDOW.store(0, Ordering::SeqCst);
}

impl Drop for GlobalShortcut {
	fn drop(&mut self) {
		OVERLAY_WINDOW.store(0, Ordering::SeqCst);
		F1_IS_DOWN.store(false, Ordering::SeqCst);
		unsafe {
			let _ = UnhookWindowsHookEx(self.hook);
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

unsafe extern "system" fn keyboard_hook(
	hook_code: i32,
	message: WPARAM,
	key_data: LPARAM,
) -> LRESULT {
	if hook_code >= 0 {
		let key = unsafe { &*(key_data.0 as *const KBDLLHOOKSTRUCT) };
		let message_id = message.0 as u32;
		let is_key_down = message_id == WM_KEYDOWN || message_id == WM_SYSKEYDOWN;
		let is_key_up = message_id == WM_KEYUP || message_id == WM_SYSKEYUP;

		if key.vkCode == VK_F1 {
			if is_key_down && !F1_IS_DOWN.swap(true, Ordering::SeqCst) {
				let control_is_down = unsafe {
					GetAsyncKeyState(VK_LCONTROL) < 0
						|| GetAsyncKeyState(VK_RCONTROL) < 0
						|| GetAsyncKeyState(VK_CONTROL) < 0
				};
				let shift_is_down = shift_is_down();
				if control_is_down && shift_is_down {
					let hwnd_value = OVERLAY_WINDOW.load(Ordering::SeqCst);
					if hwnd_value != 0 {
						let window = HWND(hwnd_value as *mut _);
						let _ = unsafe {
							PostMessageW(window, TOGGLE_EDITOR_MESSAGE, WPARAM(0), LPARAM(0))
						};
					}
				}
			} else if is_key_up {
				F1_IS_DOWN.store(false, Ordering::SeqCst);
			}
		}
	}

	unsafe { CallNextHookEx(None, hook_code, message, key_data) }
}
