//! Tracks which arrangement is saved in the currently chosen preset file.

use std::path::{Path, PathBuf};

use crate::model::ImagePlacement;

#[derive(Default)]
pub(crate) struct PresetSession {
	current_preset_path: Option<PathBuf>,
	last_saved_placements: Vec<ImagePlacement>,
}

impl PresetSession {
	pub(crate) fn has_unsaved_changes(&self, current: &[ImagePlacement]) -> bool {
		self.last_saved_placements != current
	}

	pub(crate) fn mark_saved(&mut self, path: &Path, placements: &[ImagePlacement]) {
		self.current_preset_path = Some(path.to_path_buf());
		self.last_saved_placements = placements.to_vec();
	}

	pub(crate) fn current_preset_path(&self) -> Option<&Path> {
		self.current_preset_path.as_deref()
	}
}

#[cfg(test)]
mod tests {
	use super::PresetSession;
	use crate::model::ImagePlacement;

	fn placement(name: &str) -> ImagePlacement {
		ImagePlacement {
			stored_file_name: name.to_string(),
			center_x: 10.0,
			center_y: 20.0,
			width: 50.0,
			height: 30.0,
			rotation_degrees: 0.0,
			flip_x: false,
			flip_y: false,
			opacity_percent: 100,
			z_order: 0,
		}
	}

	#[test]
	fn snapshot_is_clean_until_placements_change_or_are_added_or_removed() {
		let mut session = PresetSession::default();
		let original = vec![placement("label.png")];
		session.mark_saved(std::path::Path::new("layout.json"), &original);
		assert!(!session.has_unsaved_changes(&original));
		let mut changed = original.clone();
		changed[0].center_x += 1.0;
		assert!(session.has_unsaved_changes(&changed));
		assert!(session.has_unsaved_changes(&[]));
		assert!(session.has_unsaved_changes(&[placement("label.png"), placement("other.png")]));
	}
}
