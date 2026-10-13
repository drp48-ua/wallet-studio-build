use eframe::egui;

use crate::i18n::Language;

pub fn draw_card_tile(
    ui: &mut egui::Ui,
    name: &str,
    hash: &str,
    current_design: Option<&egui::TextureHandle>,
    original_thumbnail: Option<&egui::TextureHandle>,
    original_verified: bool,
    selected: bool,
    language: Language,
) -> bool {
    let width = ui.available_width().clamp(168.0, 190.0);
    let size = egui::vec2(width, 216.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let fill = if selected { super::app::md3::PRIMARY_CONTAINER } else { super::app::md3::SURFACE_CONTAINER_HIGH };
    let stroke = if selected {
        egui::Stroke::new(1.5_f32, super::app::md3::PRIMARY)
    } else {
        egui::Stroke::new(1.0_f32, super::app::md3::OUTLINE_VARIANT)
    };
    ui.painter().rect(rect, 14.0, fill, stroke, egui::StrokeKind::Inside);

    let content = rect.shrink(10.0);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content)
            .layout(egui::Layout::top_down(egui::Align::Center)),
    );
    let preview_size = egui::vec2(content.width(), 104.0);
    let main_thumbnail = current_design.or(original_thumbnail);
    if let Some(texture) = main_thumbnail {
        child.add(
            egui::Image::new((texture.id(), texture.size_vec2()))
                .fit_to_exact_size(preview_size)
                .maintain_aspect_ratio(true),
        );
    } else {
        let (preview, _) = child.allocate_exact_size(preview_size, egui::Sense::hover());
        child.painter().rect_filled(preview, 8.0, super::app::md3::SURFACE);
        child.painter().text(
            preview.center(),
            egui::Align2::CENTER_CENTER,
            language.text("Imagen no disponible"),
            egui::FontId::proportional(11.0),
            super::app::md3::ON_SURFACE_VARIANT,
        );
    }
    if current_design.is_some() {
        child.horizontal(|child| {
            child.label(egui::RichText::new("APLICADO · LOCAL").strong().size(8.5)
                .color(super::app::md3::PRIMARY));
            if let Some(original) = original_thumbnail {
                let mini_size = egui::vec2(50.0, 32.0);
                let (mini_rect, _) = child.allocate_exact_size(mini_size, egui::Sense::hover());
                child.put(mini_rect, egui::Image::new((original.id(), original.size_vec2())).fit_to_exact_size(mini_size).maintain_aspect_ratio(true));
            }
        });
    }
    child.add_space(3.0);
    child.label(
        egui::RichText::new(name)
            .strong()
            .size(12.0)
            .color(super::app::md3::ON_SURFACE),
    );
    child.label(
        egui::RichText::new(if original_verified {
            language.text("Original guardado")
        } else {
            language.text("Sin copia original verificable")
        })
        .size(10.0)
        .color(if original_verified { super::app::md3::SUCCESS } else { super::app::md3::ON_SURFACE_VARIANT }),
    );
    // Place the interactive surface after child labels/images so the entire
    // visible tile responds, rather than only its empty border.
    ui.interact(rect, ui.id().with(("saved-card-tile", hash)), egui::Sense::click())
        .on_hover_text(format!("{} · {}", name, short_identifier(hash))).clicked()
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
