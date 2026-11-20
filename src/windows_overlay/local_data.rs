//! Counts and safely clears only the current user's `%APPDATA%/HoverOver` folder.

use std::os::windows::fs::MetadataExt;
use std::{
	error::Error,
	fmt, fs, io,
	path::{Path, PathBuf},
};

use windows::{
	Win32::{
		Foundation::HWND,
		UI::Controls::{
			TASKDIALOG_BUTTON, TASKDIALOGCONFIG, TDF_ALLOW_DIALOG_CANCELLATION, TaskDialogIndirect,
		},
	},
	core::{HSTRING, PCWSTR},
};

const APP_FOLDER_NAME: &str = "HoverOver";
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
const CLEAR_BUTTON_ID: i32 = 1001;
const CANCEL_BUTTON_ID: i32 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LocalDataInspection {
	pub target_path: PathBuf,
	pub file_count: u64,
	pub folder_count: u64,
}

#[derive(Debug)]
pub(super) enum LocalDataError {
	MissingAppData,
	UnsafeTarget(PathBuf),
	UnsupportedEntry(PathBuf),
	ChangedAfterConfirmation(PathBuf),
	Io {
		operation: &'static str,
		path: PathBuf,
		source: io::Error,
	},
	CountOverflow,
}

impl fmt::Display for LocalDataError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::MissingAppData => write!(formatter, "Windows did not provide an APPDATA folder."),
			Self::UnsafeTarget(path) => write!(
				formatter,
				"Refusing to clear data because the target is not a verified HoverOver folder: '{}'.",
				path.display()
			),
			Self::UnsupportedEntry(path) => write!(
				formatter,
				"Refusing to clear data because a symbolic link, reparse point, or unsupported entry was found at '{}'.",
				path.display()
			),
			Self::ChangedAfterConfirmation(path) => write!(
				formatter,
				"The contents of '{}' changed after the confirmation was shown. Review the current count and try again.",
				path.display()
			),
			Self::Io {
				operation,
				path,
				source,
			} => write!(
				formatter,
				"Could not {operation} '{}': {source}",
				path.display()
			),
			Self::CountOverflow => write!(
				formatter,
				"The local data file and folder count exceeded the supported limit."
			),
		}
	}
}

impl Error for LocalDataError {
	fn source(&self) -> Option<&(dyn Error + 'static)> {
		match self {
			Self::Io { source, .. } => Some(source),
			_ => None,
		}
	}
}

struct VerifiedTarget {
	app_data_path: PathBuf,
	target_path: PathBuf,
}

pub(super) fn inspect_local_data() -> Result<Option<LocalDataInspection>, LocalDataError> {
	let Some(target) = resolve_target_from_environment()? else {
		return Ok(None);
	};
	let (file_count, folder_count) = count_directory_tree(&target.target_path)?;
	Ok(Some(LocalDataInspection {
		target_path: target.target_path,
		file_count,
		folder_count,
	}))
}

pub(super) fn confirm_clear_local_data(
	owner: HWND,
	inspection: &LocalDataInspection,
) -> Result<bool, String> {
	let title = HSTRING::from("HoverOver");
	let instruction = HSTRING::from("Clear All Local Data?");
	let content = HSTRING::from(format!(
		"This will permanently delete all locally stored HoverOver data.\n\nFiles: {}\nFolders: {}\n\nLocation: {}\n\nPreset files stored elsewhere and the HoverOver executable will remain untouched.\n\nThis action cannot be undone.",
		inspection.file_count,
		inspection.folder_count,
		inspection.target_path.display(),
	));
	let clear_label = HSTRING::from("Yes, Delete All");
	let cancel_label = HSTRING::from("Cancel");
	let buttons = [
		TASKDIALOG_BUTTON {
			nButtonID: CANCEL_BUTTON_ID,
			pszButtonText: PCWSTR(cancel_label.as_ptr()),
		},
		TASKDIALOG_BUTTON {
			nButtonID: CLEAR_BUTTON_ID,
			pszButtonText: PCWSTR(clear_label.as_ptr()),
		},
	];
	let configuration = TASKDIALOGCONFIG {
		cbSize: std::mem::size_of::<TASKDIALOGCONFIG>() as u32,
		hwndParent: owner,
		dwFlags: TDF_ALLOW_DIALOG_CANCELLATION,
		pszWindowTitle: PCWSTR(title.as_ptr()),
		pszMainInstruction: PCWSTR(instruction.as_ptr()),
		pszContent: PCWSTR(content.as_ptr()),
		cButtons: buttons.len() as u32,
		pButtons: buttons.as_ptr(),
		nDefaultButton: CANCEL_BUTTON_ID,
		..Default::default()
	};
	let mut selected_button = CANCEL_BUTTON_ID;
	unsafe { TaskDialogIndirect(&configuration, Some(&mut selected_button), None, None) }
		.map_err(|error| format!("Could not show the local-data confirmation dialog: {error}"))?;
	Ok(selected_button == CLEAR_BUTTON_ID)
}

pub(super) fn clear_local_data(
	confirmed_inspection: &LocalDataInspection,
) -> Result<PathBuf, LocalDataError> {
	let target = resolve_target_from_environment()?.ok_or_else(|| {
		LocalDataError::UnsafeTarget(PathBuf::from("%APPDATA%").join(APP_FOLDER_NAME))
	})?;
	if !path_equals_case_insensitively(&target.target_path, &confirmed_inspection.target_path) {
		return Err(LocalDataError::ChangedAfterConfirmation(target.target_path));
	}
	let current_counts = count_directory_tree(&target.target_path)?;
	if current_counts
		!= (
			confirmed_inspection.file_count,
			confirmed_inspection.folder_count,
		) {
		return Err(LocalDataError::ChangedAfterConfirmation(target.target_path));
	}
	remove_verified_target(
		&target,
		(
			confirmed_inspection.file_count,
			confirmed_inspection.folder_count,
		),
	)?;
	Ok(target.target_path)
}

fn remove_verified_target(
	target: &VerifiedTarget,
	expected_counts: (u64, u64),
) -> Result<(), LocalDataError> {
	// Repeat the full safety walk immediately before deletion, after confirmation.
	validate_target(&target.app_data_path, &target.target_path)?;
	if count_directory_tree(&target.target_path)? != expected_counts {
		return Err(LocalDataError::ChangedAfterConfirmation(
			target.target_path.clone(),
		));
	}
	validate_target(&target.app_data_path, &target.target_path)?;
	fs::remove_dir_all(&target.target_path).map_err(|source| LocalDataError::Io {
		operation: "delete local HoverOver data",
		path: target.target_path.clone(),
		source,
	})?;
	if fs::symlink_metadata(&target.target_path).is_ok() {
		return Err(LocalDataError::UnsafeTarget(target.target_path.clone()));
	}
	Ok(())
}

fn resolve_target_from_environment() -> Result<Option<VerifiedTarget>, LocalDataError> {
	let app_data = std::env::var_os("APPDATA").ok_or(LocalDataError::MissingAppData)?;
	let app_data_path = PathBuf::from(app_data);
	if !app_data_path.is_absolute() {
		return Err(LocalDataError::UnsafeTarget(app_data_path));
	}
	let canonical_app_data =
		fs::canonicalize(&app_data_path).map_err(|source| LocalDataError::Io {
			operation: "resolve the APPDATA folder",
			path: app_data_path,
			source,
		})?;
	let target_path = canonical_app_data.join(APP_FOLDER_NAME);
	match fs::symlink_metadata(&target_path) {
		Ok(_) => {}
		Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
		Err(source) => {
			return Err(LocalDataError::Io {
				operation: "inspect the local data folder",
				path: target_path,
				source,
			});
		}
	}
	validate_target(&canonical_app_data, &target_path)?;
	Ok(Some(VerifiedTarget {
		app_data_path: canonical_app_data,
		target_path,
	}))
}

fn validate_target(app_data_path: &Path, target_path: &Path) -> Result<(), LocalDataError> {
	if target_path.parent() != Some(app_data_path)
		|| !target_path
			.file_name()
			.is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(APP_FOLDER_NAME))
	{
		return Err(LocalDataError::UnsafeTarget(target_path.to_path_buf()));
	}
	let metadata = fs::symlink_metadata(target_path).map_err(|source| LocalDataError::Io {
		operation: "verify the local data folder",
		path: target_path.to_path_buf(),
		source,
	})?;
	if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 || !metadata.is_dir() {
		return Err(LocalDataError::UnsafeTarget(target_path.to_path_buf()));
	}
	let canonical_target = fs::canonicalize(target_path).map_err(|source| LocalDataError::Io {
		operation: "resolve the local data folder",
		path: target_path.to_path_buf(),
		source,
	})?;
	if !canonical_target
		.parent()
		.is_some_and(|parent| path_equals_case_insensitively(parent, app_data_path))
		|| !canonical_target
			.file_name()
			.is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(APP_FOLDER_NAME))
	{
		return Err(LocalDataError::UnsafeTarget(target_path.to_path_buf()));
	}
	Ok(())
}

fn path_equals_case_insensitively(first: &Path, second: &Path) -> bool {
	first
		.components()
		.zip(second.components())
		.all(|(first_component, second_component)| {
			first_component
				.as_os_str()
				.to_string_lossy()
				.eq_ignore_ascii_case(&second_component.as_os_str().to_string_lossy())
		})
		&& first.components().count() == second.components().count()
}

fn count_directory_tree(directory: &Path) -> Result<(u64, u64), LocalDataError> {
	let root_metadata = fs::symlink_metadata(directory).map_err(|source| LocalDataError::Io {
		operation: "inspect",
		path: directory.to_path_buf(),
		source,
	})?;
	if root_metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
		|| !root_metadata.is_dir()
	{
		return Err(LocalDataError::UnsupportedEntry(directory.to_path_buf()));
	}
	let mut file_count = 0_u64;
	let mut folder_count = 1_u64;
	let entries = fs::read_dir(directory).map_err(|source| LocalDataError::Io {
		operation: "count entries in",
		path: directory.to_path_buf(),
		source,
	})?;
	for entry in entries {
		let entry = entry.map_err(|source| LocalDataError::Io {
			operation: "read an entry in",
			path: directory.to_path_buf(),
			source,
		})?;
		let entry_path = entry.path();
		let metadata = fs::symlink_metadata(&entry_path).map_err(|source| LocalDataError::Io {
			operation: "inspect",
			path: entry_path.clone(),
			source,
		})?;
		if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
			return Err(LocalDataError::UnsupportedEntry(entry_path));
		}
		if metadata.is_dir() {
			folder_count = folder_count
				.checked_add(1)
				.ok_or(LocalDataError::CountOverflow)?;
			let (child_files, child_folders) = count_directory_tree(&entry_path)?;
			file_count = file_count
				.checked_add(child_files)
				.ok_or(LocalDataError::CountOverflow)?;
			folder_count = folder_count
				.checked_add(child_folders.saturating_sub(1))
				.ok_or(LocalDataError::CountOverflow)?;
		} else if metadata.is_file() {
			file_count = file_count
				.checked_add(1)
				.ok_or(LocalDataError::CountOverflow)?;
		} else {
			return Err(LocalDataError::UnsupportedEntry(entry_path));
		}
	}
	Ok((file_count, folder_count))
}

#[cfg(test)]
mod tests {
	use std::{
		fs,
		path::PathBuf,
		sync::atomic::{AtomicUsize, Ordering},
		time::{SystemTime, UNIX_EPOCH},
	};

	use super::{
		LocalDataError, VerifiedTarget, count_directory_tree, remove_verified_target,
		validate_target,
	};

	static NEXT_TEST_FOLDER: AtomicUsize = AtomicUsize::new(0);

	struct TemporaryFolder(PathBuf);
	impl TemporaryFolder {
		fn new() -> Self {
			let unique = NEXT_TEST_FOLDER.fetch_add(1, Ordering::Relaxed);
			let timestamp = SystemTime::now()
				.duration_since(UNIX_EPOCH)
				.map_or(0, |value| value.as_nanos());
			let path = std::env::temp_dir().join(format!(
				"hoverover-clear-tests-{}-{timestamp}-{unique}",
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

	#[test]
	fn recursive_count_includes_nested_files_and_the_root_folder() {
		let temporary = TemporaryFolder::new();
		let target = temporary.0.join("HoverOver");
		assert!(fs::create_dir_all(target.join("images/nested")).is_ok());
		assert!(fs::write(target.join("presets.json"), b"data").is_ok());
		assert!(fs::write(target.join("images/nested/image.png"), b"image").is_ok());
		assert_eq!(count_directory_tree(&target).ok(), Some((2, 3)));
	}

	#[test]
	fn target_must_be_the_direct_hoverover_child_and_not_a_reparse_point() {
		let temporary = TemporaryFolder::new();
		let canonical_app_data = fs::canonicalize(&temporary.0);
		assert!(canonical_app_data.is_ok());
		let canonical_app_data = match canonical_app_data {
			Ok(path) => path,
			Err(_) => return,
		};
		let valid_target = canonical_app_data.join("HoverOver");
		let other_target = canonical_app_data.join("Other");
		assert!(fs::create_dir_all(&valid_target).is_ok());
		assert!(fs::create_dir_all(&other_target).is_ok());
		let valid_result = validate_target(&canonical_app_data, &valid_target);
		assert!(
			valid_result.is_ok(),
			"Valid HoverOver target rejected: {valid_result:?}"
		);
		assert!(matches!(
			validate_target(&canonical_app_data, &other_target),
			Err(LocalDataError::UnsafeTarget(_))
		));
	}

	#[test]
	fn deleting_verified_data_removes_only_hoverover_and_preserves_appdata_siblings() {
		let temporary = TemporaryFolder::new();
		let canonical_app_data = fs::canonicalize(&temporary.0);
		assert!(canonical_app_data.is_ok());
		let canonical_app_data = match canonical_app_data {
			Ok(path) => path,
			Err(_) => return,
		};
		let target_path = canonical_app_data.join("HoverOver");
		assert!(fs::create_dir_all(target_path.join("images")).is_ok());
		assert!(fs::write(target_path.join("images/local.png"), b"local").is_ok());
		assert!(fs::write(canonical_app_data.join("external-preset.json"), b"external").is_ok());
		let target = VerifiedTarget {
			app_data_path: canonical_app_data.clone(),
			target_path: target_path.clone(),
		};
		assert!(remove_verified_target(&target, (1, 2)).is_ok());
		assert!(!target_path.exists());
		assert!(canonical_app_data.exists());
		assert!(canonical_app_data.join("external-preset.json").exists());
	}

	#[test]
	fn recursive_count_refuses_directory_symlinks_when_windows_allows_creating_them() {
		use std::os::windows::fs::symlink_dir;
		let temporary = TemporaryFolder::new();
		let target = temporary.0.join("HoverOver");
		let outside = temporary.0.join("outside");
		assert!(fs::create_dir_all(target.join("nested")).is_ok());
		assert!(fs::create_dir_all(&outside).is_ok());
		if symlink_dir(&outside, target.join("nested/link")).is_err() {
			return;
		}
		assert!(matches!(
			count_directory_tree(&target),
			Err(LocalDataError::UnsupportedEntry(_))
		));
	}
}
