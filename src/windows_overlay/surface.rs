//! GDI bitmap storage and native layered-window presentation.

use std::{ffi::c_void, mem::size_of, ptr::null_mut, slice};

use windows::{
	Win32::{
		Foundation::{COLORREF, HWND, POINT, RECT, SIZE},
		Graphics::Gdi::{
			AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
			CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, HBITMAP,
			HDC, HGDIOBJ, SelectObject,
		},
		UI::WindowsAndMessaging::{
			ULW_ALPHA, UPDATELAYEREDWINDOWINFO, UpdateLayeredWindow, UpdateLayeredWindowIndirect,
		},
	},
	core::Error as WinError,
};

use crate::{image_file::ImageData, model::DirtyRect};

pub struct DibSurface {
	device_context: HDC,
	bitmap: HBITMAP,
	previous_bitmap: HGDIOBJ,
	pixel_memory: *mut c_void,
	width: u32,
	height: u32,
}

impl DibSurface {
	pub unsafe fn create(image: &ImageData) -> windows::core::Result<Self> {
		let device_context = unsafe { CreateCompatibleDC(None) };
		if device_context.is_invalid() {
			return Err(WinError::from_win32());
		}

		let width = i32::try_from(image.width).map_err(|_| WinError::from_win32())?;
		let height = i32::try_from(image.height).map_err(|_| WinError::from_win32())?;
		let mut bitmap_info = BITMAPINFO::default();
		bitmap_info.bmiHeader = BITMAPINFOHEADER {
			biSize: size_of::<BITMAPINFOHEADER>() as u32,
			biWidth: width,
			biHeight: -height,
			biPlanes: 1,
			biBitCount: 32,
			biCompression: BI_RGB.0,
			..Default::default()
		};

		let mut pixel_memory: *mut c_void = null_mut();
		let bitmap = match unsafe {
			CreateDIBSection(
				device_context,
				&bitmap_info,
				DIB_RGB_COLORS,
				&mut pixel_memory,
				None,
				0,
			)
		} {
			Ok(bitmap) => bitmap,
			Err(error) => {
				let _ = unsafe { DeleteDC(device_context) };
				return Err(error);
			}
		};

		if pixel_memory.is_null() {
			let _ = unsafe { DeleteObject(bitmap) };
			let _ = unsafe { DeleteDC(device_context) };
			return Err(WinError::from_win32());
		}

		let previous_bitmap = unsafe { SelectObject(device_context, bitmap) };
		if previous_bitmap.is_invalid() {
			let _ = unsafe { DeleteObject(bitmap) };
			let _ = unsafe { DeleteDC(device_context) };
			return Err(WinError::from_win32());
		}

		let mut surface = Self {
			device_context,
			bitmap,
			previous_bitmap,
			pixel_memory,
			width: image.width,
			height: image.height,
		};
		unsafe {
			surface.update_region(
				image,
				DirtyRect {
					left: 0,
					top: 0,
					right: image.width as i32,
					bottom: image.height as i32,
				},
			);
		}
		Ok(surface)
	}

	pub unsafe fn update_region(&mut self, image: &ImageData, region: DirtyRect) {
		debug_assert_eq!(image.width, self.width);
		debug_assert_eq!(image.height, self.height);
		unsafe { copy_premultiplied_bgra_region(image, self.pixel_memory, region) };
	}

	pub unsafe fn present_full(
		&self,
		window: HWND,
		screen_x: i32,
		screen_y: i32,
		width: i32,
		height: i32,
	) -> windows::core::Result<()> {
		let destination = POINT {
			x: screen_x,
			y: screen_y,
		};
		let size = SIZE {
			cx: width,
			cy: height,
		};
		let source = POINT { x: 0, y: 0 };
		let alpha_blend = alpha_blend();

		unsafe {
			UpdateLayeredWindow(
				window,
				None,
				Some(&destination),
				Some(&size),
				self.device_context,
				Some(&source),
				COLORREF(0),
				Some(&alpha_blend),
				ULW_ALPHA,
			)
		}
	}

	pub unsafe fn present_region(
		&self,
		window: HWND,
		screen_x: i32,
		screen_y: i32,
		width: i32,
		height: i32,
		region: DirtyRect,
	) -> windows::core::Result<()> {
		let destination = POINT {
			x: screen_x,
			y: screen_y,
		};
		let size = SIZE {
			cx: width,
			cy: height,
		};
		let source = POINT { x: 0, y: 0 };
		let dirty_rectangle = RECT {
			left: region.left,
			top: region.top,
			right: region.right,
			bottom: region.bottom,
		};
		let alpha_blend = alpha_blend();
		let update = UPDATELAYEREDWINDOWINFO {
			cbSize: size_of::<UPDATELAYEREDWINDOWINFO>() as u32,
			pptDst: &destination,
			psize: &size,
			hdcSrc: self.device_context,
			pptSrc: &source,
			crKey: COLORREF(0),
			pblend: &alpha_blend,
			dwFlags: ULW_ALPHA,
			prcDirty: &dirty_rectangle,
			..Default::default()
		};

		if unsafe { UpdateLayeredWindowIndirect(window, &update) }.as_bool() {
			Ok(())
		} else {
			Err(WinError::from_win32())
		}
	}
}

impl Drop for DibSurface {
	fn drop(&mut self) {
		unsafe {
			let _ = SelectObject(self.device_context, self.previous_bitmap);
			let _ = DeleteObject(self.bitmap);
			let _ = DeleteDC(self.device_context);
		}
	}
}

fn alpha_blend() -> BLENDFUNCTION {
	BLENDFUNCTION {
		BlendOp: AC_SRC_OVER as u8,
		BlendFlags: 0,
		SourceConstantAlpha: 255,
		AlphaFormat: AC_SRC_ALPHA as u8,
	}
}

/// Layered windows need pixels in BGRA order with premultiplied alpha.
unsafe fn copy_premultiplied_bgra_region(
	image: &ImageData,
	destination: *mut c_void,
	region: DirtyRect,
) {
	let row_bytes = (region.right - region.left) as usize * 4;
	for y in region.top..region.bottom {
		let row_offset = (y as usize * image.width as usize + region.left as usize) * 4;
		let source_row = &image.rgba_pixels[row_offset..row_offset + row_bytes];
		let destination_row = unsafe {
			slice::from_raw_parts_mut(destination.cast::<u8>().add(row_offset), row_bytes)
		};

		for (rgba, bgra) in source_row
			.chunks_exact(4)
			.zip(destination_row.chunks_exact_mut(4))
		{
			if rgba[3] == 255 {
				bgra[0] = rgba[2];
				bgra[1] = rgba[1];
				bgra[2] = rgba[0];
				bgra[3] = 255;
				continue;
			}
			if rgba[3] == 0 {
				bgra.fill(0);
				continue;
			}
			let alpha = u16::from(rgba[3]);
			let premultiply = |channel: u8| ((u16::from(channel) * alpha + 127) / 255) as u8;
			bgra[0] = premultiply(rgba[2]);
			bgra[1] = premultiply(rgba[1]);
			bgra[2] = premultiply(rgba[0]);
			bgra[3] = rgba[3];
		}
	}
}
