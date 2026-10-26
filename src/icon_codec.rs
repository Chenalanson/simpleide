// Shared icon raster and ICO encoding.
//
// This file is compiled both as a module of the binary and `include!`d by
// `build.rs`, so it may only depend on `std` and the `image` crate.

use image::RgbaImage;

/// Sizes below this use the `S` monogram. Six letters cannot stay readable at
/// 16/24 px, so only 32 px and above use the wordmark.
pub const WORDMARK_MIN_SIZE: u32 = 32;

pub fn decode_png(bytes: &[u8]) -> Option<RgbaImage> {
    image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .ok()
        .map(|image| image.to_rgba8())
}

pub fn resize(source: &RgbaImage, size: u32) -> RgbaImage {
    let size = size.max(1);
    image::imageops::resize(source, size, size, image::imageops::FilterType::Lanczos3)
}

pub fn source_for<'a>(word: &'a RgbaImage, mark: &'a RgbaImage, size: u32) -> &'a RgbaImage {
    if size >= WORDMARK_MIN_SIZE {
        word
    } else {
        mark
    }
}

/// Build a multi-size Windows ICO: wordmark for 32 px and above, monogram for
/// 16/24 px. Sizes up to 128 px use 32-bit BMP entries because `LoadImageW`
/// does not reliably read PNG-compressed entries below 256 px.
pub fn encode_ico(word: &RgbaImage, mark: &RgbaImage) -> Result<Vec<u8>, String> {
    let sizes = [16u32, 24, 32, 48, 64, 128, 256];
    let mut entries = Vec::new();
    for size in sizes {
        let image = resize(source_for(word, mark, size), size);
        let data = if size >= 256 {
            encode_png(&image)?
        } else {
            encode_bmp(size, image.as_raw())
        };
        entries.push((size, data));
    }

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * entries.len();
    for (size, data) in &entries {
        let dimension = if *size >= 256 { 0 } else { *size as u8 };
        bytes.push(dimension);
        bytes.push(dimension);
        bytes.push(0);
        bytes.push(0);
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&32u16.to_le_bytes());
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += data.len();
    }
    for (_, data) in &entries {
        bytes.extend_from_slice(data);
    }
    Ok(bytes)
}

fn encode_png(image: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(image.clone())
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|err| format!("无法编码图标：{err}"))?;
    Ok(png)
}

/// Encode one 32-bit BMP icon image: BITMAPINFOHEADER, bottom-up BGRA pixels,
/// then a 1-bit transparency mask. ICO directory entries expect the doubled
/// height and no BMP file header.
fn encode_bmp(size: u32, rgba: &[u8]) -> Vec<u8> {
    let mask_stride = size.div_ceil(32) * 4;
    let xor_len = (size * size * 4) as usize;
    let and_len = (mask_stride * size) as usize;
    let mut out = Vec::with_capacity(40 + xor_len + and_len);
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(size as i32).to_le_bytes());
    out.extend_from_slice(&(size as i32 * 2).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&((xor_len + and_len) as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]);
    for y in (0..size).rev() {
        for x in 0..size {
            let index = ((y * size + x) * 4) as usize;
            out.push(rgba[index + 2]);
            out.push(rgba[index + 1]);
            out.push(rgba[index]);
            out.push(rgba[index + 3]);
        }
    }
    for y in (0..size).rev() {
        let mut row = vec![0u8; mask_stride as usize];
        for x in 0..size {
            let index = ((y * size + x) * 4) as usize;
            if rgba[index + 3] == 0 {
                row[(x / 8) as usize] |= 0x80 >> (x % 8);
            }
        }
        out.extend_from_slice(&row);
    }
    out
}
