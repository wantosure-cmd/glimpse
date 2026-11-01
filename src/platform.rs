//! OS-specific shell integration. UI/business modules pass Path rather than command text.
use std::{path::Path, process::Command};
pub fn open_folder(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    let mut command = Command::new("explorer.exe");
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(not(any(windows, target_os = "macos")))]
    return Err(media_browser::ui_text!(msg_ab0c6e1819ec).into());
    #[cfg(any(windows, target_os = "macos"))]
    {
        command
            .arg(shell_path(path))
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
pub fn reveal_file(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        Command::new("explorer.exe")
            .arg("/select,")
            .arg(shell_path(path))
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = path;
        Err(media_browser::ui_text!(msg_bbdc346833e1).into())
    }
}
pub(crate) fn shell_path(path: &Path) -> std::ffi::OsString {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        let wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        let prefix: Vec<u16> = "\\\\?\\UNC\\".encode_utf16().collect();
        let local: Vec<u16> = "\\\\?\\".encode_utf16().collect();
        if wide.starts_with(&prefix) {
            let mut result = vec![92, 92];
            result.extend_from_slice(&wide[prefix.len()..]);
            std::ffi::OsString::from_wide(&result)
        } else if wide.starts_with(&local) {
            std::ffi::OsString::from_wide(&wide[local.len()..])
        } else {
            path.as_os_str().to_owned()
        }
    }
    #[cfg(not(windows))]
    {
        path.as_os_str().to_owned()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(windows)]
    fn shell_paths_strip_verbatim_prefix_without_changing_unicode() {
        assert_eq!(
            shell_path(Path::new(r"\\?\C:\图集\1.jpg")),
            std::ffi::OsString::from(r"C:\图集\1.jpg")
        );
        assert_eq!(
            shell_path(Path::new(r"\\?\UNC\server\share\1.jpg")),
            std::ffi::OsString::from(r"\\server\share\1.jpg")
        );
    }
}
