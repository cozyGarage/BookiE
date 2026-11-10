use super::{MAX_IMAGE_PREVIEW_BYTES, PreviewImageFormat, previewable_image};

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13];
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
    bytes
}

fn gif(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = b"GIF89a".to_vec();
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes
}

fn jpeg(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = vec![0xff, 0xd8, 0xff, 0xe0, 0, 4, 0, 0, 0xff, 0xc0, 0, 11, 8];
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&[1, 1, 0x11, 0]);
    bytes
}

fn webp_lossy(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = b"RIFF\x24\0\0\0WEBPVP8 ".to_vec();
    bytes.extend_from_slice(&[0x10, 0, 0, 0]);
    bytes.extend_from_slice(&[0, 0, 0, 0x9d, 0x01, 0x2a]);
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes
}

fn webp_lossless(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = b"RIFF\x24\0\0\0WEBPVP8L".to_vec();
    bytes.extend_from_slice(&[0, 0, 0, 0, 0x2f]);
    let packed = (width - 1) | ((height - 1) << 14);
    bytes.extend_from_slice(&packed.to_le_bytes());
    bytes
}

fn webp_extended(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = b"RIFF\x24\0\0\0WEBPVP8X".to_vec();
    bytes.extend_from_slice(&[10, 0, 0, 0, 0, 0, 0, 0]);
    bytes.extend_from_slice(&(width - 1).to_le_bytes()[..3]);
    bytes.extend_from_slice(&(height - 1).to_le_bytes()[..3]);
    bytes
}

#[test]
fn every_supported_format_with_a_sane_size_is_previewed() {
    assert_eq!(previewable_image(&png(640, 480)), Some(PreviewImageFormat::Png));
    assert_eq!(previewable_image(&jpeg(640, 480)), Some(PreviewImageFormat::Jpeg));
    assert_eq!(previewable_image(&gif(64, 48)), Some(PreviewImageFormat::Gif));
    assert_eq!(previewable_image(&webp_lossy(640, 480)), Some(PreviewImageFormat::Webp));
    assert_eq!(
        previewable_image(&webp_lossless(640, 480)),
        Some(PreviewImageFormat::Webp)
    );
    assert_eq!(
        previewable_image(&webp_extended(640, 480)),
        Some(PreviewImageFormat::Webp)
    );
}

#[test]
fn other_binary_and_riff_containers_that_are_not_webp_are_not_previewed() {
    assert_eq!(previewable_image(&[]), None);
    assert_eq!(previewable_image(&[0x00, 0x41, 0xff, 0x10]), None);
    assert_eq!(previewable_image(b"%PDF-1.7 not an image"), None);
    assert_eq!(previewable_image(b"RIFF\x24\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0"), None);
}

#[test]
fn a_header_cut_short_is_not_previewed() {
    let full = png(640, 480);
    for length in [3, 8, 16, 20, full.len() - 6] {
        assert_eq!(previewable_image(&full[..length]), None, "png cut at {length}");
    }
    assert_eq!(previewable_image(&jpeg(640, 480)[..9]), None);
    assert_eq!(previewable_image(&gif(64, 48)[..8]), None);
    assert_eq!(previewable_image(&webp_lossless(640, 480)[..22]), None);
}

#[test]
fn an_image_declaring_more_pixels_than_the_budget_is_not_previewed() {
    assert!(previewable_image(&png(8000, 4000)).is_some());
    assert_eq!(previewable_image(&png(8000, 4001)), None);
    assert_eq!(previewable_image(&png(u32::MAX, u32::MAX)), None);
    assert_eq!(previewable_image(&jpeg(u16::MAX, u16::MAX)), None);
    assert_eq!(previewable_image(&webp_extended(1 << 24, 1 << 24)), None);
}

#[test]
fn an_image_declaring_no_pixels_is_not_previewed() {
    assert_eq!(previewable_image(&png(0, 480)), None);
    assert_eq!(previewable_image(&gif(64, 0)), None);
}

#[test]
fn an_image_over_the_byte_budget_is_not_previewed() {
    let mut at_budget = png(640, 480);
    at_budget.resize(MAX_IMAGE_PREVIEW_BYTES, 0);
    assert!(previewable_image(&at_budget).is_some());
    at_budget.push(0);
    assert_eq!(previewable_image(&at_budget), None);
}

#[test]
fn a_jpeg_without_a_frame_header_or_with_a_corrupt_segment_is_not_previewed() {
    assert_eq!(previewable_image(&[0xff, 0xd8, 0xff, 0xe0, 0, 4, 0, 0]), None);
    assert_eq!(
        previewable_image(&[0xff, 0xd8, 0xff, 0xe0, 0xff, 0xff, 0, 0, 0, 0]),
        None
    );
    assert_eq!(previewable_image(&[0xff, 0xd8, 0x12, 0x34, 0x56, 0x78]), None);
}
