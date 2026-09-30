use std::io::Cursor;

use base64::Engine;
use image::{GenericImageView, ImageFormat, ImageReader, Limits};

use crate::Error;
use crate::layout::ImageSource;

pub const MAX_IMAGE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_IMAGE_PIXELS: u64 = 16_000_000;

pub fn decode_image(bytes: &[u8], content_type: Option<&str>) -> Result<ImageSource, Error> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(Error::InvalidInput(
            "image exceeds 4 MiB compressed-byte limit".into(),
        ));
    }
    let format = image::guess_format(bytes)
        .map_err(|_| Error::InvalidInput("unsupported image format".into()))?;
    let mime = match format {
        ImageFormat::Png => "image/png",
        ImageFormat::Jpeg => "image/jpeg",
        _ => {
            return Err(Error::InvalidInput(
                "unsupported image format (PNG and JPEG only)".into(),
            ));
        }
    };
    if let Some(content_type) = content_type {
        let stated = content_type.split(';').next().unwrap_or("").trim();
        if !stated.eq_ignore_ascii_case(mime) {
            return Err(Error::InvalidInput(format!(
                "image Content-Type does not match {mime}: {content_type}"
            )));
        }
    }
    let dimensions = ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|error| Error::InvalidInput(format!("invalid image: {error}")))?;
    let pixels = u64::from(dimensions.0) * u64::from(dimensions.1);
    if dimensions.0 == 0 || dimensions.1 == 0 || pixels > MAX_IMAGE_PIXELS {
        return Err(Error::InvalidInput(
            "image exceeds 16-million-pixel decoded limit".into(),
        ));
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|error| Error::InvalidInput(format!("invalid image: {error}")))?;
    if decoded.dimensions() != dimensions {
        return Err(Error::InvalidInput(
            "image dimensions changed while decoding".into(),
        ));
    }
    let href = format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    );
    Ok(ImageSource {
        href,
        width: dimensions.0 as f32,
        height: dimensions.1 as f32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_oversize_and_unsupported_images() {
        assert!(decode_image(&vec![0; MAX_IMAGE_BYTES + 1], None).is_err());
        assert!(decode_image(b"GIF89a", None).is_err());
        let mut oversized_pixels = include_bytes!("../tests/render/two-pixels.png").to_vec();
        oversized_pixels[16..20].copy_from_slice(&5000u32.to_be_bytes());
        oversized_pixels[20..24].copy_from_slice(&5000u32.to_be_bytes());
        let mut crc = !0u32;
        for byte in &oversized_pixels[12..29] {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb8_8320u32 & (0u32.wrapping_sub(crc & 1)));
            }
        }
        oversized_pixels[29..33].copy_from_slice(&(!crc).to_be_bytes());
        assert!(
            decode_image(&oversized_pixels, None)
                .unwrap_err()
                .to_string()
                .contains("pixel")
        );
    }
}
