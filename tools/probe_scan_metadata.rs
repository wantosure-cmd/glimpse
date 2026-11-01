//! Standalone read-only Windows directory-metadata comparison, no Cargo dependencies.
use std::{fs, path::Path, time::Instant};
#[cfg(windows)]
fn attributes(m: &fs::Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;
    m.file_attributes()
}
#[cfg(not(windows))]
fn attributes(_: &fs::Metadata) -> u32 { 0 }
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arg = std::env::args_os().nth(1).ok_or("Missing read-only directory")?;
    let p = Path::new(&arg);
    let mut baseline = None;
    for (round, cached) in [false, true, true, false, true, false].into_iter().enumerate() {
        let start = Instant::now();
        let mut records = Vec::new();
        for item in fs::read_dir(p)? {
            let entry = item?;
            let m = if cached { entry.metadata()? } else { fs::symlink_metadata(entry.path())? };
            records.push((entry.file_name(), m.len(), m.modified()?, attributes(&m)));
        }
        records.sort_by(|a,b| a.0.cmp(&b.0));
        let ms = start.elapsed().as_secs_f64() * 1000.;
        let matches = baseline.as_ref().is_none_or(|old| old == &records);
        println!("{{\"round\":{round},\"directory_entry_metadata\":{cached},\"entries\":{},\"elapsed_ms\":{ms},\"attributes_match_baseline\":{matches}}}", records.len());
        if baseline.is_none() { baseline = Some(records); }
        if !matches { return Err("Live directory attributes changed; comparison not equivalent".into()); }
    }
    Ok(())
}
