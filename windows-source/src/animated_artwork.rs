//! Experimental APNG artwork. A valid animation file does not prove Wallet plays it.
use anyhow::{Context, Result};
use image::{AnimationDecoder, DynamicImage, ImageDecoder, ImageFormat, imageops::FilterType};
use std::io::Cursor;
use std::time::Duration;

const MAX_BYTES: usize = 32 * 1024 * 1024;
pub struct AnimatedArtwork { pub png_3x: Vec<u8>, pub png_2x: Vec<u8>, pub frames: usize }

pub fn validate_pair(png_3x: &[u8], png_2x: &[u8]) -> Result<()> {
    anyhow::ensure!(validate(png_3x, 1536, 969)? == validate(png_2x, 1024, 646)?,
        "Las escalas de APNG no conservan el mismo número de fotogramas");
    let timing = |bytes: &[u8]| -> Vec<(u16, u16)> {
        let mut position = 8; let mut result = Vec::new();
        while position < bytes.len() {
            let size = u32::from_be_bytes(bytes[position..position+4].try_into().unwrap()) as usize;
            if &bytes[position+4..position+8] == b"fcTL" {
                result.push((u16::from_be_bytes(bytes[position+28..position+30].try_into().unwrap()),
                    u16::from_be_bytes(bytes[position+30..position+32].try_into().unwrap())));
            }
            position += size + 12;
        }
        result
    };
    anyhow::ensure!(timing(png_3x).iter().zip(timing(png_2x)).all(|((a,b),(c,d))|
        u32::from(*a)*u32::from(d) == u32::from(*b)*u32::from(c)),
        "Las escalas de APNG no conservan los mismos tiempos");
    Ok(())
}

pub fn from_gif(path: &std::path::Path) -> Result<AnimatedArtwork> {
    anyhow::ensure!(path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("gif")), "El experimento requiere un GIF local");
    let frames = crate::animated_input::decode_gif(Cursor::new(crate::design_library::read_image(path)?))?;
    from_frames(&frames)
}

fn from_frames(frames: &[crate::animated_input::GifFrame]) -> Result<AnimatedArtwork> {
    anyhow::ensure!(frames.len() >= 2 && frames.len() <= crate::animated_input::MAX_FRAMES,
        "La animación experimental necesita entre 2 y 120 fotogramas");
    let png_3x = encode(frames, 1536, 969)?;
    let png_2x = encode(frames, 1024, 646)?;
    validate(&png_3x, 1536, 969)?;
    validate(&png_2x, 1024, 646)?;
    Ok(AnimatedArtwork { png_3x, png_2x, frames: frames.len() })
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 { crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1)); }
    }
    !crc
}

fn chunk(output: &mut Vec<u8>, kind: &[u8; 4], bytes: &[u8]) -> Result<()> {
    anyhow::ensure!(output.len().checked_add(bytes.len() + 12).is_some_and(|size| size <= MAX_BYTES),
        "La animación supera 32 MB por escala");
    output.extend_from_slice(&u32::try_from(bytes.len())?.to_be_bytes());
    let start = output.len();
    output.extend_from_slice(kind);
    output.extend_from_slice(bytes);
    let crc = crc32(&output[start..]);
    output.extend_from_slice(&crc.to_be_bytes());
    Ok(())
}

fn delay_fraction(delay: Duration) -> Result<(u16, u16)> {
    let mut numerator = u32::try_from(delay.as_millis())?;
    anyhow::ensure!((20..=655350).contains(&numerator), "Tiempo de fotograma no admitido");
    let mut denominator = 1000u32;
    let (mut a, mut b) = (numerator, denominator);
    while b != 0 { (a, b) = (b, a % b); }
    numerator /= a; denominator /= a;
    Ok((u16::try_from(numerator)?, u16::try_from(denominator)?))
}

fn encode(frames: &[crate::animated_input::GifFrame], width: u32, height: u32) -> Result<Vec<u8>> {
    let mut output = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes()); ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut output, b"IHDR", &ihdr)?;
    let mut actl = (frames.len() as u32).to_be_bytes().to_vec();
    actl.extend_from_slice(&0u32.to_be_bytes());
    chunk(&mut output, b"acTL", &actl)?;
    let mut sequence = 0u32;
    for (index, frame) in frames.iter().enumerate() {
        let (num, den) = delay_fraction(frame.delay)?;
        let mut control = sequence.to_be_bytes().to_vec(); sequence += 1;
        for value in [width, height, 0, 0] { control.extend_from_slice(&value.to_be_bytes()); }
        control.extend_from_slice(&num.to_be_bytes()); control.extend_from_slice(&den.to_be_bytes());
        control.extend_from_slice(&[0, 0]); // No disposal, replace whole canvas with SOURCE.
        chunk(&mut output, b"fcTL", &control)?;
        let image = &frame.image;
        let uv = crate::image_skin::crop_uv_for_card(image.width(), image.height(), 0.5, 0.5);
        let x = (uv[0] * image.width() as f32).round() as u32;
        let y = (uv[1] * image.height() as f32).round() as u32;
        let cw = ((uv[2] - uv[0]) * image.width() as f32).round() as u32;
        let ch = ((uv[3] - uv[1]) * image.height() as f32).round() as u32;
        let rgba = image.crop_imm(x, y, cw, ch).resize_exact(width, height, FilterType::Lanczos3).to_rgba8();
        let mut png = Vec::new();
        DynamicImage::ImageRgba8(rgba).write_to(&mut Cursor::new(&mut png), ImageFormat::Png)?;
        let mut position = 8;
        while position < png.len() {
            let size = u32::from_be_bytes(png[position..position+4].try_into()?) as usize;
            let kind = &png[position+4..position+8];
            let data = &png[position+8..position+8+size];
            if kind == b"IDAT" {
                if index == 0 { chunk(&mut output, b"IDAT", data)?; }
                else {
                    let mut fdat = sequence.to_be_bytes().to_vec(); sequence += 1;
                    fdat.extend_from_slice(data); chunk(&mut output, b"fdAT", &fdat)?;
                }
            }
            position += size + 12;
        }
    }
    chunk(&mut output, b"IEND", &[])?;
    Ok(output)
}

/// Verify every animation frame, CRC, sequence, timing, size and canvas before USB.
pub fn validate(bytes: &[u8], width: u32, height: u32) -> Result<usize> {
    anyhow::ensure!(bytes.len() <= MAX_BYTES && bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "APNG inválido o demasiado grande");
    let mut position = 8usize;
    let mut sequence = 0u32;
    let mut declared = None;
    let mut delays = Vec::new();
    let mut ended = false;
    while position < bytes.len() {
        anyhow::ensure!(bytes.len() - position >= 12 && !ended, "Fragmento APNG incompleto");
        let size = u32::from_be_bytes(bytes[position..position+4].try_into()?) as usize;
        let end = position.checked_add(size + 12).context("Longitud APNG inválida")?;
        anyhow::ensure!(end <= bytes.len(), "Fragmento APNG fuera del archivo");
        let kind = &bytes[position+4..position+8];
        let data = &bytes[position+8..end-4];
        anyhow::ensure!(crc32(&bytes[position+4..end-4]) == u32::from_be_bytes(bytes[end-4..end].try_into()?), "CRC APNG inválido");
        match kind {
            b"IHDR" => anyhow::ensure!(position == 8 && data.len() == 13
                && data[..4] == width.to_be_bytes() && data[4..8] == height.to_be_bytes()
                && data[8..] == [8, 6, 0, 0, 0], "Canvas APNG inválido"),
            b"acTL" => {
                anyhow::ensure!(declared.is_none() && delays.is_empty() && data.len() == 8 && data[4..] == [0,0,0,0], "Control APNG inválido");
                declared = Some(u32::from_be_bytes(data[..4].try_into()?) as usize);
            }
            b"fcTL" => {
                anyhow::ensure!(declared.is_some() && data.len() == 26 && delays.len() < 120
                    && data[..4] == sequence.to_be_bytes(), "Secuencia APNG inválida");
                sequence += 1;
                anyhow::ensure!(data[4..8] == width.to_be_bytes() && data[8..12] == height.to_be_bytes()
                    && data[12..20] == [0;8] && data[24..] == [0,0], "Fotograma APNG no cubre el canvas con SOURCE");
                let num = u16::from_be_bytes(data[20..22].try_into()?);
                let den = u16::from_be_bytes(data[22..24].try_into()?);
                anyhow::ensure!(den > 0 && f64::from(num)/f64::from(den) >= 0.02
                    && f64::from(num)/f64::from(den) <= 655.35, "Tiempo APNG inválido");
                delays.push((num, den));
            }
            b"fdAT" => {
                anyhow::ensure!(delays.len() > 1 && data.len() > 4 && data[..4] == sequence.to_be_bytes(), "Datos APNG fuera de secuencia");
                sequence += 1;
            }
            b"IDAT" => anyhow::ensure!(delays.len() == 1, "Imagen por defecto APNG inválida"),
            b"IEND" => { anyhow::ensure!(data.is_empty(), "Final APNG inválido"); ended = true; }
            _ => anyhow::bail!("Fragmento no admitido en APNG experimental"),
        }
        position = end;
    }
    anyhow::ensure!(ended && declared == Some(delays.len()) && delays.len() >= 2, "Número de fotogramas APNG inválido");
    let mut decoder = image::codecs::png::PngDecoder::new(Cursor::new(bytes))?;
    let mut limits = image::Limits::default(); limits.max_alloc = Some(128 * 1024 * 1024);
    decoder.set_limits(limits)?;
    anyhow::ensure!(decoder.is_apng()?, "El archivo PNG no contiene animación");
    let mut count = 0;
    for frame in decoder.apng()?.into_frames() {
        let frame = frame?;
        anyhow::ensure!(frame.buffer().dimensions() == (width, height), "Tamaño del fotograma decodificado inválido");
        let (num, den) = frame.delay().numer_denom_ms();
        let expected = delays.get(count).context("Fotogramas APNG adicionales")?;
        anyhow::ensure!((f64::from(num)/f64::from(den) - 1000.0*f64::from(expected.0)/f64::from(expected.1)).abs() < 1.0, "Tiempo APNG no coincide");
        count += 1;
    }
    anyhow::ensure!(count == delays.len(), "El decodificador APNG perdió fotogramas");
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_scales_preserve_every_colored_frame_and_delay() {
        let source: Vec<_> = [[255,0,0,255], [0,255,0,255], [0,0,255,255]].into_iter().enumerate().map(|(i, color)| crate::animated_input::GifFrame {
            image: DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(4, 4, image::Rgba(color))),
            delay: Duration::from_millis([40,70,110][i]),
        }).collect();
        let mut gif = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut gif);
            for frame in &source {
                encoder.encode_frame(image::Frame::from_parts(frame.image.to_rgba8(),0,0,
                    image::Delay::from_numer_denom_ms(frame.delay.as_millis() as u32,1))).unwrap();
            }
        }
        let mut frames = crate::animated_input::decode_gif(Cursor::new(gif)).unwrap();
        assert_eq!(frames.iter().map(|frame| frame.delay.as_millis()).collect::<Vec<_>>(),[40,70,110]);
        let artwork = from_frames(&frames).unwrap();
        assert!(validate_pair(&artwork.png_3x,&artwork.png_2x).is_ok());
        for (png, width, height) in [(&artwork.png_3x,1536,969), (&artwork.png_2x,1024,646)] {
            assert_eq!(validate(png, width, height).unwrap(), 3);
            let decoded = image::codecs::png::PngDecoder::new(Cursor::new(png)).unwrap().apng().unwrap().into_frames().collect_frames().unwrap();
            for (index, frame) in decoded.iter().enumerate() {
                assert_eq!(frame.buffer().get_pixel(width/2,height/2), frames[index].image.to_rgba8().get_pixel(0,0));
            }
            let mut corrupt = png.clone(); corrupt[45] ^= 1;
            assert!(validate(&corrupt, width, height).is_err());
        }
        frames[1].delay = Duration::from_millis(90);
        let wrong_timing = encode(&frames,1024,646).unwrap();
        assert!(validate_pair(&artwork.png_3x,&wrong_timing).is_err());
        assert!(from_frames(&frames[..1]).is_err());
        assert!(delay_fraction(Duration::from_secs(700)).is_err());
    }
}
