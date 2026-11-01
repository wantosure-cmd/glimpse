//! Row-wise long PNG overview/viewport decoding: no full-source pixel allocation.
use std::{fs::File, io::BufReader, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

// Input-work bound only: pixels remain streamed through bounded decoder rows.
// This is independent of the existing 64MiB output and 32MiB decoder budgets.
pub const MAX_SOURCE_PIXELS: u64 = 256_000_000;
pub fn supported_size(w: u32, h: u32) -> bool {
    w > 0
        && h > 0
        && w <= 1_048_576
        && h <= 1_048_576
        && u64::from(w) * u64::from(h) <= MAX_SOURCE_PIXELS
}

pub fn decode(
    path: &Path,
    bucket: u32,
    region: Option<Region>,
    cancelled: impl Fn() -> bool,
) -> Result<(image::RgbaImage, (u32, u32)), String> {
    let mut decoder =
        png::Decoder::new(BufReader::new(File::open(path).map_err(|e| e.to_string())?));
    decoder.set_limits(png::Limits {
        bytes: 32 * 1024 * 1024,
    });
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let (w, h) = (reader.info().width, reader.info().height);
    if !supported_size(w, h) || reader.info().interlaced {
        return Err(
            "Long PNG requires non-interlaced dimensions within bounded streaming-source limits"
                .into(),
        );
    }
    let profile = reader.info().icc_profile.as_ref().map(|p| p.to_vec());
    let r = region.unwrap_or(Region {
        x: 0,
        y: 0,
        width: w,
        height: h,
    });
    if r.width == 0
        || r.height == 0
        || r.x.checked_add(r.width).is_none_or(|n| n > w)
        || r.y.checked_add(r.height).is_none_or(|n| n > h)
    {
        return Err("Invalid long PNG viewport".into());
    }
    let scale = (f64::from(bucket.max(1)) / f64::from(r.width.max(r.height))).min(1.);
    let ow = (f64::from(r.width) * scale).round().max(1.) as u32;
    let oh = (f64::from(r.height) * scale).round().max(1.) as u32;
    // At most 64MiB output, plus decoder rows and a single accumulator row.
    if u64::from(ow) * u64::from(oh) > 16_777_216 {
        return Err("Long PNG viewport exceeds bounded output budget".into());
    }
    let (color, depth) = reader.output_color_type();
    if depth != png::BitDepth::Eight {
        return Err("Unsupported expanded PNG depth".into());
    }
    let channels = match color {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        _ => return Err("Unsupported expanded PNG color".into()),
    };
    let mut output = image::RgbaImage::new(ow, oh);
    let mut sums = vec![[0_u64; 4]; ow as usize];
    let mut count_rows = 0_u64;
    let mut dest_y = 0_u32;
    for y in 0..r.y + r.height {
        if y % 32 == 0 && cancelled() {
            return Err("Long PNG decode cancelled".into());
        }
        let row = reader
            .next_row()
            .map_err(|e| e.to_string())?
            .ok_or("Truncated PNG rows")?;
        if y < r.y {
            continue;
        }
        let bytes = row.data();
        if bytes.len() != w as usize * channels {
            return Err("Invalid PNG row size".into());
        }
        for ox in 0..ow {
            let x0 = r.x + (u64::from(ox) * u64::from(r.width) / u64::from(ow)) as u32;
            let x1 = r.x + (u64::from(ox + 1) * u64::from(r.width) / u64::from(ow)) as u32;
            let sum = &mut sums[ox as usize];
            for x in x0..x1 {
                let at = x as usize * channels;
                let rgba = match color {
                    png::ColorType::Rgb => [bytes[at], bytes[at + 1], bytes[at + 2], 255],
                    png::ColorType::Rgba => {
                        [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]
                    }
                    png::ColorType::Grayscale => [bytes[at], bytes[at], bytes[at], 255],
                    _ => [bytes[at], bytes[at], bytes[at], bytes[at + 1]],
                };
                for (s, v) in sum.iter_mut().zip(rgba) {
                    *s += u64::from(v);
                }
            }
        }
        count_rows += 1;
        let row_end = (u64::from(dest_y + 1) * u64::from(r.height) / u64::from(oh)) as u32;
        if y + 1 - r.y == row_end {
            for ox in 0..ow {
                let n = ((u64::from(ox + 1) * u64::from(r.width) / u64::from(ow))
                    - (u64::from(ox) * u64::from(r.width) / u64::from(ow)))
                    * count_rows;
                let sum = &mut sums[ox as usize];
                output.put_pixel(
                    ox,
                    dest_y,
                    image::Rgba(sum.map(|v| ((v + n / 2) / n) as u8)),
                );
                *sum = [0; 4];
            }
            count_rows = 0;
            dest_y += 1;
        }
    }
    if cancelled() {
        return Err("Long PNG decode cancelled".into());
    }
    // Whole-image previews must also validate the trailing chunk CRCs/IEND.
    // A viewport may stop early; its overview has already validated the source.
    if region.is_none() {
        reader.finish().map_err(|e| e.to_string())?;
    }
    if let Some(profile) = profile {
        if let Err(error) = crate::color::icc_to_srgb(&mut output, &profile) {
            eprintln!("Long PNG color profile: {error}");
        }
    }
    Ok((output, (w, h)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn beyond_full_decode_limit_streams_exact_rgba_regions_with_bounded_overview() {
        use std::io::Write;
        let path = std::env::temp_dir().join(format!(
            "glimpse-streamed-large-{}-{}.png",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let (w, h) = (1741, 37528);
        let expected = |x: u32, y: u32| {
            image::Rgba([
                (x % 251) as u8,
                (y % 253) as u8,
                ((x * 3 + y) % 249) as u8,
                ((x + y) % 255) as u8,
            ])
        };
        let mut encoder = png::Encoder::new(File::create(&path).unwrap(), w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        {
            let mut stream = writer.stream_writer().unwrap();
            let mut row = vec![0; w as usize * 4];
            for y in 0..h {
                for (x, pixel) in row.chunks_exact_mut(4).enumerate() {
                    pixel.copy_from_slice(&expected(x as u32, y).0);
                }
                stream.write_all(&row).unwrap();
            }
            stream.finish().unwrap();
        }
        writer.finish().unwrap();
        assert!(u64::from(w) * u64::from(h) > 64_000_000);
        assert!(supported_size(w, h));
        let (overview, size) = decode(&path, 512, None, || false).unwrap();
        assert_eq!(size, (w, h));
        assert_eq!(overview.height(), 512);
        assert!(overview.as_raw().len() < 128 * 1024);
        for y in [0, 18760, h - 38] {
            let r = Region {
                x: 500,
                y,
                width: 19,
                height: 38,
            };
            let (pixels, _) = decode(&path, 512, Some(r), || false).unwrap();
            for (x, y, p) in pixels.enumerate_pixels() {
                assert_eq!(*p, expected(x + r.x, y + r.y));
            }
        }
        assert!(!supported_size(20_000, 20_000));
        std::fs::remove_file(path).unwrap();
    }
    fn fixture() -> (std::path::PathBuf, image::RgbaImage) {
        let path = std::env::temp_dir().join(format!(
            "glimpse-long-png-{}-{}.png",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let pixels = image::RgbaImage::from_fn(17, 17003, |x, y| {
            image::Rgba([x as u8 * 11, (y % 251) as u8, ((x + y) % 253) as u8, 255])
        });
        pixels.save(&path).unwrap();
        (path, pixels)
    }
    #[test]
    fn native_viewport_pixels_and_top_middle_bottom_are_exact() {
        let (path, reference) = fixture();
        for y in [0, 8000, 16900] {
            let r = Region {
                x: 3,
                y,
                width: 11,
                height: 100,
            };
            let (pixels, size) = decode(&path, 512, Some(r), || false).unwrap();
            assert_eq!(size, (17, 17003));
            assert_eq!(
                pixels,
                image::imageops::crop_imm(&reference, r.x, r.y, r.width, r.height).to_image()
            );
        }
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn overview_is_bounded_and_cancelled_or_outside_requests_are_rejected() {
        let (path, _) = fixture();
        let (pixels, size) = decode(&path, 512, None, || false).unwrap();
        assert_eq!(size, (17, 17003));
        assert_eq!(pixels.height(), 512);
        assert!(pixels.as_raw().len() <= 512 * 4);
        assert!(decode(&path, 512, None, || true).is_err());
        let truncated = path.with_extension("truncated.png");
        let mut bytes = std::fs::read(&path).unwrap();
        bytes.truncate(bytes.len() - 8);
        std::fs::write(&truncated, bytes).unwrap();
        assert!(decode(&truncated, 512, None, || false).is_err());
        std::fs::remove_file(truncated).unwrap();
        assert!(decode(
            &path,
            512,
            Some(Region {
                x: u32::MAX,
                y: 0,
                width: 2,
                height: 1
            }),
            || false
        )
        .is_err());
        assert!(!supported_size(100_000, 100_000));
        assert!(!supported_size(0, 100));
        std::fs::remove_file(path).unwrap();
    }
}
