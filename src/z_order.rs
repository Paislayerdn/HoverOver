//! Z values are unique u16 layers: larger values draw in front, and existing
//! layers are swapped or moved to the nearest free outside value without renumbering.

use std::{collections::HashSet, error::Error, fmt};

use crate::model::ImageObject;

pub(crate) const FIRST_IMAGE_Z: u16 = 0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ZOrderError {
	DuplicateZ(u16),
	NoRoomForNewImage,
	SelectedImageMissing,
}

impl fmt::Display for ZOrderError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::DuplicateZ(value) => write!(formatter, "Another image already uses Z {value}."),
			Self::NoRoomForNewImage => {
				write!(formatter, "The scene has no room for another image.")
			}
			Self::SelectedImageMissing => {
				write!(formatter, "There is no selected image to arrange.")
			}
		}
	}
}

impl Error for ZOrderError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ZChange {
	pub index: usize,
	pub new_z: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ArrangeAvailability {
	pub bring_forward: bool,
	pub send_backward: bool,
	pub bring_to_front: bool,
	pub send_to_back: bool,
}

pub(crate) fn back_to_front_indices(z_values: &[u16]) -> Vec<usize> {
	let mut indices: Vec<usize> = (0..z_values.len()).collect();
	indices.sort_by_key(|index| z_values[*index]);
	indices
}

pub(crate) fn image_back_to_front_indices(images: &[ImageObject]) -> Vec<usize> {
	let z_values: Vec<u16> = images.iter().map(|image| image.placement.z_order).collect();
	back_to_front_indices(&z_values)
}

pub(crate) fn next_new_image_z(z_values: &[u16]) -> Option<u16> {
	match z_values.iter().copied().max() {
		None => Some(FIRST_IMAGE_Z),
		Some(maximum) => maximum.checked_add(1),
	}
}

pub(crate) fn bring_forward(z_values: &[u16], selected_index: usize) -> Option<Vec<ZChange>> {
	let selected_z = *z_values.get(selected_index)?;
	let neighbor_index = z_values
		.iter()
		.enumerate()
		.filter(|(_, z)| **z > selected_z)
		.min_by_key(|(_, z)| **z)?
		.0;
	swap_changes(z_values, selected_index, neighbor_index)
}

pub(crate) fn send_backward(z_values: &[u16], selected_index: usize) -> Option<Vec<ZChange>> {
	let selected_z = *z_values.get(selected_index)?;
	let neighbor_index = z_values
		.iter()
		.enumerate()
		.filter(|(_, z)| **z < selected_z)
		.max_by_key(|(_, z)| **z)?
		.0;
	swap_changes(z_values, selected_index, neighbor_index)
}

pub(crate) fn bring_to_front(z_values: &[u16], selected_index: usize) -> Option<Vec<ZChange>> {
	let selected_z = *z_values.get(selected_index)?;
	let maximum = z_values.iter().copied().max()?;
	if selected_z == maximum {
		return None;
	}
	Some(vec![ZChange {
		index: selected_index,
		new_z: maximum.checked_add(1)?,
	}])
}

pub(crate) fn send_to_back(z_values: &[u16], selected_index: usize) -> Option<Vec<ZChange>> {
	let selected_z = *z_values.get(selected_index)?;
	let minimum = z_values.iter().copied().min()?;
	if selected_z == minimum {
		return None;
	}
	Some(vec![ZChange {
		index: selected_index,
		new_z: minimum.checked_sub(1)?,
	}])
}

pub(crate) fn arrange_availability(z_values: &[u16], selected_index: usize) -> ArrangeAvailability {
	ArrangeAvailability {
		bring_forward: bring_forward(z_values, selected_index).is_some(),
		send_backward: send_backward(z_values, selected_index).is_some(),
		bring_to_front: bring_to_front(z_values, selected_index).is_some(),
		send_to_back: send_to_back(z_values, selected_index).is_some(),
	}
}

pub(crate) fn check_z_available(
	z_values: &[u16],
	selected_index: usize,
	wanted_z: u16,
) -> Result<(), ZOrderError> {
	let current_z = *z_values
		.get(selected_index)
		.ok_or(ZOrderError::SelectedImageMissing)?;
	if current_z == wanted_z {
		return Ok(());
	}
	if z_values
		.iter()
		.enumerate()
		.any(|(index, value)| index != selected_index && *value == wanted_z)
	{
		return Err(ZOrderError::DuplicateZ(wanted_z));
	}
	Ok(())
}

pub(crate) fn validate_unique(z_values: &[u16]) -> Result<(), ZOrderError> {
	let mut seen = HashSet::with_capacity(z_values.len());
	for value in z_values {
		if !seen.insert(*value) {
			return Err(ZOrderError::DuplicateZ(*value));
		}
	}
	Ok(())
}

fn swap_changes(z_values: &[u16], first_index: usize, second_index: usize) -> Option<Vec<ZChange>> {
	let first_z = *z_values.get(first_index)?;
	let second_z = *z_values.get(second_index)?;
	Some(vec![
		ZChange {
			index: first_index,
			new_z: second_z,
		},
		ZChange {
			index: second_index,
			new_z: first_z,
		},
	])
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn arrange_operations_swap_nearest_neighbors_and_move_outside_extremes() {
		assert_eq!(bring_forward(&[10, 40, 90], 0).unwrap()[0].new_z, 40);
		assert_eq!(send_backward(&[10, 40, 90], 2).unwrap()[0].new_z, 40);
		assert_eq!(bring_forward(&[10, 40, 90], 2), None);
		assert_eq!(send_backward(&[10, 40, 90], 0), None);
		assert_eq!(bring_to_front(&[10, 40, 90], 0).unwrap()[0].new_z, 91);
		assert_eq!(send_to_back(&[10, 40, 90], 2).unwrap()[0].new_z, 9);
		assert_eq!(send_to_back(&[0, 10], 1), None);
		assert_eq!(bring_to_front(&[10, u16::MAX], 0), None);
		assert!(bring_forward(&[10, 40, 90], 0).unwrap().len() == 2);
	}

	#[test]
	fn new_z_and_validation_use_checked_unique_values() {
		assert_eq!(next_new_image_z(&[]), Some(FIRST_IMAGE_Z));
		assert_eq!(next_new_image_z(&[FIRST_IMAGE_Z]), Some(1));
		assert_eq!(next_new_image_z(&[4, 9]), Some(10));
		assert_eq!(next_new_image_z(&[u16::MAX]), None);
		assert!(check_z_available(&[2, 4], 0, 2).is_ok());
		assert!(matches!(
			check_z_available(&[2, 4], 0, 4),
			Err(ZOrderError::DuplicateZ(4))
		));
		assert!(validate_unique(&[1, 2, 3]).is_ok());
		assert!(matches!(
			validate_unique(&[1, 2, 1]),
			Err(ZOrderError::DuplicateZ(1))
		));
	}
}
