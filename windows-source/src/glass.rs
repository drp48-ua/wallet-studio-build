//! Native glass styling: bounded vector gradients, no webview or desktop blur.
use eframe::egui::{self,Color32,Pos2,Rect,Shape,Stroke,Vec2};
pub const BACKGROUND:Color32=Color32::from_rgb(8,11,18);
pub const CYAN:Color32=Color32::from_rgb(56,189,248);
pub const VIOLET:Color32=Color32::from_rgb(139,92,246);
pub const TEXT:Color32=Color32::from_rgb(248,250,252);
pub const MUTED:Color32=Color32::from_rgb(148,163,184);
pub const LINE:Color32=Color32::from_rgba_premultiplied(22,22,22,22);
pub fn library_columns(available:f32)->Option<(f32,f32)> {
 if available.is_finite() && available>=900.0 {Some((available-328.0,312.0))}else{None}
}

fn blend(a:Color32,b:Color32,t:f32)->Color32 {
 let a=a.to_array();let b=b.to_array();let v=std::array::from_fn::<_,4,_>(|i|((a[i] as f32)*(1.0-t)+(b[i] as f32)*t).round() as u8);
 Color32::from_rgba_premultiplied(v[0],v[1],v[2],v[3])
}
pub fn gradient(rect:Rect,radius:f32,top:Color32,bottom:Color32)->Shape {
 if !rect.is_finite() || rect.width()<=0.0 || rect.height()<=0.0 {return Shape::Noop;}
 let r=radius.max(0.0).min(rect.width()*0.5).min(rect.height()*0.5);let mut mesh=egui::Mesh::default();
 mesh.colored_vertex(rect.center(),blend(top,bottom,0.5));
 for (center,start) in [(rect.left_top()+egui::vec2(r,r),180.0_f32),(rect.right_top()+egui::vec2(-r,r),270.0),(rect.right_bottom()+egui::vec2(-r,-r),0.0),(rect.left_bottom()+egui::vec2(r,-r),90.0)] {
  for step in 0..=8 {let angle=(start+step as f32*90.0/8.0).to_radians();let p=center+egui::vec2(angle.cos(),angle.sin())*r;mesh.colored_vertex(p,blend(top,bottom,((p.y-rect.top())/rect.height()).clamp(0.0,1.0)));}
 }
 let count=mesh.vertices.len() as u32-1;for index in 1..=count {mesh.add_triangle(0,index,if index==count {1}else{index+1});}
 Shape::mesh(mesh)
}
fn glow(painter:&egui::Painter,center:Pos2,radius:f32,color:Color32) {
 let mut mesh=egui::Mesh::default();mesh.colored_vertex(center,color);
 for i in 0..64 {let angle=i as f32*std::f32::consts::TAU/64.0;mesh.colored_vertex(center+egui::vec2(angle.cos(),angle.sin())*radius,Color32::TRANSPARENT);}
 for i in 1..=64 {mesh.add_triangle(0,i,if i==64 {1}else{i+1});}painter.add(Shape::mesh(mesh));
}
pub fn background(ctx:&egui::Context) {
 let rect=ctx.content_rect();let painter=ctx.layer_painter(egui::LayerId::background()).with_clip_rect(rect);
 painter.add(gradient(rect,0.0,BACKGROUND,Color32::from_rgb(11,13,25)));
 let radius=rect.width().max(rect.height())*0.57;
 glow(&painter,rect.right_top()+egui::vec2(-rect.width()*0.15,rect.height()*0.03),radius,Color32::from_rgba_unmultiplied(37,99,235,42));
 glow(&painter,rect.left_bottom()+egui::vec2(rect.width()*0.18,-rect.height()*0.1),radius*0.85,Color32::from_rgba_unmultiplied(124,58,237,31));
}
pub fn frame()->egui::Frame {
 egui::Frame::new().fill(Color32::from_rgba_unmultiplied(17,24,39,205)).stroke(Stroke::new(1.0,LINE)).corner_radius(14).inner_margin(16)
}
pub fn panel<R>(ui:&mut egui::Ui,contents:impl FnOnce(&mut egui::Ui)->R)->R {
 let slot=ui.painter().add(Shape::Noop);
 let response=frame().fill(Color32::TRANSPARENT).show(ui,|ui|{ui.set_min_width(ui.available_width());ui.vertical(contents).inner});
 let rect=response.response.rect;
 ui.painter().set(slot,Shape::Vec(vec![
  gradient(rect.translate(egui::vec2(0.0,5.0)).expand(3.0),17.0,Color32::from_black_alpha(30),Color32::from_black_alpha(45)),
  gradient(rect,14.0,Color32::from_rgba_unmultiplied(27,39,62,228),Color32::from_rgba_unmultiplied(15,23,39,202)),
 ]));
 ui.painter().line_segment([rect.left_top()+egui::vec2(18.0,1.0),rect.right_top()+egui::vec2(-18.0,1.0)],Stroke::new(1.0,Color32::from_white_alpha(18)));
 response.inner
}
pub fn button(ui:&mut egui::Ui,label:&str,enabled:bool,primary:bool,size:Vec2)->bool {
 let slot=ui.painter().add(Shape::Noop);
 let response=ui.add_enabled(enabled,egui::Button::new(egui::RichText::new(label).strong().size(13.0).color(if enabled {TEXT}else{MUTED.gamma_multiply(0.65)})).fill(Color32::TRANSPARENT).stroke(Stroke::NONE).corner_radius(10).min_size(size));
 let (top,bottom)=if !enabled {(Color32::from_rgba_unmultiplied(30,39,55,120),Color32::from_rgba_unmultiplied(20,28,43,120))}else if primary {(Color32::from_rgb(16,132,184),Color32::from_rgb(37,83,204))}else{(Color32::from_rgba_unmultiplied(47,58,79,190),Color32::from_rgba_unmultiplied(29,38,58,180))};
 let top=if enabled&&response.hovered() {blend(top,Color32::WHITE,0.09)}else{top};
 ui.painter().set(slot,gradient(response.rect,10.0,top,bottom));
 ui.painter().rect_stroke(response.rect,10.0,Stroke::new(1.0,if enabled&&(primary||response.has_focus()){CYAN.gamma_multiply(0.6)}else{LINE}),egui::StrokeKind::Inside);
 response.clicked()
}
pub fn navigation(ui:&mut egui::Ui,label:&str,index:usize,selected:bool)->egui::Response {
 let (rect,response)=ui.allocate_exact_size(egui::vec2(ui.available_width(),46.0),egui::Sense::click());
 response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,true,label));
 if selected||response.hovered() {ui.painter().add(gradient(rect,10.0,Color32::from_rgba_unmultiplied(31,95,132,if selected{100}else{50}),Color32::from_rgba_unmultiplied(28,50,84,80)));}
 if selected {ui.painter().rect_stroke(rect,10.0,Stroke::new(1.0,CYAN.gamma_multiply(0.42)),egui::StrokeKind::Inside);ui.painter().rect_filled(Rect::from_min_size(rect.left_top()+egui::vec2(0.0,12.0),egui::vec2(3.0,22.0)),2.0,CYAN);}
 icon(ui.painter(),Rect::from_center_size(rect.left_center()+egui::vec2(24.0,0.0),egui::vec2(18.0,18.0)),index,if selected{CYAN}else{MUTED});
 ui.painter().text(rect.left_center()+egui::vec2(43.0,0.0),egui::Align2::LEFT_CENTER,label,egui::FontId::proportional(13.0),if selected{TEXT}else{MUTED});
 response
}
fn icon(p:&egui::Painter,r:Rect,index:usize,color:Color32) {
 let stroke=Stroke::new(1.4,color);
 match index {
  0=>{p.rect_stroke(r.shrink2(egui::vec2(0.0,3.0)),3.0,stroke,egui::StrokeKind::Inside);p.line_segment([r.left_center(),r.right_center()],stroke);},
  1=>{for x in 0..2 {for y in 0..2 {p.rect_stroke(Rect::from_min_size(r.min+egui::vec2(x as f32*10.0,y as f32*10.0),egui::vec2(7.0,7.0)),2.0,stroke,egui::StrokeKind::Inside);}}},
  2=>{for x in 0..3 {for y in 0..3 {p.circle_filled(r.min+egui::vec2(3.0+x as f32*6.0,3.0+y as f32*6.0),1.5,color);}}},
  3=>{p.circle_stroke(r.center_top()+egui::vec2(0.0,4.0),4.0,stroke);p.rect_stroke(Rect::from_min_size(r.min+egui::vec2(2.0,11.0),egui::vec2(14.0,7.0)),5.0,stroke,egui::StrokeKind::Inside);},
  _=>{p.circle_stroke(r.center(),8.0,stroke);p.text(r.center(),egui::Align2::CENTER_CENTER,"?",egui::FontId::proportional(13.0),color);},
 }
}
#[cfg(test)] mod tests {
 use super::*;
 #[test] fn glass_library_dock_layout_stacks_small_windows_and_fits_desktop_columns() {
  assert!(library_columns(640.0).is_none());assert!(library_columns(899.0).is_none());
  for width in [900.0,1100.0,1600.0] {let (catalog,preview)=library_columns(width).unwrap();assert!(catalog>=552.0);assert_eq!(catalog+preview+16.0,width);}
 }
 #[test] fn glass_panels_and_controls_stay_inside_compact_and_large_content() {
  for size in [egui::vec2(900.0,640.0),egui::vec2(1440.0,900.0)] {
   let ctx=egui::Context::default();let input=egui::RawInput {screen_rect:Some(Rect::from_min_size(Pos2::ZERO,size)),..Default::default()};
   for _ in 0..2 {let _=ctx.run(input.clone(),|ctx|{
    background(ctx);egui::CentralPanel::default().show(ctx,|ui|{let bounds=ui.max_rect();panel(ui,|ui|{ui.label("Tus originales");button(ui,"Aplicar diseño",false,true,egui::vec2(ui.available_width(),44.0));});
     assert!(ui.min_rect().right()<=bounds.right()+1.0);assert!(ui.min_rect().bottom()<=bounds.bottom()+1.0);
    });
   });}
  }
 }
 #[test] fn ambient_is_painted_before_visible_panel_content() {
  let ctx=egui::Context::default();let input=egui::RawInput {screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(900.0,640.0))),..Default::default()};
  let output=ctx.run(input,|ctx|{background(ctx);egui::TopBottomPanel::top("visible-header").show(ctx,|ui|{ui.label("Wallet Studio visible");});egui::CentralPanel::default().show(ctx,|ui|{ui.label("Biblioteca visible");});});
  let ambient=output.shapes.iter().position(|shape|matches!(&shape.shape,Shape::Mesh(_))).expect("ambient mesh");
  let text=output.shapes.iter().position(|shape|matches!(&shape.shape,Shape::Text(_))).expect("visible panel text");
  assert!(ambient<text,"ambient must precede panel text in the actual compositing order");
 }
 #[test] fn rounded_gradient_geometry_is_bounded_for_compact_and_large_panels() {
  for size in [egui::vec2(1.0,1.0),egui::vec2(168.0,105.0),egui::vec2(1500.0,900.0)] {
   let rect=Rect::from_min_size(egui::pos2(20.0,30.0),size);let Shape::Mesh(mesh)=gradient(rect,14.0,CYAN,VIOLET)else{panic!("missing mesh")};
   assert_eq!(mesh.vertices.len(),37);assert_eq!(mesh.indices.len(),108);
   for vertex in &mesh.vertices {assert!(rect.expand(0.01).contains(vertex.pos));}
   assert!(mesh.indices.iter().all(|index|(*index as usize)<mesh.vertices.len()));
  }
 }
 #[test] fn disabled_native_glass_button_cannot_trigger_action() {
  let ctx=egui::Context::default();let mut rect=Rect::NOTHING;
  let input=egui::RawInput {screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(400.0,200.0))),..Default::default()};
  let _=ctx.run(input.clone(),|ctx|{egui::CentralPanel::default().show(ctx,|ui|{let before=ui.cursor().min;assert!(!button(ui,"Aplicar diseño",false,true,egui::vec2(180.0,44.0)));rect=Rect::from_min_size(before,egui::vec2(180.0,44.0));});});
  let mut click=input;click.events=vec![egui::Event::PointerMoved(rect.center()),egui::Event::PointerButton{pos:rect.center(),button:egui::PointerButton::Primary,pressed:true,modifiers:egui::Modifiers::NONE},egui::Event::PointerButton{pos:rect.center(),button:egui::PointerButton::Primary,pressed:false,modifiers:egui::Modifiers::NONE}];
  let _=ctx.run(click,|ctx|{egui::CentralPanel::default().show(ctx,|ui|{assert!(!button(ui,"Aplicar diseño",false,true,egui::vec2(180.0,44.0)));});});
 }
}
