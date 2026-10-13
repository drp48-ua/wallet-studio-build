use std::io::Cursor;

use anyhow::{Context, Result};
use image::{DynamicImage, GenericImageView, ImageFormat, imageops::FilterType};

pub const CARD_WIDTH: u32 = 1_536;
pub const CARD_HEIGHT: u32 = 969;

/// Non-destructive selection. Explicit edges crop freely; legacy focus keeps card framing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardCrop {
    pub focus: [f32; 2],
    pub zoom: f32,
    pub edges: Option<[f32; 4]>, // left, top, right, bottom in original-image coordinates
}

impl Default for CardCrop {
    fn default() -> Self { Self { focus: [0.5, 0.5], zoom: 1.0, edges: None } }
}

impl CardCrop {
    pub fn full_image() -> Self {
        Self { edges: Some([0.0, 0.0, 1.0, 1.0]), ..Default::default() }
    }

    /// Move one edge only, retaining at least one original pixel.
    pub fn set_edge(&mut self, index: usize, value: f32, width: u32, height: u32) {
        if index >= 4 || !value.is_finite() { return; }
        let mut edges = self.uv(width, height);
        let minimum = 1.0 / if index % 2 == 0 { width.max(1) } else { height.max(1) } as f32;
        edges[index] = if index < 2 { value.clamp(0.0, (edges[index + 2] - minimum).max(0.0)) }
            else { value.clamp((edges[index - 2] + minimum).min(1.0), 1.0) };
        self.edges = Some(edges);
    }

    pub fn bounds(self, width: u32, height: u32) -> (u32, u32, u32, u32) {
        let width = width.max(1);
        let height = height.max(1);
        if let Some(edges) = self.edges {
            let finite = |v: f32, fallback: f32| if v.is_finite() { v.clamp(0.0, 1.0) } else { fallback };
            let x = ((finite(edges[0], 0.0) * width as f32).round() as u32).min(width - 1);
            let y = ((finite(edges[1], 0.0) * height as f32).round() as u32).min(height - 1);
            let right = ((finite(edges[2], 1.0) * width as f32).round() as u32).clamp(x + 1, width);
            let bottom = ((finite(edges[3], 1.0) * height as f32).round() as u32).clamp(y + 1, height);
            return (x, y, right - x, bottom - y);
        }
        let (_, _, fit_width, fit_height) = crop_bounds_for_card(width, height, 0.5, 0.5);
        let zoom = if self.zoom.is_finite() { self.zoom.clamp(1.0, 8.0) } else { 1.0 };
        let crop_width = ((fit_width as f32 / zoom).round() as u32).clamp(1, width);
        let crop_height = ((fit_height as f32 / zoom).round() as u32).clamp(1, height);
        let finite_focus = |value: f32| if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.5 };
        let x = (finite_focus(self.focus[0]) * (width - crop_width) as f32).round() as u32;
        let y = (finite_focus(self.focus[1]) * (height - crop_height) as f32).round() as u32;
        (x, y, crop_width, crop_height)
    }

    pub fn uv(self, width: u32, height: u32) -> [f32; 4] {
        let (x, y, w, h) = self.bounds(width, height);
        [x as f32 / width.max(1) as f32, y as f32 / height.max(1) as f32,
            (x + w) as f32 / width.max(1) as f32, (y + h) as f32 / height.max(1) as f32]
    }

    pub fn translate(&mut self, width: u32, height: u32, delta: [f32; 2]) {
        let uv = self.uv(width, height);
        if self.edges.is_some() {
            let mut edges = uv;
            for axis in 0..2 {
                if !delta[axis].is_finite() { continue; }
                let movement = delta[axis].clamp(-uv[axis], 1.0 - uv[axis + 2]);
                edges[axis] += movement;
                edges[axis + 2] += movement;
            }
            self.edges = Some(edges);
            return;
        }
        for axis in 0..2 {
            let free = 1.0 - (uv[axis + 2] - uv[axis]);
            if free > 0.00001 && delta[axis].is_finite() {
                self.focus[axis] = (self.focus[axis] + delta[axis] / free).clamp(0.0, 1.0);
            }
        }
    }
}

#[derive(Clone)]
pub struct PreparedSkin {
    pub png: Vec<u8>,
    pub pdf: Vec<u8>,
    pub source_width: u32,
    pub source_height: u32,
}

impl PreparedSkin {
    pub fn from_image_with_focus(
        image: DynamicImage,
        focus_x: f32,
        focus_y: f32,
    ) -> Result<Self> {
        Self::from_image_with_crop(image, CardCrop { focus: [focus_x, focus_y], ..Default::default() })
    }

    pub fn from_image_with_crop(image: DynamicImage, crop: CardCrop) -> Result<Self> {
        let (source_width, source_height) = image.dimensions();
        anyhow::ensure!(source_width > 0 && source_height > 0, "La imagen está vacía");
        let (x, y, width, height) = crop.bounds(source_width, source_height);
        let cropped = image.crop_imm(x, y, width, height);
        let final_image = cropped.resize_exact(CARD_WIDTH, CARD_HEIGHT, FilterType::Lanczos3);
        let rgba = final_image.to_rgba8();

        let mut png = Vec::new();
        DynamicImage::ImageRgba8(rgba)
            .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
            .context("Could not encode prepared PNG")?;

        let pdf = png_to_pdf(&png).context("Could not generate card PDF artwork")?;

        Ok(Self {
            png,
            pdf,
            source_width,
            source_height,
        })
    }
}

pub fn crop_uv_for_card(
    source_width: u32,
    source_height: u32,
    focus_x: f32,
    focus_y: f32,
) -> [f32; 4] {
    CardCrop { focus: [focus_x, focus_y], ..Default::default() }.uv(source_width, source_height)
}

pub fn png_to_pdf(png_bytes: &[u8]) -> Result<Vec<u8>> {
    let img = image::load_from_memory(png_bytes)
        .context("Failed to decode image for PDF conversion")?;
    let rgb = img.to_rgb8();
    let width = rgb.width();
    let height = rgb.height();
    let raw_bytes = rgb.into_raw();
    let compressed_stream = miniz_oxide::deflate::compress_to_vec_zlib(&raw_bytes, 6);

    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n");

    let mut offsets = Vec::new();

    // 1 0 obj: Catalog
    offsets.push(pdf.len());
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    // 2 0 obj: Pages
    offsets.push(pdf.len());
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    // 3 0 obj: Page
    offsets.push(pdf.len());
    let page_obj = format!(
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Contents 4 0 R /Resources << /XObject << /Im0 5 0 R >> >> >>\nendobj\n",
        width, height
    );
    pdf.extend_from_slice(page_obj.as_bytes());

    // 4 0 obj: Contents stream
    offsets.push(pdf.len());
    let content_stream = format!("q\n{} 0 0 {} 0 0 cm\n/Im0 Do\nQ\n", width, height);
    let contents_obj = format!(
        "4 0 obj\n<< /Length {} >>\nstream\n{}endstream\nendobj\n",
        content_stream.len(),
        content_stream
    );
    pdf.extend_from_slice(contents_obj.as_bytes());

    // 5 0 obj: Image XObject
    offsets.push(pdf.len());
    let image_header = format!(
        "5 0 obj\n<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
        width, height, compressed_stream.len()
    );
    pdf.extend_from_slice(image_header.as_bytes());
    pdf.extend_from_slice(&compressed_stream);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    // xref table
    let xref_offset = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", offsets.len() + 1).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for &off in &offsets {
        pdf.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
    }

    // trailer
    let trailer = format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
        offsets.len() + 1,
        xref_offset
    );
    pdf.extend_from_slice(trailer.as_bytes());

    Ok(pdf)
}

fn crop_bounds_for_card(
    width: u32,
    height: u32,
    focus_x: f32,
    focus_y: f32,
) -> (u32, u32, u32, u32) {
    let card_ratio = CARD_WIDTH as f64 / CARD_HEIGHT as f64;
    let source_ratio = width as f64 / height as f64;

    if source_ratio > card_ratio {
        let crop_width = ((height as f64 * card_ratio).round() as u32).clamp(1, width);
        let max_x = width.saturating_sub(crop_width);
        let x = (focus_x.clamp(0.0, 1.0) * max_x as f32).round() as u32;
        (x, 0, crop_width, height)
    } else {
        let crop_height = ((width as f64 / card_ratio).round() as u32).clamp(1, height);
        let max_y = height.saturating_sub(crop_height);
        let y = (focus_y.clamp(0.0, 1.0) * max_y as f32).round() as u32;
        (0, y, width, crop_height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_selection_is_bounded_for_portrait_landscape_and_invalid_controls() {
        for (width, height) in [(2400, 1000), (800, 1800), (1536, 969), (1, 1)] {
            for crop in [CardCrop::default(), CardCrop { focus: [1.0, 0.0], zoom: 8.0, ..Default::default() },
                CardCrop { focus: [f32::NAN, f32::INFINITY], zoom: f32::NAN, ..Default::default() },
                CardCrop { edges: Some([f32::NAN, 1.0, -4.0, f32::INFINITY]), ..Default::default() }] {
                let (x, y, w, h) = crop.bounds(width, height);
                assert!(w > 0 && h > 0 && x + w <= width && y + h <= height);
                assert!(crop.uv(width, height).iter().all(|value| value.is_finite() && (0.0..=1.0).contains(value)));
            }
        }
    }

    #[test]
    fn prepared_pixels_follow_the_selected_region_instead_of_the_full_image() {
        let image = image::RgbImage::from_fn(320, 200, |x, _|
            if x < 160 { image::Rgb([255, 0, 0]) } else { image::Rgb([0, 0, 255]) });
        for (focus, color) in [(0.0, [255, 0, 0]), (1.0, [0, 0, 255])] {
            let crop = CardCrop { focus: [focus, 0.5], zoom: 3.0, ..Default::default() };
            let prepared = PreparedSkin::from_image_with_crop(DynamicImage::ImageRgb8(image.clone()), crop).unwrap();
            let result = image::load_from_memory(&prepared.png).unwrap().to_rgb8();
            assert_eq!(result.dimensions(), (CARD_WIDTH, CARD_HEIGHT));
            assert_eq!(result.get_pixel(CARD_WIDTH / 2, CARD_HEIGHT / 2).0, color);
            assert!(prepared.pdf.starts_with(b"%PDF-1.4"));
        }
    }

    #[test]
    fn zoom_reduces_selection_and_drag_moves_it_to_the_image_edge() {
        let mut crop = CardCrop { zoom: 2.0, ..Default::default() };
        assert!(crop.bounds(2400, 1000).2 < CardCrop::default().bounds(2400, 1000).2);
        crop.translate(2400, 1000, [2.0, -2.0]);
        let (x, y, w, _) = crop.bounds(2400, 1000);
        assert_eq!(x + w, 2400);
        assert_eq!(y, 0);
    }

    #[test]
    fn each_free_edge_changes_only_that_edge() {
        for (index, value, expected) in [
            (0, 0.1, (100, 0, 900, 800)), (1, 0.25, (0, 200, 1000, 600)),
            (2, 0.8, (0, 0, 800, 800)), (3, 0.75, (0, 0, 1000, 600)),
        ] {
            let mut crop = CardCrop::full_image();
            crop.set_edge(index, value, 1000, 800);
            assert_eq!(crop.bounds(1000, 800), expected);
        }
        let mut crop = CardCrop::full_image();
        crop.set_edge(0, 0.1, 1000, 800);
        crop.set_edge(2, 0.8, 1000, 800);
        assert_eq!(crop.bounds(1000, 800), (100, 0, 700, 800));
    }

    #[test]
    fn edge_cannot_cross_its_opposite_or_discard_the_last_pixel() {
        let mut crop = CardCrop::full_image();
        crop.set_edge(0, 1.0, 100, 50);
        crop.set_edge(2, 0.0, 100, 50);
        crop.set_edge(1, 1.0, 100, 50);
        crop.set_edge(3, 0.0, 100, 50);
        assert_eq!(crop.bounds(100, 50), (99, 49, 1, 1));
        let before = crop;
        crop.set_edge(2, f32::NAN, 100, 50);
        assert_eq!(crop, before);
        assert_eq!(CardCrop::full_image().bounds(1, 1), (0, 0, 1, 1));
    }

    #[test]
    fn free_drag_preserves_selection_size_and_clamps_to_original_edges() {
        let mut crop = CardCrop { edges: Some([0.1, 0.2, 0.7, 0.8]), ..Default::default() };
        crop.translate(1000, 500, [2.0, -2.0]);
        assert_eq!(crop.bounds(1000, 500), (400, 0, 600, 300));
    }

    #[test]
    fn removing_only_left_and_right_preserves_top_and_bottom_pixels_in_export() {
        let image = image::RgbImage::from_fn(320, 200, |x, y| {
            if !(80..240).contains(&x) { image::Rgb([0, 0, 0]) }
            else if y < 30 { image::Rgb([255, 0, 0]) }
            else if y >= 170 { image::Rgb([0, 255, 0]) }
            else { image::Rgb([0, 0, 255]) }
        });
        let mut crop = CardCrop::full_image();
        crop.set_edge(0, 0.25, 320, 200);
        crop.set_edge(2, 0.75, 320, 200);
        assert_eq!(crop.bounds(320, 200), (80, 0, 160, 200));
        let prepared = PreparedSkin::from_image_with_crop(DynamicImage::ImageRgb8(image), crop).unwrap();
        let result = image::load_from_memory(&prepared.png).unwrap().to_rgb8();
        assert_eq!(result.get_pixel(CARD_WIDTH / 2, 0).0, [255, 0, 0]);
        assert_eq!(result.get_pixel(CARD_WIDTH / 2, CARD_HEIGHT - 1).0, [0, 255, 0]);
        assert_eq!(result.get_pixel(CARD_WIDTH / 2, CARD_HEIGHT / 2).0, [0, 0, 255]);
    }

    #[test]
    fn test_png_to_pdf_conversion() {
        let dummy = DynamicImage::new_rgb8(10, 10);
        let mut png = Vec::new();
        dummy.write_to(&mut Cursor::new(&mut png), ImageFormat::Png).unwrap();

        let pdf = png_to_pdf(&png).expect("png_to_pdf failed");
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        let pdf_str = String::from_utf8_lossy(&pdf);
        assert!(pdf_str.contains("/FlateDecode"));
        assert!(pdf_str.contains("/MediaBox [0 0 10 10]"));
        assert!(pdf_str.contains("xref"));
    }

    #[test]
    fn crop_focus_moves_toward_image_edges() {
        let left = crop_uv_for_card(2400, 1000, 0.0, 0.5);
        let right = crop_uv_for_card(2400, 1000, 1.0, 0.5);
        assert!(left[0] < right[0]);
        assert!(left[2] < right[2]);
        assert_eq!(left[1], 0.0);
        assert_eq!(right[3], 1.0);
    }
}
