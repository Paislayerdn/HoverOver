//! Product behavior and image arrangement. This module has no Win32 handles.

use crate::{
	geometry::{self, local_to_screen, rotate},
	image_file::ImageData,
	model::{DirtyRect, ImageObject, ImagePlacement, Mode, clamp_opacity_percent},
	renderer, snapping,
};

const MIN_IMAGE_SIZE: f32 = 12.0;
const PANEL_Y_DIRECTION: i32 = 1;

pub enum MouseAction {
	None,
	AddImage,
	Quit,
}

#[derive(Clone, Copy)]
pub(crate) enum MenuAction {
	Cancel,
	Delete,
	FlipHorizontal,
	FlipVertical,
	SetProperties,
}

#[derive(Clone, Copy)]
enum DragKind {
	Move,
	Resize(usize),
	Rotate,
}

struct Drag {
	image_index: usize,
	kind: DragKind,
	start_mouse_x: f32,
	start_mouse_y: f32,
	start_center_x: f32,
	start_center_y: f32,
	start_width: f32,
	start_height: f32,
	start_rotation: f32,
	start_local_x: f32,
	start_local_y: f32,
	start_pointer_angle: f32,
}

pub struct Application {
	pub mode: Mode,
	screen_width: u32,
	screen_height: u32,
	images: Vec<ImageObject>,
	selected_image: Option<usize>,
	drag: Option<Drag>,
	dirty_region: Option<DirtyRect>,
	cursor_position: Option<(f32, f32)>,
}

impl Application {
	pub fn new(screen_width: u32, screen_height: u32) -> Self {
		Self {
			mode: Mode::Editing,
			screen_width,
			screen_height,
			images: Vec::new(),
			selected_image: None,
			drag: None,
			dirty_region: None,
			cursor_position: None,
		}
	}

	pub fn toggle_mode(&mut self) {
		self.drag = None;
		self.selected_image = None;
		self.mode = match self.mode {
			Mode::Display => Mode::Editing,
			Mode::Editing => Mode::Display,
		};
		self.mark_full_redraw();
	}

	pub fn enter_editing_mode(&mut self) -> bool {
		if self.mode == Mode::Editing {
			return false;
		}

		self.drag = None;
		self.selected_image = None;
		self.mode = Mode::Editing;
		self.mark_full_redraw();
		true
	}

	pub fn add_image(&mut self, image: ImageData, source_file_name: String) {
		let width = image.width as f32;
		let height = image.height as f32;
		let image_index = self.images.len();

		self.images.push(ImageObject {
			image,
			placement: ImagePlacement {
				source_file_name,
				center_x: self.screen_width as f32 / 2.0,
				center_y: self.screen_height as f32 / 2.0,
				width,
				height,
				rotation_degrees: 0.0,
				flip_x: false,
				flip_y: false,
				opacity_percent: 100,
			},
		});
		self.selected_image = Some(image_index);
		self.add_dirty_region(renderer::object_visual_bounds(&self.images[image_index]));
		self.add_dirty_region(renderer::toolbar_bounds(
			self.screen_width,
			self.screen_height,
		));
	}

	pub fn selected_panel_values(&self) -> Option<(i32, i32, u8)> {
		let image_index = self.selected_image?;
		let object = self.images.get(image_index)?;
		let x = (object.placement.center_x - self.screen_width as f32 / 2.0).round() as i32;
		let y = panel_y_from_screen_y(
			(object.placement.center_y - self.screen_height as f32 / 2.0).round() as i32,
		);
		Some((x, y, object.placement.opacity_percent))
	}

	pub fn set_selected_center(&mut self, x: i32, panel_y: i32) -> bool {
		self.set_selected_position_offset(x, screen_y_from_panel_y(panel_y))
	}

	pub fn set_selected_opacity(&mut self, value: i32) -> bool {
		self.set_selected_opacity_percent(value)
	}

	pub fn end_drag(&mut self) {
		self.drag = None;
	}

	pub fn is_dragging(&self) -> bool {
		self.drag.is_some()
	}

	pub fn set_selected_position_offset(&mut self, x: i32, y: i32) -> bool {
		let Some(image_index) = self.selected_image else {
			return false;
		};
		let Some(object) = self.images.get_mut(image_index) else {
			return false;
		};

		let old_bounds = renderer::object_visual_bounds(object);
		object.placement.center_x = self.screen_width as f32 / 2.0 + x as f32;
		object.placement.center_y = self.screen_height as f32 / 2.0 + y as f32;
		let new_bounds = renderer::object_visual_bounds(object);
		self.add_dirty_region(old_bounds.union(new_bounds));
		true
	}

	pub fn set_selected_opacity_percent(&mut self, value: i32) -> bool {
		let Some(image_index) = self.selected_image else {
			return false;
		};
		let Some(object) = self.images.get_mut(image_index) else {
			return false;
		};

		let opacity_percent = clamp_opacity_percent(value);
		if object.placement.opacity_percent == opacity_percent {
			return false;
		}
		let old_bounds = renderer::object_visual_bounds(object);
		object.placement.opacity_percent = opacity_percent;
		let new_bounds = renderer::object_visual_bounds(object);
		self.add_dirty_region(old_bounds.union(new_bounds));
		true
	}

	pub fn select_image_for_context_menu(&mut self, x: f32, y: f32) -> Option<(bool, bool)> {
		if self.mode != Mode::Editing {
			return None;
		}

		let image_index = self.hit_image(x, y)?;
		if self.selected_image != Some(image_index) {
			let mut dirty = renderer::object_visual_bounds(&self.images[image_index]);
			if let Some(previous_index) = self.selected_image {
				dirty = dirty.union(renderer::object_visual_bounds(&self.images[previous_index]));
			} else {
				dirty = dirty.union(renderer::toolbar_bounds(
					self.screen_width,
					self.screen_height,
				));
			}
			self.selected_image = Some(image_index);
			self.add_dirty_region(dirty);
		}

		let placement = &self.images[image_index].placement;
		Some((placement.flip_x, placement.flip_y))
	}

	pub fn apply_context_menu_action(&mut self, action: MenuAction) {
		let Some(image_index) = self.selected_image else {
			return;
		};

		match action {
			MenuAction::Delete => {
				let old_bounds = renderer::object_visual_bounds(&self.images[image_index]);
				self.images.remove(image_index);
				self.selected_image = None;
				self.drag = None;
				self.add_dirty_region(old_bounds.union(renderer::toolbar_bounds(
					self.screen_width,
					self.screen_height,
				)));
			}
			MenuAction::FlipHorizontal | MenuAction::FlipVertical => {
				let old_bounds = renderer::object_visual_bounds(&self.images[image_index]);
				let placement = &mut self.images[image_index].placement;
				match action {
					MenuAction::FlipHorizontal => placement.flip_x = !placement.flip_x,
					MenuAction::FlipVertical => placement.flip_y = !placement.flip_y,
					_ => {}
				}
				let new_bounds = renderer::object_visual_bounds(&self.images[image_index]);
				self.add_dirty_region(old_bounds.union(new_bounds));
			}
			MenuAction::Cancel | MenuAction::SetProperties => {}
		}
	}

	pub fn mouse_down(&mut self, x: f32, y: f32) -> MouseAction {
		if self.mode != Mode::Editing {
			return MouseAction::None;
		}

		if self.point_is_over_quit_button(x, y) {
			return MouseAction::Quit;
		}

		if self.selected_image.is_none() && self.point_is_over_add_button(x, y) {
			return MouseAction::AddImage;
		}

		if let Some(index) = self.selected_image {
			if let Some(kind) = self.hit_selected_controls(index, x, y) {
				self.start_drag(index, kind, x, y);
				return MouseAction::None;
			}
		}

		if let Some(index) = self.hit_image(x, y) {
			if self.selected_image != Some(index) {
				let mut dirty = renderer::object_visual_bounds(&self.images[index]);
				if let Some(previous) = self.selected_image {
					dirty = dirty.union(renderer::object_visual_bounds(&self.images[previous]));
				} else {
					dirty = dirty.union(renderer::toolbar_bounds(
						self.screen_width,
						self.screen_height,
					));
				}
				self.add_dirty_region(dirty);
			}
			self.selected_image = Some(index);
			self.start_drag(index, DragKind::Move, x, y);
			return MouseAction::None;
		}

		self.deselect_selected_image();
		MouseAction::None
	}

	pub fn deselect_selected_image(&mut self) -> bool {
		let Some(image_index) = self.selected_image else {
			self.drag = None;
			return false;
		};

		let dirty = renderer::object_visual_bounds(&self.images[image_index]).union(
			renderer::toolbar_bounds(self.screen_width, self.screen_height),
		);
		self.selected_image = None;
		self.drag = None;
		self.add_dirty_region(dirty);
		true
	}

	pub fn mouse_move(&mut self, x: f32, y: f32, shift_is_down: bool) -> bool {
		let mut cursor_moved = false;
		if self.mode == Mode::Editing && self.cursor_position != Some((x, y)) {
			if let Some((old_x, old_y)) = self.cursor_position {
				self.add_dirty_region(renderer::dot_effect_bounds(old_x, old_y));
			}
			self.cursor_position = Some((x, y));
			self.add_dirty_region(renderer::dot_effect_bounds(x, y));
			cursor_moved = true;
		}

		let Some(drag) = self.drag.as_ref() else {
			return cursor_moved;
		};
		let Some(object) = self.images.get_mut(drag.image_index) else {
			return false;
		};
		let old_bounds = renderer::object_visual_bounds(object);

		match drag.kind {
			DragKind::Move => {
				let wanted_center_x = drag.start_center_x + x - drag.start_mouse_x;
				let wanted_center_y = drag.start_center_y + y - drag.start_mouse_y;
				let outline = geometry::image_outline_bounds(
					wanted_center_x,
					wanted_center_y,
					object.placement.width,
					object.placement.height,
					object.placement.rotation_degrees,
				);
				let (snap_x, snap_y) = snapping::snap_offset(
					outline,
					self.screen_width as f32,
					self.screen_height as f32,
				);
				object.placement.center_x = wanted_center_x + snap_x;
				object.placement.center_y = wanted_center_y + snap_y;
			}
			DragKind::Rotate => {
				let current_pointer_angle = (y - drag.start_center_y)
					.atan2(x - drag.start_center_x)
					.to_degrees();
				let pointer_rotation = (current_pointer_angle - drag.start_pointer_angle + 540.0)
					.rem_euclid(360.0)
					- 180.0;
				let mut rotation = drag.start_rotation + pointer_rotation;
				if shift_is_down {
					rotation = (rotation / 45.0).round() * 45.0;
				}
				object.placement.rotation_degrees = rotation;
			}
			DragKind::Resize(handle_index) => {
				resize_image(object, drag, handle_index, x, y, shift_is_down);
			}
		}
		let new_bounds = renderer::object_visual_bounds(object);
		self.add_dirty_region(old_bounds.union(new_bounds));
		true
	}

	pub fn nudge_selected(&mut self, offset_x: i32, offset_y: i32) -> bool {
		if self.mode != Mode::Editing {
			return false;
		}
		let Some(image_index) = self.selected_image else {
			return false;
		};
		let Some(object) = self.images.get_mut(image_index) else {
			return false;
		};

		let old_bounds = renderer::object_visual_bounds(object);
		object.placement.center_x += offset_x as f32;
		object.placement.center_y += offset_y as f32;
		let new_bounds = renderer::object_visual_bounds(object);
		self.add_dirty_region(old_bounds.union(new_bounds));
		true
	}

	pub fn mouse_up(&mut self) {
		self.drag = None;
	}

	/// Builds the complete transparent desktop surface for the Windows backend.
	pub fn render(&self) -> ImageData {
		renderer::render(
			self.screen_width,
			self.screen_height,
			self.mode,
			&self.images,
			self.selected_image,
			self.cursor_position,
		)
	}

	pub fn take_dirty_region(&mut self) -> Option<DirtyRect> {
		self.dirty_region.take()
	}

	pub fn render_region(&self, frame: &mut ImageData, region: DirtyRect) {
		renderer::render_region(
			frame,
			self.mode,
			&self.images,
			self.selected_image,
			self.cursor_position,
			region,
		);
	}

	fn mark_full_redraw(&mut self) {
		self.add_dirty_region(DirtyRect::full(self.screen_width, self.screen_height));
	}

	fn add_dirty_region(&mut self, region: DirtyRect) {
		self.dirty_region = Some(match self.dirty_region {
			Some(previous) => previous.union(region),
			None => region,
		});
	}

	fn start_drag(&mut self, image_index: usize, kind: DragKind, x: f32, y: f32) {
		let object = &self.images[image_index];
		let (local_x, local_y) = screen_to_local(object, x, y);
		self.drag = Some(Drag {
			image_index,
			kind,
			start_mouse_x: x,
			start_mouse_y: y,
			start_center_x: object.placement.center_x,
			start_center_y: object.placement.center_y,
			start_width: object.placement.width,
			start_height: object.placement.height,
			start_rotation: object.placement.rotation_degrees,
			start_local_x: local_x,
			start_local_y: local_y,
			start_pointer_angle: (y - object.placement.center_y)
				.atan2(x - object.placement.center_x)
				.to_degrees(),
		});
	}

	fn hit_selected_controls(&self, index: usize, x: f32, y: f32) -> Option<DragKind> {
		let object = self.images.get(index)?;
		let handle_points = renderer::selection_handle_points(object);
		let (rotation_x, rotation_y) = handle_points[8];
		if distance_squared(x, y, rotation_x, rotation_y) <= 12.0_f32.powi(2) {
			return Some(DragKind::Rotate);
		}

		handle_points[..8]
			.iter()
			.position(|(handle_x, handle_y)| {
				distance_squared(x, y, *handle_x, *handle_y) <= 11.0_f32.powi(2)
			})
			.map(DragKind::Resize)
	}

	fn hit_image(&self, x: f32, y: f32) -> Option<usize> {
		self.images
			.iter()
			.enumerate()
			.rev()
			.find(|(_, object)| point_is_inside(object, x, y))
			.map(|(index, _)| index)
	}

	fn point_is_over_add_button(&self, x: f32, y: f32) -> bool {
		let bounds = renderer::toolbar_bounds(self.screen_width, self.screen_height);
		x >= bounds.left as f32
			&& x <= bounds.right as f32
			&& y >= bounds.top as f32
			&& y <= bounds.bottom as f32
	}

	fn point_is_over_quit_button(&self, x: f32, y: f32) -> bool {
		let bounds = renderer::quit_button_bounds(self.screen_width, self.screen_height);
		x >= bounds.left as f32
			&& x <= bounds.right as f32
			&& y >= bounds.top as f32
			&& y <= bounds.bottom as f32
	}
}

fn panel_y_from_screen_y(screen_y: i32) -> i32 {
	screen_y * PANEL_Y_DIRECTION
}

fn screen_y_from_panel_y(panel_y: i32) -> i32 {
	panel_y * PANEL_Y_DIRECTION
}

fn resize_image(
	object: &mut ImageObject,
	drag: &Drag,
	handle_index: usize,
	mouse_x: f32,
	mouse_y: f32,
	shift_is_down: bool,
) {
	let (horizontal_direction, vertical_direction) = geometry::handle_directions()[handle_index];
	let (local_x, local_y) = geometry::screen_to_local_at(
		drag.start_center_x,
		drag.start_center_y,
		drag.start_rotation,
		mouse_x,
		mouse_y,
	);
	let move_x = local_x - drag.start_local_x;
	let move_y = local_y - drag.start_local_y;

	let mut width = drag.start_width;
	let mut height = drag.start_height;

	if horizontal_direction != 0 {
		width = (drag.start_width + horizontal_direction as f32 * move_x).max(MIN_IMAGE_SIZE);
	}
	if vertical_direction != 0 {
		height = (drag.start_height + vertical_direction as f32 * move_y).max(MIN_IMAGE_SIZE);
	}

	if shift_is_down {
		let width_scale = width / drag.start_width;
		let height_scale = height / drag.start_height;
		let scale = if horizontal_direction == 0 {
			height_scale
		} else if vertical_direction == 0 {
			width_scale
		} else if (width_scale - 1.0).abs() >= (height_scale - 1.0).abs() {
			width_scale
		} else {
			height_scale
		}
		.max(MIN_IMAGE_SIZE / drag.start_width)
		.max(MIN_IMAGE_SIZE / drag.start_height);
		width = drag.start_width * scale;
		height = drag.start_height * scale;
	}

	let fixed_anchor_x = if horizontal_direction == 0 {
		0.0
	} else {
		-horizontal_direction as f32 * drag.start_width / 2.0
	};
	let fixed_anchor_y = if vertical_direction == 0 {
		0.0
	} else {
		-vertical_direction as f32 * drag.start_height / 2.0
	};
	let (anchor_world_x, anchor_world_y) = local_to_screen(
		drag.start_center_x,
		drag.start_center_y,
		drag.start_rotation,
		fixed_anchor_x,
		fixed_anchor_y,
	);

	let new_anchor_x = if horizontal_direction == 0 {
		0.0
	} else {
		-horizontal_direction as f32 * width / 2.0
	};
	let new_anchor_y = if vertical_direction == 0 {
		0.0
	} else {
		-vertical_direction as f32 * height / 2.0
	};
	let (anchor_offset_x, anchor_offset_y) =
		rotate(new_anchor_x, new_anchor_y, drag.start_rotation);

	object.placement.width = width;
	object.placement.height = height;
	object.placement.center_x = anchor_world_x - anchor_offset_x;
	object.placement.center_y = anchor_world_y - anchor_offset_y;
}

fn point_is_inside(object: &ImageObject, x: f32, y: f32) -> bool {
	let (local_x, local_y) = screen_to_local(object, x, y);
	local_x.abs() <= object.placement.width / 2.0 && local_y.abs() <= object.placement.height / 2.0
}

fn screen_to_local(object: &ImageObject, x: f32, y: f32) -> (f32, f32) {
	geometry::screen_to_local_at(
		object.placement.center_x,
		object.placement.center_y,
		object.placement.rotation_degrees,
		x,
		y,
	)
}

fn distance_squared(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
	(x1 - x2).powi(2) + (y1 - y2).powi(2)
}

#[cfg(test)]
mod tests {
	use crate::image_file::ImageData;

	use super::{Application, panel_y_from_screen_y, screen_y_from_panel_y};

	#[test]
	fn new_images_use_zero_center_relative_position() {
		let mut application = Application::new(1920, 1080);
		application.add_image(
			ImageData {
				width: 2,
				height: 2,
				rgba_pixels: vec![255; 16],
			},
			"sample.png".to_string(),
		);

		assert_eq!(application.selected_panel_values(), Some((0, 0, 100)));
		assert!(application.set_selected_position_offset(-25, 40));
		assert_eq!(application.selected_panel_values(), Some((-25, 40, 100)));
		assert!(application.deselect_selected_image());
		assert_eq!(application.selected_panel_values(), None);
		assert!(!application.deselect_selected_image());
	}

	#[test]
	fn panel_y_conversion_preserves_the_existing_downward_positive_direction() {
		for screen_y in [-100, 0, 37] {
			assert_eq!(
				screen_y_from_panel_y(panel_y_from_screen_y(screen_y)),
				screen_y
			);
		}
	}
}
