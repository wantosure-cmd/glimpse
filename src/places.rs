//! Local-only directory visits and bookmarks. No file enumeration on the UI thread.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Places {
    pub entries: Vec<Place>,
    serial: u64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Place {
    pub path: PathBuf,
    pub visits: u64,
    pub last: u64,
    pub pinned: bool,
}
impl Places {
    pub fn visit(&mut self, path: &Path) {
        self.serial = self.serial.saturating_add(1);
        if let Some(p) = self.entries.iter_mut().find(|p| p.path == path) {
            p.visits = p.visits.saturating_add(1);
            p.last = self.serial;
        } else {
            self.entries.push(Place {
                path: path.into(),
                visits: 1,
                last: self.serial,
                pinned: false,
            });
        }
        if self.entries.len() > 256 {
            if let Some(i) = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, p)| !p.pinned)
                .min_by_key(|(_, p)| p.last)
                .map(|(i, _)| i)
            {
                self.entries.remove(i);
            }
        }
    }
    pub fn toggle_pin(&mut self, path: &Path) {
        if let Some(p) = self.entries.iter_mut().find(|p| p.path == path) {
            p.pinned = !p.pinned;
        }
    }
    pub fn list(&self, kind: usize) -> Vec<Place> {
        let mut list: Vec<_> = self
            .entries
            .iter()
            .filter(|p| kind != 0 || p.pinned)
            .cloned()
            .collect();
        list.sort_by_key(|p| {
            std::cmp::Reverse(if kind == 2 {
                (p.visits, p.last)
            } else {
                (p.last, p.visits)
            })
        });
        list.truncate(if kind == 0 { 32 } else { 12 });
        list
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visits_frequency_pin_and_persistence() {
        let mut p = Places::default();
        p.visit(Path::new("A"));
        p.visit(Path::new("B"));
        p.visit(Path::new("A"));
        p.toggle_pin(Path::new("B"));
        let p: Places = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
        assert_eq!(p.list(0)[0].path, Path::new("B"));
        assert_eq!(p.list(1)[0].path, Path::new("A"));
        assert_eq!(p.list(2)[0].visits, 2);
        assert_eq!(p.entries.len(), 2);
    }
}
