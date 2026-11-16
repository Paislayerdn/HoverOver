//! Software drawing for the desktop overlay and editing controls.
//! Win32 only receives the finished RGBA pixels from this module.

use crate::{
	geometry,
	image_file::ImageData,
	model::{DirtyRect, ImageObject, Mode},
};

mod editor_chrome;
mod image_drawing;
mod pixel_primitives;

pub(crate) const ROTATION_HANDLE_GAP: f32 = 32.0;
pub(crate) const TOOLBAR_WIDTH: i32 = 180;
pub(crate) const TOOLBAR_HEIGHT: i32 = 58;
pub(crate) const TOOLBAR_BOTTOM_GAP: i32 = 28;
pub(crate) const QUIT_BUTTON_SIZE: i32 = 40;
pub(crate) const QUIT_BUTTON_RIGHT_GAP: i32 = 28;
pub(crate) const QUIT_BUTTON_BOTTOM_GAP: i32 = 28;

pub(crate) fn toolbar_bounds(width: u32, height: u32) -> DirtyRect {
	let left = (width as i32 - TOOLBAR_WIDTH) / 2;
	let top = height as i32 - TOOLBAR_BOTTOM_GAP - TOOLBAR_HEIGHT;
	DirtyRect {
		left,
		top,
		right: left + TOOLBAR_WIDTH,
		bottom: top + TOOLBAR_HEIGHT,
	}
}

pub(crate) fn quit_button_bounds(width: u32, height: u32) -> DirtyRect {
	let left = width as i32 - QUIT_BUTTON_RIGHT_GAP - QUIT_BUTTON_SIZE;
	let top = height as i32 - QUIT_BUTTON_BOTTOM_GAP - QUIT_BUTTON_SIZE;
	DirtyRect {
		left,
		top,
		right: left + QUIT_BUTTON_SIZE,
		bottom: top + QUIT_BUTTON_SIZE,
	}
}

pub(super) struct Canvas<'a> {
	pub(super) pixels: &'a mut [u8],
	pub(super) width: u32,
	pub(super) height: u32,
	pub(super) clip: DirtyRect,
}

impl Canvas<'_> {
	fn fill_region(&mut self, color: [u8; 4]) {
		pixel_primitives::fill_region(self, color);
	}

	fn blend_pixel(&mut self, x: i32, y: i32, color: [u8; 4]) {
		pixel_primitives::blend_pixel(self, x, y, color);
	}

	fn fill_rect(&mut self, left: i32, top: i32, width: i32, height: i32, color: [u8; 4]) {
		pixel_primitives::draw_filled_rect(self, left, top, width, height, color);
	}

	fn draw_rect_outline(&mut self, left: i32, top: i32, width: i32, height: i32, color: [u8; 4]) {
		pixel_primitives::draw_rect_outline(self, left, top, width, height, color);
	}

	fn draw_line(&mut self, start: (f32, f32), end: (f32, f32), color: [u8; 4]) {
		pixel_primitives::draw_line(self, start, end, color);
	}

	fn draw_handle(
		&mut self,
		center_x: f32,
		center_y: f32,
		size: i32,
		fill_color: [u8; 4],
		border_color: [u8; 4],
	) {
		pixel_primitives::draw_handle(self, center_x, center_y, size, fill_color, border_color);
	}

	fn draw_dot_pattern(&mut self, cursor_position: Option<(f32, f32)>) {
		editor_chrome::draw_dot_pattern(self, cursor_position);
	}

	fn draw_image(&mut self, object: &ImageObject) {
		image_drawing::draw_image(self, object);
	}

	fn draw_selection(&mut self, object: &ImageObject) {
		image_drawing::draw_selection(self, object);
	}

	fn draw_add_button(&mut self) {
		editor_chrome::draw_add_button(self);
	}

	fn draw_quit_button(&mut self) {
		editor_chrome::draw_quit_button(self);
	}

	fn draw_mode_label(&mut self) {
		editor_chrome::draw_mode_label(self);
	}
}

pub(crate) fn render(
	screen_width: u32,
	screen_height: u32,
	mode: Mode,
	images: &[ImageObject],
	selected_image: Option<usize>,
	cursor_position: Option<(f32, f32)>,
) -> ImageData {
	let pixels = vec![0; (screen_width * screen_height * 4) as usize];
	let mut frame = ImageData {
		width: screen_width,
		height: screen_height,
		rgba_pixels: pixels,
	};
	render_region(
		&mut frame,
		mode,
		images,
		selected_image,
		cursor_position,
		DirtyRect {
			left: 0,
			top: 0,
			right: screen_width as i32,
			bottom: screen_height as i32,
		},
	);
	frame
}

pub(crate) fn render_region(
	frame: &mut ImageData,
	mode: Mode,
	images: &[ImageObject],
	selected_image: Option<usize>,
	cursor_position: Option<(f32, f32)>,
	requested_region: DirtyRect,
) {
	let Some(clip) = clip_to_canvas(requested_region, frame.width, frame.height) else {
		return;
	};
	let background = if mode == Mode::Editing {
		[255, 255, 255, 26]
	} else {
		[0, 0, 0, 0]
	};
	let canvas_width = frame.width;
	let canvas_height = frame.height;
	let mut canvas = Canvas {
		pixels: &mut frame.rgba_pixels,
		width: canvas_width,
		height: canvas_height,
		clip,
	};
	canvas.fill_region(background);

	if mode == Mode::Editing {
		canvas.draw_dot_pattern(cursor_position);
	}

	for object in images {
		canvas.draw_image(object);
	}

	if mode == Mode::Editing {
		if let Some(index) = selected_image {
			if let Some(object) = images.get(index) {
				canvas.draw_selection(object);
			}
		} else {
			canvas.draw_add_button();
		}
		canvas.draw_quit_button();
		canvas.draw_mode_label();
	}
}

pub(crate) fn dot_effect_bounds(x: f32, y: f32) -> DirtyRect {
	editor_chrome::dot_effect_bounds(x, y)
}

pub(crate) fn object_visual_bounds(object: &ImageObject) -> DirtyRect {
	let outline = geometry::image_outline_bounds(
		object.placement.center_x,
		object.placement.center_y,
		object.placement.width,
		object.placement.height,
		object.placement.rotation_degrees,
	);
	let mut bounds = DirtyRect {
		left: outline.left.floor() as i32,
		top: outline.top.floor() as i32,
		right: outline.right.ceil() as i32,
		bottom: outline.bottom.ceil() as i32,
	};

	for (x, y) in selection_handle_points(object) {
		bounds.left = bounds.left.min(x.floor() as i32 - 7);
		bounds.top = bounds.top.min(y.floor() as i32 - 7);
		bounds.right = bounds.right.max(x.ceil() as i32 + 8);
		bounds.bottom = bounds.bottom.max(y.ceil() as i32 + 8);
	}

	bounds
}

pub(crate) fn clip_to_canvas(region: DirtyRect, width: u32, height: u32) -> Option<DirtyRect> {
	let clipped = DirtyRect {
		left: region.left.max(0),
		top: region.top.max(0),
		right: region.right.min(width as i32),
		bottom: region.bottom.min(height as i32),
	};
	(clipped.left < clipped.right && clipped.top < clipped.bottom).then_some(clipped)
}

pub(crate) fn selection_handle_points(object: &ImageObject) -> [(f32, f32); 9] {
	let directions = geometry::handle_directions();
	let mut points = [(0.0, 0.0); 9];
	for (index, (horizontal, vertical)) in directions.into_iter().enumerate() {
		points[index] = geometry::local_to_screen(
			object.placement.center_x,
			object.placement.center_y,
			object.placement.rotation_degrees,
			horizontal as f32 * object.placement.width / 2.0,
			vertical as f32 * object.placement.height / 2.0,
		);
	}
	points[8] = geometry::local_to_screen(
		object.placement.center_x,
		object.placement.center_y,
		object.placement.rotation_degrees,
		0.0,
		-object.placement.height / 2.0 - ROTATION_HANDLE_GAP,
	);
	points
}
