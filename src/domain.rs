use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaKind {
    Image,
    Video,
}

pub fn media_kind(path: &Path) -> Option<MediaKind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "apng" | "webp" | "bmp" | "tif" | "tiff" | "gif" | "heic"
        | "heif" => Some(MediaKind::Image),
        "mp4" | "m4v" | "mov" | "avi" | "3gp" | "3g2" | "mkv" | "webm" => Some(MediaKind::Video),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint {
    pub bytes: u64,
    pub modified_ns: i64,
}
impl Fingerprint {
    pub fn read(path: &Path) -> Result<Self, String> {
        let metadata = std::fs::metadata(path).map_err(|e| e.to_string())?;
        let modified = metadata.modified().map_err(|e| e.to_string())?;
        Ok(Self {
            bytes: metadata.len(),
            modified_ns: modified
                .duration_since(std::time::UNIX_EPOCH)
                .map(|time| time.as_nanos().min(i64::MAX as u128) as i64)
                .unwrap_or(0),
        })
    }
}

#[derive(Debug, Clone)]
pub struct MediaFile {
    pub path: PathBuf,
    pub parent: PathBuf,
    pub kind: MediaKind,
    pub fingerprint: Fingerprint,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub images: u64,
    pub videos: u64,
}

pub fn path_from_blob(bytes: &[u8]) -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        if bytes.len() % 2 != 0 {
            return Err(crate::ui_text!(msg_241605ca0960).into());
        }
        let wide: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        Ok(PathBuf::from(std::ffi::OsString::from_wide(&wide)))
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(PathBuf::from(std::ffi::OsString::from_vec(bytes.to_vec())))
    }
    #[cfg(not(any(windows, unix)))]
    {
        String::from_utf8(bytes.to_vec())
            .map(PathBuf::from)
            .map_err(|e| e.to_string())
    }
}

pub type MediaId = i64;
#[derive(Debug, Clone)]
pub struct MediaRecord {
    pub id: MediaId,
    pub file: MediaFile,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_ms: Option<u64>,
    pub revision: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ScanState {
    #[default]
    Unknown,
    Scanning,
    Complete,
    Partial,
}
#[derive(Debug, Clone)]
pub struct DirectoryRecord {
    pub path: PathBuf,
    pub modified_ns: Option<i64>,
    /// All indexed media bytes in this subtree, independent of view filters.
    pub recursive_bytes: u64,
    pub parent: Option<PathBuf>,
    pub direct: Counts,
    pub recursive: Counts,
    pub state: ScanState,
    pub subtree_complete: bool,
}

#[derive(Debug, Clone)]
pub struct LibrarySnapshot {
    pub generation: u64,
    pub root: PathBuf,
    pub files: Vec<MediaRecord>,
    pub directories: Vec<DirectoryRecord>,
    pub finished: bool,
    pub errors: Vec<String>,
    pub elapsed_ms: u64,
}
impl Counts {
    pub fn add(&mut self, kind: MediaKind) {
        match kind {
            MediaKind::Image => self.images += 1,
            MediaKind::Video => self.videos += 1,
        }
    }
    pub fn merge(&mut self, other: Self) {
        self.images += other.images;
        self.videos += other.videos;
    }
}

/// Stable natural comparison without integer parsing/overflow. Caller breaks ties by path.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    natural_cmp_folded(a.as_bytes(), b.as_bytes())
}

/// Compare already Unicode-lowercased UTF-8 keys using the same numeric rules.
pub(crate) fn natural_cmp_folded(a: &[u8], b: &[u8]) -> Ordering {
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let (start_a, start_b) = (i, j);
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let (mut za, mut zb) = (start_a, start_b);
            while za + 1 < i && a[za] == b'0' {
                za += 1;
            }
            while zb + 1 < j && b[zb] == b'0' {
                zb += 1;
            }
            let order = (i - za)
                .cmp(&(j - zb))
                .then_with(|| a[za..i].cmp(&b[zb..j]))
                .then_with(|| (i - start_a).cmp(&(j - start_b)));
            if order != Ordering::Equal {
                return order;
            }
        } else {
            let order = a[i].cmp(&b[j]);
            if order != Ordering::Equal {
                return order;
            }
            i += 1;
            j += 1;
        }
    }
    a.len().cmp(&b.len())
}

/// Lossless persistence representation; display strings must never serve as path identity.
pub fn path_blob(path: &Path) -> Vec<u8> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str()
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect()
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes().to_vec()
    }
    #[cfg(not(any(windows, unix)))]
    {
        path.to_string_lossy().as_bytes().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_order_without_overflow() {
        let mut v = vec![
            "10.jpg",
            "2.jpg",
            "01.jpg",
            "1.jpg",
            "999999999999999999999999999.jpg",
        ];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(
            v,
            vec![
                "1.jpg",
                "01.jpg",
                "2.jpg",
                "10.jpg",
                "999999999999999999999999999.jpg"
            ]
        );
    }
    #[test]
    fn unicode_and_case() {
        assert_eq!(natural_cmp("图集2A", "图集2a"), Ordering::Equal);
        assert_eq!(natural_cmp("图集2", "图集10"), Ordering::Less);
    }
    #[test]
    fn classification_excludes_archives() {
        assert_eq!(media_kind(Path::new("照片.HEIC")), Some(MediaKind::Image));
        assert_eq!(media_kind(Path::new("clip.3GP")), Some(MediaKind::Video));
        assert_eq!(media_kind(Path::new("photos.zip")), None);
    }
    #[cfg(windows)]
    #[test]
    fn paths_are_lossless_even_with_unpaired_surrogates() {
        use std::os::windows::ffi::OsStringExt;
        let path = PathBuf::from(std::ffi::OsString::from_wide(&[65, 0xd800, 66]));
        assert_eq!(path_blob(&path), vec![65, 0, 0, 216, 66, 0]);
    }
}
