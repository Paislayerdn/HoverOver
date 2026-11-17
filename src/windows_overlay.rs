//! Native window lifecycle and message routing. Keyboard, file dialog, and
//! bitmap details are kept in the sibling backstage modules.

use std::error::Error;

use base64::{Engine, engine::general_purpose::STANDARD};

use windows::{
	Win32::{
		Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM},
		Graphics::Gdi::ScreenToClient,
		System::LibraryLoader::GetModuleHandleW,
		UI::Input::KeyboardAndMouse::{
			GetCapture, GetFocus, ReleaseCapture, SetCapture, SetFocus, VK_DOWN, VK_LEFT,
			VK_RETURN, VK_RIGHT, VK_UP,
		},
		UI::WindowsAndMessaging::{
			CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow,
			DispatchMessageW, GWL_EXSTYLE, GWLP_USERDATA, GetCursorPos, GetMessageW,
			GetSystemMetrics, GetWindowLongPtrW, HWND_TOPMOST, IDC_ARROW, IDYES, LoadCursorW,
			MB_DEFBUTTON2, MB_ICONERROR, MB_ICONQUESTION, MB_OK, MB_YESNO, MSG, MessageBoxW,
			PostQuitMessage, RegisterClassW, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
			SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_SHOW, SWP_FRAMECHANGED, SWP_NOMOVE,
			SWP_NOSIZE, SWP_SHOWWINDOW, SetWindowLongPtrW, SetWindowPos, ShowWindow,
			TranslateMessage, WM_CAPTURECHANGED, WM_CHAR, WM_DESTROY, WM_HOTKEY, WM_KEYDOWN,
			WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCCREATE, WM_RBUTTONUP, WNDCLASSW,
			WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
		},
	},
	core::{Error as WinError, HSTRING, PCWSTR, w},
};

use crate::{
	app::{Application, MouseAction},
	image_file, image_store,
	model::Mode,
	renderer,
};

mod keyboard;
use keyboard::ToggleHotkey;
mod surface;
use surface::DibSurface;
mod context_menu;
mod file_dialog;
mod panel;
use panel::{FocusField, Panel, PanelAction};
mod presets_menu;
use presets_menu::PresetsMenuAction;
mod single_instance;

pub(super) const WINDOW_CLASS_NAME: PCWSTR = windows::core::w!("HoverOver");

/// Starts HoverOver and keeps its native window alive until Windows closes it.
pub fn run() -> Result<(), Box<dyn Error>> {
	let _instance_guard = match single_instance::acquire()? {
		single_instance::InstanceStatus::Primary(guard) => guard,
		single_instance::InstanceStatus::AlreadyRunning => {
			single_instance::notify_existing_instance();
			return Ok(());
		}
	};

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
			modal_ui_open: false,
			panel: None,
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
			WINDOW_CLASS_NAME,
			WINDOW_CLASS_NAME,
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

		let toggle_hotkey = match ToggleHotkey::register(window) {
			Ok(hotkey) => hotkey,
			Err(error) => {
				let _ = DestroyWindow(window);
				return Err(error.into());
			}
		};
		runtime.panel = Some(Panel::create(window, instance.into())?);
		refresh_panel_values(&mut runtime);

		runtime.surface.present_full(
			window,
			screen.left,
			screen.top,
			screen.width,
			screen.height,
		)?;
		let _ = ShowWindow(window, SW_SHOW);
		let message_loop_result = run_message_loop();
		drop(toggle_hotkey);
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
	modal_ui_open: bool,
	panel: Option<Panel>,
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
		hCursor: unsafe { LoadCursorW(None, IDC_ARROW)? },
		lpszClassName: WINDOW_CLASS_NAME,
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
	if unsafe { (*runtime_pointer).modal_ui_open } && message != WM_DESTROY {
		return unsafe { DefWindowProcW(window, message, wparam, lparam) };
	}
	let runtime = unsafe { &mut *runtime_pointer };

	match message {
		WM_HOTKEY if wparam.0 as i32 == keyboard::TOGGLE_HOTKEY_ID => {
			if !runtime.modal_ui_open {
				update_editor_mode(window, runtime, EditorModeRequest::Toggle);
			}
			LRESULT(0)
		}
		single_instance::SHOW_EDITOR_MESSAGE => {
			if !runtime.modal_ui_open {
				update_editor_mode(window, runtime, EditorModeRequest::Editing);
			}
			LRESULT(0)
		}
		WM_LBUTTONDOWN => {
			unsafe {
				let _ = SetFocus(window);
			}
			handle_panel_actions(window, runtime);
			let (x, y) = mouse_position(lparam);
			match runtime.application.mouse_down(x, y) {
				MouseAction::OpenPresets => {
					let button = renderer::toolbar_button_bounds(
						renderer::ToolbarButton::Presets,
						runtime.screen.width as u32,
						runtime.screen.height as u32,
					);
					runtime.modal_ui_open = true;
					let menu_result = presets_menu::show(
						window,
						runtime.screen.left + button.left,
						runtime.screen.top + button.top,
					);
					runtime.modal_ui_open = false;
					match menu_result {
						Ok(PresetsMenuAction::Save) => save_current_preset(window, runtime),
						Ok(PresetsMenuAction::Load) => load_selected_preset(window, runtime),
						Ok(PresetsMenuAction::Cancel) => {}
						Err(error) => report_user_error(
							window,
							runtime,
							format!("Could not show the presets menu: {error}"),
						),
					}
				}
				MouseAction::AddImage => {
					runtime.modal_ui_open = true;
					let result = unsafe { file_dialog::choose_image(window) };
					runtime.modal_ui_open = false;
					unsafe {
						let _ = SetFocus(window);
					}
					match result {
						Ok(Some(path)) => match image_file::load(&path) {
							Ok(image) => match image_store::app_image_folder()
								.and_then(|folder| image_store::import_image_into(&folder, &path))
							{
								Ok(stored_file_name) => {
									runtime.application.add_image(image, stored_file_name)
								}
								Err(error) => report_user_error(
									window,
									runtime,
									format!("Could not store selected image: {error}"),
								),
							},
							Err(error) => report_user_error(
								window,
								runtime,
								format!("Could not load selected image: {error}"),
							),
						},
						Ok(None) => {}
						Err(error) => report_user_error(
							window,
							runtime,
							format!("Could not open the image picker: {error}"),
						),
					}
					refresh_panel_values(runtime);
				}
				MouseAction::Quit => {
					unsafe {
						let _ = DestroyWindow(window);
					}
					return LRESULT(0);
				}
				MouseAction::None => {}
			}
			if runtime.application.is_dragging() {
				unsafe {
					let _ = SetCapture(window);
				}
			}
			refresh_panel_values(runtime);
			redraw_or_log(window, runtime);
			LRESULT(0)
		}
		WM_MOUSEMOVE => {
			let (x, y) = current_mouse_position(window, lparam);
			let shift_is_down = keyboard::shift_is_down();
			if runtime.application.mouse_move(x, y, shift_is_down) {
				redraw_or_log(window, runtime);
				if runtime.application.is_dragging() {
					refresh_panel_values(runtime);
				}
			}
			LRESULT(0)
		}
		WM_KEYDOWN => {
			if runtime.application.mode == Mode::Editing && unsafe { GetFocus() } == window {
				if wparam.0 as u32 == VK_RETURN.0 as u32 {
					if runtime.application.deselect_selected_image() {
						refresh_panel_values(runtime);
						redraw_or_log(window, runtime);
					}
					return LRESULT(0);
				}
				if let Some((offset_x, offset_y)) = arrow_key_offset(wparam.0 as u32) {
					if runtime.application.nudge_selected(offset_x, offset_y) {
						refresh_panel_values(runtime);
						redraw_or_log(window, runtime);
					}
					return LRESULT(0);
				}
			}
			unsafe { DefWindowProcW(window, message, wparam, lparam) }
		}
		WM_CHAR if wparam.0 == 0x0d => LRESULT(0),
		WM_LBUTTONUP => {
			let (x, y) = current_mouse_position(window, lparam);
			let shift_is_down = keyboard::shift_is_down();
			let moved = runtime.application.mouse_move(x, y, shift_is_down);
			runtime.application.mouse_up();
			if unsafe { GetCapture() } == window {
				unsafe {
					let _ = ReleaseCapture();
				}
			}
			refresh_panel_values(runtime);
			if moved {
				redraw_or_log(window, runtime);
			}
			LRESULT(0)
		}
		WM_CAPTURECHANGED => {
			runtime.application.end_drag();
			refresh_panel_values(runtime);
			LRESULT(0)
		}
		panel::PANEL_ACTION_MESSAGE => {
			handle_panel_actions(window, runtime);
			LRESULT(0)
		}
		WM_RBUTTONUP => {
			unsafe {
				let _ = SetFocus(window);
			}
			handle_panel_actions(window, runtime);
			let (x, y) = mouse_position(lparam);
			if let Some((flip_x, flip_y)) = runtime.application.select_image_for_context_menu(x, y)
			{
				refresh_panel_values(runtime);
				redraw_or_log(window, runtime);
				runtime.modal_ui_open = true;
				let result = context_menu::show(window, x, y, flip_x, flip_y);
				runtime.modal_ui_open = false;
				match result {
					Ok(action) => handle_context_menu_action(window, runtime, action),
					Err(error) => report_user_error(
						window,
						runtime,
						format!("Could not show the image menu: {error}"),
					),
				}
			}
			LRESULT(0)
		}
		WM_DESTROY => {
			unsafe {
				PostQuitMessage(0);
			}
			LRESULT(0)
		}
		_ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
	}
}

fn handle_context_menu_action(window: HWND, runtime: &mut Runtime, action: crate::app::MenuAction) {
	match action {
		crate::app::MenuAction::SetProperties => {
			if let Some(panel) = runtime.panel.as_mut() {
				panel.show_and_focus(
					runtime.application.selected_panel_values(),
					FocusField::X,
					(
						runtime.screen.left,
						runtime.screen.top,
						runtime.screen.width,
						runtime.screen.height,
					),
				);
			}
		}
		_ => {
			runtime.application.apply_context_menu_action(action);
			refresh_panel_values(runtime);
			redraw_or_log(window, runtime);
		}
	}
}

fn handle_panel_actions(window: HWND, runtime: &mut Runtime) {
	let actions = match runtime.panel.as_mut() {
		Some(panel) => panel.take_actions(),
		None => return,
	};
	let mut image_changed = false;
	for action in actions {
		match action {
			PanelAction::SetX(x) => {
				if let Some((_, y, _)) = runtime.application.selected_panel_values() {
					image_changed |= runtime.application.set_selected_center(x, y);
				}
			}
			PanelAction::SetY(y) => {
				if let Some((x, _, _)) = runtime.application.selected_panel_values() {
					image_changed |= runtime.application.set_selected_center(x, y);
				}
			}
			PanelAction::SetOpacity(opacity) => {
				image_changed |= runtime.application.set_selected_opacity(opacity);
			}
			PanelAction::Hidden => unsafe {
				let _ = SetFocus(window);
			},
		}
	}
	refresh_panel_values(runtime);
	if image_changed {
		redraw_or_log(window, runtime);
	}
}

fn refresh_panel_values(runtime: &mut Runtime) {
	if let Some(panel) = runtime.panel.as_mut() {
		panel.set_values(runtime.application.selected_panel_values());
	}
}

fn save_current_preset(window: HWND, runtime: &mut Runtime) {
	runtime.modal_ui_open = true;
	let chosen_path = unsafe {
		file_dialog::choose_preset_to_save(window, runtime.application.current_preset_path())
	};
	runtime.modal_ui_open = false;
	let path = match chosen_path {
		Ok(Some(path)) => path,
		Ok(None) => return,
		Err(error) => {
			report_user_error(
				window,
				runtime,
				format!("Could not open the preset save dialog: {error}"),
			);
			return;
		}
	};
	let folder = match image_store::app_image_folder() {
		Ok(folder) => folder,
		Err(error) => {
			report_user_error(
				window,
				runtime,
				format!("Could not find the image folder: {error}"),
			);
			return;
		}
	};
	let placements = runtime.application.current_placements();
	let mut preset = crate::presets::PresetFile::from_placements(
		&placements,
		runtime.screen.width as u32,
		runtime.screen.height as u32,
	);
	for image in &mut preset.images {
		let image_path = folder.join(&image.stored_file_name);
		match std::fs::read(&image_path) {
			Ok(bytes) => image.image_data_base64 = STANDARD.encode(bytes),
			Err(error) => {
				report_user_error(
					window,
					runtime,
					format!("Could not read image '{}': {error}", image_path.display()),
				);
				return;
			}
		}
	}
	if let Err(error) = crate::presets::save_preset_file(&path, &preset) {
		report_user_error(
			window,
			runtime,
			format!("Could not save the preset: {error}"),
		);
		return;
	}
	runtime.application.mark_preset_saved(&path);
}

fn load_selected_preset(window: HWND, runtime: &mut Runtime) {
	if runtime.application.has_unsaved_changes()
		&& !ask_yes_no(
			window,
			runtime,
			"Discard unsaved changes and load a preset?",
		) {
		return;
	}
	runtime.modal_ui_open = true;
	let chosen_path = unsafe { file_dialog::choose_preset_to_open(window) };
	runtime.modal_ui_open = false;
	let path = match chosen_path {
		Ok(Some(path)) => path,
		Ok(None) => return,
		Err(error) => {
			report_user_error(
				window,
				runtime,
				format!("Could not open the preset picker: {error}"),
			);
			return;
		}
	};
	let preset = match crate::presets::load_preset_file(&path) {
		Ok(preset) => preset,
		Err(error) => {
			report_user_error(
				window,
				runtime,
				format!("Could not load the preset: {error}"),
			);
			return;
		}
	};
	let (entries, mut problems) =
		preset.into_placements(runtime.screen.width as u32, runtime.screen.height as u32);
	let folder = match image_store::app_image_folder() {
		Ok(folder) => folder,
		Err(error) => {
			report_user_error(
				window,
				runtime,
				format!("Could not find the image folder: {error}"),
			);
			return;
		}
	};
	let (images, image_problems) = image_store::load_embedded_images(&folder, entries);
	problems.extend(image_problems);
	runtime.application.replace_images(images);
	runtime.application.mark_preset_saved(&path);
	refresh_panel_values(runtime);
	redraw_or_log(window, runtime);
	if !problems.is_empty() {
		let message = format!(
			"Some preset entries could not be loaded:\n\n{}",
			problems.join("\n")
		);
		show_message_box(window, runtime, &message, MB_OK | MB_ICONERROR);
	}
}

fn ask_yes_no(window: HWND, runtime: &mut Runtime, message: &str) -> bool {
	let message_text = HSTRING::from(message);
	runtime.modal_ui_open = true;
	let result = unsafe {
		MessageBoxW(
			window,
			&message_text,
			w!("HoverOver"),
			MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2,
		)
	};
	runtime.modal_ui_open = false;
	result == IDYES
}

fn show_message_box(
	window: HWND,
	runtime: &mut Runtime,
	message: &str,
	flags: windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_STYLE,
) {
	let message_text = HSTRING::from(message);
	runtime.modal_ui_open = true;
	unsafe {
		let _ = MessageBoxW(window, &message_text, w!("HoverOver"), flags);
	}
	runtime.modal_ui_open = false;
}

fn arrow_key_offset(key: u32) -> Option<(i32, i32)> {
	match key {
		key if key == VK_LEFT.0 as u32 => Some((-1, 0)),
		key if key == VK_RIGHT.0 as u32 => Some((1, 0)),
		key if key == VK_UP.0 as u32 => Some((0, -1)),
		key if key == VK_DOWN.0 as u32 => Some((0, 1)),
		_ => None,
	}
}

fn report_user_error(window: HWND, runtime: &mut Runtime, message: String) {
	eprintln!("{message}");
	show_message_box(window, runtime, &message, MB_OK | MB_ICONERROR);
}

#[derive(Clone, Copy)]
enum EditorModeRequest {
	Toggle,
	Editing,
}

fn update_editor_mode(window: HWND, runtime: &mut Runtime, request: EditorModeRequest) {
	let mode_changed = match request {
		EditorModeRequest::Toggle => {
			runtime.application.toggle_mode();
			true
		}
		EditorModeRequest::Editing => runtime.application.enter_editing_mode(),
	};
	if !mode_changed {
		return;
	}

	if let Err(error) = unsafe { set_input_mode(window, runtime.application.mode) } {
		eprintln!("Could not change HoverOver mode: {error}");
	}
	match runtime.application.mode {
		Mode::Editing => {}
		Mode::Display => {
			if let Some(panel) = runtime.panel.as_mut() {
				panel.set_values(None);
				panel.hide();
			}
			unsafe {
				let _ = SetFocus(window);
			}
		}
	}
	if let Err(error) = unsafe { redraw(window, runtime) } {
		eprintln!("Could not redraw HoverOver: {error}");
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

fn current_mouse_position(window: HWND, lparam: LPARAM) -> (f32, f32) {
	let mut position = POINT::default();
	if unsafe { GetCursorPos(&mut position) }.is_ok()
		&& unsafe { ScreenToClient(window, &mut position) }.0 != 0
	{
		(position.x as f32, position.y as f32)
	} else {
		mouse_position(lparam)
	}
}
