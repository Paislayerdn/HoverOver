//! Drawing and selecting image pixels.

use super::{Canvas, selection_handle_points};
use crate::model::ImageObject;

pub(super) fn draw_image(canvas: &mut Canvas, object: &ImageObject) {
	if object.placement.width < 1.0 || object.placement.height < 1.0 {
		return;
	}
	let radians = object.placement.rotation_degrees.to_radians();
	let cosine = radians.cos();
	let sine = radians.sin();
	let half_width =
		(cosine.abs() * object.placement.width + sine.abs() * object.placement.height) / 2.0;
	let half_height =
		(sine.abs() * object.placement.width + cosine.abs() * object.placement.height) / 2.0;
	let left = (object.placement.center_x - half_width)
		.floor()
		.max(canvas.clip.left as f32) as u32;
	let top = (object.placement.center_y - half_height)
		.floor()
		.max(canvas.clip.top as f32) as u32;
	let right = (object.placement.center_x + half_width)
		.ceil()
		.min(canvas.width.min(canvas.clip.right as u32) as f32) as u32;
	let bottom = (object.placement.center_y + half_height)
		.ceil()
		.min(canvas.height.min(canvas.clip.bottom as u32) as f32) as u32;
	for y in top..bottom {
		for x in left..right {
			let offset_x = x as f32 + 0.5 - object.placement.center_x;
			let offset_y = y as f32 + 0.5 - object.placement.center_y;
			let mut local_x = offset_x * cosine + offset_y * sine;
			let mut local_y = -offset_x * sine + offset_y * cosine;
			if object.placement.flip_x {
				local_x = -local_x;
			}
			if object.placement.flip_y {
				local_y = -local_y;
			}
			if local_x.abs() > object.placement.width / 2.0
				|| local_y.abs() > object.placement.height / 2.0
			{
				continue;
			}
			let source_x = ((local_x / object.placement.width + 0.5) * object.image.width as f32)
				.floor()
				.clamp(0.0, object.image.width.saturating_sub(1) as f32)
				as u32;
			let source_y = ((local_y / object.placement.height + 0.5) * object.image.height as f32)
				.floor()
				.clamp(0.0, object.image.height.saturating_sub(1) as f32)
				as u32;
			let source_offset = ((source_y * object.image.width + source_x) * 4) as usize;
			let pixel = &object.image.rgba_pixels[source_offset..source_offset + 4];
			canvas.blend_pixel(x as i32, y as i32, [pixel[0], pixel[1], pixel[2], pixel[3]]);
		}
	}
}

pub(super) fn draw_selection(canvas: &mut Canvas, object: &ImageObject) {
	let handles = selection_handle_points(object);
	let outline = [40, 40, 40, 255];
	for edge in 0..4 {
		let first = handles[edge * 2];
		let next_corner = ((edge + 1) % 4) * 2;
		let second = handles[next_corner];
		canvas.draw_line(first, second, outline);
	}
	canvas.draw_line(handles[1], handles[8], outline);
	for (x, y) in &handles[..8] {
		canvas.draw_handle(*x, *y, 8, [255, 255, 255, 255], outline);
	}
	canvas.draw_handle(
		handles[8].0,
		handles[8].1,
		10,
		[255, 255, 255, 255],
		outline,
	);
}
