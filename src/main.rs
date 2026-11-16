#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use windows::{
	Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
	core::{HSTRING, w},
};

mod app;
mod geometry;
mod image_file;
mod model;
mod renderer;
mod snapping;
mod windows_overlay;

fn main() {
	if let Err(error) = windows_overlay::run() {
		eprintln!("HoverOver could not start: {error}");
		let error_message = HSTRING::from(format!("HoverOver could not start: {error}"));
		unsafe {
			let _ = MessageBoxW(None, &error_message, w!("HoverOver"), MB_OK | MB_ICONERROR);
		}
		std::process::exit(1);
	}
}
