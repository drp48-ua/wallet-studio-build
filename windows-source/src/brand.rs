//! Render the original logo at the display's physical pixel size.
use eframe::egui;

pub struct BrandLogo {
    source:image::DynamicImage,
    pub header:egui::TextureHandle,
    pub hero:egui::TextureHandle,
    scale:f32,
}
impl BrandLogo {
    pub fn new(ctx:&egui::Context)->Self {
        let source=image::load_from_memory(include_bytes!("../assets/wallet-studio.png"))
            .expect("bundled Wallet Studio logo is a valid image");
        let scale=ctx.pixels_per_point();
        let header=Self::texture(ctx,&source,44.0,scale,"brand-header");
        let hero=Self::texture(ctx,&source,56.0,scale,"brand-hero");
        Self {source,header,hero,scale}
    }
    fn texture(ctx:&egui::Context,source:&image::DynamicImage,points:f32,scale:f32,name:&str)->egui::TextureHandle {
        let pixels=(points*scale).round().clamp(1.0,512.0) as u32;
        // A single high-quality reduction avoids reducing to 256 first and
        // letting the GPU undersample the metal detail again at 44/56 points.
        let rgba=source.resize_exact(pixels,pixels,image::imageops::FilterType::Lanczos3).to_rgba8();
        ctx.load_texture(name,egui::ColorImage::from_rgba_unmultiplied([pixels as usize,pixels as usize],rgba.as_raw()),egui::TextureOptions::LINEAR)
    }
    pub fn refresh_scale(&mut self,ctx:&egui::Context) {
        let scale=ctx.pixels_per_point();
        if (scale-self.scale).abs()<0.001 {return;}
        self.header=Self::texture(ctx,&self.source,44.0,scale,"brand-header");
        self.hero=Self::texture(ctx,&self.source,56.0,scale,"brand-hero");
        self.scale=scale;
    }
}
