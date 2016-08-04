use std::ptr;
use std::slice;

use xcb;
use ffi::image::*;

pub struct Image {
	ptr:   *mut xcb_image_t,
	owned: bool,
}

impl Image {
	pub fn annotate(&self) {
		unsafe {
			xcb_image_annotate(self.ptr)
		}
	}

	pub fn width(&self) -> u16 {
		unsafe {
			(*self.ptr).width
		}
	}

	pub fn height(&self) -> u16 {
		unsafe {
			(*self.ptr).height
		}
	}

	pub fn format(&self) -> xcb::ImageFormat {
		unsafe {
			(*self.ptr).format
		}
	}

	pub fn scanline_pad(&self) -> u8 {
		unsafe {
			(*self.ptr).scanline_pad
		}
	}

	pub fn depth(&self) -> u8 {
		unsafe {
			(*self.ptr).depth
		}
	}

	pub fn bpp(&self) -> u8 {
		unsafe {
			(*self.ptr).bpp
		}
	}

	pub fn unit(&self) -> u8 {
		unsafe {
			(*self.ptr).unit
		}
	}

	pub fn plane_mask(&self) -> u32 {
		unsafe {
			(*self.ptr).plane_mask
		}
	}

	pub fn byte_order(&self) -> xcb::ImageOrder {
		unsafe {
			(*self.ptr).byte_order
		}
	}

	pub fn bit_order(&self) -> xcb::ImageOrder {
		unsafe {
			(*self.ptr).bit_order
		}
	}

	pub fn stride(&self) -> u32 {
		unsafe {
			(*self.ptr).stride
		}
	}

	pub fn size(&self) -> u32 {
		unsafe {
			(*self.ptr).size
		}
	}

	pub fn data(&self) -> &[u8] {
		unsafe {
			slice::from_raw_parts((*self.ptr).data, (*self.ptr).size as usize)
		}
	}

	pub fn put(&mut self, x: u32, y: u32, pixel: u32) {
		unsafe {
			xcb_image_put_pixel(self.ptr, x, y, pixel)
		}
	}

	pub fn get(&self, x: u32, y: u32) -> u32 {
		unsafe {
			xcb_image_get_pixel(self.ptr, x, y)
		}
	}
}

impl Drop for Image {
	fn drop(&mut self) {
		unsafe {
			if !self.owned {
				(*self.ptr).data = ptr::null_mut();
			}

			xcb_image_destroy(self.ptr);
		}
	}
}

pub fn get(c: &xcb::Connection, drawable: xcb::Drawable, x: i16, y: i16, width: u16, height: u16, plane_mask: u32, format: xcb::ImageFormat) -> Result<Image, ()> {
	unsafe {
		let ptr = xcb_image_get(c.get_raw_conn(), drawable, x, y, width, height, plane_mask, format);

		if ptr.is_null() {
			Err(())
		}
		else {
			Ok(Image {
				ptr:   ptr,
				owned: true,
			})
		}
	}
}

pub fn put(c: &xcb::Connection, drawable: xcb::Drawable, gc: xcb::Gcontext, image: &Image, x: i16, y: i16) -> xcb::VoidCookie {
	unsafe {
		xcb::VoidCookie {
			cookie:  xcb_image_put(c.get_raw_conn(), drawable, gc, image.ptr, x, y, 0),
			conn:    c.get_raw_conn(),
			checked: false,
		}
	}
}

pub fn is_native(c: &xcb::Connection, image: &Image) -> bool {
	unsafe {
		xcb_image_native(c.get_raw_conn(), image.ptr, 0) == image.ptr
	}
}

pub fn to_native(c: &xcb::Connection, image: &Image) -> Option<Image> {
	unsafe {
		let ptr = xcb_image_native(c.get_raw_conn(), image.ptr, 1);

		if ptr == image.ptr || ptr.is_null() {
			None
		}
		else {
			Some(Image {
				ptr:   ptr,
				owned: true,
			})
		}
	}
}

#[cfg(feature = "shm")]
pub mod shm {
	use std::ptr;
	use std::ops::{Deref, DerefMut};

	use xcb;
	use xcb::ffi::*;
	use xcb::ffi::shm::*;
	use ffi::image::*;
	use libc::{shmget, shmat, shmdt, shmctl, IPC_CREAT, IPC_RMID};

	pub struct Image {
		conn: *mut xcb_connection_t,
		base: super::Image,
		shm:  xcb_shm_segment_info_t,
	}

	pub fn create(c: &xcb::Connection, depth: u8, width: u16, height: u16) -> Result<Image, ()> {
		unsafe {
			let setup  = c.get_setup();
			let format = try!(setup.pixmap_formats().find(|f| f.depth() == depth).ok_or(()));
			let image  = xcb_image_create(width, height, xcb::IMAGE_FORMAT_Z_PIXMAP,
				format.scanline_pad(), format.depth(), format.bits_per_pixel(),
				setup.bitmap_format_scanline_unit(), setup.image_byte_order() as u32, setup.bitmap_format_bit_order() as u32,
				ptr::null_mut(), !0, ptr::null_mut());

			if image.is_null() {
				return Err(());
			}

			let id = match shmget(0, (*image).size as usize, IPC_CREAT | 0o666) {
				-1 => {
					xcb_image_destroy(image);
					return Err(());
				}

				id => id
			};

			let addr = match shmat(id, ptr::null(), 0) {
				addr if addr as isize == -1 => {
					xcb_image_destroy(image);
					shmctl(id, IPC_RMID, ptr::null_mut());

					return Err(());
				}

				addr => addr
			};

			let seg = c.generate_id();
			xcb::shm::attach(c, seg, id as u32, false);
			(*image).data = addr as *mut _;

			Ok(Image {
				conn: c.get_raw_conn(),

				base: super::Image {
					ptr:   image,
					owned: false,
				},

				shm:   xcb_shm_segment_info_t {
					shmseg:  seg,
					shmid:   id as u32,
					shmaddr: addr as *mut _,
				}
			})
		}
	}

	impl Deref for Image {
		type Target = super::Image;

		fn deref(&self) -> &Self::Target {
			&self.base
		}
	}

	impl DerefMut for Image {
		fn deref_mut(&mut self) -> &mut Self::Target {
			&mut self.base
		}
	}

	impl Drop for Image {
		fn drop(&mut self) {
			unsafe {
				xcb_shm_detach(self.conn, self.shm.shmseg);
				shmdt(self.shm.shmaddr as *mut _);
				shmctl(self.shm.shmid as i32, IPC_RMID, ptr::null_mut());
			}
		}
	}

	pub fn get<'a>(c: &xcb::Connection, drawable: xcb::Drawable, image: &'a mut Image, x: i16, y: i16, plane_mask: u32) -> Result<&'a mut Image, ()> {
		unsafe {
			match xcb_image_shm_get(c.get_raw_conn(), drawable, image.base.ptr, image.shm, x, y, plane_mask) {
				1 => Ok(image),
				_ => Err(())
			}
		}
	}

	pub fn put<'a>(c: &xcb::Connection, drawable: xcb::Drawable, gc: xcb::Gcontext, image: &'a Image, src_x: i16, src_y: i16, dest_x: i16, dest_y: i16, src_width: u16, src_height: u16, send_event: bool) -> Result<&'a Image, ()> {
		unsafe {
			if !xcb_image_shm_put(c.get_raw_conn(), drawable, gc, image.base.ptr, image.shm, src_x, src_y, dest_x, dest_y, src_width, src_height, send_event as u8).is_null() {
				Ok(image)
			}
			else {
				Err(())
			}
		}
	}
}