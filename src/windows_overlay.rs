//! Native window lifecycle and message routing. Keyboard, file dialog, and
//! bitmap details are kept in the sibling backstage modules.

use std::error::Error;

use windows::{
	Win32::{
		Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
		System::LibraryLoader::GetModuleHandleW,
		UI::WindowsAndMessaging::{
			CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow,
			DispatchMessageW, GWL_EXSTYLE, GWLP_USERDATA, GetMessageW, GetSystemMetrics,
			GetWindowLongPtrW, HWND_TOPMOST, MSG, PostQuitMessage, RegisterClassW,
			SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_SHOW,
			SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SetWindowLongPtrW,
			SetWindowPos, ShowWindow, TranslateMessage, WM_DESTROY, WM_LBUTTONDOWN, WM_LBUTTONUP,
			WM_MOUSEMOVE, WM_NCCREATE, WNDCLASSW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
			WS_EX_TRANSPARENT, WS_POPUP,
		},
	},
	core::{Error as WinError, PCWSTR},
};

use crate::{
	app::{Application, MouseAction},
	image_file,
	model::Mode,
	renderer,
};

mod keyboard;
use keyboard::GlobalShortcut;
mod surface;
use surface::DibSurface;
mod file_dialog;

/// Starts HoverOver and keeps its native window alive until Windows closes it.
pub fn run() -> Result<(), Box<dyn Error>> {
	unsafe {
		let screen = virtual_screen_bounds();
		let application = Application::new(screen.width as u32, screen.height as u32);
		let frame = application.render();
		let surface = DibSurface::create(&frame)?;
		let mut runtime = Box::new(Runtime {
			application,
			screen,
			surface,
			frame,
		});
		let instance = GetModuleHandleW(PCWSTR::null())?;
		register_window_class(instance.into())?;

		let initial_window_style = match runtime.application.mode {
			Mode::Display => WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW,
			Mode::Editing => WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
		};

		// The heap allocation keeps the application state at a stable address
		// while Windows calls the window procedure.
		let window = CreateWindowExW(
			initial_window_style,
			windows::core::w!("HoverOver"),
			windows::core::w!("HoverOver"),
			WS_POPUP,
			screen.left,
			screen.top,
			screen.width,
			screen.height,
			None,
			None,
			instance,
			Some((&mut *runtime as *mut Runtime).cast()),
		)?;

		let keyboard_hook = match GlobalShortcut::install(window, instance) {
			Ok(hook) => hook,
			Err(error) => {
				let _ = DestroyWindow(window);
				return Err(error.into());
			}
		};

		runtime.surface.present_full(
			window,
			screen.left,
			screen.top,
			screen.width,
			screen.height,
		)?;
		let _ = ShowWindow(window, SW_SHOW);
		let message_loop_result = run_message_loop();
		drop(keyboard_hook);
		message_loop_result?;
	}

	Ok(())
}

#[derive(Clone, Copy)]
struct ScreenBounds {
	left: i32,
	top: i32,
	width: i32,
	height: i32,
}

struct Runtime {
	application: Application,
	screen: ScreenBounds,
	surface: DibSurface,
	frame: image_file::ImageData,
}

unsafe fn virtual_screen_bounds() -> ScreenBounds {
	ScreenBounds {
		left: unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) },
		top: unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) },
		width: unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) },
		height: unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) },
	}
}

unsafe fn register_window_class(instance: HINSTANCE) -> windows::core::Result<()> {
	let class = WNDCLASSW {
		style: CS_HREDRAW | CS_VREDRAW,
		lpfnWndProc: Some(window_procedure),
		hInstance: instance,
		lpszClassName: windows::core::w!("HoverOver"),
		..Default::default()
	};

	if unsafe { RegisterClassW(&class) } == 0 {
		return Err(WinError::from_win32());
	}
	Ok(())
}

unsafe fn run_message_loop() -> windows::core::Result<()> {
	let mut message = MSG::default();
	loop {
		let result = unsafe { GetMessageW(&mut message, None, 0, 0) }.0;
		if result == -1 {
			return Err(WinError::from_win32());
		}
		if result == 0 {
			break;
		}
		unsafe {
			let _ = TranslateMessage(&message);
			DispatchMessageW(&message);
		}
	}
	Ok(())
}

unsafe extern "system" fn window_procedure(
	window: HWND,
	message: u32,
	wparam: WPARAM,
	lparam: LPARAM,
) -> LRESULT {
	if message == WM_NCCREATE {
		let create_info = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
		unsafe {
			SetWindowLongPtrW(window, GWLP_USERDATA, create_info.lpCreateParams as isize);
		}
	}

	let runtime_pointer = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *mut Runtime;
	if runtime_pointer.is_null() {
		return unsafe { DefWindowProcW(window, message, wparam, lparam) };
	}
	let runtime = unsafe { &mut *runtime_pointer };
	let application = &mut runtime.application;

	match message {
		keyboard::TOGGLE_EDITOR_MESSAGE => {
			application.toggle_mode();
			if let Err(error) = unsafe { set_input_mode(window, application.mode) } {
				eprintln!("Could not change HoverOver mode: {error}");
			}
			if let Err(error) = unsafe { redraw(window, runtime) } {
				eprintln!("Could not redraw HoverOver: {error}");
			}
			LRESULT(0)
		}
		WM_LBUTTONDOWN => {
			let (x, y) = mouse_position(lparam);
			match application.mouse_down(x, y) {
				MouseAction::AddImage => match unsafe { file_dialog::choose_image(window) } {
					Ok(Some(path)) => match image_file::load(&path) {
						Ok(image) => application.add_image(
							image,
							path.file_name()
								.map(|name| name.to_string_lossy().into_owned())
								.unwrap_or_default(),
						),
						Err(error) => eprintln!("Could not load selected image: {error}"),
					},
					Ok(None) => {}
					Err(error) => eprintln!("Could not open the image picker: {error}"),
				},
				MouseAction::Quit => {
					unsafe {
						let _ = DestroyWindow(window);
					}
					return LRESULT(0);
				}
				MouseAction::None => {}
			}
			redraw_or_log(window, runtime);
			LRESULT(0)
		}
		WM_MOUSEMOVE => {
			let (x, y) = mouse_position(lparam);
			let shift_is_down = keyboard::shift_is_down();
			if application.mouse_move(x, y, shift_is_down) {
				redraw_or_log(window, runtime);
			}
			LRESULT(0)
		}
		WM_LBUTTONUP => {
			let (x, y) = mouse_position(lparam);
			let shift_is_down = keyboard::shift_is_down();
			let moved = application.mouse_move(x, y, shift_is_down);
			application.mouse_up();
			if moved {
				redraw_or_log(window, runtime);
			}
			LRESULT(0)
		}
		WM_DESTROY => {
			unsafe {
				keyboard::clear_active_window();
				PostQuitMessage(0);
			}
			LRESULT(0)
		}
		_ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
	}
}

unsafe fn set_input_mode(window: HWND, mode: Mode) -> windows::core::Result<()> {
	let mut extended_style = unsafe { GetWindowLongPtrW(window, GWL_EXSTYLE) } as u32;
	match mode {
		Mode::Display => extended_style |= WS_EX_TRANSPARENT.0,
		Mode::Editing => extended_style &= !WS_EX_TRANSPARENT.0,
	}
	unsafe {
		SetWindowLongPtrW(window, GWL_EXSTYLE, extended_style as isize);
		SetWindowPos(
			window,
			HWND_TOPMOST,
			0,
			0,
			0,
			0,
			SWP_FRAMECHANGED | SWP_SHOWWINDOW | SWP_NOMOVE | SWP_NOSIZE,
		)?;
	}
	Ok(())
}

unsafe fn redraw(window: HWND, runtime: &mut Runtime) -> windows::core::Result<()> {
	let Some(requested_region) = runtime.application.take_dirty_region() else {
		return Ok(());
	};
	let Some(region) =
		renderer::clip_to_canvas(requested_region, runtime.frame.width, runtime.frame.height)
	else {
		return Ok(());
	};

	runtime
		.application
		.render_region(&mut runtime.frame, region);
	unsafe { runtime.surface.update_region(&runtime.frame, region) };

	if region.left == 0
		&& region.top == 0
		&& region.right == runtime.frame.width as i32
		&& region.bottom == runtime.frame.height as i32
	{
		unsafe {
			runtime.surface.present_full(
				window,
				runtime.screen.left,
				runtime.screen.top,
				runtime.screen.width,
				runtime.screen.height,
			)
		}
	} else {
		unsafe {
			runtime.surface.present_region(
				window,
				runtime.screen.left,
				runtime.screen.top,
				runtime.screen.width,
				runtime.screen.height,
				region,
			)
		}
	}
}
fn redraw_or_log(window: HWND, runtime: &mut Runtime) {
	if let Err(error) = unsafe { redraw(window, runtime) } {
		eprintln!("Could not redraw HoverOver: {error}");
	}
}

fn mouse_position(lparam: LPARAM) -> (f32, f32) {
	let x = (lparam.0 as u32 & 0xFFFF) as u16 as i16 as f32;
	let y = ((lparam.0 as u32 >> 16) & 0xFFFF) as u16 as i16 as f32;
	(x, y)
}
