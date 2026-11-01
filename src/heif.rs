//! Small, isolated C ABI adapter. Own all buffers for the lifetime required by libheif.
use libloading::Library;
use std::{
    ffi::{CStr, c_char, c_void},
    path::Path,
    ptr,
};

#[repr(C)]
struct Error {
    code: i32,
    subcode: i32,
    message: *const c_char,
}
impl Error {
    unsafe fn check(self) -> Result<(), String> {
        if self.code == 0 {
            Ok(())
        } else {
            let msg = if self.message.is_null() {
                "unknown".into()
            } else {
                unsafe { CStr::from_ptr(self.message) }
                    .to_string_lossy()
                    .into_owned()
            };
            Err(media_browser::ui_format!(msg_c2005d8d96b2,
                self.code,
                self.subcode,
                msg = msg
            ))
        }
    }
}

// libheif heif_reader API v1: seekable, buffered input without a whole-file Vec.
#[repr(C)]
struct FileReaderApi {
    version: i32,
    position: unsafe extern "C" fn(*mut c_void) -> i64,
    read: unsafe extern "C" fn(*mut c_void, usize, *mut c_void) -> i32,
    seek: unsafe extern "C" fn(i64, *mut c_void) -> i32,
    wait: unsafe extern "C" fn(i64, *mut c_void) -> i32,
}
struct HeaderFile {
    input: std::io::BufReader<std::fs::File>,
    length: u64,
    requested: u64,
}
unsafe extern "C" fn header_position(user: *mut c_void) -> i64 {
    use std::io::Seek;
    unsafe {
        (&mut *user.cast::<HeaderFile>())
            .input
            .stream_position()
            .ok()
            .and_then(|p| i64::try_from(p).ok())
            .unwrap_or(-1)
    }
}
unsafe extern "C" fn header_read(data: *mut c_void, size: usize, user: *mut c_void) -> i32 {
    use std::io::Read;
    if size == 0 {
        return 0;
    }
    if data.is_null() || user.is_null() {
        return 1;
    }
    unsafe {
        let file = &mut *user.cast::<HeaderFile>();
        // A malformed metadata box must not request an unbounded allocation/read.
        if size > 64 * 1024 * 1024 || file.requested.saturating_add(size as u64) > 64 * 1024 * 1024
        {
            return 1;
        }
        file.requested += size as u64;
        if file
            .input
            .read_exact(std::slice::from_raw_parts_mut(data.cast::<u8>(), size))
            .is_ok()
        {
            0
        } else {
            1
        }
    }
}
unsafe extern "C" fn header_seek(position: i64, user: *mut c_void) -> i32 {
    use std::io::{Seek, SeekFrom};
    unsafe {
        let file = &mut *user.cast::<HeaderFile>();
        if position < 0 || position as u64 > file.length {
            return 1;
        }
        if file.input.seek(SeekFrom::Start(position as u64)).is_ok() {
            0
        } else {
            1
        }
    }
}
unsafe extern "C" fn header_wait(size: i64, user: *mut c_void) -> i32 {
    unsafe {
        if size >= 0 && size as u64 <= (*user.cast::<HeaderFile>()).length {
            0
        } else {
            2
        }
    }
}

pub struct Heif {
    lib: Library,
}
impl Heif {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            lib: unsafe { crate::native::load("heif.dll")? },
        })
    }
    pub fn version(&self) -> String {
        unsafe {
            let f = self
                .lib
                .get::<unsafe extern "C" fn() -> *const c_char>(b"heif_get_version\0")
                .unwrap();
            CStr::from_ptr(f()).to_string_lossy().into_owned()
        }
    }
    /// Parse container geometry, including HEIF rotation, without HEVC pixel decoding.
    pub fn dimensions(&self, path: &Path) -> Result<(u32, u32), String> {
        self.dimensions_with_read_count(path)
            .map(|(w, h, _)| (w, h))
    }
    pub fn dimensions_with_read_count(&self, path: &Path) -> Result<(u32, u32, u64), String> {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let length = file.metadata().map_err(|e| e.to_string())?.len();
        if length > 512 * 1024 * 1024 {
            return Err("HEIF container exceeds size limit".into());
        }
        let mut source = HeaderFile {
            input: std::io::BufReader::with_capacity(64 * 1024, file),
            length,
            requested: 0,
        };
        let reader = FileReaderApi {
            version: 1,
            position: header_position,
            read: header_read,
            seek: header_seek,
            wait: header_wait,
        };
        unsafe {
            let alloc = self
                .lib
                .get::<unsafe extern "C" fn() -> *mut c_void>(b"heif_context_alloc\0")
                .map_err(|e| e.to_string())?;
            let free = self
                .lib
                .get::<unsafe extern "C" fn(*mut c_void)>(b"heif_context_free\0")
                .map_err(|e| e.to_string())?;
            let read = self
                .lib
                .get::<unsafe extern "C" fn(
                    *mut c_void,
                    *const FileReaderApi,
                    *mut c_void,
                    *const c_void,
                ) -> Error>(b"heif_context_read_from_reader\0")
                .map_err(|e| e.to_string())?;
            let primary = self
                .lib
                .get::<unsafe extern "C" fn(*mut c_void, *mut *mut c_void) -> Error>(
                    b"heif_context_get_primary_image_handle\0",
                )
                .map_err(|e| e.to_string())?;
            let release = self
                .lib
                .get::<unsafe extern "C" fn(*mut c_void)>(b"heif_image_handle_release\0")
                .map_err(|e| e.to_string())?;
            let width = self
                .lib
                .get::<unsafe extern "C" fn(*const c_void) -> i32>(b"heif_image_handle_get_width\0")
                .map_err(|e| e.to_string())?;
            let height = self
                .lib
                .get::<unsafe extern "C" fn(*const c_void) -> i32>(
                    b"heif_image_handle_get_height\0",
                )
                .map_err(|e| e.to_string())?;
            let context = alloc();
            if context.is_null() {
                return Err(media_browser::ui_text!(msg_ff7f1d6e58be).into());
            }
            let result = (|| {
                read(
                    context,
                    &reader,
                    (&mut source as *mut HeaderFile).cast(),
                    ptr::null(),
                )
                .check()?;
                let mut handle = ptr::null_mut();
                primary(context, &mut handle).check()?;
                if handle.is_null() {
                    return Err("HEIF primary image missing".into());
                }
                let dimensions = (width(handle), height(handle));
                release(handle);
                if dimensions.0 <= 0 || dimensions.1 <= 0 {
                    return Err("Invalid HEIF dimensions".into());
                }
                Ok((dimensions.0 as u32, dimensions.1 as u32, source.requested))
            })();
            free(context);
            result
        }
    }
    pub fn decode(&self, path: &Path) -> Result<image::RgbaImage, String> {
        use std::io::Read;
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() > 512 * 1024 * 1024 {
            return Err(media_browser::ui_text!(msg_095968eeacbd).into());
        }
        let mut bytes = Vec::new();
        file.take(512 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 512 * 1024 * 1024 {
            return Err(media_browser::ui_text!(msg_095968eeacbd).into());
        }
        unsafe { self.decode_memory(&bytes) }
    }
    unsafe fn decode_memory(&self, bytes: &[u8]) -> Result<image::RgbaImage, String> {
        unsafe {
            macro_rules! sym {
                ($name:literal, $ty:ty) => {
                    *self
                        .lib
                        .get::<$ty>(concat!($name, "\0").as_bytes())
                        .map_err(|e| e.to_string())?
                };
            }
            let alloc = sym!("heif_context_alloc", unsafe extern "C" fn() -> *mut c_void);
            let free = sym!("heif_context_free", unsafe extern "C" fn(*mut c_void));
            let limit = sym!(
                "heif_context_set_maximum_image_size_limit",
                unsafe extern "C" fn(*mut c_void, i32)
            );
            let read = sym!(
                "heif_context_read_from_memory_without_copy",
                unsafe extern "C" fn(*mut c_void, *const c_void, usize, *const c_void) -> Error
            );
            let primary = sym!(
                "heif_context_get_primary_image_handle",
                unsafe extern "C" fn(*mut c_void, *mut *mut c_void) -> Error
            );
            let release_handle = sym!(
                "heif_image_handle_release",
                unsafe extern "C" fn(*mut c_void)
            );
            let decode = sym!(
                "heif_decode_image",
                unsafe extern "C" fn(
                    *const c_void,
                    *mut *mut c_void,
                    i32,
                    i32,
                    *const c_void,
                ) -> Error
            );
            let release_img = sym!("heif_image_release", unsafe extern "C" fn(*mut c_void));
            let width = sym!(
                "heif_image_get_width",
                unsafe extern "C" fn(*const c_void, i32) -> i32
            );
            let height = sym!(
                "heif_image_get_height",
                unsafe extern "C" fn(*const c_void, i32) -> i32
            );
            let plane = sym!(
                "heif_image_get_plane_readonly2",
                unsafe extern "C" fn(*const c_void, i32, *mut usize) -> *const u8
            );
            let ctx = alloc();
            if ctx.is_null() {
                return Err(media_browser::ui_text!(msg_ff7f1d6e58be).into());
            }
            limit(ctx, 10000);
            let mut handle = ptr::null_mut();
            let mut img = ptr::null_mut();
            let result = (|| {
                read(ctx, bytes.as_ptr().cast(), bytes.len(), ptr::null()).check()?;
                primary(ctx, &mut handle).check()?;
                // RGB / interleaved RGBA. Default options apply container transforms.
                decode(handle, &mut img, 1, 11, ptr::null()).check()?;
                let (w, h) = (width(img, 10), height(img, 10));
                if w <= 0 || h <= 0 || (w as u64) * (h as u64) > 100_000_000 {
                    return Err(media_browser::ui_text!(msg_7429f31a8d69).into());
                }
                let mut stride = 0usize;
                let data = plane(img, 10, &mut stride);
                let row = w as usize * 4;
                if data.is_null() || stride < row {
                    return Err(media_browser::ui_text!(msg_f85c27093806).into());
                }
                let mut rgba = vec![0; row * h as usize];
                for y in 0..h as usize {
                    ptr::copy_nonoverlapping(
                        data.add(y * stride),
                        rgba.as_mut_ptr().add(y * row),
                        row,
                    );
                }
                let mut output = image::RgbaImage::from_raw(w as u32, h as u32, rgba)
                    .ok_or_else(|| media_browser::ui_text!(msg_adaf6c238f5a).to_string())?;
                let profile_size = sym!(
                    "heif_image_get_raw_color_profile_size",
                    unsafe extern "C" fn(*const c_void) -> usize
                );
                let profile_read = sym!(
                    "heif_image_get_raw_color_profile",
                    unsafe extern "C" fn(*const c_void, *mut c_void) -> Error
                );
                let size = profile_size(img);
                if size > 0 && size <= 4 * 1024 * 1024 {
                    let mut profile = vec![0; size];
                    if profile_read(img, profile.as_mut_ptr().cast())
                        .check()
                        .is_ok()
                    {
                        if let Err(e) = media_browser::color::icc_to_srgb(&mut output, &profile) {
                            eprintln!("{}", media_browser::ui_format!(msg_4f036fc3ae7a, e = e));
                        }
                    }
                }
                Ok(output)
            })();
            if !img.is_null() {
                release_img(img);
            }
            if !handle.is_null() {
                release_handle(handle);
            }
            free(ctx);
            result
        }
    }
}
