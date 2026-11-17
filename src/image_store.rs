//! Copies added images into HoverOver's private images folder.

use std::{
	error::Error,
	fs, io,
	path::{Path, PathBuf},
};

const MAX_FILENAME_ATTEMPTS: u32 = 10_000;

use crate::{image_file, model::ImageObject, presets::EmbeddedPresetImage};

#[derive(Debug)]
pub enum ImageStoreError {
	MissingAppData,
	Io {
		operation: &'static str,
		path: PathBuf,
		source: io::Error,
	},
	MissingFileName,
	TooManyNameCollisions,
}

impl std::fmt::Display for ImageStoreError {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::MissingAppData => {
				write!(formatter, "Windows did not provide an APPDATA folder")
			}
			Self::Io {
				operation,
				path,
				source,
			} => write!(
				formatter,
				"Could not {operation} '{}': {source}",
				path.display()
			),
			Self::MissingFileName => write!(formatter, "The selected image has no file name"),
			Self::TooManyNameCollisions => {
				write!(
					formatter,
					"Could not choose a unique stored image file name"
				)
			}
		}
	}
}

impl Error for ImageStoreError {
	fn source(&self) -> Option<&(dyn Error + 'static)> {
		match self {
			Self::Io { source, .. } => Some(source),
			_ => None,
		}
	}
}

/// Finds and creates `%APPDATA%/HoverOver/images` for the running user.
pub fn app_image_folder() -> Result<PathBuf, ImageStoreError> {
	let app_data = std::env::var_os("APPDATA").ok_or(ImageStoreError::MissingAppData)?;
	let folder = PathBuf::from(app_data).join("HoverOver").join("images");
	fs::create_dir_all(&folder).map_err(|source| ImageStoreError::Io {
		operation: "create the image folder",
		path: folder.clone(),
		source,
	})?;
	Ok(folder)
}

/// Copies a selected image and returns its plain stored file name.
pub fn import_image_into(folder: &Path, source_path: &Path) -> Result<String, ImageStoreError> {
	fs::create_dir_all(folder).map_err(|source| ImageStoreError::Io {
		operation: "create the image folder",
		path: folder.to_path_buf(),
		source,
	})?;

	let source_file_name = match source_path.file_name() {
		Some(file_name) => file_name.to_string_lossy().into_owned(),
		None => return Err(ImageStoreError::MissingFileName),
	};
	let source_stem = match source_path.file_stem() {
		Some(file_stem) => file_stem.to_string_lossy().into_owned(),
		None => "image".to_string(),
	};
	let extension = source_path
		.extension()
		.map(|extension| extension.to_string_lossy().to_lowercase());
	let safe_stem = sanitize_component(&source_stem);
	let safe_stem = if safe_stem.is_empty() {
		"image".to_string()
	} else {
		safe_stem
	};
	let safe_extension = match extension.as_deref() {
		Some(extension) => {
			let safe_extension = sanitize_component(extension);
			if safe_extension.is_empty() {
				None
			} else {
				Some(safe_extension)
			}
		}
		None => None,
	};
	let (safe_stem, safe_extension) = avoid_double_dots(safe_stem, safe_extension);
	let extension_suffix = match safe_extension.as_deref() {
		Some(extension) => format!(".{extension}"),
		None => String::new(),
	};
	let mut stored_file_name = format!("{safe_stem}{extension_suffix}");
	if stored_file_name.is_empty() {
		stored_file_name = sanitize_component(&source_file_name);
	}

	let mut destination = folder.join(&stored_file_name);
	let mut collision_number = 2;
	while destination.exists() {
		if collision_number > MAX_FILENAME_ATTEMPTS {
			return Err(ImageStoreError::TooManyNameCollisions);
		}
		let collision_suffix = format!("-{collision_number}");
		stored_file_name = format!("{safe_stem}{collision_suffix}{extension_suffix}");
		destination = folder.join(&stored_file_name);
		collision_number += 1;
	}

	fs::copy(source_path, &destination).map_err(|source| ImageStoreError::Io {
		operation: "copy the selected image",
		path: source_path.to_path_buf(),
		source,
	})?;
	Ok(stored_file_name)
}

pub(crate) fn import_image_bytes_into(
	folder: &Path,
	preferred_file_name: &str,
	bytes: &[u8],
) -> Result<String, ImageStoreError> {
	fs::create_dir_all(folder).map_err(|source| ImageStoreError::Io {
		operation: "create the image folder",
		path: folder.to_path_buf(),
		source,
	})?;
	let (safe_stem, safe_extension) = sanitized_file_parts(preferred_file_name)?;
	let extension_suffix =
		safe_extension.map_or_else(String::new, |extension| format!(".{extension}"));
	let mut candidate_names = Vec::new();
	for collision_number in 1..=MAX_FILENAME_ATTEMPTS {
		let file_name = if collision_number == 1 {
			format!("{safe_stem}{extension_suffix}")
		} else {
			format!("{safe_stem}-{collision_number}{extension_suffix}")
		};
		let destination = folder.join(&file_name);
		match fs::read(&destination) {
			Ok(existing_bytes) if existing_bytes == bytes => return Ok(file_name),
			Ok(_) => candidate_names.push(file_name),
			Err(error) if error.kind() == io::ErrorKind::NotFound => {
				candidate_names.push(file_name)
			}
			Err(source) => {
				return Err(ImageStoreError::Io {
					operation: "inspect a stored image",
					path: destination,
					source,
				});
			}
		}
	}
	for file_name in candidate_names {
		let destination = folder.join(&file_name);
		if !destination.exists() {
			fs::write(&destination, bytes).map_err(|source| ImageStoreError::Io {
				operation: "write an embedded image",
				path: destination,
				source,
			})?;
			return Ok(file_name);
		}
	}
	Err(ImageStoreError::TooManyNameCollisions)
}

pub(crate) fn load_embedded_images(
	folder: &Path,
	entries: Vec<EmbeddedPresetImage>,
) -> (Vec<ImageObject>, Vec<String>) {
	let mut images = Vec::new();
	let mut problems = Vec::new();
	for (index, mut entry) in entries.into_iter().enumerate() {
		let image = match image_file::load_from_bytes(&entry.image_bytes) {
			Ok(image) if image.width <= 16_384 && image.height <= 16_384 => image,
			Ok(_) => {
				problems.push(format!(
					"Image {} was skipped because its pixel dimensions exceed 16384.",
					index + 1
				));
				continue;
			}
			Err(_) => {
				problems.push(format!(
					"Image {} was skipped because the embedded data is not a supported image.",
					index + 1
				));
				continue;
			}
		};
		match import_image_bytes_into(
			folder,
			&entry.placement.stored_file_name,
			&entry.image_bytes,
		) {
			Ok(file_name) => entry.placement.stored_file_name = file_name,
			Err(error) => {
				problems.push(format!(
					"Image {} could not be copied into the local image folder: {error}",
					index + 1
				));
				continue;
			}
		}
		images.push(ImageObject {
			placement: entry.placement,
			image,
		});
	}
	(images, problems)
}

fn sanitized_file_parts(file_name: &str) -> Result<(String, Option<String>), ImageStoreError> {
	let path = Path::new(file_name);
	let stem = path
		.file_stem()
		.ok_or(ImageStoreError::MissingFileName)?
		.to_string_lossy();
	let extension = path
		.extension()
		.map(|value| value.to_string_lossy().to_lowercase());
	let safe_stem = sanitize_component(&stem);
	let safe_stem = if safe_stem.is_empty() {
		"image".to_string()
	} else {
		safe_stem
	};
	let safe_extension = extension
		.map(|value| sanitize_component(&value))
		.filter(|value| !value.is_empty());
	Ok(avoid_double_dots(safe_stem, safe_extension))
}

fn sanitize_component(component: &str) -> String {
	let mut sanitized: String = component
		.chars()
		.map(|character| {
			if character.is_alphanumeric() || matches!(character, '-' | '_' | '.') {
				character
			} else {
				'_'
			}
		})
		.collect();
	while sanitized.contains("..") {
		sanitized = sanitized.replace("..", "._");
	}
	sanitized
}

fn avoid_double_dots(mut stem: String, mut extension: Option<String>) -> (String, Option<String>) {
	while stem.contains("..") {
		stem = stem.replace("..", "._");
	}
	if stem.ends_with('.') {
		stem.pop();
		stem.push('_');
	}
	if let Some(extension_value) = extension.as_mut() {
		while extension_value.contains("..") {
			*extension_value = extension_value.replace("..", "._");
		}
		if extension_value.starts_with('.') {
			extension_value.remove(0);
			extension_value.insert(0, '_');
		}
	}
	(stem, extension)
}

#[cfg(test)]
mod tests {
	use std::{
		fs,
		path::{Path, PathBuf},
		sync::atomic::{AtomicUsize, Ordering},
		time::{SystemTime, UNIX_EPOCH},
	};

	use super::{ImageStoreError, import_image_into, load_embedded_images, sanitize_component};
	use crate::{model::ImagePlacement, presets::EmbeddedPresetImage};

	static NEXT_TEST_FOLDER: AtomicUsize = AtomicUsize::new(0);

	struct TemporaryFolder(PathBuf);

	impl TemporaryFolder {
		fn new() -> Self {
			let unique_number = NEXT_TEST_FOLDER.fetch_add(1, Ordering::Relaxed);
			let time_number = match SystemTime::now().duration_since(UNIX_EPOCH) {
				Ok(duration) => duration.as_nanos(),
				Err(_) => 0,
			};
			let path = std::env::temp_dir().join(format!(
				"hoverover-image-store-{}-{time_number}-{unique_number}",
				std::process::id()
			));
			assert!(fs::create_dir_all(&path).is_ok());
			Self(path)
		}

		fn path(&self) -> &Path {
			&self.0
		}
	}

	impl Drop for TemporaryFolder {
		fn drop(&mut self) {
			let _ = fs::remove_dir_all(&self.0);
		}
	}

	#[test]
	fn file_name_sanitizing_replaces_spaces_slashes_and_dot_dot_and_keeps_unicode() {
		assert_eq!(
			sanitize_component("my picture/name..png"),
			"my_picture_name._png"
		);
		assert_eq!(sanitize_component("猫 2.png"), "猫_2.png");
		assert_eq!(sanitize_component("without_extension"), "without_extension");
	}

	#[test]
	fn import_preserves_a_missing_extension_and_copies_identical_bytes() {
		let temporary_folder = TemporaryFolder::new();
		let source_path = temporary_folder.path().join("source file");
		let destination_folder = temporary_folder.path().join("images");
		let original_bytes = [0, 1, 2, 13, 255];
		let source_result = fs::write(&source_path, original_bytes);
		assert!(source_result.is_ok());

		let stored_name = import_image_into(&destination_folder, &source_path);
		assert!(matches!(stored_name, Ok(ref name) if name == "source_file"));
		let copied_bytes = fs::read(destination_folder.join("source_file"));
		assert!(
			matches!(copied_bytes, Ok(ref bytes) if bytes.as_slice() == original_bytes.as_slice())
		);
	}

	#[test]
	fn import_numbers_collisions_starting_at_two() {
		let temporary_folder = TemporaryFolder::new();
		let source_path = temporary_folder.path().join("label.PNG");
		let destination_folder = temporary_folder.path().join("images");
		assert!(fs::write(&source_path, b"image").is_ok());
		assert!(fs::create_dir_all(&destination_folder).is_ok());
		assert!(fs::write(destination_folder.join("label.png"), b"old one").is_ok());
		assert!(fs::write(destination_folder.join("label-2.png"), b"old two").is_ok());

		let stored_name = import_image_into(&destination_folder, &source_path);
		assert!(matches!(stored_name, Ok(ref name) if name == "label-3.png"));
	}

	#[test]
	fn missing_source_returns_an_error_without_panicking() {
		let temporary_folder = TemporaryFolder::new();
		let destination_folder = temporary_folder.path().join("images");
		let source_path = temporary_folder.path().join("missing.png");

		let result = import_image_into(&destination_folder, &source_path);
		assert!(matches!(result, Err(ImageStoreError::Io { .. })));
		let destination_contents = fs::read_dir(destination_folder);
		let Ok(destination_entries) = destination_contents else {
			panic!("The image folder should exist after the attempted import");
		};
		assert_eq!(destination_entries.count(), 0);
	}

	#[test]
	fn embedded_preset_images_load_in_a_new_folder_and_repeat_load_reuses_file() {
		let temporary_folder = TemporaryFolder::new();
		let images_folder = temporary_folder.path().join("new-computer-images");
		let mut encoded_png = Vec::new();
		let encoder = image::codecs::png::PngEncoder::new(&mut encoded_png);
		let encoded_result = image::ImageEncoder::write_image(
			encoder,
			&[255_u8, 20, 30, 255],
			1,
			1,
			image::ExtendedColorType::Rgba8,
		);
		assert!(encoded_result.is_ok());
		let make_entry = || EmbeddedPresetImage {
			placement: ImagePlacement {
				stored_file_name: "label.png".to_string(),
				center_x: 50.0,
				center_y: 50.0,
				width: 20.0,
				height: 20.0,
				rotation_degrees: 0.0,
				flip_x: false,
				flip_y: false,
				opacity_percent: 100,
			},
			image_bytes: encoded_png.clone(),
		};
		let (first_load, first_problems) = load_embedded_images(&images_folder, vec![make_entry()]);
		assert!(first_problems.is_empty());
		assert_eq!(first_load.len(), 1);
		assert_eq!(first_load[0].image.width, 1);
		let (second_load, second_problems) =
			load_embedded_images(&images_folder, vec![make_entry()]);
		assert!(second_problems.is_empty());
		assert_eq!(second_load.len(), 1);
		let directory_entries = fs::read_dir(&images_folder);
		assert_eq!(
			directory_entries
				.map(|entries| entries.count())
				.unwrap_or(0),
			1
		);
	}

	#[test]
	fn embedded_import_reuses_identical_numbered_variant_even_when_base_is_missing() {
		let temporary_folder = TemporaryFolder::new();
		let images_folder = temporary_folder.path().join("images");
		assert!(fs::create_dir_all(&images_folder).is_ok());
		assert!(fs::write(images_folder.join("label-2.png"), b"same bytes").is_ok());
		let imported = super::import_image_bytes_into(&images_folder, "label.png", b"same bytes");
		assert!(matches!(imported, Ok(ref name) if name == "label-2.png"));
	}

	#[test]
	fn embedded_images_that_are_not_decodable_are_skipped_and_reported() {
		let temporary_folder = TemporaryFolder::new();
		let entry = EmbeddedPresetImage {
			placement: ImagePlacement {
				stored_file_name: "broken.png".to_string(),
				center_x: 1.0,
				center_y: 1.0,
				width: 12.0,
				height: 12.0,
				rotation_degrees: 0.0,
				flip_x: false,
				flip_y: false,
				opacity_percent: 100,
			},
			image_bytes: vec![1, 2, 3],
		};
		let (images, problems) = load_embedded_images(temporary_folder.path(), vec![entry]);
		assert!(images.is_empty());
		assert_eq!(problems.len(), 1);
	}
}
