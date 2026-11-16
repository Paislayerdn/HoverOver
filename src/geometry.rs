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

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OutlineBounds {
	pub left: f32,
	pub top: f32,
	pub right: f32,
	pub bottom: f32,
}

pub(crate) fn image_outline_bounds(
	center_x: f32,
	center_y: f32,
	width: f32,
	height: f32,
	rotation_degrees: f32,
) -> OutlineBounds {
	let radians = rotation_degrees.to_radians();
	let half_width = (radians.cos().abs() * width + radians.sin().abs() * height) / 2.0;
	let half_height = (radians.sin().abs() * width + radians.cos().abs() * height) / 2.0;
	OutlineBounds {
		left: center_x - half_width,
		top: center_y - half_height,
		right: center_x + half_width,
		bottom: center_y + half_height,
	}
}

#[cfg(test)]
mod tests {
	use super::{image_outline_bounds, local_to_screen, rotate, screen_to_local_at};

	fn assert_points_close(actual: (f32, f32), expected: (f32, f32)) {
		assert!((actual.0 - expected.0).abs() < 0.001);
		assert!((actual.1 - expected.1).abs() < 0.001);
	}

	#[test]
	fn rotating_by_an_angle_and_its_negative_returns_the_original_point() {
		let point = (23.5, -41.25);
		for angle in [-179.0, -90.0, -37.5, 0.0, 68.0, 135.0, 270.0] {
			let rotated = rotate(point.0, point.1, angle);
			let restored = rotate(rotated.0, rotated.1, -angle);
			assert_points_close(restored, point);
		}
	}

	#[test]
	fn local_to_screen_and_screen_to_local_return_the_original_point() {
		let center = (120.0, 75.0);
		let point = (-12.5, 31.75);
		for angle in [-135.0, -45.0, 0.0, 22.5, 90.0, 179.0] {
			let screen = local_to_screen(center.0, center.1, angle, point.0, point.1);
			let restored = screen_to_local_at(center.0, center.1, angle, screen.0, screen.1);
			assert_points_close(restored, point);
		}
	}

	#[test]
	fn image_outline_bounds_account_for_rotation() {
		let bounds = image_outline_bounds(50.0, 40.0, 20.0, 10.0, 90.0);
		assert!((bounds.left - 45.0).abs() < 0.001);
		assert!((bounds.top - 30.0).abs() < 0.001);
		assert!((bounds.right - 55.0).abs() < 0.001);
		assert!((bounds.bottom - 50.0).abs() < 0.001);
	}
}
