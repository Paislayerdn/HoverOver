//! Drawing the editor toolbar, quit button, and background dots.

use super::{
	Canvas, DOT_RADIUS, DOT_SPACING, QUIT_BUTTON_SIZE, TOOLBAR_HEIGHT, TOOLBAR_WIDTH,
	quit_button_bounds, toolbar_bounds,
};

pub(super) fn draw_add_button(canvas: &mut Canvas) {
	let bounds = toolbar_bounds(canvas.width, canvas.height);
	canvas.fill_rect(
		bounds.left,
		bounds.top,
		TOOLBAR_WIDTH,
		TOOLBAR_HEIGHT,
		[255, 255, 255, 225],
	);
	canvas.draw_rect_outline(
		bounds.left,
		bounds.top,
		TOOLBAR_WIDTH,
		TOOLBAR_HEIGHT,
		[90, 90, 90, 240],
	);
	let center_x = canvas.width as i32 / 2;
	let center_y = bounds.top + TOOLBAR_HEIGHT / 2;
	canvas.fill_rect(center_x - 1, center_y - 11, 3, 23, [35, 35, 35, 255]);
	canvas.fill_rect(center_x - 11, center_y - 1, 23, 3, [35, 35, 35, 255]);
}

pub(super) fn draw_quit_button(canvas: &mut Canvas) {
	let bounds = quit_button_bounds(canvas.width, canvas.height);
	canvas.fill_rect(
		bounds.left,
		bounds.top,
		QUIT_BUTTON_SIZE,
		QUIT_BUTTON_SIZE,
		[255, 255, 255, 235],
	);
	canvas.draw_rect_outline(
		bounds.left,
		bounds.top,
		QUIT_BUTTON_SIZE,
		QUIT_BUTTON_SIZE,
		[65, 65, 65, 255],
	);
	// A small bitmap letter keeps the prototype independent of system fonts.
	let q_rows = [
		0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
	];
	let pixel_size = 2;
	let glyph_left = bounds.left + (QUIT_BUTTON_SIZE - 5 * pixel_size) / 2;
	let glyph_top = bounds.top + (QUIT_BUTTON_SIZE - 7 * pixel_size) / 2;
	for (row, bits) in q_rows.into_iter().enumerate() {
		for column in 0..5 {
			if bits & (1 << (4 - column)) != 0 {
				canvas.fill_rect(
					glyph_left + column * pixel_size,
					glyph_top + row as i32 * pixel_size,
					pixel_size,
					pixel_size,
					[25, 25, 25, 255],
				);
			}
		}
	}
}

pub(super) fn draw_dot_pattern(canvas: &mut Canvas) {
	let clip = canvas.clip;
	let mut center_y = 8 + (clip.top - DOT_RADIUS - 8).div_euclid(DOT_SPACING) * DOT_SPACING;
	while center_y < clip.bottom + DOT_RADIUS {
		let mut center_x = 8 + (clip.left - DOT_RADIUS - 8).div_euclid(DOT_SPACING) * DOT_SPACING;
		while center_x < clip.right + DOT_RADIUS {
			for offset_y in -DOT_RADIUS..=DOT_RADIUS {
				for offset_x in -DOT_RADIUS..=DOT_RADIUS {
					if offset_x * offset_x + offset_y * offset_y <= DOT_RADIUS * DOT_RADIUS {
						canvas.blend_pixel(
							center_x + offset_x,
							center_y + offset_y,
							[255, 255, 255, 85],
						);
					}
				}
			}
			center_x += DOT_SPACING;
		}
		center_y += DOT_SPACING;
	}
}
