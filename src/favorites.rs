//! Path based local favorites. Writes complete replacement files before committing.
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Favorites {
    pub paths: HashSet<PathBuf>,
}
impl Favorites {
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.to_string()),
        }
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let temporary = path.with_extension("tmp");
        std::fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(temporary, path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn favorites_roundtrip_and_bad_data_not_overwritten() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "favorites-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        let path = root.join("favorites.json");
        assert!(Favorites::load(&path).unwrap().paths.is_empty());
        let mut f = Favorites::default();
        f.paths.insert(root.join("Unicode-照片.png"));
        f.save(&path).unwrap();
        assert_eq!(Favorites::load(&path).unwrap().paths, f.paths);
        f.paths.clear();
        f.save(&path).unwrap();
        assert!(Favorites::load(&path).unwrap().paths.is_empty());
        std::fs::write(&path, b"broken favorites").unwrap();
        assert!(Favorites::load(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"broken favorites");
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
