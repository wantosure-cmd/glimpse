//! Read-only path resolution for virtual filesystems without GetFinalPathName support.
use std::{
    io,
    path::{Path, PathBuf},
};

pub fn resolve_read(path: &Path) -> io::Result<PathBuf> {
    match std::fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(error) => {
            #[cfg(windows)]
            if matches!(error.raw_os_error(), Some(1 | 50 | 1005)) {
                // CloudDrive2 can enumerate/stat/open a path while final-path lookup
                // reports ERROR_UNRECOGNIZED_VOLUME. Never apply this to deletion.
                let path = std::path::absolute(path)?;
                let metadata = std::fs::symlink_metadata(&path)?;
                if !metadata.file_type().is_symlink() {
                    return Ok(path);
                }
            }
            Err(error)
        }
    }
}

/// Scanner outputs canonical verbatim paths on Windows except the virtual-volume fallback.
pub fn virtual_hint(path: &Path) -> bool {
    let text = path.as_os_str().to_string_lossy();
    cfg!(windows)
        && path.is_absolute()
        && (text.starts_with(r"\\?\UNC\")
            || (text.starts_with(r"\\") && !text.starts_with(r"\\?\"))
            || !text.starts_with(r"\\?\"))
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn unc_paths_use_network_budget_including_verbatim_prefix() {
        use super::*;
        assert!(virtual_hint(Path::new(r"\\server\share\media")));
        assert!(virtual_hint(Path::new(r"\\?\UNC\server\share\media")));
        assert!(!virtual_hint(Path::new(r"\\?\C:\media")));
        assert!(!virtual_hint(Path::new("relative")));
    }
}
