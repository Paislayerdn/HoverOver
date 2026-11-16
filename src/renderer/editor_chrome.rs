//! Drawing the editor toolbar, quit button, and background dots.

use crate::model::DirtyRect;

use super::{
	Canvas, QUIT_BUTTON_SIZE, TOOLBAR_HEIGHT, TOOLBAR_WIDTH, quit_button_bounds, toolbar_bounds,
};

// Tune these values to change the grid spacing and cursor-radius falloff.
const DOT_SPACING: i32 = 16;
const DOT_MAX_RADIUS: f32 = 6.5;
const DOT_MIN_RADIUS: f32 = 2.25;
const DOT_EFFECT_DISTANCE: f32 = 152.0;

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

pub(super) fn draw_mode_label(canvas: &mut Canvas) {
	const LABEL: &str = "Hover Over: Editing mode";
	const PIXEL_SIZE: i32 = 2;
	const CHARACTER_WIDTH: i32 = 5;
	const CHARACTER_SPACING: i32 = 1;
	const PADDING_X: i32 = 10;
	const PADDING_Y: i32 = 6;
	let text_width = (LABEL.chars().count() as i32 * (CHARACTER_WIDTH + CHARACTER_SPACING)
		- CHARACTER_SPACING)
		* PIXEL_SIZE;
	let text_height = 7 * PIXEL_SIZE;
	let label_width = text_width + PADDING_X * 2;
	let label_height = text_height + PADDING_Y * 2;
	let label_left = (canvas.width as i32 - label_width) / 2;
	let label_top = 12;

	canvas.fill_rect(
		label_left,
		label_top,
		label_width,
		label_height,
		[20, 20, 20, 225],
	);
	canvas.draw_rect_outline(
		label_left,
		label_top,
		label_width,
		label_height,
		[230, 230, 230, 230],
	);

	let mut character_left = label_left + PADDING_X;
	for character in LABEL.chars() {
		for (row_index, row_bits) in glyph_rows(character).into_iter().enumerate() {
			for column in 0..CHARACTER_WIDTH {
				if row_bits & (1 << (CHARACTER_WIDTH - 1 - column)) != 0 {
					canvas.fill_rect(
						character_left + column * PIXEL_SIZE,
						label_top + PADDING_Y + row_index as i32 * PIXEL_SIZE,
						PIXEL_SIZE,
						PIXEL_SIZE,
						[255, 255, 255, 255],
					);
				}
			}
		}
		character_left += (CHARACTER_WIDTH + CHARACTER_SPACING) * PIXEL_SIZE;
	}
}

fn glyph_rows(character: char) -> [u8; 7] {
	match character {
		'H' => [
			0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
		],
		'O' => [
			0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
		],
		'E' => [
			0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
		],
		'o' => [
			0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
		],
		'v' => [
			0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
		],
		'e' => [
			0b01110, 0b10001, 0b10001, 0b11111, 0b10000, 0b10001, 0b01110,
		],
		'r' => [
			0b10110, 0b11001, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000,
		],
		':' => [
			0b00000, 0b00100, 0b00100, 0b00000, 0b00100, 0b00100, 0b00000,
		],
		'd' => [
			0b00001, 0b00001, 0b01111, 0b10001, 0b10001, 0b10011, 0b01101,
		],
		'i' => [
			0b00100, 0b00000, 0b01100, 0b00100, 0b00100, 0b00100, 0b01110,
		],
		't' => [
			0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0b00101, 0b00010,
		],
		'n' => [
			0b00000, 0b00000, 0b10110, 0b11001, 0b10001, 0b10001, 0b10001,
		],
		'g' => [
			0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b10001, 0b01110,
		],
		'm' => [
			0b00000, 0b00000, 0b11011, 0b10101, 0b10101, 0b10101, 0b10101,
		],
		_ => [0; 7],
	}
}

pub(super) fn dot_effect_bounds(x: f32, y: f32) -> DirtyRect {
	let effect_extent = DOT_EFFECT_DISTANCE + DOT_MAX_RADIUS;
	DirtyRect {
		left: (x - effect_extent).floor() as i32,
		top: (y - effect_extent).floor() as i32,
		right: (x + effect_extent).ceil() as i32,
		bottom: (y + effect_extent).ceil() as i32,
	}
}

pub(super) fn draw_dot_pattern(canvas: &mut Canvas, cursor_position: Option<(f32, f32)>) {
	let clip = canvas.clip;
	let maximum_radius_in_pixels = DOT_MAX_RADIUS.ceil() as i32;
	let mut center_y =
		8 + (clip.top - maximum_radius_in_pixels - 8).div_euclid(DOT_SPACING) * DOT_SPACING;
	while center_y < clip.bottom + maximum_radius_in_pixels {
		let mut center_x =
			8 + (clip.left - maximum_radius_in_pixels - 8).div_euclid(DOT_SPACING) * DOT_SPACING;
		while center_x < clip.right + maximum_radius_in_pixels {
			let radius = radius_for_dot(center_x as f32, center_y as f32, cursor_position);
			for offset_y in -maximum_radius_in_pixels..=maximum_radius_in_pixels {
				for offset_x in -maximum_radius_in_pixels..=maximum_radius_in_pixels {
					if (offset_x * offset_x + offset_y * offset_y) as f32 <= radius * radius {
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

fn radius_for_dot(
	dot_center_x: f32,
	dot_center_y: f32,
	cursor_position: Option<(f32, f32)>,
) -> f32 {
	let Some((cursor_x, cursor_y)) = cursor_position else {
		return DOT_MIN_RADIUS;
	};

	let distance_x = dot_center_x - cursor_x;
	let distance_y = dot_center_y - cursor_y;
	let distance = (distance_x * distance_x + distance_y * distance_y).sqrt();
	let influence = (1.0 - distance / DOT_EFFECT_DISTANCE).clamp(0.0, 1.0);
	DOT_MIN_RADIUS + (DOT_MAX_RADIUS - DOT_MIN_RADIUS) * influence
}

#[cfg(test)]
mod tests {
	use super::{DOT_EFFECT_DISTANCE, DOT_MAX_RADIUS, DOT_MIN_RADIUS, radius_for_dot};

	#[test]
	fn circle_radius_fades_from_the_cursor_to_the_minimum() {
		let maximum_radius = radius_for_dot(20.0, 30.0, Some((20.0, 30.0)));
		let half_distance_radius =
			radius_for_dot(20.0 + DOT_EFFECT_DISTANCE / 2.0, 30.0, Some((20.0, 30.0)));
		let minimum_radius = radius_for_dot(20.0 + DOT_EFFECT_DISTANCE, 30.0, Some((20.0, 30.0)));
		assert_eq!(maximum_radius, DOT_MAX_RADIUS);
		assert_eq!(
			half_distance_radius,
			(DOT_MAX_RADIUS + DOT_MIN_RADIUS) / 2.0
		);
		assert_eq!(minimum_radius, DOT_MIN_RADIUS);
		assert_eq!(radius_for_dot(20.0, 30.0, None), DOT_MIN_RADIUS);
	}
}
