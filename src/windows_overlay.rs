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
			MB_DEFBUTTON2, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONQUESTION, MB_OK, MB_YESNO, MSG,
			MessageBoxW, PostQuitMessage, RegisterClassW, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
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
mod local_data;
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
						Ok(PresetsMenuAction::ClearLocalData) => {
							clear_all_local_data(window, runtime)
						}
						Ok(PresetsMenuAction::Cancel) => {}
						Err(error) => report_user_error(
							window,
							runtime,
							format!("Could not show the presets menu: {error}"),
						),
					}
				}
				MouseAction::AddImage => {
					if !runtime.application.can_add_image() {
						report_user_error(
							window,
							runtime,
							"The scene has no room for another image.".to_string(),
						);
					} else {
						runtime.modal_ui_open = true;
						let result = unsafe { file_dialog::choose_image(window) };
						runtime.modal_ui_open = false;
						unsafe {
							let _ = SetFocus(window);
						}
						match result {
							Ok(Some(path)) => match image_file::load(&path) {
								Ok(image) => {
									match image_store::app_image_folder().and_then(|folder| {
										image_store::import_image_into(&folder, &path)
									}) {
										Ok(stored_file_name) => {
											if let Err(error) = runtime
												.application
												.add_image(image, stored_file_name)
											{
												report_user_error(
													window,
													runtime,
													error.to_string(),
												);
											}
										}
										Err(error) => report_user_error(
											window,
											runtime,
											format!("Could not store selected image: {error}"),
										),
									}
								}
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
			if let Some(menu_state) = runtime.application.select_image_for_context_menu(x, y) {
				refresh_panel_values(runtime);
				redraw_or_log(window, runtime);
				runtime.modal_ui_open = true;
				let result = context_menu::show(window, x, y, menu_state);
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
				if let Some(values) = runtime.application.selected_panel_values() {
					image_changed |= runtime.application.set_selected_center(x, values.y);
				}
			}
			PanelAction::SetY(y) => {
				if let Some(values) = runtime.application.selected_panel_values() {
					image_changed |= runtime.application.set_selected_center(values.x, y);
				}
			}
			PanelAction::SetOpacity(opacity) => {
				image_changed |= runtime.application.set_selected_opacity(opacity);
			}
			PanelAction::SetZ(z) => match runtime.application.set_selected_z(z) {
				Ok(changed) => image_changed |= changed,
				Err(error) => report_user_error(window, runtime, error.to_string()),
			},
			PanelAction::SetRotation(rotation_degrees) => {
				image_changed |= runtime.application.set_selected_rotation(rotation_degrees);
			}
			PanelAction::RejectedZ => report_user_error(
				window,
				runtime,
				"Z must be an integer from 0 to 65535.".to_string(),
			),
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
	let placements = runtime.application.current_placements();
	let mut preset = crate::presets::PresetFile::from_placements(
		&placements,
		runtime.screen.width as u32,
		runtime.screen.height as u32,
	);
	if let Err(error) = embed_current_images(&runtime.application, &mut preset) {
		report_user_error(window, runtime, error);
		return;
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

fn embed_current_images(
	application: &Application,
	preset: &mut crate::presets::PresetFile,
) -> Result<(), String> {
	for (image_index, image) in preset.images.iter_mut().enumerate() {
		let encoded_image = application
			.image_png_bytes(image_index)
			.ok_or_else(|| format!("Could not find in-memory image {}.", image_index + 1))?
			.map_err(|error| {
				format!(
					"Could not encode in-memory image {} for the preset: {error}",
					image_index + 1
				)
			})?;
		image.image_data_base64 = STANDARD.encode(encoded_image);
	}
	Ok(())
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
	let preset_had_images = !preset.images.is_empty();
	let (entries, mut problems) =
		match preset.into_placements(runtime.screen.width as u32, runtime.screen.height as u32) {
			Ok(result) => result,
			Err(error) => {
				report_user_error(window, runtime, error);
				return;
			}
		};
	let mut image_folder = None;
	let image_load = if entries.is_empty() {
		image_store::EmbeddedImageLoad::default()
	} else {
		let folder = match image_store::app_image_folder_path() {
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
		image_folder = Some(folder.clone());
		image_store::load_embedded_images(&folder, entries)
	};
	problems.extend(image_load.problems);
	if preset_had_images && image_load.images.is_empty() {
		if let Some(folder) = image_folder.as_deref() {
			problems.extend(image_store::cleanup_imported_files(
				folder,
				&image_load.newly_imported_files,
			));
		}
		if problems.is_empty() {
			problems.push("No images from this preset could be loaded.".to_string());
		}
		let message = format!(
			"No images from this preset could be loaded. Your current scene was kept.\n\n{}",
			problems.join("\n")
		);
		show_message_box(window, runtime, &message, MB_OK | MB_ICONERROR);
		return;
	}
	if let Err(error) = runtime.application.replace_images(image_load.images) {
		if let Some(folder) = image_folder.as_deref() {
			let cleanup_problems =
				image_store::cleanup_imported_files(folder, &image_load.newly_imported_files);
			let cleanup_details = if cleanup_problems.is_empty() {
				String::new()
			} else {
				format!("\n\nCleanup details:\n{}", cleanup_problems.join("\n"))
			};
			report_user_error(
				window,
				runtime,
				format!("Could not replace the scene: {error}{cleanup_details}"),
			);
			return;
		}
		report_user_error(window, runtime, error.to_string());
		return;
	}
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

fn clear_all_local_data(window: HWND, runtime: &mut Runtime) {
	let inspection = match local_data::inspect_local_data() {
		Ok(Some(inspection)) => inspection,
		Ok(None) => {
			show_message_box(
				window,
				runtime,
				"No local HoverOver data folder exists.",
				MB_OK | MB_ICONINFORMATION,
			);
			return;
		}
		Err(error) => {
			report_user_error(
				window,
				runtime,
				format!("Could not safely inspect local HoverOver data: {error}"),
			);
			return;
		}
	};
	runtime.modal_ui_open = true;
	let confirmation = local_data::confirm_clear_local_data(window, &inspection);
	runtime.modal_ui_open = false;
	let confirmed = match confirmation {
		Ok(confirmed) => confirmed,
		Err(error) => {
			report_user_error(window, runtime, error);
			return;
		}
	};
	if !confirmed {
		return;
	}
	match local_data::clear_local_data(&inspection) {
		Ok(path) => show_message_box(
			window,
			runtime,
			&format!("Local HoverOver data was deleted from:\n{}", path.display()),
			MB_OK | MB_ICONINFORMATION,
		),
		Err(error) => report_user_error(
			window,
			runtime,
			format!(
				"Local data deletion did not complete. Some files may already have been removed.\n\n{error}"
			),
		),
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

#[cfg(test)]
mod tests {
	use std::{
		fs,
		path::PathBuf,
		sync::atomic::{AtomicUsize, Ordering},
		time::{SystemTime, UNIX_EPOCH},
	};

	use base64::{Engine, engine::general_purpose::STANDARD};

	use super::embed_current_images;
	use crate::{
		app::Application,
		image_file::{ImageData, load_from_bytes},
		presets::PresetFile,
	};

	static NEXT_TEST_FOLDER: AtomicUsize = AtomicUsize::new(0);

	struct TemporaryFolder(PathBuf);

	impl TemporaryFolder {
		fn new() -> Self {
			let unique = NEXT_TEST_FOLDER.fetch_add(1, Ordering::Relaxed);
			let timestamp = SystemTime::now()
				.duration_since(UNIX_EPOCH)
				.map_or(0, |value| value.as_nanos());
			let path = std::env::temp_dir().join(format!(
				"hoverover-preset-image-tests-{}-{timestamp}-{unique}",
				std::process::id()
			));
			assert!(fs::create_dir_all(&path).is_ok());
			Self(path)
		}
	}

	impl Drop for TemporaryFolder {
		fn drop(&mut self) {
			let _ = fs::remove_dir_all(&self.0);
		}
	}

	fn test_image(red: u8, green: u8, blue: u8) -> ImageData {
		ImageData {
			width: 1,
			height: 1,
			rgba_pixels: vec![red, green, blue, 255],
		}
	}

	fn preset_from_application(application: &Application) -> PresetFile {
		let placements = application.current_placements();
		let mut preset = PresetFile::from_placements(&placements, 100, 100);
		assert!(embed_current_images(application, &mut preset).is_ok());
		preset
	}

	fn serialized_image_pixels(preset: &PresetFile, image_index: usize) -> Vec<u8> {
		let json = serde_json::to_string(preset);
		assert!(json.is_ok());
		let Ok(json) = json else {
			return Vec::new();
		};
		let serialized_preset: Result<PresetFile, _> = serde_json::from_str(&json);
		assert!(serialized_preset.is_ok());
		let Ok(serialized_preset) = serialized_preset else {
			return Vec::new();
		};
		let Some(encoded_image) = serialized_preset.images.get(image_index) else {
			return Vec::new();
		};
		let image_bytes = STANDARD.decode(&encoded_image.image_data_base64);
		assert!(image_bytes.is_ok());
		let Ok(image_bytes) = image_bytes else {
			return Vec::new();
		};
		let image = load_from_bytes(&image_bytes);
		assert!(image.is_ok());
		image.map_or_else(|_| Vec::new(), |image| image.rgba_pixels)
	}

	#[test]
	fn filename_reuse_after_local_data_clear_keeps_each_images_own_pixels_in_the_preset() {
		let temporary = TemporaryFolder::new();
		let image_folder = temporary.0.join("HoverOver").join("images");
		let reused_image_path = image_folder.join("image.png");
		let first_image_bytes = crate::image_file::encode_png(&test_image(255, 0, 0));
		assert!(first_image_bytes.is_ok());
		let first_source_path = temporary.0.join("image.png");
		assert!(
			fs::write(
				&first_source_path,
				first_image_bytes.as_deref().unwrap_or_default()
			)
			.is_ok()
		);
		let first_stored_name =
			crate::image_store::import_image_into(&image_folder, &first_source_path);
		assert!(first_stored_name.is_ok());
		let Ok(first_stored_name) = first_stored_name else {
			return;
		};
		assert_eq!(first_stored_name, "image.png");
		let first_image = crate::image_file::load(&reused_image_path);
		assert!(first_image.is_ok());
		let Ok(first_image) = first_image else {
			return;
		};

		let mut application = Application::new(100, 100);
		assert!(
			application
				.add_image(first_image, first_stored_name)
				.is_ok()
		);

		// Simulate clearing local data: remove backing files but keep the scene in memory.
		assert!(fs::remove_dir_all(temporary.0.join("HoverOver")).is_ok());
		let second_image_bytes = crate::image_file::encode_png(&test_image(0, 0, 255));
		assert!(second_image_bytes.is_ok());
		let second_source_path = temporary.0.join("replacement").join("image.png");
		assert!(
			second_source_path
				.parent()
				.is_some_and(|path| fs::create_dir_all(path).is_ok())
		);
		assert!(
			fs::write(
				&second_source_path,
				second_image_bytes.as_deref().unwrap_or_default()
			)
			.is_ok()
		);
		let second_stored_name =
			crate::image_store::import_image_into(&image_folder, &second_source_path);
		assert!(second_stored_name.is_ok());
		let Ok(second_stored_name) = second_stored_name else {
			return;
		};
		assert_eq!(second_stored_name, "image.png");
		let second_image = crate::image_file::load(&reused_image_path);
		assert!(second_image.is_ok());
		let Ok(second_image) = second_image else {
			return;
		};
		assert!(
			application
				.add_image(second_image, second_stored_name)
				.is_ok()
		);

		let placements_before_saving = application.current_placements();
		let preset = preset_from_application(&application);
		let preset_path = temporary.0.join("saved-preset.json");
		assert!(crate::presets::save_preset_file(&preset_path, &preset).is_ok());
		let saved_preset = crate::presets::load_preset_file(&preset_path);
		assert!(saved_preset.is_ok());
		let Ok(saved_preset) = saved_preset else {
			return;
		};
		assert_eq!(
			serialized_image_pixels(&saved_preset, 0),
			vec![255, 0, 0, 255]
		);
		assert_eq!(
			serialized_image_pixels(&saved_preset, 1),
			vec![0, 0, 255, 255]
		);
		assert_eq!(application.current_placements(), placements_before_saving);
		assert_eq!(
			application
				.image_png_bytes(0)
				.and_then(Result::ok)
				.and_then(|bytes| load_from_bytes(&bytes).ok())
				.map(|image| image.rgba_pixels),
			Some(vec![255, 0, 0, 255])
		);
		assert_eq!(
			application
				.image_png_bytes(1)
				.and_then(Result::ok)
				.and_then(|bytes| load_from_bytes(&bytes).ok())
				.map(|image| image.rgba_pixels),
			Some(vec![0, 0, 255, 255])
		);
	}

	#[test]
	fn preset_saving_still_embeds_images_with_valid_backing_files() {
		let temporary = TemporaryFolder::new();
		let image_folder = temporary.0.join("images");
		let backing_file = image_folder.join("ordinary.png");
		let backing_bytes = crate::image_file::encode_png(&test_image(20, 180, 40));
		assert!(backing_bytes.is_ok());
		let source_path = temporary.0.join("ordinary.png");
		assert!(fs::write(&source_path, backing_bytes.as_deref().unwrap_or_default()).is_ok());
		let stored_file_name = crate::image_store::import_image_into(&image_folder, &source_path);
		assert!(stored_file_name.is_ok());
		let Ok(stored_file_name) = stored_file_name else {
			return;
		};
		let loaded_image = crate::image_file::load(&backing_file);
		assert!(loaded_image.is_ok());
		let Ok(loaded_image) = loaded_image else {
			return;
		};

		let mut application = Application::new(100, 100);
		assert!(
			application
				.add_image(loaded_image, stored_file_name)
				.is_ok()
		);
		let preset = preset_from_application(&application);
		let preset_path = temporary.0.join("ordinary-preset.json");
		assert!(crate::presets::save_preset_file(&preset_path, &preset).is_ok());
		let saved_preset = crate::presets::load_preset_file(&preset_path);
		assert!(saved_preset.is_ok());
		let Ok(saved_preset) = saved_preset else {
			return;
		};
		assert_eq!(
			serialized_image_pixels(&saved_preset, 0),
			vec![20, 180, 40, 255]
		);
		assert!(backing_file.exists());
	}
}
