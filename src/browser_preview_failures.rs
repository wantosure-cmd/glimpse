//! Variant errors remain available to rendering; status counts distinct media.
use media_browser::domain::MediaId;
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct PreviewFailures {
    variants: HashMap<String, (MediaId, String)>,
    media_variants: HashMap<MediaId, usize>,
}

impl PreviewFailures {
    pub fn insert(&mut self, key: String, id: MediaId, message: String) {
        if let Some((previous, _)) = self.variants.insert(key, (id, message)) {
            let count = self.media_variants.get_mut(&previous).expect("indexed failure");
            *count -= 1;
            if *count == 0 {
                self.media_variants.remove(&previous);
            }
        }
        *self.media_variants.entry(id).or_default() += 1;
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.variants.get(key).map(|(_, message)| message)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.variants.contains_key(key)
    }

    pub fn values(&self) -> impl Iterator<Item = &String> {
        self.variants.values().map(|(_, message)| message)
    }

    pub fn len(&self) -> usize {
        self.media_variants.len()
    }

    pub fn variant_len(&self) -> usize {
        self.variants.len()
    }

    pub fn is_empty(&self) -> bool {
        self.variants.is_empty()
    }

    pub fn clear(&mut self) {
        self.variants.clear();
        self.media_variants.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_media_without_losing_variant_errors() {
        let mut errors = PreviewFailures::default();
        for size in [0, 64, 512] {
            errors.insert(format!("bad-a-{size}"), 10, format!("size {size}"));
        }
        assert_eq!(errors.len(), 1);
        assert_eq!(errors.variant_len(), 3);
        assert_eq!(errors.get("bad-a-64").map(String::as_str), Some("size 64"));
        errors.insert("bad-b-64".into(), 20, "another file".into());
        assert_eq!(errors.len(), 2);
        assert_eq!(errors.variant_len(), 4);
        errors.insert("bad-a-64".into(), 10, "latest retry error".into());
        assert_eq!(errors.len(), 2);
        assert_eq!(errors.variant_len(), 4);
        assert_eq!(errors.get("bad-a-64").map(String::as_str), Some("latest retry error"));
        errors.clear();
        assert_eq!(errors.len(), 0);
        assert_eq!(errors.variant_len(), 0);
        assert!(errors.is_empty());
    }

    #[test]
    fn replacing_variant_owner_drops_old_media_only_after_last_variant() {
        let mut errors = PreviewFailures::default();
        errors.insert("small".into(), 1, "small".into());
        errors.insert("large".into(), 1, "large".into());
        errors.insert("small".into(), 2, "replacement".into());
        assert_eq!(errors.len(), 2);
        errors.insert("large".into(), 2, "replacement".into());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors.variant_len(), 2);
    }
}
