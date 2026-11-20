//! Self-contained preset JSON and placement validation without UI dependencies.

use std::{
	collections::HashSet,
	error::Error,
	fs, io,
	path::{Path, PathBuf},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};

use crate::{
	geometry::image_outline_bounds,
	model::{ImagePlacement, clamp_opacity_percent},
	z_order::FIRST_IMAGE_Z,
};

const CURRENT_PRESET_VERSION: u32 = 2;
const MIN_VISIBLE_IMAGE_PIXELS: f32 = 40.0;
const MIN_IMAGE_SIZE: f32 = 12.0;
const MAX_IMAGE_SIZE: f32 = 16_384.0;

#[derive(Debug)]
pub(crate) enum PresetError {
	MissingAppData,
	Io {
		operation: &'static str,
		path: PathBuf,
		source: io::Error,
	},
	Json {
		path: PathBuf,
		source: serde_json::Error,
	},
}

impl std::fmt::Display for PresetError {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::MissingAppData => write!(formatter, "Windows did not provide an APPDATA folder"),
			Self::Io {
				operation,
				path,
				source,
			} => write!(
				formatter,
				"Could not {operation} '{}': {source}",
				path.display()
			),
			Self::Json { path, source } => write!(
				formatter,
				"Could not read preset '{}': {source}",
				path.display()
			),
		}
	}
}

impl Error for PresetError {
	fn source(&self) -> Option<&(dyn Error + 'static)> {
		match self {
			Self::Io { source, .. } => Some(source),
			Self::Json { source, .. } => Some(source),
			Self::MissingAppData => None,
		}
	}
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct PresetFile {
	pub version: u32,
	pub images: Vec<PresetImage>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct PresetImage {
	pub stored_file_name: String,
	pub image_data_base64: String,
	/// Image center X offset in pixels from the center of the screen.
	pub x: f32,
	/// Image center Y offset in pixels from the center of the screen; positive is down.
	pub y: f32,
	pub width: f32,
	pub height: f32,
	pub rotation_degrees: f32,
	pub flip_x: bool,
	pub flip_y: bool,
	pub opacity_percent: u8,
	#[serde(default)]
	pub z_order: Option<i64>,
}

pub(crate) struct EmbeddedPresetImage {
	pub placement: ImagePlacement,
	pub image_bytes: Vec<u8>,
}

impl PresetFile {
	pub(crate) fn from_placements(
		placements: &[ImagePlacement],
		screen_width: u32,
		screen_height: u32,
	) -> Self {
		Self {
			version: CURRENT_PRESET_VERSION,
			images: placements
				.iter()
				.map(|placement| PresetImage {
					stored_file_name: placement.stored_file_name.clone(),
					image_data_base64: String::new(),
					x: placement.center_x - screen_width as f32 / 2.0,
					y: placement.center_y - screen_height as f32 / 2.0,
					width: placement.width,
					height: placement.height,
					rotation_degrees: placement.rotation_degrees,
					flip_x: placement.flip_x,
					flip_y: placement.flip_y,
					opacity_percent: placement.opacity_percent,
					z_order: Some(i64::from(placement.z_order)),
				})
				.collect(),
		}
	}

	pub(crate) fn into_placements(
		self,
		screen_width: u32,
		screen_height: u32,
	) -> Result<(Vec<EmbeddedPresetImage>, Vec<String>), String> {
		if !matches!(self.version, 1 | CURRENT_PRESET_VERSION) {
			return Err(format!(
				"Preset version {} is not supported; expected version 1 or {}.",
				self.version, CURRENT_PRESET_VERSION
			));
		}
		let mut placements = Vec::new();
		let mut problems = Vec::new();
		let mut seen_z_values = HashSet::new();
		for (index, preset_image) in self.images.into_iter().enumerate() {
			let entry_number = index + 1;
			let z_value = if self.version == 1 {
				let position = u16::try_from(index).ok();
				position.and_then(|offset| FIRST_IMAGE_Z.checked_add(offset))
			} else {
				match preset_image.z_order {
					None => {
						problems.push(format!(
							"Image {entry_number} was skipped because its Z value is missing."
						));
						continue;
					}
					Some(value) => match u16::try_from(value) {
						Ok(value) => Some(value),
						Err(_) => {
							problems.push(format!(
								"Image {entry_number} was skipped because its Z value is outside 0 to 65535."
							));
							continue;
						}
					},
				}
			};
			let Some(z_value) = z_value else {
				problems.push(format!(
					"Image {entry_number} was skipped because its legacy array position cannot fit in the Z range."
				));
				continue;
			};
			if seen_z_values.contains(&z_value) {
				problems.push(format!(
					"Image {entry_number} was skipped because Z {} duplicates an earlier image.",
					z_value
				));
				continue;
			}
			let decoded_image_bytes = match STANDARD.decode(&preset_image.image_data_base64) {
				Ok(bytes) => bytes,
				Err(_) => {
					problems.push(format!(
						"Image {entry_number} was skipped because its embedded image data is invalid."
					));
					continue;
				}
			};
			if !is_plain_stored_file_name(&preset_image.stored_file_name) {
				problems.push(format!(
					"Image {entry_number} was skipped because its stored file name is not a plain file name."
				));
				continue;
			}
			if !numbers_are_finite(&preset_image) {
				problems.push(format!(
					"Image {entry_number} was skipped because one or more numbers are not finite."
				));
				continue;
			}
			if !(MIN_IMAGE_SIZE..=MAX_IMAGE_SIZE).contains(&preset_image.width)
				|| !(MIN_IMAGE_SIZE..=MAX_IMAGE_SIZE).contains(&preset_image.height)
			{
				problems.push(format!(
					"Image {entry_number} was skipped because its size is outside 12 to 16384 pixels."
				));
				continue;
			}
			let mut placement = ImagePlacement {
				stored_file_name: preset_image.stored_file_name,
				center_x: screen_width as f32 / 2.0 + preset_image.x,
				center_y: screen_height as f32 / 2.0 + preset_image.y,
				width: preset_image.width,
				height: preset_image.height,
				rotation_degrees: preset_image.rotation_degrees,
				flip_x: preset_image.flip_x,
				flip_y: preset_image.flip_y,
				opacity_percent: clamp_opacity_percent(i32::from(preset_image.opacity_percent)),
				z_order: z_value,
			};
			keep_placement_reachable(&mut placement, screen_width, screen_height);
			seen_z_values.insert(z_value);
			placements.push(EmbeddedPresetImage {
				placement,
				image_bytes: decoded_image_bytes,
			});
		}
		Ok((placements, problems))
	}
}

fn numbers_are_finite(image: &PresetImage) -> bool {
	[
		image.x,
		image.y,
		image.width,
		image.height,
		image.rotation_degrees,
	]
	.into_iter()
	.all(f32::is_finite)
}

fn is_plain_stored_file_name(file_name: &str) -> bool {
	!file_name.is_empty()
		&& !file_name.contains('/')
		&& !file_name.contains('\\')
		&& !file_name.contains("..")
}

fn keep_placement_reachable(placement: &mut ImagePlacement, screen_width: u32, screen_height: u32) {
	let bounds = image_outline_bounds(
		placement.center_x,
		placement.center_y,
		placement.width,
		placement.height,
		placement.rotation_degrees,
	);
	placement.center_x += reachable_axis_offset(bounds.left, bounds.right, screen_width);
	placement.center_y += reachable_axis_offset(bounds.top, bounds.bottom, screen_height);
}

fn reachable_axis_offset(start: f32, end: f32, screen_size: u32) -> f32 {
	let outline_size = end - start;
	let minimum_visible = MIN_VISIBLE_IMAGE_PIXELS
		.min(outline_size)
		.min(screen_size as f32);
	if minimum_visible <= 0.0 {
		return 0.0;
	}
	let minimum_start = minimum_visible - outline_size;
	let maximum_start = screen_size as f32 - minimum_visible;
	start.clamp(minimum_start, maximum_start) - start
}

pub(crate) fn app_presets_folder() -> Result<PathBuf, PresetError> {
	let app_data = std::env::var_os("APPDATA").ok_or(PresetError::MissingAppData)?;
	let folder = PathBuf::from(app_data).join("HoverOver").join("presets");
	fs::create_dir_all(&folder).map_err(|source| PresetError::Io {
		operation: "create the presets folder",
		path: folder.clone(),
		source,
	})?;
	Ok(folder)
}

pub(crate) fn save_preset_file(path: &Path, preset: &PresetFile) -> Result<(), PresetError> {
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).map_err(|source| PresetError::Io {
			operation: "create the preset folder",
			path: parent.to_path_buf(),
			source,
		})?;
	}
	let temporary_path = PathBuf::from(format!("{}.tmp", path.to_string_lossy()));
	let json_bytes = serde_json::to_vec_pretty(preset).map_err(|source| PresetError::Json {
		path: path.to_path_buf(),
		source,
	})?;
	fs::write(&temporary_path, json_bytes).map_err(|source| PresetError::Io {
		operation: "write the temporary preset file",
		path: temporary_path.clone(),
		source,
	})?;
	if let Err(source) = replace_file(&temporary_path, path) {
		let _ = fs::remove_file(&temporary_path);
		return Err(PresetError::Io {
			operation: "replace the preset file",
			path: path.to_path_buf(),
			source,
		});
	}
	Ok(())
}

#[cfg(windows)]
fn replace_file(temporary_path: &Path, path: &Path) -> io::Result<()> {
	use std::os::windows::ffi::OsStrExt;
	use windows::{
		Win32::Storage::FileSystem::{
			MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
		},
		core::PCWSTR,
	};
	let temporary_name: Vec<u16> = temporary_path
		.as_os_str()
		.encode_wide()
		.chain(Some(0))
		.collect();
	let destination_name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
	unsafe {
		MoveFileExW(
			PCWSTR(temporary_name.as_ptr()),
			PCWSTR(destination_name.as_ptr()),
			MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
		)
	}
	.map_err(|_| io::Error::last_os_error())
}

#[cfg(not(windows))]
fn replace_file(temporary_path: &Path, path: &Path) -> io::Result<()> {
	fs::rename(temporary_path, path)
}

pub(crate) fn load_preset_file(path: &Path) -> Result<PresetFile, PresetError> {
	let json_bytes = fs::read(path).map_err(|source| PresetError::Io {
		operation: "read the preset file",
		path: path.to_path_buf(),
		source,
	})?;
	serde_json::from_slice(&json_bytes).map_err(|source| PresetError::Json {
		path: path.to_path_buf(),
		source,
	})
}

#[cfg(test)]
mod tests {
	use super::{
		FIRST_IMAGE_Z, PresetFile, PresetImage, keep_placement_reachable, load_preset_file,
		save_preset_file,
	};
	use crate::model::ImagePlacement;
	use base64::{Engine, engine::general_purpose::STANDARD};
	use std::{
		fs,
		path::PathBuf,
		sync::atomic::{AtomicUsize, Ordering},
		time::{SystemTime, UNIX_EPOCH},
	};

	fn convert_valid_preset(preset: PresetFile) -> (Vec<super::EmbeddedPresetImage>, Vec<String>) {
		let conversion = preset.into_placements(1920, 1080);
		assert!(conversion.is_ok(), "Preset conversion unexpectedly failed");
		match conversion {
			Ok(value) => value,
			Err(error) => panic!("Preset conversion unexpectedly failed: {error}"),
		}
	}
	static NEXT_TEST_FOLDER: AtomicUsize = AtomicUsize::new(0);
	struct TemporaryFolder(PathBuf);
	impl TemporaryFolder {
		fn new() -> Self {
			let unique = NEXT_TEST_FOLDER.fetch_add(1, Ordering::Relaxed);
			let timestamp = SystemTime::now()
				.duration_since(UNIX_EPOCH)
				.map_or(0, |value| value.as_nanos());
			let path = std::env::temp_dir().join(format!(
				"hoverover-presets-{}-{timestamp}-{unique}",
				std::process::id()
			));
			assert!(fs::create_dir_all(&path).is_ok());
			Self(path)
		}
	}
	impl Drop for TemporaryFolder {
		fn drop(&mut self) {
			let _ = fs::remove_dir_all(&self.0);
		}
	}
	fn sample_placement() -> ImagePlacement {
		ImagePlacement {
			stored_file_name: "label.png".to_string(),
			center_x: 985.5,
			center_y: 522.75,
			width: 320.0,
			height: 180.0,
			rotation_degrees: 22.0,
			flip_x: true,
			flip_y: false,
			opacity_percent: 72,
			z_order: 45_321,
		}
	}
	fn sample_preset() -> PresetFile {
		PresetFile {
			version: 2,
			images: vec![PresetImage {
				stored_file_name: "label.png".to_string(),
				image_data_base64: STANDARD.encode(b"example image bytes"),
				x: 0.0,
				y: 0.0,
				width: 100.0,
				height: 80.0,
				rotation_degrees: 0.0,
				flip_x: false,
				flip_y: false,
				opacity_percent: 100,
				z_order: Some(0),
			}],
		}
	}
	#[test]
	fn placement_preset_round_trips_center_relative_values_and_embedded_data() {
		let original = sample_placement();
		let preset = PresetFile::from_placements(&[original.clone()], 1920, 1080);
		let json = serde_json::to_string(&preset);
		assert!(json.is_ok());
		let decoded = json
			.ok()
			.and_then(|text| serde_json::from_str::<PresetFile>(&text).ok());
		assert!(decoded.is_some());
		let mut decoded = match decoded {
			Some(value) => value,
			None => return,
		};
		decoded.images[0].image_data_base64 = STANDARD.encode(b"embedded");
		let (entries, problems) = convert_valid_preset(decoded);
		assert!(problems.is_empty());
		assert_eq!(entries.len(), 1);
		assert!((entries[0].placement.center_x - original.center_x).abs() < 0.001);
		assert!((entries[0].placement.center_y - original.center_y).abs() < 0.001);
		assert_eq!(entries[0].image_bytes, b"embedded");
		assert_eq!(entries[0].placement.z_order, original.z_order);
	}
	#[test]
	fn invalid_entries_and_corrupt_base64_are_skipped_and_reported() {
		let mut invalid = sample_preset().images.remove(0);
		invalid.image_data_base64 = "%%%".to_string();
		let preset = PresetFile {
			version: 2,
			images: vec![invalid],
		};
		let (entries, problems) = convert_valid_preset(preset);
		assert!(entries.is_empty());
		assert_eq!(problems.len(), 1);
	}
	#[test]
	fn unsupported_version_rejects_the_file_with_a_problem() {
		let preset = PresetFile {
			version: 3,
			images: vec![sample_preset().images.remove(0)],
		};
		assert!(
			matches!(preset.into_placements(1920, 1080), Err(message) if message.contains("version 3 is not supported"))
		);
	}

	#[test]
	fn version_one_migrates_array_positions_to_first_image_z_and_preserves_order() {
		let mut first = sample_preset().images.remove(0);
		first.z_order = Some(500);
		let second = first.clone();
		let preset = PresetFile {
			version: 1,
			images: vec![first, second],
		};
		let (entries, problems) = convert_valid_preset(preset);
		assert!(problems.is_empty());
		assert_eq!(entries[0].placement.z_order, FIRST_IMAGE_Z);
		assert_eq!(entries[1].placement.z_order, FIRST_IMAGE_Z + 1);
	}

	#[test]
	fn version_two_skips_missing_duplicate_and_out_of_range_z_entries() {
		let valid = sample_preset().images.remove(0);
		let duplicate = valid.clone();
		let mut missing = valid.clone();
		missing.z_order = None;
		let mut too_large = valid.clone();
		too_large.z_order = Some(65_536);
		let mut last_valid = valid.clone();
		last_valid.z_order = Some(45_000);
		let preset = PresetFile {
			version: 2,
			images: vec![valid, duplicate, missing, too_large, last_valid],
		};
		let (entries, problems) = convert_valid_preset(preset);
		assert_eq!(entries.len(), 2);
		assert_eq!(entries[0].placement.z_order, 0);
		assert_eq!(entries[1].placement.z_order, 45_000);
		assert_eq!(problems.len(), 3);
	}

	#[test]
	fn invalid_earlier_entry_does_not_claim_its_z_value() {
		let mut invalid_first = sample_preset().images.remove(0);
		invalid_first.z_order = Some(5);
		invalid_first.image_data_base64 = "%%%".to_string();
		let mut valid_second = sample_preset().images.remove(0);
		valid_second.z_order = Some(5);
		let preset = PresetFile {
			version: 2,
			images: vec![invalid_first, valid_second],
		};
		let (entries, problems) = convert_valid_preset(preset);
		assert_eq!(entries.len(), 1);
		assert_eq!(entries[0].placement.z_order, 5);
		assert_eq!(problems.len(), 1);
		assert!(problems[0].contains("embedded image data is invalid"));
	}

	#[test]
	fn version_one_json_without_z_fields_migrates_array_positions() {
		let json = r#"{"version":1,"images":[{"stored_file_name":"first.png","image_data_base64":"AQ==","x":0.0,"y":0.0,"width":100.0,"height":80.0,"rotation_degrees":0.0,"flip_x":false,"flip_y":false,"opacity_percent":100},{"stored_file_name":"second.png","image_data_base64":"Ag==","x":0.0,"y":0.0,"width":100.0,"height":80.0,"rotation_degrees":0.0,"flip_x":false,"flip_y":false,"opacity_percent":100}]}"#;
		let parsed = serde_json::from_str::<PresetFile>(json);
		assert!(
			parsed.is_ok(),
			"Version 1 JSON without z_order should parse: {parsed:?}"
		);
		let preset = match parsed {
			Ok(preset) => preset,
			Err(error) => panic!("Version 1 JSON did not parse: {error}"),
		};
		let (entries, problems) = convert_valid_preset(preset);
		assert!(problems.is_empty());
		assert_eq!(entries.len(), 2);
		assert_eq!(entries[0].placement.z_order, FIRST_IMAGE_Z);
		assert_eq!(entries[1].placement.z_order, FIRST_IMAGE_Z + 1);
	}
	#[test]
	fn path_save_load_round_trip_and_overwrite_leave_no_temporary_file() {
		let folder = TemporaryFolder::new();
		let path = folder.0.join("layout.json");
		let mut preset = sample_preset();
		assert!(save_preset_file(&path, &preset).is_ok());
		preset.images[0].x = 55.0;
		assert!(save_preset_file(&path, &preset).is_ok());
		assert!(matches!(load_preset_file(&path), Ok(ref loaded) if loaded == &preset));
		assert!(path.with_extension("json.tmp").exists() == false);
	}
	#[test]
	fn keep_reachable_moves_rotated_outline_back_inside_visible_limits() {
		let mut placement = sample_placement();
		placement.center_x = -500.0;
		placement.center_y = 2_000.0;
		keep_placement_reachable(&mut placement, 800, 600);
		let bounds = crate::geometry::image_outline_bounds(
			placement.center_x,
			placement.center_y,
			placement.width,
			placement.height,
			placement.rotation_degrees,
		);
		assert!(bounds.right.min(800.0) - bounds.left.max(0.0) >= 40.0);
		assert!(bounds.bottom.min(600.0) - bounds.top.max(0.0) >= 40.0);
	}
}
