//! Coordinate transforms shared by image interaction and drawing.

pub(crate) fn rotate(x: f32, y: f32, degrees: f32) -> (f32, f32) {
	let radians = degrees.to_radians();
	let cosine = radians.cos();
	let sine = radians.sin();
	(x * cosine - y * sine, x * sine + y * cosine)
}

pub(crate) fn local_to_screen(
	center_x: f32,
	center_y: f32,
	angle: f32,
	x: f32,
	y: f32,
) -> (f32, f32) {
	let (offset_x, offset_y) = rotate(x, y, angle);
	(center_x + offset_x, center_y + offset_y)
}

pub(crate) fn screen_to_local_at(
	center_x: f32,
	center_y: f32,
	angle: f32,
	x: f32,
	y: f32,
) -> (f32, f32) {
	rotate(x - center_x, y - center_y, -angle)
}

pub(crate) fn handle_directions() -> [(i32, i32); 8] {
	[
		(-1, -1),
		(0, -1),
		(1, -1),
		(1, 0),
		(1, 1),
		(0, 1),
		(-1, 1),
		(-1, 0),
	]
}
