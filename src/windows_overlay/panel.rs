//! Always-available native editing panel and its deferred actions.

use std::collections::VecDeque;

use windows::{
	Win32::{
		Foundation::{
			ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM,
		},
		Graphics::Gdi::{COLOR_BTNFACE, GetSysColorBrush},
		UI::{
			Input::KeyboardAndMouse::{EnableWindow, GetFocus, SetFocus},
			WindowsAndMessaging::{
				CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CallWindowProcW, CreateWindowExW,
				DefWindowProcW, DestroyWindow, EN_CHANGE, EN_KILLFOCUS, ES_AUTOHSCROLL,
				GWLP_USERDATA, GWLP_WNDPROC, GetWindowLongPtrW, GetWindowTextLengthW,
				GetWindowTextW, HMENU, HWND_TOPMOST, IsWindowVisible, RegisterClassW, SW_HIDE,
				SW_SHOW, SWP_NOMOVE, SWP_NOSIZE, SendMessageW, SetWindowLongPtrW, SetWindowPos,
				ShowWindow, WINDOW_STYLE, WM_CHAR, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_KEYDOWN,
				WM_NCCREATE, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD, WS_EX_TOOLWINDOW,
				WS_EX_TOPMOST, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
			},
		},
	},
	core::{Error, HSTRING, PCWSTR, Result, w},
};

use crate::model::PanelValues;

pub(super) const PANEL_ACTION_MESSAGE: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 3;

const PANEL_CLASS: PCWSTR = w!("HoverOverToolPanel");
const ID_X_EDIT: usize = 101;
const ID_Y_EDIT: usize = 102;
const ID_OPACITY_EDIT: usize = 103;
const ID_Z_EDIT: usize = 104;
const PANEL_WIDTH: i32 = 280;
const PANEL_HEIGHT: i32 = 230;

#[derive(Clone, Copy)]
pub(super) enum PanelAction {
	SetX(i32),
	SetY(i32),
	SetOpacity(i32),
	SetZ(u16),
	RejectedZ,
	Hidden,
}

#[derive(Clone, Copy)]
pub(super) enum FocusField {
	X,
}

struct PanelState {
	owner: HWND,
	instance: HINSTANCE,
	x_edit: HWND,
	y_edit: HWND,
	opacity_edit: HWND,
	z_edit: HWND,
	suppress_changes: bool,
	changed_edits: [bool; 4],
	actions: VecDeque<PanelAction>,
}

pub(super) struct Panel {
	window: HWND,
	state: Box<PanelState>,
}

impl Panel {
	pub(super) fn create(owner: HWND, instance: HINSTANCE) -> Result<Self> {
		register_class(instance)?;
		let mut state = Box::new(PanelState {
			owner,
			instance,
			x_edit: HWND::default(),
			y_edit: HWND::default(),
			opacity_edit: HWND::default(),
			z_edit: HWND::default(),
			suppress_changes: true,
			changed_edits: [false; 4],
			actions: VecDeque::new(),
		});
		let window = unsafe {
			CreateWindowExW(
				WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
				PANEL_CLASS,
				w!("Hover Over"),
				WS_POPUP | WS_CAPTION | WS_SYSMENU,
				80,
				80,
				PANEL_WIDTH,
				PANEL_HEIGHT,
				owner,
				None,
				instance,
				Some((&mut *state as *mut PanelState).cast()),
			)?
		};
		state.suppress_changes = false;
		Ok(Self { window, state })
	}

	pub(super) fn show_values(&mut self, values: Option<PanelValues>) {
		self.set_values(values);
		unsafe {
			let _ = ShowWindow(self.window, SW_SHOW);
			let _ = SetWindowPos(
				self.window,
				HWND_TOPMOST,
				0,
				0,
				0,
				0,
				SWP_NOMOVE | SWP_NOSIZE,
			);
		}
	}

	pub(super) fn hide(&mut self) {
		unsafe {
			let _ = ShowWindow(self.window, SW_HIDE);
		}
	}

	pub(super) fn set_values(&mut self, values: Option<PanelValues>) {
		self.state.suppress_changes = true;
		match values {
			Some(values) => {
				set_edit_value_if_not_focused(self.state.x_edit, &values.x.to_string());
				set_edit_value_if_not_focused(self.state.y_edit, &values.y.to_string());
				set_edit_value_if_not_focused(self.state.opacity_edit, &values.opacity.to_string());
				set_edit_value_if_not_focused(self.state.z_edit, &values.z.to_string());
				set_edit_enabled(self.state.x_edit, true);
				set_edit_enabled(self.state.y_edit, true);
				set_edit_enabled(self.state.opacity_edit, true);
				set_edit_enabled(self.state.z_edit, true);
			}
			None => {
				self.state.changed_edits = [false; 4];
				set_edit_value(self.state.x_edit, "");
				set_edit_value(self.state.y_edit, "");
				set_edit_value(self.state.opacity_edit, "");
				set_edit_value(self.state.z_edit, "");
				set_edit_enabled(self.state.x_edit, false);
				set_edit_enabled(self.state.y_edit, false);
				set_edit_enabled(self.state.opacity_edit, false);
				set_edit_enabled(self.state.z_edit, false);
			}
		}
		self.state.suppress_changes = false;
	}

	pub(super) fn focus_field(&mut self, field: FocusField) {
		let edit = match field {
			FocusField::X => self.state.x_edit,
		};
		unsafe {
			let _ = SetFocus(edit);
			let _ = SendMessageW(edit, 0x00B1, WPARAM(0), LPARAM(-1));
		}
	}

	pub(super) fn show_and_focus(
		&mut self,
		values: Option<PanelValues>,
		field: FocusField,
		screen: (i32, i32, i32, i32),
	) {
		if unsafe { IsWindowVisible(self.window).as_bool() } == false {
			let (left, top, width, height) = screen;
			unsafe {
				let _ = SetWindowPos(
					self.window,
					HWND_TOPMOST,
					left + (width - PANEL_WIDTH) / 2,
					top + (height - PANEL_HEIGHT) / 2,
					0,
					0,
					SWP_NOSIZE,
				);
			}
		}
		self.show_values(values);
		self.focus_field(field);
	}

	pub(super) fn take_actions(&mut self) -> Vec<PanelAction> {
		self.state.actions.drain(..).collect()
	}
}

impl Drop for Panel {
	fn drop(&mut self) {
		unsafe {
			let _ = DestroyWindow(self.window);
		}
	}
}

fn register_class(instance: HINSTANCE) -> Result<()> {
	let class = WNDCLASSW {
		style: CS_HREDRAW | CS_VREDRAW,
		lpfnWndProc: Some(panel_window_procedure),
		hInstance: instance,
		hbrBackground: unsafe { GetSysColorBrush(COLOR_BTNFACE) },
		lpszClassName: PANEL_CLASS,
		..Default::default()
	};
	if unsafe { RegisterClassW(&class) } == 0
		&& unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS
	{
		return Err(Error::from_win32());
	}
	Ok(())
}

unsafe extern "system" fn panel_window_procedure(
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
	let state_pointer = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *mut PanelState;
	if state_pointer.is_null() {
		return unsafe { DefWindowProcW(window, message, wparam, lparam) };
	}
	let state = unsafe { &mut *state_pointer };
	match message {
		WM_CREATE => {
			if create_controls(window, state).is_err() {
				return LRESULT(-1);
			}
			LRESULT(0)
		}
		WM_COMMAND if !state.suppress_changes => {
			let control_id = wparam.0 & 0xffff;
			let notification = (wparam.0 >> 16) & 0xffff;
			if notification == EN_CHANGE as usize {
				set_edit_changed(state, control_id, true);
			} else if notification == EN_KILLFOCUS as usize {
				commit_edit(state, control_id);
			}
			LRESULT(0)
		}
		WM_CLOSE => {
			hide_panel_and_notify(state, window);
			LRESULT(0)
		}
		_ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
	}
}

fn hide_panel_and_notify(state: &mut PanelState, panel_window: HWND) {
	unsafe {
		let _ = ShowWindow(panel_window, SW_HIDE);
	}
	state.actions.push_back(PanelAction::Hidden);
	notify_owner(state);
}

fn create_controls(window: HWND, state: &mut PanelState) -> Result<()> {
	create_label(window, state.instance, "X", 16, 20)?;
	state.x_edit = create_edit(window, state.instance, "", 100, 16, ID_X_EDIT)?;
	create_label(window, state.instance, "Y", 16, 62)?;
	state.y_edit = create_edit(window, state.instance, "", 100, 58, ID_Y_EDIT)?;
	create_label(window, state.instance, "Opacity", 16, 104)?;
	state.opacity_edit = create_edit(window, state.instance, "", 100, 100, ID_OPACITY_EDIT)?;
	create_label(window, state.instance, "Z", 16, 146)?;
	state.z_edit = create_edit(window, state.instance, "", 100, 142, ID_Z_EDIT)?;
	for edit in [state.x_edit, state.y_edit, state.opacity_edit, state.z_edit] {
		unsafe {
			let _ = EnableWindow(edit, false);
		}
	}
	Ok(())
}

fn create_label(window: HWND, instance: HINSTANCE, label: &str, x: i32, y: i32) -> Result<HWND> {
	let label = HSTRING::from(label);
	unsafe {
		CreateWindowExW(
			Default::default(),
			w!("STATIC"),
			&label,
			WS_CHILD | WS_VISIBLE,
			x,
			y + 4,
			76,
			22,
			window,
			HMENU(0 as *mut _),
			instance,
			None,
		)
	}
}

fn create_edit(
	window: HWND,
	instance: HINSTANCE,
	initial: &str,
	x: i32,
	y: i32,
	id: usize,
) -> Result<HWND> {
	let initial = HSTRING::from(initial);
	let edit = unsafe {
		CreateWindowExW(
			Default::default(),
			w!("EDIT"),
			&initial,
			WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
			x,
			y,
			150,
			26,
			window,
			HMENU(id as *mut _),
			instance,
			None,
		)?
	};
	let original_procedure = unsafe {
		SetWindowLongPtrW(
			edit,
			GWLP_WNDPROC,
			edit_window_procedure as *const () as usize as isize,
		)
	};
	unsafe {
		SetWindowLongPtrW(edit, GWLP_USERDATA, original_procedure);
	}
	Ok(edit)
}

fn commit_edit(state: &mut PanelState, control_id: usize) {
	if !edit_is_changed(state, control_id) {
		notify_owner(state);
		return;
	}
	set_edit_changed(state, control_id, false);
	let action = match control_id {
		ID_X_EDIT => read_edit_integer(state.x_edit).map(PanelAction::SetX),
		ID_Y_EDIT => read_edit_integer(state.y_edit).map(PanelAction::SetY),
		ID_OPACITY_EDIT => {
			parse_opacity_text(&read_edit_text(state.opacity_edit)).map(PanelAction::SetOpacity)
		}
		ID_Z_EDIT => match parse_z_text(&read_edit_text(state.z_edit)) {
			Some(value) => Some(PanelAction::SetZ(value)),
			None => Some(PanelAction::RejectedZ),
		},
		_ => None,
	};
	if let Some(action) = action {
		state.actions.push_back(action);
	}
	notify_owner(state);
}

fn edit_is_changed(state: &PanelState, control_id: usize) -> bool {
	edit_index(control_id).is_some_and(|index| state.changed_edits[index])
}

fn set_edit_changed(state: &mut PanelState, control_id: usize, changed: bool) {
	if let Some(index) = edit_index(control_id) {
		state.changed_edits[index] = changed;
	}
}

fn edit_index(control_id: usize) -> Option<usize> {
	match control_id {
		ID_X_EDIT => Some(0),
		ID_Y_EDIT => Some(1),
		ID_OPACITY_EDIT => Some(2),
		ID_Z_EDIT => Some(3),
		_ => None,
	}
}

fn notify_owner(state: &PanelState) {
	unsafe {
		let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
			state.owner,
			PANEL_ACTION_MESSAGE,
			WPARAM(0),
			LPARAM(0),
		);
	}
}

unsafe extern "system" fn edit_window_procedure(
	window: HWND,
	message: u32,
	wparam: WPARAM,
	lparam: LPARAM,
) -> LRESULT {
	if message == WM_CHAR && wparam.0 == 0x0d {
		return LRESULT(0);
	}
	if message == WM_KEYDOWN && wparam.0 == 0x0d {
		let Ok(parent) = (unsafe { windows::Win32::UI::WindowsAndMessaging::GetParent(window) })
		else {
			return LRESULT(0);
		};
		let edit_id =
			unsafe { windows::Win32::UI::WindowsAndMessaging::GetDlgCtrlID(window) } as usize;
		unsafe {
			let _ = SendMessageW(
				parent,
				WM_COMMAND,
				WPARAM(((EN_KILLFOCUS as usize) << 16) | edit_id),
				LPARAM(window.0 as isize),
			);
			let _ = SetFocus(parent);
		}
		return LRESULT(0);
	}
	if message == WM_KEYDOWN && wparam.0 == 0x1b {
		let Ok(panel_window) =
			(unsafe { windows::Win32::UI::WindowsAndMessaging::GetParent(window) })
		else {
			return LRESULT(0);
		};
		let state_pointer =
			unsafe { GetWindowLongPtrW(panel_window, GWLP_USERDATA) } as *mut PanelState;
		if !state_pointer.is_null() {
			let state = unsafe { &mut *state_pointer };
			let edit_id =
				unsafe { windows::Win32::UI::WindowsAndMessaging::GetDlgCtrlID(window) } as usize;
			set_edit_changed(state, edit_id, false);
			hide_panel_and_notify(state, panel_window);
		}
		return LRESULT(0);
	}
	let original_procedure = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) };
	let original_procedure = unsafe {
		std::mem::transmute::<
			isize,
			Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>,
		>(original_procedure)
	};
	unsafe { CallWindowProcW(original_procedure, window, message, wparam, lparam) }
}

fn set_edit_value_if_not_focused(edit: HWND, value: &str) {
	if unsafe { GetFocus() } == edit || edit.0.is_null() {
		return;
	}
	let value = HSTRING::from(value);
	unsafe {
		let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowTextW(edit, &value);
	}
}

fn set_edit_value(edit: HWND, value: &str) {
	let value = HSTRING::from(value);
	unsafe {
		let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowTextW(edit, &value);
	}
}

fn set_edit_enabled(edit: HWND, enabled: bool) {
	unsafe {
		let _ = EnableWindow(edit, enabled);
	}
}

fn read_edit_integer(edit: HWND) -> Option<i32> {
	parse_integer_text(&read_edit_text(edit))
}

fn read_edit_text(edit: HWND) -> String {
	let text_length = unsafe { GetWindowTextLengthW(edit) }.max(0) as usize;
	let mut text = vec![0u16; text_length + 1];
	let written = unsafe { GetWindowTextW(edit, &mut text) }.max(0) as usize;
	String::from_utf16_lossy(&text[..written])
}

fn parse_integer_text(text: &str) -> Option<i32> {
	text.trim().parse().ok()
}

fn parse_opacity_text(text: &str) -> Option<i32> {
	parse_integer_text(text)
}

fn parse_z_text(text: &str) -> Option<u16> {
	text.trim().parse().ok()
}

#[cfg(test)]
mod tests {
	use super::{parse_integer_text, parse_opacity_text, parse_z_text};
	use crate::model::clamp_opacity_percent;

	#[test]
	fn integer_text_parser_accepts_integers_and_rejects_empty_or_garbage() {
		assert_eq!(parse_integer_text("42"), Some(42));
		assert_eq!(parse_integer_text(" -17 "), Some(-17));
		assert_eq!(parse_integer_text(" 8 "), Some(8));
		assert_eq!(parse_integer_text(""), None);
		assert_eq!(parse_integer_text("   "), None);
		assert_eq!(parse_integer_text("hello"), None);
	}

	#[test]
	fn opacity_text_is_clamped_to_the_supported_range() {
		assert_eq!(
			parse_opacity_text("-10").map(clamp_opacity_percent),
			Some(0)
		);
		assert_eq!(
			parse_opacity_text("45").map(clamp_opacity_percent),
			Some(45)
		);
		assert_eq!(
			parse_opacity_text("150").map(clamp_opacity_percent),
			Some(100)
		);
		assert_eq!(parse_opacity_text("bad"), None);
	}

	#[test]
	fn z_text_parser_accepts_u16_values_and_rejects_invalid_values() {
		assert_eq!(parse_z_text("0"), Some(0));
		assert_eq!(parse_z_text(" 65535 "), Some(65535));
		for invalid in ["-1", "65536", "", "garbage"] {
			assert_eq!(parse_z_text(invalid), None);
		}
	}
}
