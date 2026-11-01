//! Bounded ISO BMFF display-header fast path. Unknown structures fall back to FFprobe.
//! Layout reference: https://developer.apple.com/documentation/quicktime-file-format/track_header_atom
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};
#[derive(serde::Serialize)]
pub struct VideoHeader {
    pub width: u32,
    pub height: u32,
    pub duration_ms: Option<u64>,
    pub fragmented: bool,
}
fn u32be(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?))
}
fn u64be(data: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(data.get(at..at + 8)?.try_into().ok()?))
}
fn boxes(mut data: &[u8]) -> impl Iterator<Item = ([u8; 4], &[u8])> {
    let mut count = 0;
    std::iter::from_fn(move || {
        count += 1;
        if count > 4096 {
            return None;
        }
        let mut size = u32be(data, 0)? as usize;
        let kind = data.get(4..8)?.try_into().ok()?;
        let head = if size == 1 {
            size = usize::try_from(u64be(data, 8)?).ok()?;
            16
        } else {
            8
        };
        if size == 0 {
            size = data.len();
        }
        if size < head || size > data.len() {
            return None;
        }
        let body = &data[head..size];
        data = &data[size..];
        Some((kind, body))
    })
}
fn child<'a>(data: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(data).find(|(k, _)| k == kind).map(|(_, b)| b)
}
fn parse(data: &[u8], allow_non_square: bool) -> Option<VideoHeader> {
    // Aperture semantics stay with the complete decoder; SAR requires agreement
    // with integral track geometry and the conservative duration policy below.
    if data
        .windows(4)
        .any(|k| matches!(k, b"clap" | b"tapt" | b"cmov"))
    {
        return None;
    }
    let movie = child(data, b"mvhd")?;
    let (scale, duration) = match *movie.first()? {
        0 => (u32be(movie, 12)?, u64::from(u32be(movie, 16)?)),
        1 => (u32be(movie, 20)?, u64be(movie, 24)?),
        _ => return None,
    };
    let fragmented = child(data, b"mvex").is_some();
    let duration_ms =
        if scale > 0 && duration > 0 && duration != u64::MAX && duration != u64::from(u32::MAX) {
            Some(duration.checked_mul(1000)?.checked_div(u64::from(scale))?)
        } else if fragmented {
            None
        } else {
            return None;
        };
    let mut result = None;
    for (_, track) in boxes(data).filter(|(kind, _)| kind == b"trak") {
        let media = child(track, b"mdia")?;
        if child(media, b"hdlr")?.get(8..12)? != b"vide" {
            continue;
        }
        if result.is_some() {
            return None;
        } // Alternate video tracks need full probing.
        let stsd = child(child(child(media, b"minf")?, b"stbl")?, b"stsd")?;
        if u32be(stsd, 4)? != 1 {
            return None;
        }
        let (codec, description) = boxes(stsd.get(8..)?).next()?;
        if !matches!(
            &codec,
            b"avc1" | b"avc3" | b"hvc1" | b"hev1" | b"mp4v" | b"s263" | b"av01" | b"vp09"
        ) {
            return None;
        }
        let encoded_width = u16::from_be_bytes(description.get(24..26)?.try_into().ok()?) as u32;
        let encoded_height = u16::from_be_bytes(description.get(26..28)?.try_into().ok()?) as u32;
        let mut display_width = encoded_width;
        if let Some(pasp) = child(description.get(78..)?, b"pasp") {
            let horizontal = u32be(pasp, 0)?;
            let vertical = u32be(pasp, 4)?;
            if horizontal == 0
                || vertical == 0
                || ((!allow_non_square || fragmented) && horizontal != vertical)
            {
                return None;
            }
            // Fractional movie milliseconds can round above FFprobe's stream
            // duration. Keep those new SAR cases on the historical full probe.
            // Require whole seconds and matching track durations for all tracks.
            if horizontal != vertical {
                if scale == 0 || duration % u64::from(scale) != 0 {
                    return None;
                }
                for (_, other) in boxes(data).filter(|(kind, _)| kind == b"trak") {
                    let track_header = child(other, b"tkhd")?;
                    let track_duration = match *track_header.first()? {
                        0 => u64::from(u32be(track_header, 20)?),
                        1 => u64be(track_header, 28)?,
                        _ => return None,
                    };
                    if track_duration != duration {
                        return None;
                    }
                }
            }
            // Match the complete probe's positive nearest-integer SAR width.
            // Bound arithmetic and still demand exact agreement with tkhd;
            // don't guess around aperture,crop,fractional track or matrix data.
            let scaled = u64::from(encoded_width).checked_mul(u64::from(horizontal))?;
            display_width = u32::try_from(
                scaled
                    .checked_add(u64::from(vertical) / 2)?
                    .checked_div(u64::from(vertical))?,
            )
            .ok()?;
        }
        let header = child(track, b"tkhd")?;
        let matrix = match *header.first()? {
            0 => 40,
            1 => 52,
            _ => return None,
        };
        let fixed_width = u32be(header, matrix + 36)?;
        let fixed_height = u32be(header, matrix + 40)?;
        if fixed_width & 65535 != 0 || fixed_height & 65535 != 0 {
            return None;
        }
        let mut width = fixed_width >> 16;
        let mut height = fixed_height >> 16;
        if (width, height) != (display_width, encoded_height) {
            return None;
        }
        let a = u32be(header, matrix)? as i32;
        if u32be(header, matrix + 8)? != 0
            || u32be(header, matrix + 20)? != 0
            || u32be(header, matrix + 32)? != 1 << 30
        {
            return None;
        }
        let b = u32be(header, matrix + 4)? as i32;
        let c = u32be(header, matrix + 12)? as i32;
        let d = u32be(header, matrix + 16)? as i32;
        if a == 0 && d == 0 && b.unsigned_abs() == 65536 && c.unsigned_abs() == 65536 {
            std::mem::swap(&mut width, &mut height);
        } else if !(a.unsigned_abs() == 65536 && d.unsigned_abs() == 65536 && b == 0 && c == 0) {
            return None;
        }
        if width == 0 || height == 0 || width > 16384 || height > 16384 {
            return None;
        }
        result = Some(VideoHeader {
            width,
            height,
            duration_ms,
            fragmented,
        });
    }
    result
}
pub fn probe(path: &Path) -> Option<VideoHeader> {
    probe_with_policy(path, true)
}
/// Preserve the historical strict-square path for same-build comparisons.
pub fn probe_square_pixels_only(path: &Path) -> Option<VideoHeader> {
    probe_with_policy(path, false)
}
fn probe_with_policy(path: &Path, allow_non_square: bool) -> Option<VideoHeader> {
    if !path.extension().is_some_and(|e| {
        ["mp4", "mov", "m4v", "3gp", "3g2"]
            .iter()
            .any(|x| e.eq_ignore_ascii_case(x))
    }) {
        return None;
    }
    let mut file = File::open(path).ok()?;
    let end = file.metadata().ok()?.len();
    let mut offset = 0u64;
    for _ in 0..128 {
        if offset.checked_add(8)? > end {
            return None;
        }
        file.seek(SeekFrom::Start(offset)).ok()?;
        let mut header = [0; 16];
        file.read_exact(&mut header[..8]).ok()?;
        let mut size = u64::from(u32be(&header, 0)?);
        let head = if size == 1 {
            file.read_exact(&mut header[8..]).ok()?;
            size = u64be(&header, 8)?;
            16
        } else {
            8
        };
        if size == 0 {
            size = end.checked_sub(offset)?;
        }
        if size < head || offset.checked_add(size)? > end {
            return None;
        }
        if &header[4..8] == b"moov" {
            if size > 32 * 1024 * 1024 {
                return None;
            }
            let mut data = vec![0; usize::try_from(size - head).ok()?];
            file.read_exact(&mut data).ok()?;
            return parse(&data, allow_non_square);
        }
        offset = offset.checked_add(size)?;
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_movie_header_matches_known_size_and_rotation() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("test-media/format-matrix-v1");
        for (name, w, h) in [("h264-mp4", 640, 360), ("hevc-rotated-mov", 360, 640)] {
            let folder = base.join(name);
            if !folder.is_dir() {
                continue;
            }
            let path = std::fs::read_dir(folder)
                .unwrap()
                .flatten()
                .map(|e| e.path())
                .find(|p| p.is_file())
                .unwrap();
            let header = probe(&path);
            if name == "h264-mp4" {
                assert!(header.is_some(), "ordinary MP4 must exercise the fast path");
            }
            if let Some(meta) = header {
                assert_eq!((meta.width, meta.height), (w, h));
                assert!(meta.duration_ms.unwrap_or(0) > 30_000);
            }
        }
    }
    fn sar_movie(horizontal: u32, vertical: u32, track_width: u32, rotation: bool) -> Vec<u8> {
        fn atom(kind: &[u8; 4], body: Vec<u8>) -> Vec<u8> {
            let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
            out.extend(kind);
            out.extend(body);
            out
        }
        fn put(out: &mut [u8], at: usize, n: u32) {
            out[at..at + 4].copy_from_slice(&n.to_be_bytes());
        }
        let mut movie = vec![0; 100];
        put(&mut movie, 12, 1000);
        put(&mut movie, 16, 65000);
        let mut tkhd = vec![0; 84];
        put(&mut tkhd, 20, 65000);
        put(&mut tkhd, 76, track_width << 16);
        put(&mut tkhd, 80, 144 << 16);
        put(&mut tkhd, 72, 1 << 30);
        if rotation {
            put(&mut tkhd, 44, 1 << 16);
            put(&mut tkhd, 52, (-65536i32) as u32);
        } else {
            put(&mut tkhd, 40, 1 << 16);
            put(&mut tkhd, 56, 1 << 16);
        }
        let mut handler = vec![0; 12];
        handler[8..12].copy_from_slice(b"vide");
        let mut description = vec![0; 78];
        description[24..26].copy_from_slice(&176u16.to_be_bytes());
        description[26..28].copy_from_slice(&144u16.to_be_bytes());
        let mut pasp = horizontal.to_be_bytes().to_vec();
        pasp.extend(vertical.to_be_bytes());
        description.extend(atom(b"pasp", pasp));
        let mut stsd = vec![0; 4];
        stsd.extend(1u32.to_be_bytes());
        stsd.extend(atom(b"s263", description));
        let mut media = atom(b"hdlr", handler);
        media.extend(atom(b"minf", atom(b"stbl", atom(b"stsd", stsd))));
        let mut track = atom(b"tkhd", tkhd);
        track.extend(atom(b"mdia", media));
        let mut out = atom(b"mvhd", movie);
        out.extend(atom(b"trak", track));
        out
    }
    #[test]
    fn non_square_display_headers_require_matching_track_geometry() {
        let data = sar_movie(16, 11, 256, false);
        let header = parse(&data, true).unwrap();
        assert_eq!(
            (header.width, header.height, header.duration_ms),
            (256, 144, Some(65000))
        );
        assert!(parse(&data, false).is_none());
        let mut fractional_duration = data.clone();
        fractional_duration[24..28].copy_from_slice(&65134u32.to_be_bytes());
        assert!(parse(&fractional_duration, true).is_none());
        let mut mismatched_duration = data.clone();
        let track_at = boxes(&data).next().unwrap().1.len() + 8;
        mismatched_duration[track_at + 8 + 8 + 20..track_at + 8 + 8 + 24]
            .copy_from_slice(&64999u32.to_be_bytes());
        assert!(parse(&mismatched_duration, true).is_none());
        let square = sar_movie(1, 1, 176, false);
        assert!(parse(&square, true).is_some() && parse(&square, false).is_some());
        let rotated = parse(&sar_movie(16, 11, 256, true), true).unwrap();
        assert_eq!((rotated.width, rotated.height), (144, 256));
        for data in [
            sar_movie(0, 11, 256, false),
            sar_movie(16, 0, 256, false),
            sar_movie(16, 11, 255, false),
            sar_movie(u32::MAX, 1, 256, false),
        ] {
            assert!(parse(&data, true).is_none());
        }
        let mut aperture = sar_movie(16, 11, 256, false);
        aperture.extend(b"clap");
        assert!(parse(&aperture, true).is_none());
        let mut fragmented = sar_movie(16, 11, 256, false);
        fragmented.extend([0, 0, 0, 8]);
        fragmented.extend(b"mvex");
        assert!(parse(&fragmented, true).is_none());
    }
    #[test]
    fn malformed_atoms_are_bounded() {
        for bytes in [vec![], vec![0; 7], vec![255; 64], vec![0; 128]] {
            assert!(parse(&bytes, true).is_none());
        }
    }
}
