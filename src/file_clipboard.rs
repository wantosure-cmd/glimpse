//! File transfer clipboard; Explorer owns the actual copy/move on paste.
#[cfg(windows)]
pub fn files(paths: &[std::path::PathBuf], cut: bool) -> Result<(), String> {
    use std::{ffi::c_void, os::windows::ffi::OsStrExt};
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetActiveWindow() -> *mut c_void;
        fn OpenClipboard(h: *mut c_void) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn SetClipboardData(f: u32, h: *mut c_void) -> *mut c_void;
        fn RegisterClipboardFormatW(s: *const u16) -> u32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalAlloc(flags: u32, n: usize) -> *mut c_void;
        fn GlobalLock(h: *mut c_void) -> *mut c_void;
        fn GlobalUnlock(h: *mut c_void) -> i32;
        fn GlobalFree(h: *mut c_void) -> *mut c_void;
    }
    unsafe fn block(bytes: &[u8]) -> Result<*mut c_void, String> {
        unsafe {
            let h = GlobalAlloc(2, bytes.len());
            if h.is_null() {
                return Err("Clipboard allocation failed".into());
            }
            let p = GlobalLock(h);
            if p.is_null() {
                GlobalFree(h);
                return Err("Clipboard lock failed".into());
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), p as *mut u8, bytes.len());
            GlobalUnlock(h);
            Ok(h)
        }
    }
    if paths.is_empty() {
        return Ok(());
    }
    let mut data = Vec::from(20u32.to_le_bytes());
    data.extend_from_slice(&[0; 12]);
    data.extend_from_slice(&1u32.to_le_bytes());
    for path in paths {
        for w in super::platform::shell_path(path)
            .encode_wide()
            .chain(Some(0))
        {
            data.extend_from_slice(&w.to_le_bytes());
        }
    }
    data.extend_from_slice(&[0, 0]);
    unsafe {
        let hd = block(&data)?;
        let effect = match block(&(if cut { 2u32 } else { 1u32 }).to_le_bytes()) {
            Ok(v) => v,
            Err(e) => {
                GlobalFree(hd);
                return Err(e);
            }
        };
        let name: Vec<_> = "Preferred DropEffect"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let format = RegisterClipboardFormatW(name.as_ptr());
        let owner = GetActiveWindow();
        if owner.is_null() || format == 0 || OpenClipboard(owner) == 0 {
            GlobalFree(hd);
            GlobalFree(effect);
            return Err("Clipboard is busy".into());
        }
        let result = if EmptyClipboard() == 0 {
            Err("Cannot clear clipboard".into())
        } else if SetClipboardData(15, hd).is_null() {
            Err("Cannot write file clipboard".into())
        } else {
            Ok(())
        };
        if result.is_err() {
            GlobalFree(hd);
            GlobalFree(effect);
        } else if SetClipboardData(format, effect).is_null() {
            GlobalFree(effect);
            CloseClipboard();
            return Err("Cannot write clipboard transfer mode".into());
        }
        CloseClipboard();
        result
    }
}
#[cfg(not(windows))]
pub fn files(_: &[std::path::PathBuf], _: bool) -> Result<(), String> {
    Err("File clipboard is not implemented on this platform".into())
}
