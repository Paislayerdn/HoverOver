//! Shared image, mode, and dirty-region data used by the app and renderer.

use crate::image_file::ImageData;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy)]
pub(crate) struct DirtyRect {
	pub left: i32,
	pub top: i32,
	pub right: i32,
	pub bottom: i32,
}

impl DirtyRect {
	pub(crate) fn full(width: u32, height: u32) -> Self {
		Self {
			left: 0,
			top: 0,
			right: width as i32,
			bottom: height as i32,
		}
	}

	pub(crate) fn union(self, other: Self) -> Self {
		Self {
			left: self.left.min(other.left),
			top: self.top.min(other.top),
			right: self.right.max(other.right),
			bottom: self.bottom.max(other.bottom),
		}
	}
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
	Display,
	Editing,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct ImagePlacement {
	/// Plain file name inside the images folder. Never a full path.
	pub(crate) stored_file_name: String,
	pub(crate) center_x: f32,
	pub(crate) center_y: f32,
	pub(crate) width: f32,
	pub(crate) height: f32,
	pub(crate) rotation_degrees: f32,
	#[serde(default)]
	pub(crate) flip_x: bool,
	#[serde(default)]
	pub(crate) flip_y: bool,
	#[serde(default = "default_opacity_percent")]
	pub(crate) opacity_percent: u8,
	pub(crate) z_order: u16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PanelValues {
	pub x: i32,
	pub y: i32,
	pub opacity: u8,
	pub z: u16,
	pub rotation_degrees: f32,
}

fn default_opacity_percent() -> u8 {
	100
}

pub(crate) fn clamp_opacity_percent(value: i32) -> u8 {
	value.clamp(0, 100) as u8
}

pub(crate) struct ImageObject {
	pub(crate) placement: ImagePlacement,
	pub(crate) image: ImageData,
}

#[cfg(test)]
mod tests {
	use super::clamp_opacity_percent;

	#[test]
	fn opacity_values_are_clamped_to_the_supported_range() {
		assert_eq!(clamp_opacity_percent(-20), 0);
		assert_eq!(clamp_opacity_percent(45), 45);
		assert_eq!(clamp_opacity_percent(150), 100);
	}
}
