//! Loads image files into a simple, platform-independent RGBA pixel buffer.

use std::{error::Error, path::Path};

pub struct ImageData {
	pub width: u32,
	pub height: u32,
	pub rgba_pixels: Vec<u8>,
}

pub fn load(path: &Path) -> Result<ImageData, Box<dyn Error>> {
	let image = image::open(path)?.into_rgba8();
	let (width, height) = image.dimensions();

	Ok(ImageData {
		width,
		height,
		rgba_pixels: image.into_raw(),
	})
}

pub fn load_from_bytes(bytes: &[u8]) -> Result<ImageData, image::ImageError> {
	let image = image::load_from_memory(bytes)?.into_rgba8();
	let (width, height) = image.dimensions();
	Ok(ImageData {
		width,
		height,
		rgba_pixels: image.into_raw(),
	})
}
