use eframe::egui;
use crate::app::md3;

pub const CODE_LIFETIME:u64=600;
pub const RESEND_COOLDOWN:u64=60;
pub fn remaining(started:Option<std::time::Instant>,now:std::time::Instant,limit:u64)->u64 {
    started.map(|instant|limit.saturating_sub(now.saturating_duration_since(instant).as_secs())).unwrap_or(0)
}

pub fn hero(ui:&mut egui::Ui,logo:&egui::TextureHandle,compact:bool) {
    ui.horizontal(|ui| {
        ui.add(egui::Image::new((logo.id(),egui::vec2(56.0,56.0))));
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("Wallet Studio").size(23.0).strong().color(md3::ON_SURFACE));
            ui.label(egui::RichText::new("TU ESPACIO PERSONAL").size(10.0).color(md3::PRIMARY));
        });
    });
    ui.add_space(if compact {18.0} else {36.0});
    ui.label(egui::RichText::new("Tu Wallet.\nA tu manera.").size(if compact {32.0} else {48.0}).strong().color(md3::ON_SURFACE));
    ui.add_space(12.0);
    ui.add(egui::Label::new(egui::RichText::new("Diseños que van contigo. Tus originales, siempre protegidos.").size(if compact {14.0} else {17.0}).color(md3::ON_SURFACE_VARIANT)).wrap());
    ui.add_space(if compact {16.0} else {24.0});
    let width=ui.available_width().min(if compact {280.0} else {500.0});
    let (rect,_)=ui.allocate_exact_size(egui::vec2(width,width*0.61+18.0),egui::Sense::hover());
    let card=egui::Rect::from_min_size(rect.min+egui::vec2(0.0,12.0),egui::vec2(width-18.0,width*0.61));
    ui.painter().rect_filled(card.translate(egui::vec2(14.0,-12.0)),20.0,md3::SECONDARY_CONTAINER);
    ui.painter().rect_filled(card,20.0,md3::PRIMARY_CONTAINER);
    ui.painter().rect_stroke(card,20.0,egui::Stroke::new(1.0_f32,md3::PRIMARY),egui::StrokeKind::Inside);
    ui.painter().text(card.min+egui::vec2(24.0,25.0),egui::Align2::LEFT_TOP,"WALLET STUDIO",egui::FontId::proportional(16.0),md3::ON_PRIMARY_CONTAINER);
    ui.painter().text(card.min+egui::vec2(24.0,59.0),egui::Align2::LEFT_TOP,"Hecha para ti.",egui::FontId::proportional(29.0),md3::ON_SURFACE);
    ui.painter().text(card.left_bottom()+egui::vec2(24.0,-24.0),egui::Align2::LEFT_BOTTOM,"DISEÑA · GUARDA · PERSONALIZA",egui::FontId::proportional(11.0),md3::PRIMARY);
}

pub fn benefits(ui:&mut egui::Ui) {
    let features=[("Originales protegidos","Copias verificadas sin reemplazar tu original."),
        ("Tu cuenta, hasta 3 equipos","Acceso con el correo de tu compra."),
        ("Continúa donde lo dejaste","Copias e historial con sincronización privada.")];
    ui.columns(3,|columns| {
        for (column,(title,description)) in columns.iter_mut().zip(features) {
            egui::Frame::new().fill(md3::SURFACE_CONTAINER).corner_radius(10).inner_margin(12).show(column,|ui| {
                ui.set_max_width(ui.available_width());
                ui.add(egui::Label::new(egui::RichText::new(title).strong().size(13.0).color(md3::PRIMARY)).wrap());
                ui.add(egui::Label::new(egui::RichText::new(description).size(11.5).color(md3::ON_SURFACE_VARIANT)).wrap());
            });
        }
    });
}

pub fn page(ui:&mut egui::Ui,logo:&egui::TextureHandle,form:impl FnOnce(&mut egui::Ui)) {
    let width=ui.available_width();
    let body_height=(ui.available_height()-90.0).max(380.0);
    let gap=if width>1300.0 {64.0} else {28.0};
    let form_width=(width*0.44).clamp(420.0,680.0).min(width);
    let hero_width=(width-form_width-gap).max(1.0);
    ui.allocate_ui_with_layout(egui::vec2(width,body_height),egui::Layout::left_to_right(egui::Align::Min),|ui| {
        ui.spacing_mut().item_spacing.x=gap;
        ui.allocate_ui_with_layout(egui::vec2(hero_width,body_height),egui::Layout::top_down(egui::Align::Min),|ui| {
            ui.spacing_mut().item_spacing.x=8.0;
            ui.set_width(hero_width);ui.set_max_width(hero_width);
            hero(ui,logo,body_height<620.0);
        });
        ui.allocate_ui_with_layout(egui::vec2(form_width,body_height),egui::Layout::top_down(egui::Align::Min),|ui| {
            ui.spacing_mut().item_spacing.x=8.0;
            ui.set_width(form_width);ui.set_max_width(form_width);
            // Only long forms scroll vertically inside their own panel. The
            // welcome artwork and horizontal benefits never create page scroll.
            egui::ScrollArea::vertical().id_salt("account-form-scroll").auto_shrink([false,true]).max_height(body_height).show(ui,form);
        });
    });
    ui.add_space(16.0);
    benefits(ui);
}

pub fn steps(ui:&mut egui::Ui,stage:u8,recovery:bool) {
    let title=match stage {0=>"01 / TU CORREO",1=>"02 / INICIAR SESIÓN",_=>if recovery {"02 / RECUPERAR ACCESO"} else {"02 / ACTIVAR TU CUENTA"}};
    ui.label(egui::RichText::new(title).size(11.0).strong().color(md3::PRIMARY));
    ui.add_space(12.0);
}

pub fn notice(ui:&mut egui::Ui,text:&str,busy:bool) {
    egui::Frame::new().fill(md3::SURFACE_CONTAINER_HIGH).corner_radius(10)
        .inner_margin(egui::Margin::same(12)).show(ui,|ui| {
            ui.horizontal_wrapped(|ui| {
                if busy {ui.spinner();}
                ui.label(egui::RichText::new(text).size(12.0).color(md3::ON_SURFACE_VARIANT));
            });
        });
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn welcome_and_horizontal_benefits_fit_window_and_fullscreen() {
        for size in [egui::vec2(960.0,640.0),egui::vec2(1180.0,760.0),egui::vec2(1920.0,1080.0)] {
            let ctx=egui::Context::default();
            let logo=ctx.load_texture("layout-test-logo",egui::ColorImage::filled([1,1],egui::Color32::WHITE),egui::TextureOptions::LINEAR);
            let input=egui::RawInput {screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,size)),..Default::default()};
            // Run two frames so font/layout state is settled, as in the app.
            for _ in 0..2 {
                let _=ctx.run(input.clone(),|ctx| {
                    egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(28,16))).show(ctx,|ui| {
                        let bounds=ui.max_rect();
                        page(ui,&logo,|ui| {ui.allocate_space(egui::vec2(ui.available_width(),450.0));});
                        assert!(ui.min_rect().right()<=bounds.right()+1.0,"horizontal overflow at {size:?}: {:?}",ui.min_rect());
                        assert!(ui.min_rect().bottom()<=bounds.bottom()+1.0,"vertical page overflow at {size:?}: {:?}",ui.min_rect());
                    });
                });
            }
        }
    }
    #[test] fn resend_becomes_available_before_code_expiry_and_stays_available_after_expiry() {
        let sent=std::time::Instant::now();
        assert_eq!(remaining(Some(sent),sent+std::time::Duration::from_secs(59),RESEND_COOLDOWN),1);
        assert_eq!(remaining(Some(sent),sent+std::time::Duration::from_secs(60),RESEND_COOLDOWN),0);
        assert_eq!(remaining(Some(sent),sent+std::time::Duration::from_secs(599),CODE_LIFETIME),1);
        assert_eq!(remaining(Some(sent),sent+std::time::Duration::from_secs(601),CODE_LIFETIME),0);
        assert_eq!(remaining(None,sent,RESEND_COOLDOWN),0);
    }
}
