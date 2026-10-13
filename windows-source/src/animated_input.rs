//! GIF playback is local only. Each frozen frame becomes static card artwork.
use anyhow::{Context, Result, ensure};
use image::{AnimationDecoder, DynamicImage, ImageDecoder};
use std::io::{BufRead, Seek, SeekFrom};
use std::time::Duration;

pub const MAX_FRAMES: usize = 120;
pub const MAX_INPUT_BYTES: u64 = 32 * 1024 * 1024;
const MAX_PIXELS: u64 = 16_000_000;
const MAX_DECODED_BYTES: u64 = 128 * 1024 * 1024;
const MIN_DELAY: Duration = Duration::from_millis(20);

#[derive(Debug)]
pub struct GifFrame {
    pub image: DynamicImage,
    pub delay: Duration,
}

/// Locate the displayed frame directly, even after a suspended UI. Work is
/// bounded by MAX_FRAMES, rather than by the number of missed animation ticks.
pub fn playback_position(frames: &[GifFrame], first: usize, elapsed: Duration) -> Option<(usize, Duration)> {
    if frames.is_empty() || first >= frames.len() { return None; }
    let cycle: u128 = frames.iter().map(|frame| frame.delay.max(MIN_DELAY).as_nanos()).sum();
    let mut remaining = elapsed.as_nanos() % cycle;
    for offset in 0..frames.len() {
        let index = (first + offset) % frames.len();
        let delay = frames[index].delay.max(MIN_DELAY).as_nanos();
        if remaining < delay {
            return Some((index, Duration::from_nanos((delay - remaining) as u64)));
        }
        remaining -= delay;
    }
    None
}

pub fn decode_gif(mut reader: impl BufRead + Seek) -> Result<Vec<GifFrame>> {
    ensure!(reader.seek(SeekFrom::End(0))? <= MAX_INPUT_BYTES,
        "GIF demasiado grande; máximo 32 MB por archivo");
    reader.seek(SeekFrom::Start(0))?;
    let mut decoder = image::codecs::gif::GifDecoder::new(reader).context("GIF no válido")?;
    let (width, height) = decoder.dimensions();
    ensure!(u64::from(width) * u64::from(height) <= MAX_PIXELS, "GIF demasiado grande; máximo 16 megapíxeles");
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_DECODED_BYTES);
    decoder.set_limits(limits)?;
    let mut frames = Vec::new();
    let frame_bytes = u64::from(width) * u64::from(height) * 4;
    for frame in decoder.into_frames() {
        ensure!(frames.len() < MAX_FRAMES && frame_bytes * (frames.len() as u64 + 1) <= MAX_DECODED_BYTES,
            "GIF demasiado largo; usa hasta 120 fotogramas y 128 MB descomprimidos");
        let frame = frame?;
        let (numerator, denominator) = frame.delay().numer_denom_ms();
        let delay = Duration::from_secs_f64(f64::from(numerator) / f64::from(denominator) / 1000.0).max(MIN_DELAY);
        frames.push(GifFrame { image: DynamicImage::ImageRgba8(frame.into_buffer()), delay });
    }
    ensure!(!frames.is_empty(), "El GIF no contiene fotogramas");
    Ok(frames)
}

pub fn selected_frame(frames: &[GifFrame], index: usize) -> Option<DynamicImage> {
    frames.get(index).map(|frame| frame.image.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn playback_respects_delays_boundaries_selection_and_long_suspension() {
        let frames: Vec<_> = [0, 70, 110].into_iter().map(|delay| GifFrame {
            image: DynamicImage::new_rgba8(1, 1), delay: Duration::from_millis(delay),
        }).collect();
        assert_eq!(playback_position(&frames, 0, Duration::ZERO), Some((0, MIN_DELAY)));
        assert_eq!(playback_position(&frames, 0, Duration::from_millis(20)), Some((1, Duration::from_millis(70))));
        assert_eq!(playback_position(&frames, 0, Duration::from_millis(95)), Some((2, Duration::from_millis(105))));
        assert_eq!(playback_position(&frames, 1, Duration::from_millis(70)), Some((2, Duration::from_millis(110))));
        assert_eq!(playback_position(&frames, 0, Duration::from_millis(86_400_095)), Some((2, Duration::from_millis(105))));
        assert_eq!(playback_position(&frames, 0, Duration::from_millis(200)), Some((0, MIN_DELAY)));
        assert!(playback_position(&[], 0, Duration::ZERO).is_none());
        assert!(playback_position(&frames, 3, Duration::ZERO).is_none());
    }

    #[test]
    fn gif_decoder_composes_transparent_frames_and_preserves_timing() {
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            let red = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
            let mut overlay = image::RgbaImage::new(2, 2);
            overlay.put_pixel(1, 1, image::Rgba([0, 0, 255, 255]));
            for (buffer, delay) in [(red, 70), (overlay, 110)] {
                encoder.encode_frame(image::Frame::from_parts(buffer, 0, 0, image::Delay::from_numer_denom_ms(delay, 1))).unwrap();
            }
        }
        // The encoder defaults to Background disposal. This fixture needs Keep
        // to verify that decoding uses composed canvases rather than raw deltas.
        let controls: Vec<_> = bytes.windows(3).enumerate().filter_map(|(index, part)|
            (part == [0x21, 0xf9, 0x04]).then_some(index + 3)).collect();
        assert_eq!(controls.len(), 2);
        for index in controls { bytes[index] = (bytes[index] & !0x1c) | 0x04; }
        let frames = decode_gif(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(frames[0].delay, Duration::from_millis(70));
        assert_eq!(frames[1].delay, Duration::from_millis(110));
        let composed = frames[1].image.to_rgba8();
        assert_eq!(composed.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(composed.get_pixel(1, 1).0, [0, 0, 255, 255]);
    }

    #[test]
    fn public_rotating_earth_fixture_plays_and_freezes_to_static_assets() {
        let bytes = include_bytes!("../../test-assets/rotating-earth.gif");
        let frames = decode_gif(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(frames.len(), 44);
        assert!(frames.iter().all(|frame| frame.image.width() == 400 && frame.image.height() == 400));
        assert_eq!(frames.iter().map(|frame| frame.delay).sum::<Duration>(), Duration::from_millis(3960));
        let (index, _) = playback_position(&frames, 0, Duration::from_millis(900)).unwrap();
        assert_eq!(index, 10);
        let frozen = selected_frame(&frames, index).unwrap();
        let skin = crate::image_skin::PreparedSkin::from_image_with_focus(frozen, 0.5, 0.5).unwrap();
        assert!(crate::wallet_backup::valid_asset("cardBackgroundCombined@3x.png", &skin.png));
        assert!(crate::wallet_backup::valid_asset("cardBackgroundCombined.pdf", &skin.pdf));
        assert!(!crate::wallet_backup::valid_asset("cardBackgroundCombined@3x.png", bytes));
        assert_ne!(frames[0].image.to_rgba8(), frames[index].image.to_rgba8());
    }

    #[test]
    fn gif_frames_decode_select_and_export_static_artwork() {
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            for color in [[255, 0, 0, 255], [0, 0, 255, 255]] {
                encoder.encode_frame(image::Frame::new(image::RgbaImage::from_pixel(2, 2, image::Rgba(color)))).unwrap();
            }
        }
        let frames = decode_gif(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(selected_frame(&frames, 1).unwrap().to_rgba8().get_pixel(0, 0).0, [0, 0, 255, 255]);
        assert!(selected_frame(&frames, 2).is_none());
        let skin = crate::image_skin::PreparedSkin::from_image_with_focus(selected_frame(&frames, 1).unwrap(), 0.5, 0.5).unwrap();
        assert!(skin.png.starts_with(b"\x89PNG"));
        assert!(skin.pdf.starts_with(b"%PDF-"));
    }
    #[test]
    fn invalid_and_oversized_gifs_fail_closed() {
        assert!(decode_gif(std::io::Cursor::new(b"invalid")).is_err());
        let mut header = b"GIF89a".to_vec();
        header.extend_from_slice(&65535u16.to_le_bytes());
        header.extend_from_slice(&65535u16.to_le_bytes());
        header.extend_from_slice(&[0, 0, 0]);
        assert!(decode_gif(std::io::Cursor::new(header)).is_err());
    }
    #[test]
    fn excessive_frame_count_is_rejected_instead_of_silently_truncated() {
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            for _ in 0..=MAX_FRAMES {
                encoder.encode_frame(image::Frame::new(image::RgbaImage::new(1, 1))).unwrap();
            }
        }
        assert!(decode_gif(std::io::Cursor::new(bytes)).is_err());
    }
    #[test]
    fn oversized_compressed_input_is_rejected_before_decode() {
        let error = decode_gif(std::io::Cursor::new(vec![0; MAX_INPUT_BYTES as usize + 1])).unwrap_err();
        assert!(error.to_string().contains("32 MB"));
    }
}
