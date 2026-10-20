//! # Wallet Studio · Glassmorphism UI kit (egui)
//!
//! Componentes visuales puros para la modernización de `windows-source/src/app.rs`.
//! No tocan estado, hilos, canales ni callbacks: reciben datos y devuelven
//! `egui::Response` para que los handlers existentes sigan intactos.
//!
//! ## Integración
//! Copiar este archivo a `windows-source/src/glassmorphism.rs` y declararlo:
//!
//! ```ignore
//! mod glassmorphism;
//! use glassmorphism::*;
//! ```
//!
//! ## Nota de versión
//! El brief pide la base de API de **egui 0.31** (`Frame::new`, `CornerRadius`,
//! `StrokeKind`, `Margin` con `i8`). El árbol de Wallet Studio está en **egui 0.33**;
//! este módulo usa únicamente APIs presentes en ambas versiones y se verificó con
//! `cargo check` contra `egui 0.33.3`. No usa APIs eliminadas ni features extra.
//!
//! ## Ejemplo rápido
//! ```ignore
//! let t = GlassTokens::default();
//! paint_app_background(ui.painter(), ui.max_rect(), &t);
//!
//! let items = [
//!     SidebarItem::new(GlassIcon::Cards, "Tarjetas"),
//!     SidebarItem::new(GlassIcon::Library, "Biblioteca"),
//!     SidebarItem::new(GlassIcon::Keyboard, "Teclado"),
//! ];
//! if let Some(index) = sidebar(ui, &t, &items, &mut selected) {
//!     // mapear index -> AppTab, como hoy
//! }
//!
//! floating_dock(ctx, &t, "main", |ui| {
//!     if glass_button(ui, &t, "Elegir imagen…", true, Some(GlassIcon::Upload)).clicked() {
//!         // abrir selector existente
//!     }
//!     if gradient_button_sized(ui, &t, "Aplicar diseño", true, vec2(260.0, 44.0), None).clicked() {
//!         // acción existente
//!     }
//! });
//! ```
#![allow(dead_code)]

use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{
    pos2, vec2, Align2, Area, Color32, Context, CursorIcon, FontId, Frame, Id, InnerResponse,
    Margin, Order, Painter, Pos2, Rect, Response, RichText, Sense, Shadow, Shape, Stroke,
    StrokeKind, Ui, Vec2,
};
use std::hash::Hash;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Tokens
// ---------------------------------------------------------------------------

/// Paleta y geometría del tema Glassmorphism oscuro.
#[derive(Clone, Copy)]
pub struct GlassTokens {
    // Fondo
    pub bg_top: Color32,
    pub bg_bottom: Color32,
    pub bg_glow: Color32,

    // Superficies
    pub glass: Color32,
    pub glass_strong: Color32,
    pub glass_hover: Color32,

    // Bordes
    pub border: Color32,
    pub border_soft: Color32,
    pub border_active: Color32,

    // Acentos
    pub cyan: Color32,
    pub blue: Color32,
    pub green: Color32,
    pub amber: Color32,
    pub red: Color32,

    // Texto
    pub text: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,

    // Radios
    pub radius_card: u8,
    pub radius_control: u8,
    pub radius_pill: u8,
    pub radius_phone: u8,

    pub card_shadow: Shadow,
}

impl Default for GlassTokens {
    fn default() -> Self {
        Self {
            bg_top: Color32::from_rgb(10, 15, 28),
            bg_bottom: Color32::from_rgb(5, 7, 12),
            bg_glow: Color32::from_rgba_unmultiplied(56, 189, 248, 22),

            glass: Color32::from_rgba_unmultiplied(17, 24, 39, 176),
            glass_strong: Color32::from_rgba_unmultiplied(17, 24, 39, 218),
            glass_hover: Color32::from_rgba_unmultiplied(32, 44, 64, 218),

            border: Color32::from_rgba_unmultiplied(255, 255, 255, 20),
            border_soft: Color32::from_rgba_unmultiplied(255, 255, 255, 10),
            border_active: Color32::from_rgba_unmultiplied(56, 189, 248, 128),

            cyan: Color32::from_rgb(56, 189, 248),
            blue: Color32::from_rgb(37, 99, 235),
            green: Color32::from_rgb(16, 185, 129),
            amber: Color32::from_rgb(245, 158, 11),
            red: Color32::from_rgb(239, 68, 68),

            text: Color32::from_rgb(248, 250, 252),
            text_secondary: Color32::from_rgb(148, 163, 184),
            text_muted: Color32::from_rgb(100, 116, 139),

            radius_card: 14,
            radius_control: 10,
            radius_pill: 255,
            radius_phone: 24,

            card_shadow: Shadow {
                offset: [0, 10],
                blur: 28,
                spread: 0,
                color: Color32::from_rgba_unmultiplied(0, 0, 0, 120),
            },
        }
    }
}

impl GlassTokens {
    pub fn dark() -> Self {
        Self::default()
    }

    pub fn gradient_left(&self) -> Color32 {
        self.cyan
    }

    pub fn gradient_right(&self) -> Color32 {
        self.blue
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GradientAxis {
    Horizontal,
    Vertical,
}

// ---------------------------------------------------------------------------
// Helpers de color
// ---------------------------------------------------------------------------

fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t)
        .round()
        .clamp(0.0, 255.0) as u8
}

/// Interpola dos colores en sRGB no premultiplicado (alpha incluido).
pub fn mix_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let [ar, ag, ab, aa] = a.to_srgba_unmultiplied();
    let [br, bg, bb, ba] = b.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(
        lerp_u8(ar, br, t),
        lerp_u8(ag, bg, t),
        lerp_u8(ab, bb, t),
        lerp_u8(aa, ba, t),
    )
}

/// Devuelve el color con el alpha indicado, conservando RGB.
pub fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    let [r, g, b, _] = color.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, alpha)
}

// ---------------------------------------------------------------------------
// Fondo y superficies
// ---------------------------------------------------------------------------

fn rounded_rect_perimeter(rect: Rect, radius: f32, corner_steps: usize) -> Vec<Pos2> {
    let r = radius.clamp(0.0, rect.width().min(rect.height()) * 0.5);
    if r <= 0.5 {
        return vec![
            rect.left_top(),
            rect.right_top(),
            rect.right_bottom(),
            rect.left_bottom(),
        ];
    }

    let steps = corner_steps.max(1);
    let pi = std::f32::consts::PI;
    let corners = [
        (pos2(rect.left() + r, rect.top() + r), pi, 1.5 * pi),
        (pos2(rect.right() - r, rect.top() + r), 1.5 * pi, 2.0 * pi),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0, 0.5 * pi),
        (pos2(rect.left() + r, rect.bottom() - r), 0.5 * pi, pi),
    ];

    let capacity = steps * corners.len() + corners.len();
    let mut points = Vec::with_capacity(capacity);
    for (center, start, end) in corners {
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let angle = start + (end - start) * t;
            points.push(pos2(
                center.x + angle.cos() * r,
                center.y + angle.sin() * r,
            ));
        }
    }
    points
}

fn arc_points(center: Pos2, radius: f32, start: f32, end: f32, segments: usize) -> Vec<Pos2> {
    let segments = segments.max(2);
    (0..=segments)
        .map(|index| {
            let t = index as f32 / segments as f32;
            let angle = start + (end - start) * t;
            pos2(
                center.x + angle.cos() * radius,
                center.y + angle.sin() * radius,
            )
        })
        .collect()
}

/// Gradiente lineal con esquinas redondeadas reales (mesh triangular).
pub fn paint_gradient_rect(
    painter: &Painter,
    rect: Rect,
    radius: f32,
    from: Color32,
    to: Color32,
    axis: GradientAxis,
) {
    if rect.width() <= 0.1 || rect.height() <= 0.1 {
        return;
    }

    let perimeter = rounded_rect_perimeter(rect, radius, 7);
    if perimeter.is_empty() {
        return;
    }

    let color_at = |point: Pos2| {
        let t = match axis {
            GradientAxis::Horizontal => (point.x - rect.left()) / rect.width(),
            GradientAxis::Vertical => (point.y - rect.top()) / rect.height(),
        };
        mix_color(from, to, t.clamp(0.0, 1.0))
    };

    let mut mesh = Mesh::default();
    let center = rect.center();
    mesh.vertices.push(Vertex {
        pos: center,
        uv: WHITE_UV,
        color: color_at(center),
    });
    for point in &perimeter {
        mesh.vertices.push(Vertex {
            pos: *point,
            uv: WHITE_UV,
            color: color_at(*point),
        });
    }

    let count = perimeter.len() as u32;
    for index in 0..count {
        mesh.add_triangle(0, 1 + index, 1 + (index + 1) % count);
    }
    painter.add(Shape::mesh(mesh));
}

fn paint_soft_shadow(painter: &Painter, rect: Rect, radius: f32, strength: f32) {
    let base = rect.translate(vec2(0.0, 2.0));
    for step in 0..4 {
        let grow = 2.0 + step as f32 * 3.0;
        let alpha = (strength * 255.0 * (1.0 - step as f32 / 5.0) * 0.5)
            .clamp(0.0, 255.0) as u8;
        painter.rect_filled(
            base.expand(grow),
            radius + grow,
            Color32::from_rgba_unmultiplied(0, 0, 0, alpha),
        );
    }
}

fn paint_glow(
    painter: &Painter,
    rect: Rect,
    radius: f32,
    color: Color32,
    intensity: f32,
) {
    for step in 0..3 {
        let grow = 1.0 + step as f32 * 2.0;
        let alpha = (intensity * (1.0 - step as f32 * 0.28) * 160.0)
            .clamp(0.0, 255.0) as u8;
        painter.rect_stroke(
            rect.expand(grow),
            radius + grow,
            Stroke::new(1.0 + grow * 0.25, with_alpha(color, alpha)),
            StrokeKind::Outside,
        );
    }
}

/// Fondo de la aplicación: degradado vertical + halo cian/azul superior.
pub fn paint_app_background(painter: &Painter, rect: Rect, tokens: &GlassTokens) {
    paint_gradient_rect(
        painter,
        rect,
        0.0,
        tokens.bg_top,
        tokens.bg_bottom,
        GradientAxis::Vertical,
    );

    let center = pos2(rect.center().x, rect.top() + rect.height() * 0.02);
    for (index, scale) in [0.16_f32, 0.26, 0.38, 0.52].iter().enumerate() {
        let alpha = (10.0 - index as f32 * 2.0).max(2.0) as u8;
        painter.circle_filled(
            center,
            rect.width().max(rect.height()) * scale,
            with_alpha(tokens.bg_glow, alpha),
        );
    }
}

/// `Frame` base para paneles/tarjetas glass.
pub fn glass_frame(tokens: &GlassTokens) -> Frame {
    Frame::new()
        .fill(tokens.glass)
        .stroke(Stroke::new(1.0_f32, tokens.border))
        .corner_radius(tokens.radius_card)
        .shadow(tokens.card_shadow)
}

/// Tarjeta glass con padding estándar de 16 px.
pub fn glass_card<R>(
    ui: &mut Ui,
    tokens: &GlassTokens,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    glass_frame(tokens)
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.vertical(add_contents).inner
        })
        .inner
}

/// Pinta una superficie glass con brillo superior, borde y glow opcional.
pub fn paint_glass_rect(
    painter: &Painter,
    rect: Rect,
    radius: f32,
    tokens: &GlassTokens,
    hover: f32,
    active: bool,
) {
    let hover = hover.clamp(0.0, 1.0);
    if active || hover > 0.01 {
        paint_glow(
            painter,
            rect,
            radius,
            tokens.cyan,
            if active { 0.55 } else { 0.25 * hover },
        );
    }

    let base = mix_color(tokens.glass, tokens.glass_hover, hover);
    painter.rect_filled(rect, radius, base);

    let sheen = Rect::from_min_max(
        rect.left_top(),
        pos2(rect.right(), rect.top() + rect.height() * 0.45),
    );
    let sheen_alpha = (14.0 + hover * 12.0) as u8;
    paint_gradient_rect(
        painter,
        sheen,
        radius,
        with_alpha(Color32::WHITE, sheen_alpha),
        with_alpha(Color32::WHITE, 0),
        GradientAxis::Vertical,
    );

    if active {
        painter.rect_filled(rect, radius, with_alpha(tokens.cyan, 10));
    }

    let border = if active {
        tokens.border_active
    } else {
        mix_color(tokens.border, tokens.border_active, hover * 0.5)
    };
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(1.0_f32, border),
        StrokeKind::Inside,
    );
}

// ---------------------------------------------------------------------------
// Iconos vectoriales
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlassIcon {
    Cards,
    Library,
    Keyboard,
    Account,
    Help,
    Plus,
    Upload,
    Download,
    CloudDown,
    CloudUp,
    ShieldCheck,
    Restore,
    Sync,
    Devices,
    Logout,
    Search,
    ChevronDown,
    Close,
    Check,
    Phone,
    Lock,
}

/// Pinta un icono de línea de 20×20 (escala automática al `rect`).
pub fn paint_icon(
    painter: &Painter,
    icon: GlassIcon,
    rect: Rect,
    color: Color32,
    stroke_width: f32,
) {
    let r = rect.shrink(rect.width().min(rect.height()) * 0.04);
    let c = r.center();
    let w = r.width();
    let h = r.height();
    let stroke = Stroke::new(stroke_width, color);
    let line = |points: Vec<Pos2>| {
        painter.add(Shape::line(points, stroke));
    };
    let pi = std::f32::consts::PI;

    match icon {
        GlassIcon::Cards => {
            let card = Rect::from_center_size(c, vec2(w * 0.84, h * 0.62));
            painter.rect_stroke(card, w * 0.07, stroke, StrokeKind::Inside);
            painter.line_segment(
                [
                    pos2(card.left(), card.top() + h * 0.20),
                    pos2(card.right(), card.top() + h * 0.20),
                ],
                stroke,
            );
        }
        GlassIcon::Library => {
            painter.line_segment(
                [
                    pos2(c.x - w * 0.28, c.y - h * 0.30),
                    pos2(c.x - w * 0.28, c.y + h * 0.30),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    pos2(c.x - w * 0.10, c.y - h * 0.30),
                    pos2(c.x - w * 0.10, c.y + h * 0.30),
                ],
                stroke,
            );
            line(vec![
                pos2(c.x + w * 0.04, c.y + h * 0.30),
                pos2(c.x + w * 0.22, c.y + h * 0.22),
                pos2(c.x + w * 0.30, c.y - h * 0.26),
                pos2(c.x + w * 0.12, c.y - h * 0.30),
            ]);
            painter.line_segment(
                [
                    pos2(c.x - w * 0.34, c.y + h * 0.33),
                    pos2(c.x + w * 0.34, c.y + h * 0.33),
                ],
                stroke,
            );
        }
        GlassIcon::Keyboard => {
            let keyboard = Rect::from_center_size(c, vec2(w * 0.86, h * 0.60));
            painter.rect_stroke(keyboard, w * 0.06, stroke, StrokeKind::Inside);
            let dot = (w * 0.024).max(0.8);
            for row in 0..3 {
                let y = keyboard.top() + keyboard.height() * (0.25 + row as f32 * 0.25);
                for column in 0..4 {
                    let x = keyboard.left()
                        + keyboard.width() * (0.17 + column as f32 * 0.22);
                    painter.circle_filled(pos2(x, y), dot, color);
                }
            }
        }
        GlassIcon::Account => {
            painter.circle_stroke(pos2(c.x, c.y - h * 0.15), w * 0.15, stroke);
            line(arc_points(
                pos2(c.x, c.y + h * 0.32),
                w * 0.29,
                pi * 1.12,
                pi * 1.88,
                14,
            ));
        }
        GlassIcon::Help => {
            painter.circle_stroke(c, w * 0.38, stroke);
            painter.text(
                c + vec2(0.0, h * 0.02),
                Align2::CENTER_CENTER,
                "?",
                FontId::proportional(h * 0.52),
                color,
            );
        }
        GlassIcon::Plus => {
            painter.line_segment(
                [pos2(c.x - w * 0.26, c.y), pos2(c.x + w * 0.26, c.y)],
                stroke,
            );
            painter.line_segment(
                [pos2(c.x, c.y - h * 0.26), pos2(c.x, c.y + h * 0.26)],
                stroke,
            );
        }
        GlassIcon::Upload => {
            line(vec![
                pos2(c.x - w * 0.30, c.y + h * 0.10),
                pos2(c.x - w * 0.30, c.y + h * 0.30),
                pos2(c.x + w * 0.30, c.y + h * 0.30),
                pos2(c.x + w * 0.30, c.y + h * 0.10),
            ]);
            painter.line_segment(
                [pos2(c.x, c.y + h * 0.22), pos2(c.x, c.y - h * 0.30)],
                stroke,
            );
            line(vec![
                pos2(c.x - w * 0.12, c.y - h * 0.17),
                pos2(c.x, c.y - h * 0.30),
                pos2(c.x + w * 0.12, c.y - h * 0.17),
            ]);
        }
        GlassIcon::Download => {
            line(vec![
                pos2(c.x - w * 0.30, c.y + h * 0.06),
                pos2(c.x - w * 0.30, c.y + h * 0.30),
                pos2(c.x + w * 0.30, c.y + h * 0.30),
                pos2(c.x + w * 0.30, c.y + h * 0.06),
            ]);
            painter.line_segment(
                [pos2(c.x, c.y - h * 0.30), pos2(c.x, c.y + h * 0.18)],
                stroke,
            );
            line(vec![
                pos2(c.x - w * 0.12, c.y + h * 0.06),
                pos2(c.x, c.y + h * 0.18),
                pos2(c.x + w * 0.12, c.y + h * 0.06),
            ]);
        }
        GlassIcon::CloudDown | GlassIcon::CloudUp => {
            painter.circle_stroke(pos2(c.x - w * 0.15, c.y - h * 0.08), w * 0.15, stroke);
            painter.circle_stroke(pos2(c.x + w * 0.02, c.y - h * 0.14), w * 0.19, stroke);
            painter.circle_stroke(pos2(c.x + w * 0.20, c.y - h * 0.02), w * 0.13, stroke);
            painter.line_segment(
                [
                    pos2(c.x - w * 0.29, c.y + h * 0.08),
                    pos2(c.x + w * 0.33, c.y + h * 0.08),
                ],
                stroke,
            );
            if matches!(icon, GlassIcon::CloudDown) {
                painter.line_segment(
                    [pos2(c.x, c.y + h * 0.14), pos2(c.x, c.y + h * 0.36)],
                    stroke,
                );
                line(vec![
                    pos2(c.x - w * 0.09, c.y + h * 0.26),
                    pos2(c.x, c.y + h * 0.36),
                    pos2(c.x + w * 0.09, c.y + h * 0.26),
                ]);
            } else {
                painter.line_segment(
                    [pos2(c.x, c.y + h * 0.36), pos2(c.x, c.y + h * 0.14)],
                    stroke,
                );
                line(vec![
                    pos2(c.x - w * 0.09, c.y + h * 0.24),
                    pos2(c.x, c.y + h * 0.14),
                    pos2(c.x + w * 0.09, c.y + h * 0.24),
                ]);
            }
        }
        GlassIcon::ShieldCheck => {
            let points = vec![
                pos2(c.x, c.y - h * 0.34),
                pos2(c.x + w * 0.30, c.y - h * 0.24),
                pos2(c.x + w * 0.26, c.y + h * 0.10),
                pos2(c.x, c.y + h * 0.34),
                pos2(c.x - w * 0.26, c.y + h * 0.10),
                pos2(c.x - w * 0.30, c.y - h * 0.24),
                pos2(c.x, c.y - h * 0.34),
            ];
            line(points);
            line(vec![
                pos2(c.x - w * 0.12, c.y + h * 0.02),
                pos2(c.x - w * 0.02, c.y + h * 0.12),
                pos2(c.x + w * 0.17, c.y - h * 0.10),
            ]);
        }
        GlassIcon::Restore => {
            line(arc_points(c, w * 0.30, -pi * 0.35, pi * 1.05, 18));
            line(vec![
                pos2(c.x - w * 0.06, c.y - h * 0.32),
                pos2(c.x + w * 0.18, c.y - h * 0.24),
                pos2(c.x + w * 0.02, c.y - h * 0.06),
            ]);
        }
        GlassIcon::Sync => {
            line(arc_points(c, w * 0.28, pi * 0.12, pi * 0.90, 12));
            line(arc_points(c, w * 0.28, pi * 1.12, pi * 1.90, 12));
            line(vec![
                pos2(c.x + w * 0.18, c.y - h * 0.24),
                pos2(c.x + w * 0.30, c.y - h * 0.12),
                pos2(c.x + w * 0.14, c.y - h * 0.08),
            ]);
            line(vec![
                pos2(c.x - w * 0.18, c.y + h * 0.24),
                pos2(c.x - w * 0.30, c.y + h * 0.12),
                pos2(c.x - w * 0.14, c.y + h * 0.08),
            ]);
        }
        GlassIcon::Devices => {
            let monitor = Rect::from_center_size(c + vec2(-w * 0.10, -h * 0.06), vec2(w * 0.58, h * 0.40));
            painter.rect_stroke(monitor, w * 0.04, stroke, StrokeKind::Inside);
            painter.line_segment(
                [
                    pos2(monitor.center().x, monitor.bottom()),
                    pos2(monitor.center().x, monitor.bottom() + h * 0.12),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    pos2(monitor.center().x - w * 0.13, monitor.bottom() + h * 0.12),
                    pos2(monitor.center().x + w * 0.13, monitor.bottom() + h * 0.12),
                ],
                stroke,
            );
            let phone = Rect::from_center_size(c + vec2(w * 0.30, h * 0.12), vec2(w * 0.24, h * 0.46));
            painter.rect_stroke(phone, w * 0.05, stroke, StrokeKind::Inside);
        }
        GlassIcon::Logout => {
            let door = Rect::from_min_max(
                pos2(c.x - w * 0.30, c.y - h * 0.32),
                pos2(c.x + w * 0.02, c.y + h * 0.32),
            );
            painter.rect_stroke(door, w * 0.04, stroke, StrokeKind::Inside);
            painter.line_segment(
                [pos2(c.x - w * 0.10, c.y), pos2(c.x + w * 0.32, c.y)],
                stroke,
            );
            line(vec![
                pos2(c.x + w * 0.20, c.y - h * 0.10),
                pos2(c.x + w * 0.32, c.y),
                pos2(c.x + w * 0.20, c.y + h * 0.10),
            ]);
        }
        GlassIcon::Search => {
            painter.circle_stroke(c + vec2(-w * 0.08, -h * 0.08), w * 0.26, stroke);
            painter.line_segment(
                [c + vec2(w * 0.11, h * 0.11), c + vec2(w * 0.31, h * 0.31)],
                stroke,
            );
        }
        GlassIcon::ChevronDown => {
            line(vec![
                pos2(c.x - w * 0.20, c.y - h * 0.08),
                pos2(c.x, c.y + h * 0.12),
                pos2(c.x + w * 0.20, c.y - h * 0.08),
            ]);
        }
        GlassIcon::Close => {
            painter.line_segment(
                [c + vec2(-w * 0.22, -h * 0.22), c + vec2(w * 0.22, h * 0.22)],
                stroke,
            );
            painter.line_segment(
                [c + vec2(w * 0.22, -h * 0.22), c + vec2(-w * 0.22, h * 0.22)],
                stroke,
            );
        }
        GlassIcon::Check => {
            line(vec![
                pos2(c.x - w * 0.28, c.y + h * 0.02),
                pos2(c.x - w * 0.08, c.y + h * 0.22),
                pos2(c.x + w * 0.30, c.y - h * 0.22),
            ]);
        }
        GlassIcon::Phone => {
            let body = Rect::from_center_size(c, vec2(w * 0.46, h * 0.80));
            painter.rect_stroke(body, w * 0.09, stroke, StrokeKind::Inside);
            painter.line_segment(
                [
                    pos2(body.center().x - w * 0.08, body.top() + h * 0.08),
                    pos2(body.center().x + w * 0.08, body.top() + h * 0.08),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    pos2(body.center().x - w * 0.08, body.bottom() - h * 0.07),
                    pos2(body.center().x + w * 0.08, body.bottom() - h * 0.07),
                ],
                stroke,
            );
        }
        GlassIcon::Lock => {
            let body = Rect::from_center_size(c + vec2(0.0, h * 0.10), vec2(w * 0.56, h * 0.44));
            painter.rect_stroke(body, w * 0.05, stroke, StrokeKind::Inside);
            line(arc_points(
                pos2(c.x, c.y - h * 0.10),
                w * 0.20,
                pi,
                2.0 * pi,
                12,
            ));
            painter.line_segment(
                [
                    pos2(c.x - w * 0.20, c.y - h * 0.10),
                    pos2(c.x - w * 0.20, c.y + h * 0.02),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    pos2(c.x + w * 0.20, c.y - h * 0.10),
                    pos2(c.x + w * 0.20, c.y + h * 0.02),
                ],
                stroke,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Botones
// ---------------------------------------------------------------------------

/// CTA principal con degradado cian→azul, ocupando todo el ancho disponible.
pub fn gradient_button(ui: &mut Ui, tokens: &GlassTokens, label: &str, enabled: bool) -> Response {
    let width = ui.available_width().max(120.0);
    gradient_button_sized(ui, tokens, label, enabled, vec2(width, 44.0), None)
}

/// CTA principal con tamaño e icono opcionales.
pub fn gradient_button_sized(
    ui: &mut Ui,
    tokens: &GlassTokens,
    label: &str,
    enabled: bool,
    size: Vec2,
    icon: Option<GlassIcon>,
) -> Response {
    let interactive = enabled && ui.is_enabled();
    let sense = if interactive { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let id = response.id;
    let hover = ui
        .ctx()
        .animate_bool_with_time(id.with("hover"), interactive && response.hovered(), 0.12);
    let pressed = interactive && response.is_pointer_button_down_on();
    let response = if interactive {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    };

    let radius = tokens.radius_control as f32;
    let painter = ui.painter();

    if enabled {
        let lighten = (hover * 0.10 - if pressed { 0.08 } else { 0.0 }).max(0.0);
        let left = mix_color(tokens.cyan, Color32::WHITE, lighten);
        let right = mix_color(tokens.blue, Color32::WHITE, lighten);
        if interactive {
            paint_glow(painter, rect, radius, tokens.cyan, 0.20 + hover * 0.30);
        }
        paint_gradient_rect(painter, rect, radius, left, right, GradientAxis::Horizontal);
    } else {
        paint_gradient_rect(
            painter,
            rect,
            radius,
            with_alpha(tokens.text_muted, 70),
            with_alpha(tokens.text_muted, 45),
            GradientAxis::Horizontal,
        );
    }

    let sheen = Rect::from_min_max(
        rect.left_top(),
        pos2(rect.right(), rect.top() + rect.height() * 0.5),
    );
    paint_gradient_rect(
        painter,
        sheen,
        radius,
        with_alpha(Color32::WHITE, if enabled { (26.0 + hover * 12.0) as u8 } else { 10 }),
        with_alpha(Color32::WHITE, 0),
        GradientAxis::Vertical,
    );

    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(
            1.0_f32,
            with_alpha(Color32::WHITE, if enabled { (40.0 + hover * 30.0) as u8 } else { 16 }),
        ),
        StrokeKind::Inside,
    );

    let foreground = if enabled {
        Color32::WHITE
    } else {
        with_alpha(tokens.text, 120)
    };
    let font = FontId::proportional(14.0);

    match icon {
        Some(icon) => {
            let icon_rect = Rect::from_center_size(
                pos2(rect.left() + 22.0, rect.center().y),
                vec2(18.0, 18.0),
            );
            paint_icon(painter, icon, icon_rect, foreground, 1.6);
            painter.text(
                pos2(rect.left() + 40.0, rect.center().y),
                Align2::LEFT_CENTER,
                label,
                font,
                foreground,
            );
        }
        None => {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                label,
                font,
                foreground,
            );
        }
    }

    response
}

/// Botón secundario glass (dock, acciones de fila, reintentos).
pub fn glass_button(
    ui: &mut Ui,
    tokens: &GlassTokens,
    label: &str,
    enabled: bool,
    icon: Option<GlassIcon>,
) -> Response {
    let width = ui.available_width().max(120.0);
    glass_button_sized(ui, tokens, label, enabled, vec2(width, 42.0), icon)
}

pub fn glass_button_sized(
    ui: &mut Ui,
    tokens: &GlassTokens,
    label: &str,
    enabled: bool,
    size: Vec2,
    icon: Option<GlassIcon>,
) -> Response {
    let interactive = enabled && ui.is_enabled();
    let sense = if interactive { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let id = response.id;
    let hover = ui
        .ctx()
        .animate_bool_with_time(id.with("hover"), interactive && response.hovered(), 0.12);
    let pressed = interactive && response.is_pointer_button_down_on();
    let response = if interactive {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    };

    let radius = tokens.radius_control as f32;
    let painter = ui.painter();
    paint_glass_rect(painter, rect, radius, tokens, hover, false);
    if pressed {
        painter.rect_filled(rect, radius, with_alpha(tokens.text, 8));
    }

    let foreground = if enabled {
        mix_color(tokens.text_secondary, tokens.text, hover)
    } else {
        tokens.text_muted
    };
    let font = FontId::proportional(13.5);

    match icon {
        Some(icon) => {
            let icon_rect = Rect::from_center_size(
                pos2(rect.left() + 21.0, rect.center().y),
                vec2(17.0, 17.0),
            );
            paint_icon(painter, icon, icon_rect, foreground, 1.5);
            painter.text(
                pos2(rect.left() + 38.0, rect.center().y),
                Align2::LEFT_CENTER,
                label,
                font,
                foreground,
            );
        }
        None => {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                label,
                font,
                foreground,
            );
        }
    }

    response
}

// ---------------------------------------------------------------------------
// Toggle switch
// ---------------------------------------------------------------------------

/// Switch iOS-style animado. Devuelve la respuesta del widget.
pub fn toggle_switch(
    ui: &mut Ui,
    tokens: &GlassTokens,
    on: &mut bool,
    enabled: bool,
) -> Response {
    let interactive = enabled && ui.is_enabled();
    let sense = if interactive { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(vec2(46.0, 26.0), sense);
    if interactive && response.clicked() {
        *on = !*on;
    }
    let response = if interactive {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    };

    let anim = ui
        .ctx()
        .animate_bool_with_time(response.id.with("knob"), *on, 0.16);
    let radius = rect.height() * 0.5;
    let painter = ui.painter();

    let off = mix_color(tokens.glass_strong, tokens.border, 0.35);
    let on_color = mix_color(tokens.blue, tokens.cyan, 0.40);
    painter.rect_filled(rect, radius, mix_color(off, on_color, anim));

    if anim > 0.01 {
        paint_glow(painter, rect, radius, tokens.cyan, anim * 0.35);
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(1.0_f32, with_alpha(tokens.cyan, (150.0 * anim) as u8)),
            StrokeKind::Inside,
        );
    } else {
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(1.0_f32, tokens.border),
            StrokeKind::Inside,
        );
    }

    let knob_x = lerp_f32(rect.left() + 13.0, rect.right() - 13.0, anim);
    let knob = pos2(knob_x, rect.center().y);
    painter.circle_filled(
        knob + vec2(0.0, 1.5),
        9.5,
        Color32::from_rgba_unmultiplied(0, 0, 0, 70),
    );
    painter.circle_filled(
        knob,
        9.5,
        if enabled {
            Color32::WHITE
        } else {
            with_alpha(Color32::WHITE, 160)
        },
    );

    response
}

// ---------------------------------------------------------------------------
// KPI cards
// ---------------------------------------------------------------------------

/// Tarjeta de métrica: icono en burbuja, valor grande y etiqueta.
pub fn kpi_card(
    ui: &mut Ui,
    tokens: &GlassTokens,
    icon: GlassIcon,
    value: &str,
    label: &str,
    accent: Color32,
) -> Response {
    let width = ui.available_width().max(120.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, 132.0), Sense::hover());
    let hover = ui
        .ctx()
        .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.14);

    let radius = tokens.radius_card as f32;
    let painter = ui.painter();
    paint_soft_shadow(painter, rect, radius, 0.18);
    paint_glass_rect(painter, rect, radius, tokens, hover, false);

    let accent_bar = Rect::from_min_max(
        pos2(rect.left() + 18.0, rect.top() + 1.0),
        pos2(rect.right() - 18.0, rect.top() + 2.5),
    );
    painter.rect_filled(accent_bar, 1.0, with_alpha(accent, (90.0 + hover * 80.0) as u8));

    let icon_center = pos2(rect.center().x, rect.top() + 34.0);
    painter.circle_filled(icon_center, 23.0, with_alpha(accent, 26));
    painter.circle_stroke(
        icon_center,
        23.0,
        Stroke::new(1.0_f32, with_alpha(accent, 110)),
    );
    paint_icon(
        painter,
        icon,
        Rect::from_center_size(icon_center, vec2(22.0, 22.0)),
        accent,
        1.7,
    );

    painter.text(
        pos2(rect.center().x, rect.top() + 78.0),
        Align2::CENTER_CENTER,
        value,
        FontId::proportional(27.0),
        tokens.text,
    );
    painter.text(
        pos2(rect.center().x, rect.top() + 104.0),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(12.5),
        tokens.text_secondary,
    );

    response
}

// ---------------------------------------------------------------------------
// Sidebar
// ---------------------------------------------------------------------------

pub struct SidebarItem<'a> {
    pub icon: GlassIcon,
    pub label: &'a str,
}

impl<'a> SidebarItem<'a> {
    pub const fn new(icon: GlassIcon, label: &'a str) -> Self {
        Self { icon, label }
    }
}

/// Sidebar de iconos con pill activa luminosa. Devuelve el índice pulsado.
pub fn sidebar(
    ui: &mut Ui,
    tokens: &GlassTokens,
    items: &[SidebarItem<'_>],
    selected: &mut usize,
) -> Option<usize> {
    let mut clicked = None;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        for (index, item) in items.iter().enumerate() {
            let width = ui.available_width();
            let (rect, response) =
                ui.allocate_exact_size(vec2(width, 46.0), Sense::click());
            let active = index == *selected;
            let hover = ui.ctx().animate_bool_with_time(
                response.id.with("hover"),
                response.hovered(),
                0.12,
            );
            let radius = 12.0;
            let painter = ui.painter();

            if active {
                paint_glow(painter, rect, radius, tokens.cyan, 0.35);
                painter.rect_filled(rect, radius, with_alpha(tokens.cyan, 26));
                painter.rect_stroke(
                    rect,
                    radius,
                    Stroke::new(1.0_f32, tokens.border_active),
                    StrokeKind::Inside,
                );
                let bar = Rect::from_min_size(
                    pos2(rect.left() + 1.5, rect.center().y - 10.0),
                    vec2(3.0, 20.0),
                );
                painter.rect_filled(bar, 1.5, tokens.cyan);
            } else if hover > 0.01 {
                painter.rect_filled(rect, radius, with_alpha(tokens.text, (10.0 * hover) as u8));
            }

            let foreground = if active {
                tokens.text
            } else {
                mix_color(tokens.text_secondary, tokens.text, hover)
            };
            paint_icon(
                painter,
                item.icon,
                Rect::from_center_size(
                    pos2(rect.left() + 26.0, rect.center().y),
                    vec2(20.0, 20.0),
                ),
                if active { tokens.cyan } else { foreground },
                1.6,
            );
            painter.text(
                pos2(rect.left() + 48.0, rect.center().y),
                Align2::LEFT_CENTER,
                item.label,
                FontId::proportional(14.0),
                foreground,
            );

            if response.clicked() {
                *selected = index;
                clicked = Some(index);
            }
        }
    });
    clicked
}

pub fn sidebar_heading(ui: &mut Ui, tokens: &GlassTokens, text: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(text).size(10.5).color(tokens.text_muted));
    ui.add_space(2.0);
}

// ---------------------------------------------------------------------------
// Floating dock
// ---------------------------------------------------------------------------

/// Dock flotante inferior glass. El contenido lo aporta la vista llamadora.
pub fn floating_dock<R>(
    ctx: &Context,
    tokens: &GlassTokens,
    id_source: impl Hash,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    Area::new(Id::new(("wallet-studio-glass-dock", id_source)))
        .order(Order::Foreground)
        .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -18.0))
        .show(ctx, |ui| {
            Frame::new()
                .fill(tokens.glass_strong)
                .stroke(Stroke::new(1.0_f32, tokens.border))
                .corner_radius(tokens.radius_card + 4)
                .inner_margin(Margin::symmetric(14, 10))
                .shadow(tokens.card_shadow)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(10.0, 0.0);
                        add_contents(ui)
                    })
                    .inner
                })
                .inner
        })
}

pub fn dock_separator(ui: &mut Ui, tokens: &GlassTokens) {
    let (rect, _) = ui.allocate_exact_size(vec2(1.0, 28.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.5, tokens.border);
}

// ---------------------------------------------------------------------------
// Drag & drop box
// ---------------------------------------------------------------------------

pub struct DropBoxResponse {
    pub response: Response,
    pub button_clicked: bool,
    pub drag_hovered: bool,
}

/// Caja dashed para `.passthm`. Toda la caja actúa como botón de selección.
pub fn dashed_drop_box(
    ui: &mut Ui,
    tokens: &GlassTokens,
    title: &str,
    subtitle: &str,
    button_label: &str,
    enabled: bool,
) -> DropBoxResponse {
    let width = ui.available_width().max(200.0);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(vec2(width, 190.0), sense);

    let pointer = ui.ctx().pointer_hover_pos();
    let dragging = ui.input(|input| !input.raw.hovered_files.is_empty());
    let drag_hovered = enabled && dragging && pointer.is_some_and(|p| rect.contains(p));
    let hover = ui.ctx().animate_bool_with_time(
        response.id.with("hover"),
        response.hovered() || drag_hovered,
        0.12,
    );
    let response = if enabled {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    };

    let radius = tokens.radius_card as f32;
    let painter = ui.painter();

    let fill = if drag_hovered {
        mix_color(tokens.glass_strong, with_alpha(tokens.cyan, 34), 0.35)
    } else {
        mix_color(tokens.glass, tokens.glass_hover, hover * 0.4)
    };
    painter.rect_filled(rect, radius, fill);
    if drag_hovered {
        paint_glow(painter, rect, radius, tokens.cyan, 0.45);
    }

    let border = if drag_hovered {
        tokens.border_active
    } else {
        mix_color(tokens.border, tokens.border_active, hover * 0.6)
    };
    for shape in Shape::dashed_line(
        &rounded_rect_perimeter(rect, radius, 8),
        Stroke::new(1.0_f32, border),
        7.0,
        5.0,
    ) {
        painter.add(shape);
    }

    let icon_center = pos2(rect.center().x, rect.top() + 44.0);
    painter.circle_filled(icon_center, 22.0, with_alpha(tokens.cyan, 22));
    paint_icon(
        painter,
        GlassIcon::Upload,
        Rect::from_center_size(icon_center, vec2(22.0, 22.0)),
        tokens.cyan,
        1.7,
    );
    painter.text(
        pos2(rect.center().x, rect.top() + 84.0),
        Align2::CENTER_CENTER,
        title,
        FontId::proportional(14.5),
        tokens.text,
    );
    painter.text(
        pos2(rect.center().x, rect.top() + 104.0),
        Align2::CENTER_CENTER,
        subtitle,
        FontId::proportional(12.0),
        tokens.text_secondary,
    );

    let button_width = 210.0_f32.min(rect.width() - 32.0);
    let button_rect = Rect::from_center_size(
        pos2(rect.center().x, rect.bottom() - 32.0),
        vec2(button_width, 36.0),
    );
    paint_glass_rect(painter, button_rect, 10.0, tokens, hover * 0.7, false);
    painter.text(
        button_rect.center(),
        Align2::CENTER_CENTER,
        button_label,
        FontId::proportional(13.0),
        if enabled {
            tokens.text
        } else {
            tokens.text_muted
        },
    );

    DropBoxResponse {
        button_clicked: enabled && response.clicked(),
        drag_hovered,
        response,
    }
}

/// Consume los archivos soltados en este frame (`ctx.input_mut`).
pub fn take_dropped_files(ctx: &Context) -> Vec<PathBuf> {
    ctx.input_mut(|input| std::mem::take(&mut input.raw.dropped_files))
        .into_iter()
        .filter_map(|file| file.path)
        .collect()
}

// ---------------------------------------------------------------------------
// Marco iPhone
// ---------------------------------------------------------------------------

/// Marco de iPhone con pantalla recortada. El cierre pinta dentro del screen.
pub fn iphone_mockup_frame<R>(
    ui: &mut Ui,
    tokens: &GlassTokens,
    width: f32,
    add_screen: impl FnOnce(&mut Painter, Rect) -> R,
) -> (Rect, R) {
    let width = width.max(180.0);
    let height = width * 2.03;
    let (outer, _response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let radius = (width * 0.135).clamp(28.0, 54.0);
    let painter = ui.painter();

    paint_soft_shadow(painter, outer, radius, 0.35);
    paint_gradient_rect(
        painter,
        outer,
        radius,
        Color32::from_rgb(66, 72, 84),
        Color32::from_rgb(15, 17, 22),
        GradientAxis::Vertical,
    );
    painter.rect_stroke(
        outer,
        radius,
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 30)),
        StrokeKind::Inside,
    );

    let bezel = (width * 0.026).max(8.0);
    let screen = outer.shrink(bezel);
    let screen_radius = (radius - bezel).max(18.0);
    painter.rect_filled(screen.expand(1.0), screen_radius, Color32::from_rgb(0, 0, 0));
    paint_gradient_rect(
        painter,
        screen,
        screen_radius,
        Color32::from_rgb(21, 30, 47),
        Color32::from_rgb(6, 9, 16),
        GradientAxis::Vertical,
    );

    let button_width = (width * 0.012).max(2.5);
    for (offset, button_height) in [(-width * 0.10, width * 0.055), (width * 0.02, width * 0.085)] {
        let button = Rect::from_min_size(
            pos2(outer.left() - button_width * 0.4, outer.center().y + offset),
            vec2(button_width, button_height),
        );
        painter.rect_filled(button, button_width * 0.5, Color32::from_rgb(70, 76, 88));
    }
    let power = Rect::from_min_size(
        pos2(
            outer.right() - button_width * 0.6,
            outer.center().y - width * 0.06,
        ),
        vec2(button_width, width * 0.12),
    );
    painter.rect_filled(power, button_width * 0.5, Color32::from_rgb(70, 76, 88));

    let glow_center = pos2(
        screen.left() + screen.width() * 0.25,
        screen.top() + screen.height() * 0.18,
    );
    painter.circle_filled(glow_center, screen.width() * 0.55, with_alpha(tokens.blue, 26));
    painter.circle_filled(
        glow_center + vec2(screen.width() * 0.35, screen.height() * 0.10),
        screen.width() * 0.35,
        with_alpha(tokens.cyan, 18),
    );

    let mut screen_painter = painter.with_clip_rect(screen.shrink(1.0));
    let content = add_screen(&mut screen_painter, screen);

    let island = Rect::from_center_size(
        pos2(screen.center().x, screen.top() + width * 0.045),
        vec2(width * 0.29, width * 0.072),
    );
    painter.rect_filled(island, island.height() * 0.5, Color32::from_rgb(0, 0, 0));
    let speaker = Rect::from_center_size(
        pos2(screen.center().x, screen.bottom() + bezel * 0.35),
        vec2(width * 0.18, width * 0.006),
    );
    painter.rect_filled(speaker, speaker.height() * 0.5, Color32::from_rgb(40, 44, 52));

    (outer, content)
}

/// Teclado numérico 3×4 de ejemplo para el interior del marco iPhone.
pub fn paint_number_keypad(painter: &mut Painter, screen: Rect, tokens: &GlassTokens) {
    painter.text(
        pos2(screen.center().x, screen.top() + screen.height() * 0.16),
        Align2::CENTER_CENTER,
        "9:41",
        FontId::proportional(screen.height() * 0.065),
        tokens.text,
    );

    let area = Rect::from_min_max(
        pos2(
            screen.left() + screen.width() * 0.06,
            screen.top() + screen.height() * 0.52,
        ),
        pos2(
            screen.right() - screen.width() * 0.06,
            screen.bottom() - screen.height() * 0.09,
        ),
    );
    let columns = 3;
    let rows = 4;
    let gap = area.width() * 0.035;
    let key_width = (area.width() - gap * (columns - 1) as f32) / columns as f32;
    let key_height = (area.height() - gap * (rows - 1) as f32) / rows as f32;

    let labels = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "", "0", "delete"];
    let letters = ["", "ABC", "DEF", "GHI", "JKL", "MNO", "PQRS", "TUV", "WXYZ", "", "", ""];

    for index in 0..12 {
        let column = index % columns;
        let row = index / columns;
        let key = Rect::from_min_size(
            pos2(
                area.left() + column as f32 * (key_width + gap),
                area.top() + row as f32 * (key_height + gap),
            ),
            vec2(key_width, key_height),
        );
        if labels[index].is_empty() {
            continue;
        }
        painter.rect_filled(key, key_height * 0.18, with_alpha(Color32::WHITE, 26));
        if labels[index] == "delete" {
            paint_icon(
                painter,
                GlassIcon::Close,
                Rect::from_center_size(key.center(), vec2(key_height * 0.34, key_height * 0.34)),
                tokens.text,
                1.4,
            );
            continue;
        }
        painter.text(
            key.center() - vec2(0.0, key_height * 0.06),
            Align2::CENTER_CENTER,
            labels[index],
            FontId::proportional(key_height * 0.34),
            tokens.text,
        );
        if !letters[index].is_empty() {
            painter.text(
                key.center() + vec2(0.0, key_height * 0.24),
                Align2::CENTER_CENTER,
                letters[index],
                FontId::proportional(key_height * 0.11),
                tokens.text_secondary,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Tests (sin GPU)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_mix_hits_both_endpoints() {
        let black = Color32::from_rgb(0, 0, 0);
        let white = Color32::from_rgb(255, 255, 255);
        assert_eq!(mix_color(black, white, 0.0), black);
        assert_eq!(mix_color(black, white, 1.0), white);
    }

    #[test]
    fn perimeter_points_stay_inside_the_rect() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(200.0, 100.0));
        let points = rounded_rect_perimeter(rect, 14.0, 6);
        assert!(!points.is_empty());
        assert!(points.iter().all(|point| rect.expand(0.5).contains(*point)));
    }

    #[test]
    fn every_icon_variant_is_debug_printable() {
        let icons = [
            GlassIcon::Cards,
            GlassIcon::Library,
            GlassIcon::Keyboard,
            GlassIcon::Account,
            GlassIcon::Help,
            GlassIcon::Plus,
            GlassIcon::Upload,
            GlassIcon::Download,
            GlassIcon::CloudDown,
            GlassIcon::CloudUp,
            GlassIcon::ShieldCheck,
            GlassIcon::Restore,
            GlassIcon::Sync,
            GlassIcon::Devices,
            GlassIcon::Logout,
            GlassIcon::Search,
            GlassIcon::ChevronDown,
            GlassIcon::Close,
            GlassIcon::Check,
            GlassIcon::Phone,
            GlassIcon::Lock,
        ];
        for icon in icons {
            assert!(!format!("{icon:?}").is_empty());
        }
    }
}
