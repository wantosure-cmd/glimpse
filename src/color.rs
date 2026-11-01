//! Basic SDR ICC conversion, shared by still-image paths.
pub fn icc_to_srgb(image: &mut image::RgbaImage, icc: &[u8]) -> Result<(), String> {
    if icc.len() > 4 * 1024 * 1024 {
        return Err(crate::ui_text!(msg_3d37cdb74bd1).into());
    }
    let source = moxcms::ColorProfile::new_from_slice(icc).map_err(|e| e.to_string())?;
    if source.color_space != moxcms::DataColorSpace::Rgb {
        return Err(crate::ui_text!(msg_4ef3edc9c3dc).into());
    }
    let transform = source
        .create_transform_8bit(
            moxcms::Layout::Rgba,
            &moxcms::ColorProfile::new_srgb(),
            moxcms::Layout::Rgba,
            Default::default(),
        )
        .map_err(|e| e.to_string())?;
    let mut output = vec![0; image.as_raw().len()];
    transform
        .transform(image.as_raw(), &mut output)
        .map_err(|e| e.to_string())?;
    image.as_mut().copy_from_slice(&output);
    Ok(())
}
