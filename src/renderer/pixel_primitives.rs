//! Pixel-level fills, lines, rectangles, and alpha blending.

use super::Canvas;

pub(super) fn fill_region(canvas: &mut Canvas, color: [u8; 4]) {
	for y in canvas.clip.top..canvas.clip.bottom {
		let row_start = ((y as u32 * canvas.width + canvas.clip.left as u32) * 4) as usize;
		let row_end = ((y as u32 * canvas.width + canvas.clip.right as u32) * 4) as usize;
		for pixel in canvas.pixels[row_start..row_end].chunks_exact_mut(4) {
			pixel.copy_from_slice(&color);
		}
	}
}

pub(super) fn draw_handle(
	canvas: &mut Canvas,
	center_x: f32,
	center_y: f32,
	size: i32,
	fill_color: [u8; 4],
	border_color: [u8; 4],
) {
	let left = center_x.round() as i32 - size / 2;
	let top = center_y.round() as i32 - size / 2;
	canvas.fill_rect(left, top, size, size, fill_color);
	canvas.draw_rect_outline(left, top, size, size, border_color);
}

pub(super) fn draw_filled_rect(
	canvas: &mut Canvas,
	left: i32,
	top: i32,
	width: i32,
	height: i32,
	color: [u8; 4],
) {
	for y in top..top + height {
		for x in left..left + width {
			canvas.blend_pixel(x, y, color);
		}
	}
}

pub(super) fn draw_rect_outline(
	canvas: &mut Canvas,
	left: i32,
	top: i32,
	width: i32,
	height: i32,
	color: [u8; 4],
) {
	canvas.draw_line(
		(left as f32, top as f32),
		((left + width) as f32, top as f32),
		color,
	);
	canvas.draw_line(
		((left + width) as f32, top as f32),
		((left + width) as f32, (top + height) as f32),
		color,
	);
	canvas.draw_line(
		((left + width) as f32, (top + height) as f32),
		(left as f32, (top + height) as f32),
		color,
	);
	canvas.draw_line(
		(left as f32, (top + height) as f32),
		(left as f32, top as f32),
		color,
	);
}

pub(super) fn draw_line(canvas: &mut Canvas, start: (f32, f32), end: (f32, f32), color: [u8; 4]) {
	let delta_x = end.0 - start.0;
	let delta_y = end.1 - start.1;
	let steps = delta_x.abs().max(delta_y.abs()).ceil().max(1.0) as i32;
	for step in 0..=steps {
		let amount = step as f32 / steps as f32;
		canvas.blend_pixel(
			(start.0 + delta_x * amount).round() as i32,
			(start.1 + delta_y * amount).round() as i32,
			color,
		);
	}
}

pub(super) fn blend_pixel(canvas: &mut Canvas, x: i32, y: i32, source: [u8; 4]) {
	if x < canvas.clip.left
		|| y < canvas.clip.top
		|| x >= canvas.clip.right
		|| y >= canvas.clip.bottom
		|| x < 0
		|| y < 0
		|| x >= canvas.width as i32
		|| y >= canvas.height as i32
		|| source[3] == 0
	{
		return;
	}
	let offset = ((y as u32 * canvas.width + x as u32) * 4) as usize;
	let destination = &mut canvas.pixels[offset..offset + 4];
	if source[3] == 255 {
		destination.copy_from_slice(&source);
		return;
	}
	let source_alpha = u32::from(source[3]);
	if destination[3] == 255 {
		let inverse_alpha = 255 - source_alpha;
		for channel in 0..3 {
			destination[channel] = ((u32::from(source[channel]) * source_alpha
				+ u32::from(destination[channel]) * inverse_alpha
				+ 127)
				/ 255) as u8;
		}
		return;
	}
	let source_alpha = source[3] as f32 / 255.0;
	let destination_alpha = destination[3] as f32 / 255.0;
	let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
	if output_alpha <= 0.0 {
		return;
	}
	for channel in 0..3 {
		let source_color = source[channel] as f32 / 255.0;
		let destination_color = destination[channel] as f32 / 255.0;
		let output_color = (source_color * source_alpha
			+ destination_color * destination_alpha * (1.0 - source_alpha))
			/ output_alpha;
		destination[channel] = (output_color * 255.0).round() as u8;
	}
	destination[3] = (output_alpha * 255.0).round() as u8;
}
