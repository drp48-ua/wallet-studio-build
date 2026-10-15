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
    if compact {return;}
    ui.add_space(50.0);
    ui.label(egui::RichText::new("Tu Wallet.\nA tu manera.").size(43.0).strong().color(md3::ON_SURFACE));
    ui.add_space(16.0);
    ui.label(egui::RichText::new("Diseños que van contigo. Tus originales, siempre protegidos.").size(17.0).color(md3::ON_SURFACE_VARIANT));
    ui.add_space(28.0);
    let width=ui.available_width().min(390.0);
    let (rect,_)=ui.allocate_exact_size(egui::vec2(width,width*0.61+18.0),egui::Sense::hover());
    let card=egui::Rect::from_min_size(rect.min+egui::vec2(0.0,12.0),egui::vec2(width-18.0,width*0.61));
    ui.painter().rect_filled(card.translate(egui::vec2(14.0,-12.0)),20.0,md3::SECONDARY_CONTAINER);
    ui.painter().rect_filled(card,20.0,md3::PRIMARY_CONTAINER);
    ui.painter().rect_stroke(card,20.0,egui::Stroke::new(1.0_f32,md3::PRIMARY),egui::StrokeKind::Inside);
    ui.painter().text(card.min+egui::vec2(24.0,25.0),egui::Align2::LEFT_TOP,"WALLET STUDIO",egui::FontId::proportional(16.0),md3::ON_PRIMARY_CONTAINER);
    ui.painter().text(card.min+egui::vec2(24.0,59.0),egui::Align2::LEFT_TOP,"Hecha para ti.",egui::FontId::proportional(29.0),md3::ON_SURFACE);
    ui.painter().text(card.left_bottom()+egui::vec2(24.0,-24.0),egui::Align2::LEFT_BOTTOM,"DISEÑA · GUARDA · PERSONALIZA",egui::FontId::proportional(11.0),md3::PRIMARY);
    ui.add_space(28.0);
    for (title,description) in [("Originales protegidos","Copias verificadas sin reemplazar tu original."),
        ("Tu cuenta, hasta 3 equipos","Acceso con el correo de tu compra."),
        ("Continúa donde lo dejaste","Copias e historial con sincronización privada.")] {
        ui.horizontal_top(|ui| {
            ui.label(egui::RichText::new("•").size(22.0).color(md3::PRIMARY));
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(title).strong().size(14.0));
                ui.label(egui::RichText::new(description).size(12.0).color(md3::ON_SURFACE_VARIANT));
            });
        });ui.add_space(13.0);
    }
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
    #[test] fn resend_becomes_available_before_code_expiry_and_stays_available_after_expiry() {
        let sent=std::time::Instant::now();
        assert_eq!(remaining(Some(sent),sent+std::time::Duration::from_secs(59),RESEND_COOLDOWN),1);
        assert_eq!(remaining(Some(sent),sent+std::time::Duration::from_secs(60),RESEND_COOLDOWN),0);
        assert_eq!(remaining(Some(sent),sent+std::time::Duration::from_secs(599),CODE_LIFETIME),1);
        assert_eq!(remaining(Some(sent),sent+std::time::Duration::from_secs(601),CODE_LIFETIME),0);
        assert_eq!(remaining(None,sent,RESEND_COOLDOWN),0);
    }
}
