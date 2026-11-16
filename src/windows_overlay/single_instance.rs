//! Keep one overlay process running and ask it to enter editor mode.

use std::{thread, time::Duration};

use windows::{
	Win32::{
		Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, LPARAM, WPARAM},
		System::Threading::CreateMutexW,
		UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_APP},
	},
	core::PCWSTR,
};

use super::WINDOW_CLASS_NAME;

pub(super) const SHOW_EDITOR_MESSAGE: u32 = WM_APP + 2;
const MUTEX_NAME: windows::core::PCWSTR = windows::core::w!("HoverOver-single-instance");
const WINDOW_LOOKUP_ATTEMPTS: usize = 10;
const WINDOW_LOOKUP_DELAY: Duration = Duration::from_millis(100);

pub(super) enum InstanceStatus {
	Primary(SingleInstanceGuard),
	AlreadyRunning,
}

pub(super) struct SingleInstanceGuard {
	mutex_handle: windows::Win32::Foundation::HANDLE,
}

pub(super) fn acquire() -> windows::core::Result<InstanceStatus> {
	let mutex_handle = unsafe { CreateMutexW(None, false, MUTEX_NAME)? };
	if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
		unsafe {
			let _ = CloseHandle(mutex_handle);
		}
		return Ok(InstanceStatus::AlreadyRunning);
	}

	Ok(InstanceStatus::Primary(SingleInstanceGuard {
		mutex_handle,
	}))
}

pub(super) fn notify_existing_instance() {
	for _ in 0..WINDOW_LOOKUP_ATTEMPTS {
		if let Ok(window) = unsafe { FindWindowW(WINDOW_CLASS_NAME, PCWSTR::null()) } {
			if !window.0.is_null()
				&& unsafe {
					PostMessageW(window, SHOW_EDITOR_MESSAGE, WPARAM(0), LPARAM(0))
				}
				.is_ok()
			{
				return;
			}
		}
		thread::sleep(WINDOW_LOOKUP_DELAY);
	}
}

impl Drop for SingleInstanceGuard {
	fn drop(&mut self) {
		unsafe {
			let _ = CloseHandle(self.mutex_handle);
		}
	}
}
