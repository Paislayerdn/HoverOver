//! Calculate small offsets that align an image outline with screen edges.

use crate::geometry::OutlineBounds;

pub(crate) const EDGE_SNAP_DISTANCE: f32 = 12.0;

pub(crate) fn snap_offset(
	bounds: OutlineBounds,
	screen_width: f32,
	screen_height: f32,
) -> (f32, f32) {
	(
		snap_axis(bounds.left, bounds.right, screen_width),
		snap_axis(bounds.top, bounds.bottom, screen_height),
	)
}

fn snap_axis(near_edge: f32, far_edge: f32, screen_extent: f32) -> f32 {
	let near_offset = -near_edge;
	let far_offset = screen_extent - far_edge;
	let near_distance = near_offset.abs();
	let far_distance = far_offset.abs();
	let near_is_close = near_distance <= EDGE_SNAP_DISTANCE;
	let far_is_close = far_distance <= EDGE_SNAP_DISTANCE;

	match (near_is_close, far_is_close) {
		(true, true) if far_distance < near_distance => far_offset,
		(true, _) => near_offset,
		(false, true) => far_offset,
		(false, false) => 0.0,
	}
}

#[cfg(test)]
mod tests {
	use crate::geometry::OutlineBounds;

	use super::snap_offset;

	#[test]
	fn snaps_near_edges_flush_to_the_screen() {
		let bounds = OutlineBounds {
			left: 9.0,
			top: 15.0,
			right: 60.0,
			bottom: 89.0,
		};
		assert_eq!(snap_offset(bounds, 200.0, 100.0), (-9.0, 11.0));
	}

	#[test]
	fn releases_edges_beyond_the_snap_distance() {
		let bounds = OutlineBounds {
			left: 13.0,
			top: 13.0,
			right: 40.0,
			bottom: 40.0,
		};
		assert_eq!(snap_offset(bounds, 100.0, 100.0), (0.0, 0.0));
	}

	#[test]
	fn snaps_each_axis_independently() {
		let bounds = OutlineBounds {
			left: 7.0,
			top: 30.0,
			right: 50.0,
			bottom: 70.0,
		};
		assert_eq!(snap_offset(bounds, 100.0, 100.0), (-7.0, 0.0));
	}

	#[test]
	fn snaps_to_the_right_and_bottom_edges() {
		let bounds = OutlineBounds {
			left: 40.0,
			top: 25.0,
			right: 95.0,
			bottom: 91.0,
		};
		assert_eq!(snap_offset(bounds, 100.0, 100.0), (5.0, 9.0));
	}
}
