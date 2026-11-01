//! Native libraries are loaded only from the application's trusted runtime directory.
use std::path::PathBuf;

pub fn runtime_dir() -> PathBuf {
    if cfg!(test) {
        return PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor/runtime");
    }
    let exe = std::env::current_exe().expect(media_browser::ui_text!(msg_649d18c0eaf7));
    let adjacent = exe.parent().unwrap().join("runtime");
    if adjacent.is_dir() {
        return adjacent;
    }
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if [source.join("target/release"), source.join("target/debug")]
        .iter()
        .any(|p| Some(p.as_path()) == exe.parent())
    {
        source.join("vendor/runtime")
    } else {
        adjacent
    }
}

pub unsafe fn load(name: &str) -> Result<libloading::Library, String> {
    let path = runtime_dir().join(name);
    #[cfg(windows)]
    {
        // Search only the DLL's own directory and Windows' system directories.
        unsafe { libloading::os::windows::Library::load_with_flags(&path, 0x100 | 0x800) }
            .map(Into::into)
            .map_err(|e| format!("{}: {e}", path.display()))
    }
    #[cfg(not(windows))]
    unsafe {
        libloading::Library::new(&path).map_err(|e| format!("{}: {e}", path.display()))
    }
}
