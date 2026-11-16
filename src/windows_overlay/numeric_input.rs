//! Show short native dialogs for entering image position or opacity values.

use windows::{
	Win32::{
		Foundation::{
			ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, RECT,
			WPARAM,
		},
		Graphics::Gdi::{COLOR_BTNFACE, GetSysColorBrush},
		UI::{
			Input::KeyboardAndMouse::{EnableWindow, SetFocus},
			WindowsAndMessaging::{
				BS_DEFPUSHBUTTON, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreateWindowExW,
				DefWindowProcW, DestroyWindow, DispatchMessageW, ES_AUTOHSCROLL, GWLP_USERDATA,
				GetMessageW, GetWindowLongPtrW, GetWindowRect, GetWindowTextLengthW,
				GetWindowTextW, HMENU, IsDialogMessageW, MSG, PostQuitMessage, RegisterClassW,
				SW_SHOW, SendMessageW, SetForegroundWindow, SetWindowLongPtrW, ShowWindow,
				TranslateMessage, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_KEYDOWN,
				WM_NCCREATE, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD, WS_EX_DLGMODALFRAME,
				WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
			},
		},
	},
	core::{Error, HSTRING, PCWSTR, Result, w},
};

const DIALOG_CLASS: PCWSTR = w!("HoverOverNumberEntry");
const ID_FIRST_EDIT: usize = 101;
const ID_SECOND_EDIT: usize = 102;
const ID_OK: usize = 1;
const ID_CANCEL: usize = 2;
const POSITION_WINDOW_WIDTH: i32 = 360;
const POSITION_WINDOW_HEIGHT: i32 = 180;
const OPACITY_WINDOW_HEIGHT: i32 = 145;

#[derive(Clone, Copy)]
pub(super) enum NumericInputResult {
	Position { x: i32, y: i32 },
	Opacity(i32),
}

enum NumericInputKind {
	Position { initial_x: i32, initial_y: i32 },
	Opacity { initial_opacity: u8 },
}

struct DialogState {
	kind: NumericInputKind,
	instance: HINSTANCE,
	first_edit: HWND,
	second_edit: HWND,
	finished: bool,
	result: Option<NumericInputResult>,
}

struct DisabledOwner(HWND);

impl Drop for DisabledOwner {
	fn drop(&mut self) {
		unsafe {
			let _ = EnableWindow(self.0, true);
		}
	}
}

pub(super) fn position(owner: HWND, initial_x: i32, initial_y: i32) -> Result<Option<(i32, i32)>> {
	let kind = NumericInputKind::Position {
		initial_x,
		initial_y,
	};
	match show(owner, kind)? {
		Some(NumericInputResult::Position { x, y }) => Ok(Some((x, y))),
		_ => Ok(None),
	}
}

pub(super) fn opacity(owner: HWND, initial_opacity: u8) -> Result<Option<i32>> {
	match show(owner, NumericInputKind::Opacity { initial_opacity })? {
		Some(NumericInputResult::Opacity(value)) => Ok(Some(value)),
		_ => Ok(None),
	}
}

fn show(owner: HWND, kind: NumericInputKind) -> Result<Option<NumericInputResult>> {
	let instance: HINSTANCE =
		unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(PCWSTR::null())? }.into();
	register_class(instance)?;
	let state = &mut DialogState {
		kind,
		instance,
		first_edit: HWND::default(),
		second_edit: HWND::default(),
		finished: false,
		result: None,
	};
	let mut owner_bounds = RECT::default();
	unsafe { GetWindowRect(owner, &mut owner_bounds)? };
	let window_height = match state.kind {
		NumericInputKind::Position { .. } => POSITION_WINDOW_HEIGHT,
		NumericInputKind::Opacity { .. } => OPACITY_WINDOW_HEIGHT,
	};
	let window_left =
		owner_bounds.left + (owner_bounds.right - owner_bounds.left - POSITION_WINDOW_WIDTH) / 2;
	let window_top =
		owner_bounds.top + (owner_bounds.bottom - owner_bounds.top - window_height) / 2;
	let _disabled_owner = DisabledOwner(owner);
	unsafe {
		let _ = EnableWindow(owner, false);
	}
	let dialog = unsafe {
		CreateWindowExW(
			WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_DLGMODALFRAME,
			DIALOG_CLASS,
			match state.kind {
				NumericInputKind::Position { .. } => w!("Set position"),
				NumericInputKind::Opacity { .. } => w!("Set opacity"),
			},
			WS_POPUP | WS_CAPTION | WS_SYSMENU,
			window_left,
			window_top,
			POSITION_WINDOW_WIDTH,
			window_height,
			owner,
			None,
			instance,
			Some((state as *mut DialogState).cast()),
		)?
	};
	unsafe {
		let _ = SetForegroundWindow(dialog);
		let _ = ShowWindow(dialog, SW_SHOW);
	}

	while !state.finished {
		let mut message = MSG::default();
		let result = unsafe { GetMessageW(&mut message, None, 0, 0) }.0;
		if result == -1 {
			unsafe {
				let _ = DestroyWindow(dialog);
			}
			return Err(Error::from_win32());
		}
		if result == 0 {
			unsafe {
				PostQuitMessage(message.wParam.0 as i32);
			}
			break;
		}
		if message.message == WM_KEYDOWN && (message.wParam.0 == 0x0d || message.wParam.0 == 0x1b) {
			let command_id = if message.wParam.0 == 0x0d {
				ID_OK
			} else {
				ID_CANCEL
			};
			unsafe {
				let _ = SendMessageW(dialog, WM_COMMAND, WPARAM(command_id), LPARAM(0));
			}
			continue;
		}
		if !unsafe { IsDialogMessageW(dialog, &message) }.as_bool() {
			unsafe {
				let _ = TranslateMessage(&message);
				DispatchMessageW(&message);
			}
		}
	}
	Ok(state.result)
}

fn register_class(instance: HINSTANCE) -> Result<()> {
	let class = WNDCLASSW {
		style: CS_HREDRAW | CS_VREDRAW,
		lpfnWndProc: Some(dialog_window_procedure),
		hInstance: instance,
		hbrBackground: unsafe { GetSysColorBrush(COLOR_BTNFACE) },
		lpszClassName: DIALOG_CLASS,
		..Default::default()
	};
	if unsafe { RegisterClassW(&class) } == 0
		&& unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS
	{
		return Err(Error::from_win32());
	}
	Ok(())
}

unsafe extern "system" fn dialog_window_procedure(
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
	let state_pointer = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *mut DialogState;
	if state_pointer.is_null() {
		return unsafe { DefWindowProcW(window, message, wparam, lparam) };
	}
	let state = unsafe { &mut *state_pointer };
	match message {
		WM_CREATE => {
			if create_controls(window, state).is_err() {
				return LRESULT(-1);
			}
			unsafe {
				let _ = SetFocus(state.first_edit);
			}
			LRESULT(0)
		}
		WM_COMMAND => {
			match wparam.0 & 0xffff {
				ID_OK => {
					if let Some(result) = read_result(state) {
						state.result = Some(result);
						state.finished = true;
						unsafe {
							let _ = DestroyWindow(window);
						}
					}
				}
				ID_CANCEL => {
					state.finished = true;
					unsafe {
						let _ = DestroyWindow(window);
					}
				}
				_ => {}
			}
			LRESULT(0)
		}
		WM_CLOSE => {
			state.finished = true;
			unsafe {
				let _ = DestroyWindow(window);
			}
			LRESULT(0)
		}
		_ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
	}
}

fn create_controls(window: HWND, state: &mut DialogState) -> Result<()> {
	let (label, first_value, second_value) = match state.kind {
		NumericInputKind::Position {
			initial_x,
			initial_y,
		} => (None, initial_x.to_string(), Some(initial_y.to_string())),
		NumericInputKind::Opacity { initial_opacity } => {
			(Some("Opacity (0-100)"), initial_opacity.to_string(), None)
		}
	};
	let first_y = if second_value.is_some() { 18 } else { 32 };
	let first_label = match label {
		Some(label) => label,
		None => "X",
	};
	create_child(
		window,
		state.instance,
		w!("STATIC"),
		first_label,
		WS_CHILD | WS_VISIBLE,
		18,
		first_y + 4,
		90,
		22,
		10,
	)?;
	state.first_edit = create_edit(
		window,
		state.instance,
		&first_value,
		110,
		first_y,
		ID_FIRST_EDIT,
	)?;

	if let Some(second_value) = second_value {
		create_child(
			window,
			state.instance,
			w!("STATIC"),
			"Y",
			WS_CHILD | WS_VISIBLE,
			18,
			55,
			30,
			22,
			11,
		)?;
		state.second_edit = create_edit(
			window,
			state.instance,
			&second_value,
			110,
			51,
			ID_SECOND_EDIT,
		)?;
	}

	let buttons_y = if state.second_edit.0.is_null() {
		82
	} else {
		100
	};
	create_button(window, state.instance, w!("OK"), ID_OK, buttons_y, true)?;
	create_button(
		window,
		state.instance,
		w!("Cancel"),
		ID_CANCEL,
		buttons_y,
		false,
	)?;
	Ok(())
}

fn create_edit(
	window: HWND,
	instance: HINSTANCE,
	value: &str,
	x: i32,
	y: i32,
	id: usize,
) -> Result<HWND> {
	let value = HSTRING::from(value);
	unsafe {
		CreateWindowExW(
			Default::default(),
			w!("EDIT"),
			&value,
			WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
			x,
			y,
			220,
			24,
			window,
			HMENU(id as *mut _),
			instance,
			None,
		)
	}
}

fn create_button(
	window: HWND,
	instance: HINSTANCE,
	label: PCWSTR,
	id: usize,
	y: i32,
	is_default: bool,
) -> Result<HWND> {
	let x = if id == ID_OK { 190 } else { 270 };
	let style = WS_CHILD
		| WS_VISIBLE
		| WS_TABSTOP
		| if is_default {
			WINDOW_STYLE(BS_DEFPUSHBUTTON as u32)
		} else {
			Default::default()
		};
	unsafe {
		CreateWindowExW(
			Default::default(),
			w!("BUTTON"),
			label,
			style,
			x,
			y,
			70,
			26,
			window,
			HMENU(id as *mut _),
			instance,
			None,
		)
	}
}

fn create_child(
	window: HWND,
	instance: HINSTANCE,
	class: PCWSTR,
	label: &str,
	style: WINDOW_STYLE,
	x: i32,
	y: i32,
	width: i32,
	height: i32,
	id: usize,
) -> Result<HWND> {
	let label = HSTRING::from(label);
	unsafe {
		CreateWindowExW(
			Default::default(),
			class,
			&label,
			style,
			x,
			y,
			width,
			height,
			window,
			HMENU(id as *mut _),
			instance,
			None,
		)
	}
}

fn read_result(state: &DialogState) -> Option<NumericInputResult> {
	let first_value = read_integer(state.first_edit)?;
	match state.kind {
		NumericInputKind::Position { .. } => {
			let second_value = read_integer(state.second_edit)?;
			Some(NumericInputResult::Position {
				x: first_value,
				y: second_value,
			})
		}
		NumericInputKind::Opacity { .. } => {
			Some(NumericInputResult::Opacity(first_value.clamp(0, 100)))
		}
	}
}

fn read_integer(edit: HWND) -> Option<i32> {
	let text_length = unsafe { GetWindowTextLengthW(edit) }.max(0) as usize;
	let mut text = vec![0u16; text_length + 1];
	let written = unsafe { GetWindowTextW(edit, &mut text) }.max(0) as usize;
	String::from_utf16_lossy(&text[..written])
		.trim()
		.parse()
		.ok()
}
