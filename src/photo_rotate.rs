//! Lossless orientation edits. Compressed pixels and unrelated metadata are retained.
use crate::domain::Fingerprint;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
static SERIAL: AtomicU64 = AtomicU64::new(0);

pub fn supported(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
        matches!(
            s.to_ascii_lowercase().as_str(),
            "jpg" | "jpeg" | "png" | "webp" | "tif" | "tiff"
        )
    })
}
fn turn(o: u16, clockwise: bool) -> Result<u16, String> {
    let map = if clockwise {
        [6, 7, 8, 5, 2, 3, 4, 1]
    } else {
        [8, 5, 6, 7, 4, 1, 2, 3]
    };
    map.get(o.wrapping_sub(1) as usize)
        .copied()
        .ok_or(crate::ui_text!(msg_42ee741d9565).into())
}
fn u16_at(b: &[u8], p: usize, le: bool) -> Result<u16, String> {
    let a: [u8; 2] = b
        .get(p..p + 2)
        .ok_or(crate::ui_text!(msg_42497205d344))?
        .try_into()
        .unwrap();
    Ok(if le {
        u16::from_le_bytes(a)
    } else {
        u16::from_be_bytes(a)
    })
}
fn u32_at(b: &[u8], p: usize, le: bool) -> Result<u32, String> {
    let a: [u8; 4] = b
        .get(p..p + 4)
        .ok_or(crate::ui_text!(msg_42497205d344))?
        .try_into()
        .unwrap();
    Ok(if le {
        u32::from_le_bytes(a)
    } else {
        u32::from_be_bytes(a)
    })
}
fn put16(b: &mut [u8], p: usize, v: u16, le: bool) {
    b[p..p + 2].copy_from_slice(&if le { v.to_le_bytes() } else { v.to_be_bytes() });
}
fn put32(b: &mut [u8], p: usize, v: u32, le: bool) {
    b[p..p + 4].copy_from_slice(&if le { v.to_le_bytes() } else { v.to_be_bytes() });
}
fn empty_tiff() -> Vec<u8> {
    vec![b'I', b'I', 42, 0, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0]
}
fn tiff_orientation(input: &[u8], clockwise: bool, single_page: bool) -> Result<Vec<u8>, String> {
    let mut b = input.to_vec();
    let le = match b.get(..2) {
        Some(b"II") => true,
        Some(b"MM") => false,
        _ => return Err(crate::ui_text!(msg_8b51506b04a1).into()),
    };
    if u16_at(&b, 2, le)? != 42 {
        return Err(crate::ui_text!(msg_eb562dad0107).into());
    }
    let start = u32_at(&b, 4, le)? as usize;
    let n = u16_at(&b, start, le)? as usize;
    let end = start
        .checked_add(2 + n * 12)
        .ok_or(crate::ui_text!(msg_9d97b824dda8))?;
    let next = u32_at(&b, end, le)?;
    if single_page && next != 0 {
        return Err(crate::ui_text!(msg_1dc76671f5a2).into());
    }
    let mut orientation_entry = None;
    for i in 0..n {
        let p = start + 2 + i * 12;
        if u16_at(&b, p, le)? == 0x112 {
            if orientation_entry.is_some()
                || u16_at(&b, p + 2, le)? != 3
                || u32_at(&b, p + 4, le)? != 1
            {
                return Err(crate::ui_text!(msg_d2f807ecb760).into());
            }
            orientation_entry = Some(p);
        }
    }
    if let Some(p) = orientation_entry {
        let value = turn(u16_at(&b, p + 8, le)?, clockwise)?;
        put16(&mut b, p + 8, value, le);
    } else {
        if n == u16::MAX as usize {
            return Err(crate::ui_text!(msg_eb1236676fc1).into());
        }
        let mut entries: Vec<Vec<u8>> = (0..n)
            .map(|i| b[start + 2 + i * 12..start + 14 + i * 12].to_vec())
            .collect();
        let mut entry = vec![0; 12];
        put16(&mut entry, 0, 0x112, le);
        put16(&mut entry, 2, 3, le);
        put32(&mut entry, 4, 1, le);
        put16(&mut entry, 8, turn(1, clockwise)?, le);
        entries.push(entry);
        entries.sort_by_key(|e| u16_at(e, 0, le).unwrap());
        if b.len() % 2 != 0 {
            b.push(0);
        }
        let offset = u32::try_from(b.len()).map_err(|_| crate::ui_text!(msg_0a7fefdd06a2))?;
        let mut ifd = vec![0; 2 + entries.len() * 12 + 4];
        put16(&mut ifd, 0, entries.len() as u16, le);
        for (i, e) in entries.iter().enumerate() {
            ifd[2 + i * 12..14 + i * 12].copy_from_slice(e);
        }
        put32(&mut ifd, 2 + entries.len() * 12, next, le);
        b.extend(ifd);
        put32(&mut b, 4, offset, le);
    }
    Ok(b)
}
fn exif_update(b: &[u8], clockwise: bool) -> Result<Vec<u8>, String> {
    let empty = empty_tiff();
    tiff_orientation(if b.is_empty() { &empty } else { b }, clockwise, false)
}
fn jpeg(b: &[u8], clockwise: bool) -> Result<Vec<u8>, String> {
    if !b.starts_with(&[255, 216]) {
        return Err(crate::ui_text!(msg_3ea94fce15a5).into());
    }
    let mut p = 2;
    let mut found = None;
    let mut saw_scan = false;
    while p < b.len() {
        let marker_start = p;
        if b[p] != 255 {
            return Err(crate::ui_text!(msg_3134b8d4c7c3).into());
        }
        while b.get(p) == Some(&255) {
            p += 1;
        }
        let marker = *b.get(p).ok_or(crate::ui_text!(msg_537ab60fcc47))?;
        p += 1;
        if marker == 0xda || marker == 0xd9 {
            saw_scan = marker == 0xda;
            break;
        }
        if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        let len = u16_at(b, p, false)? as usize;
        if len < 2 || p + len > b.len() {
            return Err(crate::ui_text!(msg_0713d9733bb3).into());
        }
        if marker == 0xe1 && b.get(p + 2..p + 8) == Some(b"Exif\0\0") {
            if found.is_some() {
                return Err(crate::ui_text!(msg_14cc2d8f7485).into());
            }
            found = Some((marker_start, p + len, p + 8));
        }
        p += len;
    }
    if !saw_scan {
        return Err(crate::ui_text!(msg_d281ee2dadfa).into());
    }
    let (start, end, tiff) = found.unwrap_or((2, 2, 2));
    let data = exif_update(if found.is_some() { &b[tiff..end] } else { &[] }, clockwise)?;
    let len = u16::try_from(data.len() + 8).map_err(|_| crate::ui_text!(msg_2a810e78038b))?;
    let mut out = Vec::with_capacity(b.len() + data.len() + 10);
    out.extend_from_slice(&b[..start]);
    out.extend([255, 225]);
    out.extend(len.to_be_bytes());
    out.extend(b"Exif\0\0");
    out.extend(data);
    out.extend_from_slice(&b[end..]);
    Ok(out)
}
fn crc(b: &[u8]) -> u32 {
    let mut c = !0u32;
    for x in b {
        c ^= *x as u32;
        for _ in 0..8 {
            c = (c >> 1) ^ (0xedb88320u32 & (0u32.wrapping_sub(c & 1)));
        }
    }
    !c
}
fn png(b: &[u8], clockwise: bool) -> Result<Vec<u8>, String> {
    if !b.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(crate::ui_text!(msg_44f1ff9e8d85).into());
    }
    let mut p = 8;
    let mut chunks = Vec::new();
    let mut exif = None;
    let mut ended = false;
    while p < b.len() {
        let n = u32_at(b, p, false)? as usize;
        let end = p
            .checked_add(n + 12)
            .ok_or(crate::ui_text!(msg_4229c39b5799))?;
        let chunk = b.get(p..end).ok_or(crate::ui_text!(msg_96fbebbf53ca))?;
        if crc(&chunk[4..n + 8]) != u32_at(chunk, n + 8, false)? {
            return Err(crate::ui_text!(msg_19cb8e295608).into());
        }
        match &chunk[4..8] {
            b"acTL" => return Err(crate::ui_text!(msg_a4a61b4f6a71).into()),
            b"eXIf" => {
                if exif.is_some() {
                    return Err(crate::ui_text!(msg_42fd489c90c4).into());
                }
                exif = Some(&chunk[8..8 + n]);
            }
            b"IEND" => ended = true,
            _ => {}
        }
        chunks.push(chunk);
        p = end;
    }
    if !ended {
        return Err(crate::ui_text!(msg_e951e20e3390).into());
    }
    let data = exif_update(exif.unwrap_or(&[]), clockwise)?;
    let mut new = Vec::new();
    new.extend((data.len() as u32).to_be_bytes());
    new.extend(b"eXIf");
    new.extend(data);
    new.extend(crc(&new[4..]).to_be_bytes());
    let mut out = b[..8].to_vec();
    let mut inserted = false;
    for c in chunks {
        if &c[4..8] == b"eXIf" {
            continue;
        }
        if !inserted && &c[4..8] == b"IDAT" {
            out.extend(&new);
            inserted = true;
        }
        out.extend(c);
    }
    if !inserted {
        return Err(crate::ui_text!(msg_297b8199d214).into());
    }
    Ok(out)
}
fn webp(b: &[u8], clockwise: bool) -> Result<Vec<u8>, String> {
    if b.get(..4) != Some(b"RIFF")
        || b.get(8..12) != Some(b"WEBP")
        || u32_at(b, 4, true)? as usize + 8 != b.len()
    {
        return Err(crate::ui_text!(msg_04105d199354).into());
    }
    let mut p = 12;
    let mut chunks = Vec::new();
    let mut exif = None;
    let mut extended = false;
    let mut dimensions = None;
    while p < b.len() {
        let n = u32_at(b, p + 4, true)? as usize;
        let end = p
            .checked_add(8 + n + (n % 2))
            .ok_or(crate::ui_text!(msg_93bbcc5ec589))?;
        let c = b.get(p..end).ok_or(crate::ui_text!(msg_be268c2c97c7))?;
        match &c[..4] {
            b"ANIM" | b"ANMF" => return Err(crate::ui_text!(msg_ab3aecd86267).into()),
            b"VP8X" => {
                if n != 10 || extended || c[8] & 2 != 0 {
                    return Err(crate::ui_text!(msg_81c34f7814e8).into());
                }
                extended = true;
            }
            b"EXIF" => {
                if exif.is_some() {
                    return Err(crate::ui_text!(msg_ab7758c89180).into());
                }
                let d = &c[8..8 + n];
                exif = Some(d.strip_prefix(b"Exif\0\0").unwrap_or(d));
            }
            b"VP8 " if n >= 10 && c.get(11..14) == Some(&[0x9d, 1, 0x2a]) => {
                dimensions = Some((
                    u16_at(c, 14, true)? as u32 & 0x3fff,
                    u16_at(c, 16, true)? as u32 & 0x3fff,
                    false,
                ));
            }
            b"VP8L" if n >= 5 && c[8] == 0x2f => {
                let x = u32_at(c, 9, true)?;
                dimensions = Some((
                    (x & 0x3fff) + 1,
                    ((x >> 14) & 0x3fff) + 1,
                    x & (1 << 28) != 0,
                ));
            }
            _ => {}
        }
        chunks.push(c);
        p = end;
    }
    let data = exif_update(exif.unwrap_or(&[]), clockwise)?;
    let mut out = b[..12].to_vec();
    if !extended {
        let (w, h, alpha) = dimensions.ok_or(crate::ui_text!(msg_5ba57344056d))?;
        if w == 0 || h == 0 {
            return Err(crate::ui_text!(msg_cd83cf9e5412).into());
        }
        out.extend(b"VP8X");
        out.extend(10u32.to_le_bytes());
        out.extend([8 | if alpha { 16 } else { 0 }, 0, 0, 0]);
        out.extend(&((w - 1).to_le_bytes())[..3]);
        out.extend(&((h - 1).to_le_bytes())[..3]);
    }
    for c in chunks {
        if &c[..4] == b"EXIF" {
            continue;
        }
        let start = out.len();
        out.extend(c);
        if &c[..4] == b"VP8X" {
            out[start + 8] |= 8;
        }
    }
    out.extend(b"EXIF");
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(&data);
    if data.len() % 2 != 0 {
        out.push(0);
    }
    let n = u32::try_from(out.len() - 8).map_err(|_| crate::ui_text!(msg_35e3c6d83d21))?;
    put32(&mut out, 4, n, true);
    Ok(out)
}
pub fn rotated_bytes(path: &Path, bytes: &[u8], clockwise: bool) -> Result<Vec<u8>, String> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => jpeg(bytes, clockwise),
        "png" => png(bytes, clockwise),
        "webp" => webp(bytes, clockwise),
        "tif" | "tiff" => tiff_orientation(bytes, clockwise, true),
        _ => Err(crate::ui_text!(msg_75b64763f77e).into()),
    }
}

/// Sibling temporary + OS replacement. Never truncate the original or fall back to delete.
pub fn save(path: &Path, expected: &Fingerprint, clockwise: bool) -> Result<(), String> {
    if !supported(path) {
        return Err(crate::ui_text!(msg_604ad9acc1ac).into());
    }
    let before = Fingerprint::read(path)?;
    if &before != expected {
        return Err(crate::ui_text!(msg_8e71c19c083c).into());
    }
    if before.bytes > 256 * 1024 * 1024 {
        return Err(crate::ui_text!(msg_01e66d9a7617).into());
    }
    for ancestor in path.ancestors() {
        let m = fs::symlink_metadata(ancestor).map_err(|e| e.to_string())?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if m.file_attributes() & 0x400 != 0 {
                return Err(crate::ui_text!(msg_e870ccd1db48).into());
            }
        }
        if m.file_type().is_symlink() {
            return Err(crate::ui_text!(msg_521691544690).into());
        }
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1 | 4);
    }
    let mut source = options.open(path).map_err(|e| e.to_string())?;
    let mut input = Vec::new();
    Read::by_ref(&mut source)
        .take(256 * 1024 * 1024 + 1)
        .read_to_end(&mut input)
        .map_err(|e| e.to_string())?;
    if input.len() as u64 != before.bytes {
        return Err(crate::ui_text!(msg_0b91760bf1c4).into());
    }
    let output = rotated_bytes(path, &input, clockwise)?;
    let parent = path.parent().ok_or(crate::ui_text!(msg_b6a54ae1312f))?;
    let suffix = format!(
        "{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    );
    let temp = parent.join(format!(".media-browser-rotate-{suffix}.tmp"));
    let backup = parent.join(format!(".media-browser-rotate-{suffix}.backup"));
    if backup.exists() {
        return Err(crate::ui_text!(msg_7fe71cc634f3).into());
    }
    let result = (|| {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        f.write_all(&output)
            .and_then(|_| f.sync_all())
            .map_err(|e| e.to_string())?;
        drop(f);
        if Fingerprint::read(path)? != before {
            return Err(crate::ui_text!(msg_e915282d03a2).into());
        }
        // ReplaceFile itself needs a write-sharing handle. Release our deny-write
        // reader only after the final version check; retain the OS backup until
        // its bytes are verified, so even a last-moment external edit is recoverable.
        drop(source);
        replace(path, &temp, &backup)?;
        #[cfg(windows)]
        if fs::read(&backup).map_err(|e| e.to_string())? != input {
            return Err(crate::ui_format!(msg_6ba8d2646ada, backup.display()));
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    // Backup exists only after a successful OS replace. Never remove it on failure.
    if result.is_ok() {
        if let Err(e) = fs::remove_file(&backup) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(crate::ui_format!(msg_d98a7f4041fb,
                    backup.display(),
                    e = e
                ));
            }
        }
    }
    result
}
#[cfg(windows)]
fn replace(path: &Path, temp: &Path, backup: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn ReplaceFileW(
            original: *const u16,
            replacement: *const u16,
            backup: *const u16,
            flags: u32,
            exclude: *mut std::ffi::c_void,
            reserved: *mut std::ffi::c_void,
        ) -> i32;
    }
    let wide = |p: &Path| {
        p.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let (a, b, c) = (wide(path), wide(temp), wide(backup));
    if unsafe {
        ReplaceFileW(
            a.as_ptr(),
            b.as_ptr(),
            c.as_ptr(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(crate::ui_format!(msg_f5533d81a313,
            std::io::Error::last_os_error(),
            backup.display()
        ));
    }
    Ok(())
}
#[cfg(not(windows))]
fn replace(path: &Path, temp: &Path, _backup: &Path) -> Result<(), String> {
    fs::rename(temp, path).map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageDecoder, ImageFormat, RgbImage};
    use std::io::Cursor;
    fn fixture(format: ImageFormat) -> Vec<u8> {
        let img = RgbImage::from_fn(32, 24, |x, y| {
            image::Rgb(if x < 16 {
                if y < 12 { [240, 20, 20] } else { [20, 20, 240] }
            } else if y < 12 {
                [20, 240, 20]
            } else {
                [240, 240, 20]
            })
        });
        let mut c = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(img)
            .write_to(&mut c, format)
            .unwrap();
        c.into_inner()
    }
    fn displayed(b: &[u8]) -> image::RgbaImage {
        let reader = image::ImageReader::new(Cursor::new(b))
            .with_guessed_format()
            .unwrap();
        let mut decoder = reader.into_decoder().unwrap();
        let orientation = decoder.orientation().unwrap();
        let mut img = DynamicImage::from_decoder(decoder).unwrap();
        img.apply_orientation(orientation);
        img.into_rgba8()
    }
    #[test]
    fn lossless_display_rotation_and_inverse_for_four_formats() {
        for (name, format) in [
            ("a.jpg", ImageFormat::Jpeg),
            ("a.png", ImageFormat::Png),
            ("a.webp", ImageFormat::WebP),
            ("a.tiff", ImageFormat::Tiff),
        ] {
            let original = fixture(format);
            let pixels = displayed(&original);
            let rotated = rotated_bytes(Path::new(name), &original, true).unwrap();
            assert_eq!(
                displayed(&rotated),
                image::imageops::rotate90(&pixels),
                "{name}"
            );
            let restored = rotated_bytes(Path::new(name), &rotated, false).unwrap();
            assert_eq!(displayed(&restored), pixels, "{name}");
            let mut four = original.clone();
            for _ in 0..4 {
                four = rotated_bytes(Path::new(name), &four, true).unwrap();
            }
            assert_eq!(displayed(&four), pixels);
            if format == ImageFormat::Jpeg {
                let sos = original.windows(2).position(|x| x == [255, 218]).unwrap();
                let rsos = rotated.windows(2).position(|x| x == [255, 218]).unwrap();
                assert_eq!(&original[sos..], &rotated[rsos..]);
            }
        }
    }
    #[test]
    fn all_existing_mirror_orientations_compose_with_rotation() {
        for o in 1..=8 {
            let mut tiff = tiff_orientation(&empty_tiff(), true, false).unwrap();
            let start = u32_at(&tiff, 4, true).unwrap() as usize;
            put16(&mut tiff, start + 10, o, true);
            let mut b = fixture(ImageFormat::Jpeg);
            let mut segment = vec![255, 225];
            segment.extend(((tiff.len() + 8) as u16).to_be_bytes());
            segment.extend(b"Exif\0\0");
            segment.extend(tiff);
            b.splice(2..2, segment);
            let expected = image::imageops::rotate90(&displayed(&b));
            assert_eq!(
                displayed(&jpeg(&b, true).unwrap()),
                expected,
                "orientation {o}"
            );
            assert_eq!(
                displayed(&jpeg(&b, false).unwrap()),
                image::imageops::rotate270(&displayed(&b))
            );
        }
    }
    #[test]
    fn malformed_and_unsupported_do_not_produce_output() {
        for name in [
            "a.heic", "a.gif", "a.bmp", "a.jpg", "a.png", "a.webp", "a.tiff",
        ] {
            assert!(rotated_bytes(Path::new(name), b"broken", true).is_err());
        }
        let mut b = fixture(ImageFormat::Png);
        b[20] ^= 1;
        assert!(png(&b, true).is_err());
        let mut b = empty_tiff();
        put32(&mut b, 10, 8, true);
        assert!(tiff_orientation(&b, true, true).is_err());
    }
    #[test]
    fn actual_replace_keeps_format_and_rejects_stale_version() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "rotation-owned-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir(&root).unwrap();
        let path = root.join("专属照片.jpg");
        let original = fixture(ImageFormat::Jpeg);
        fs::write(&path, &original).unwrap();
        let fingerprint = Fingerprint::read(&path).unwrap();
        save(&path, &fingerprint, true).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_eq!(
            displayed(&bytes),
            image::imageops::rotate90(&displayed(&original))
        );
        assert!(save(&path, &fingerprint, true).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    }
}

#[cfg(test)]
mod depth_tests {
    use super::*;
    use image::{DynamicImage, ImageBuffer, ImageDecoder, ImageFormat, Rgb};
    use std::io::Cursor;
    #[test]
    fn metadata_rotation_preserves_sixteen_bit_pixels() {
        for (name, format) in [("a.png", ImageFormat::Png), ("a.tif", ImageFormat::Tiff)] {
            let img = ImageBuffer::from_fn(9, 7, |x, y| {
                Rgb([((x * 1000) + 11) as u16, ((y * 2000) + 17) as u16, 54321u16])
            });
            let mut encoded = Cursor::new(Vec::new());
            DynamicImage::ImageRgb16(img.clone())
                .write_to(&mut encoded, format)
                .unwrap();
            let output = rotated_bytes(Path::new(name), encoded.get_ref(), true).unwrap();
            let mut decoder = image::ImageReader::new(Cursor::new(output))
                .with_guessed_format()
                .unwrap()
                .into_decoder()
                .unwrap();
            assert_eq!(decoder.color_type(), image::ColorType::Rgb16);
            let o = decoder.orientation().unwrap();
            let mut actual = DynamicImage::from_decoder(decoder).unwrap();
            actual.apply_orientation(o);
            assert_eq!(actual.to_rgb16(), image::imageops::rotate90(&img));
        }
    }
}
