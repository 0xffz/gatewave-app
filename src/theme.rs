//! Palette, typography and global egui style for Number Desk.

use std::sync::atomic::{AtomicU32, Ordering};

use egui::style::ScrollStyle;
use egui::{Color32, Context, FontFamily, FontId, Stroke, Vec2};

pub const BG: Color32 = Color32::from_rgb(0x0b, 0x0b, 0x0c);
pub const FG: Color32 = Color32::from_rgb(0xf2, 0xf2, 0xf0);
pub const WHITE: Color32 = Color32::WHITE;
pub const ROW_BG: Color32 = Color32::from_rgb(0x10, 0x10, 0x12);
pub const RIGHT_BG: Color32 = Color32::from_rgb(0x0d, 0x0d, 0x0f);
pub const CARD_BG: Color32 = Color32::from_rgb(0x12, 0x12, 0x14);
pub const SUMMARY_BG: Color32 = Color32::from_rgb(0x15, 0x15, 0x17);
pub const GREEN: Color32 = Color32::from_rgb(0x8a, 0xff, 0xc1);
/// "Connected" green for light (inverted) surfaces, where the mint `GREEN` washes out.
pub const GREEN_ON_LIGHT: Color32 = Color32::from_rgb(0x17, 0x9c, 0x63);
pub const SNACK_ERROR: Color32 = Color32::from_rgb(0xff, 0x5c, 0x5c);
pub const SNACK_SUCCESS: Color32 = Color32::from_rgb(0x1f, 0xbf, 0x7a);
pub const RED_HOVER: Color32 = Color32::from_rgb(0xff, 0x7a, 0x7a);

/// `rgba(255,255,255,a)`
pub fn white(a: f32) -> Color32 {
    Color32::from_white_alpha((a * 255.0).round() as u8)
}

/// `rgba(0,0,0,a)`
pub fn black(a: f32) -> Color32 {
    Color32::from_black_alpha((a * 255.0).round() as u8)
}

/// CSS `opacity` approximation.
pub fn op(c: Color32, opacity: f32) -> Color32 {
    if opacity >= 1.0 {
        c
    } else {
        c.gamma_multiply(opacity)
    }
}

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(
        f(a.r(), b.r()),
        f(a.g(), b.g()),
        f(a.b(), b.b()),
        f(a.a(), b.a()),
    )
}

// ---------------------------------------------------------------------------
// Runtime text scale (adjusted from the debug panel; 1.0 in normal use)

static FONT_SCALE_BITS: AtomicU32 = AtomicU32::new(0x3f80_0000); // 1.0f32

/// Multiplier applied to every design font size.
pub fn font_scale() -> f32 {
    f32::from_bits(FONT_SCALE_BITS.load(Ordering::Relaxed))
}

#[cfg_attr(not(debug_assertions), allow(dead_code))]
pub fn set_font_scale(scale: f32) {
    FONT_SCALE_BITS.store(scale.clamp(0.5, 2.5).to_bits(), Ordering::Relaxed);
}

/// Proportional text — egui's default (Ubuntu-Light). The design's weight variants collapse onto
/// the single bundled weight.
pub fn sans(size: f32) -> FontId {
    FontId::new(size * font_scale(), FontFamily::Proportional)
}
pub fn sans_med(size: f32) -> FontId {
    sans(size)
}
pub fn sans_semi(size: f32) -> FontId {
    sans(size)
}
/// Monospace text — egui's default (Hack).
pub fn mono(size: f32) -> FontId {
    FontId::new(size * font_scale(), FontFamily::Monospace)
}
pub fn mono_semi(size: f32) -> FontId {
    mono(size)
}

pub fn apply_style(ctx: &Context) {
    ctx.set_theme(egui::Theme::Dark);
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = BG;
    visuals.extreme_bg_color = ROW_BG;
    visuals.override_text_color = Some(FG);
    visuals.selection.bg_fill = white(0.25);
    visuals.selection.stroke = Stroke::new(1.0, FG);
    visuals.text_cursor.stroke = Stroke::new(1.0, FG);
    // Panel separator lines.
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, white(0.08));
    // Scrollbar handle colours.
    visuals.widgets.inactive.bg_fill = white(0.12);
    visuals.widgets.hovered.bg_fill = white(0.2);
    visuals.widgets.active.bg_fill = white(0.25);
    ctx.all_styles_mut(|style| {
        style.visuals = visuals.clone();
        style.spacing.item_spacing = Vec2::ZERO;
        let mut scroll = ScrollStyle::thin();
        scroll.bar_width = 8.0;
        style.spacing.scroll = scroll;
        style.interaction.selectable_labels = false;
    });
}
