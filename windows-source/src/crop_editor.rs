use eframe::egui;
use crate::image_skin::CardCrop;

/// Draw original pixels with a freely movable selection and independent edge handles.
pub fn selection(ui: &mut egui::Ui, texture: &egui::TextureHandle,
    dimensions: [u32; 2], crop: &mut CardCrop, max_height: f32) {
    let [width, height] = dimensions;
    let scale = (ui.available_width().max(80.0) / width.max(1) as f32)
        .min(max_height / height.max(1) as f32);
    let size = egui::vec2(width as f32 * scale, height as f32 * scale);
    let (image_rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().image(texture.id(), image_rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
    let uv = crop.uv(width, height);
    let selected = egui::Rect::from_min_max(
        image_rect.min + egui::vec2(uv[0] * size.x, uv[1] * size.y),
        image_rect.min + egui::vec2(uv[2] * size.x, uv[3] * size.y));
    let shade = egui::Color32::from_black_alpha(160);
    for rect in [
        egui::Rect::from_min_max(image_rect.min, egui::pos2(image_rect.max.x, selected.min.y)),
        egui::Rect::from_min_max(egui::pos2(image_rect.min.x, selected.max.y), image_rect.max),
        egui::Rect::from_min_max(egui::pos2(image_rect.min.x, selected.min.y), egui::pos2(selected.min.x, selected.max.y)),
        egui::Rect::from_min_max(egui::pos2(selected.max.x, selected.min.y), egui::pos2(image_rect.max.x, selected.max.y)),
    ] { ui.painter().rect_filled(rect, 0.0, shade); }
    let color = crate::app::md3::PRIMARY;
    ui.painter().rect_stroke(selected, 0.0, egui::Stroke::new(2.0_f32, color), egui::StrokeKind::Inside);
    for fraction in [1.0 / 3.0, 2.0 / 3.0] {
        let x = selected.min.x + selected.width() * fraction;
        let y = selected.min.y + selected.height() * fraction;
        ui.painter().line_segment([egui::pos2(x, selected.min.y), egui::pos2(x, selected.max.y)], egui::Stroke::new(1.0_f32, egui::Color32::from_white_alpha(90)));
        ui.painter().line_segment([egui::pos2(selected.min.x, y), egui::pos2(selected.max.x, y)], egui::Stroke::new(1.0_f32, egui::Color32::from_white_alpha(90)));
    }
    // Put handles last so they take pointer precedence over the move surface.
    let body = ui.interact(selected.shrink((selected.width().min(selected.height()) / 4.0).min(8.0)), ui.id().with("crop-move"), egui::Sense::drag())
        .on_hover_cursor(egui::CursorIcon::Grab);
    if body.dragged() {
        let delta = body.drag_motion();
        crop.translate(width, height, [delta.x / size.x, delta.y / size.y]);
        ui.ctx().request_repaint();
    }
    for (index, point) in [selected.left_center(), selected.center_top(),
        selected.right_center(), selected.center_bottom()].into_iter().enumerate() {
        let vertical = index % 2 == 0;
        let handle_size = if vertical { egui::vec2(16.0, selected.height().max(16.0)) }
            else { egui::vec2(selected.width().max(16.0), 16.0) };
        let handle = egui::Rect::from_center_size(point, handle_size);
        ui.painter().rect_filled(egui::Rect::from_center_size(point,
            if vertical { egui::vec2(6.0, 24.0) } else { egui::vec2(24.0, 6.0) }), 2.0, color);
        let response = ui.interact(handle, ui.id().with(("crop-edge", index)), egui::Sense::drag())
            .on_hover_cursor(if vertical { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::ResizeVertical });
        if response.dragged() {
            let delta = response.drag_motion();
            crop.set_edge(index, uv[index] + if vertical { delta.x / size.x } else { delta.y / size.y }, width, height);
            ui.ctx().request_repaint();
        }
    }
    for (index, (point, horizontal, vertical)) in [
        (selected.left_top(), 0, 1), (selected.right_top(), 2, 1),
        (selected.left_bottom(), 0, 3), (selected.right_bottom(), 2, 3),
    ].into_iter().enumerate() {
        let handle = egui::Rect::from_center_size(point, egui::vec2(18.0, 18.0));
        ui.painter().rect_filled(handle.shrink(3.0), 3.0, color);
        let response = ui.interact(handle, ui.id().with(("crop-corner", index)), egui::Sense::drag())
            .on_hover_cursor(if index == 0 || index == 3 { egui::CursorIcon::ResizeNwSe } else { egui::CursorIcon::ResizeNeSw });
        if response.dragged() {
            let delta = response.drag_motion();
            crop.set_edge(horizontal, uv[horizontal] + delta.x / size.x, width, height);
            crop.set_edge(vertical, uv[vertical] + delta.y / size.y, width, height);
            ui.ctx().request_repaint();
        }
    }
    let (_, _, selected_width, selected_height) = crop.bounds(width, height);
    ui.label(format!("Área seleccionada: {selected_width} × {selected_height} px · recorte libre"));
}

pub fn edge_controls(ui: &mut egui::Ui, dimensions: [u32; 2], crop: &mut CardCrop) {
    ui.label("Quitar de cada lado (%)");
    for (index, label) in ["Izquierda", "Arriba", "Derecha", "Abajo"].into_iter().enumerate() {
        let edges = crop.uv(dimensions[0], dimensions[1]);
        let mut percent = if index < 2 { edges[index] } else { 1.0 - edges[index] } * 100.0;
        let minimum = 1.0 / dimensions[index % 2].max(1) as f32;
        let maximum = if index < 2 { edges[index + 2] - minimum }
            else { 1.0 - edges[index - 2] - minimum };
        if ui.add(egui::Slider::new(&mut percent, 0.0..=(maximum * 100.0).max(0.0)).text(label).suffix(" %")).changed() {
            crop.set_edge(index, if index < 2 { percent / 100.0 } else { 1.0 - percent / 100.0 }, dimensions[0], dimensions[1]);
        }
    }
}
