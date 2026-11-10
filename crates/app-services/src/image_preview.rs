pub const MAX_IMAGE_PREVIEW_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_IMAGE_PREVIEW_PIXELS: u64 = 32_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewImageFormat {
    Png,
    Jpeg,
    Gif,
    Webp,
}

pub fn previewable_image(bytes: &[u8]) -> Option<PreviewImageFormat> {
    if bytes.len() > MAX_IMAGE_PREVIEW_BYTES {
        return None;
    }
    let format = sniff_image_format(bytes)?;
    let (width, height) = declared_dimensions(format, bytes)?;
    let pixels = u64::from(width) * u64::from(height);
    (width > 0 && height > 0 && pixels <= MAX_IMAGE_PREVIEW_PIXELS).then_some(format)
}

fn sniff_image_format(bytes: &[u8]) -> Option<PreviewImageFormat> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        return Some(PreviewImageFormat::Png);
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some(PreviewImageFormat::Jpeg);
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some(PreviewImageFormat::Gif);
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some(PreviewImageFormat::Webp);
    }
    None
}

fn declared_dimensions(format: PreviewImageFormat, bytes: &[u8]) -> Option<(u32, u32)> {
    match format {
        PreviewImageFormat::Png => png_dimensions(bytes),
        PreviewImageFormat::Jpeg => jpeg_dimensions(bytes),
        PreviewImageFormat::Gif => gif_dimensions(bytes),
        PreviewImageFormat::Webp => webp_dimensions(bytes),
    }
}

fn be16(bytes: &[u8], at: usize) -> Option<u32> {
    let pair = bytes.get(at..at + 2)?;
    Some(u32::from(u16::from_be_bytes([pair[0], pair[1]])))
}

fn le16(bytes: &[u8], at: usize) -> Option<u32> {
    let pair = bytes.get(at..at + 2)?;
    Some(u32::from(u16::from_le_bytes([pair[0], pair[1]])))
}

fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.get(12..16)? != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?);
    let height = u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?);
    Some((width, height))
}

fn gif_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    Some((le16(bytes, 6)?, le16(bytes, 8)?))
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut at = 2;
    while at + 4 <= bytes.len() {
        if bytes[at] != 0xff {
            return None;
        }
        let marker = bytes[at + 1];
        if marker == 0xff {
            at += 1;
            continue;
        }
        let standalone = marker == 0x01 || (0xd0..=0xd8).contains(&marker);
        if standalone {
            at += 2;
            continue;
        }
        let is_frame = (0xc0..=0xcf).contains(&marker) && !matches!(marker, 0xc4 | 0xc8 | 0xcc);
        if is_frame {
            return Some((be16(bytes, at + 7)?, be16(bytes, at + 5)?));
        }
        at += 2 + usize::try_from(be16(bytes, at + 2)?).ok()?;
    }
    None
}

fn webp_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    match bytes.get(12..16)? {
        b"VP8 " => {
            if bytes.get(23..26)? != [0x9d, 0x01, 0x2a] {
                return None;
            }
            Some((le16(bytes, 26)? & 0x3fff, le16(bytes, 28)? & 0x3fff))
        }
        b"VP8L" => {
            if *bytes.get(20)? != 0x2f {
                return None;
            }
            let packed = u32::from_le_bytes(bytes.get(21..25)?.try_into().ok()?);
            Some(((packed & 0x3fff) + 1, ((packed >> 14) & 0x3fff) + 1))
        }
        b"VP8X" => {
            let width = u32::from_le_bytes([*bytes.get(24)?, *bytes.get(25)?, *bytes.get(26)?, 0]) + 1;
            let height = u32::from_le_bytes([*bytes.get(27)?, *bytes.get(28)?, *bytes.get(29)?, 0]) + 1;
            Some((width, height))
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "image_preview_tests.rs"]
mod tests;
