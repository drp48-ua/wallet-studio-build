use eframe::egui;

use crate::i18n::Language;

pub fn draw_card_tile(
    ui: &mut egui::Ui,
    width:f32,
    name: &str,
    hash: &str,
    current_design: Option<&egui::TextureHandle>,
    original_thumbnail: Option<&egui::TextureHandle>,
    original_verified: bool,
    reference_foreign: bool,
    selected: bool,
    language: Language,
) -> bool {
    use super::app::md3;
    let art_width=(width-24.0).max(1.0);let art_height=art_width*969.0/1536.0;
    let (rect,_)=ui.allocate_exact_size(egui::vec2(width,art_height+124.0),egui::Sense::hover());
    // Offscreen tiles reserve layout only. Textures and snapshots are already
    // cached by the worker; gallery paint never reads or decodes an image.
    if ui.is_rect_visible(rect) {
        ui.painter().rect(rect,14.0,if selected {md3::PRIMARY_CONTAINER} else {md3::SURFACE_CONTAINER},
            egui::Stroke::new(if selected {1.5_f32} else {1.0_f32},if selected {md3::PRIMARY} else {md3::OUTLINE_VARIANT}),egui::StrokeKind::Inside);
        let content=rect.shrink(12.0);
        let mut child=ui.new_child(egui::UiBuilder::new().max_rect(content).layout(egui::Layout::top_down(egui::Align::Min)));
        child.add(egui::Label::new(egui::RichText::new(name).strong().size(14.0).color(md3::ON_SURFACE)).truncate()).on_hover_text(name);
        child.add_space(4.0);
        let size=egui::vec2(art_width,art_height);
        if let Some(texture)=current_design.or(original_thumbnail) {
            child.add(egui::Image::new((texture.id(),texture.size_vec2())).fit_to_exact_size(size).maintain_aspect_ratio(true).corner_radius(10));
        } else {
            let (preview,_)=child.allocate_exact_size(size,egui::Sense::hover());child.painter().rect_filled(preview,10.0,md3::SURFACE_CONTAINER_HIGH);
            child.painter().text(preview.center(),egui::Align2::CENTER_CENTER,language.text("Imagen no disponible"),egui::FontId::proportional(12.0),md3::ON_SURFACE_VARIANT);
        }
        child.add_space(8.0);
        child.horizontal(|child| {
            if let Some(original)=original_thumbnail {
                child.add(egui::Image::new((original.id(),original.size_vec2())).fit_to_exact_size(egui::vec2(56.0,36.0)).maintain_aspect_ratio(true).corner_radius(4));
            } else {
                let (mini,_)=child.allocate_exact_size(egui::vec2(56.0,36.0),egui::Sense::hover());child.painter().rect_filled(mini,4.0,md3::SURFACE_CONTAINER_HIGH);
            }
            child.vertical(|child| {
                child.label(egui::RichText::new(if reference_foreign {"Referencia de otro iPhone"} else if current_design.is_some() {"Último diseño verificado"} else if original_thumbnail.is_some() {"Original local"} else {"Vista pendiente"}).strong().size(10.5).color(if current_design.is_some() {md3::ON_SECONDARY_CONTAINER} else {md3::ON_SURFACE}));
                child.label(egui::RichText::new(if reference_foreign {"No autoriza transferir a este iPhone"} else if original_verified {"Original guardado y verificado"} else {"Original pendiente de verificar"}).size(10.0).color(if original_verified {md3::SUCCESS} else {md3::ON_SURFACE_VARIANT}));
            });
        });
        child.label(egui::RichText::new("Copia de este equipo").size(9.5).color(md3::ON_SURFACE_VARIANT));
    }
    ui.interact(rect,ui.id().with(("saved-card-tile",hash)),egui::Sense::click()).on_hover_text(name).clicked()
}

pub fn short_identifier(hash: &str) -> String {
    hash.chars().take(7).collect()
}

#[cfg(test)]
mod tests {
    use super::short_identifier;

    #[test]
    fn gallery_identifier_is_bounded_and_does_not_mutate_identity() {
        let hash = "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=";
        assert_eq!(short_identifier(hash), "OM6NYhw");
        assert_eq!(hash, "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=");
        assert_eq!(short_identifier("abc"), "abc");
    }
}
