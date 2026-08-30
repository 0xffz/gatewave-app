//! Palette, typography and global egui style for the Number Desk mock.

use std::sync::Arc;

use egui::style::ScrollStyle;
use egui::{Color32, Context, FontData, FontDefinitions, FontFamily, FontId, Stroke, Vec2};

pub const BG: Color32 = Color32::from_rgb(0x0b, 0x0b, 0x0c);
pub const FG: Color32 = Color32::from_rgb(0xf2, 0xf2, 0xf0);
pub const WHITE: Color32 = Color32::WHITE;
pub const ROW_BG: Color32 = Color32::from_rgb(0x10, 0x10, 0x12);
pub const RIGHT_BG: Color32 = Color32::from_rgb(0x0d, 0x0d, 0x0f);
pub const CARD_BG: Color32 = Color32::from_rgb(0x12, 0x12, 0x14);
pub const SUMMARY_BG: Color32 = Color32::from_rgb(0x15, 0x15, 0x17);
pub const GREEN: Color32 = Color32::from_rgb(0x8a, 0xff, 0xc1);
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

const SANS: &str = "SpaceGrotesk-Regular";
const SANS_MED: &str = "SpaceGrotesk-Medium";
const SANS_SEMI: &str = "SpaceGrotesk-SemiBold";
const MONO: &str = "JetBrainsMono-Regular";
const MONO_MED: &str = "JetBrainsMono-Medium";
const MONO_SEMI: &str = "JetBrainsMono-SemiBold";

fn font(name: &str, size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(name.into()))
}

pub fn sans(size: f32) -> FontId {
    font(SANS, size)
}
pub fn sans_med(size: f32) -> FontId {
    font(SANS_MED, size)
}
pub fn sans_semi(size: f32) -> FontId {
    font(SANS_SEMI, size)
}
pub fn mono(size: f32) -> FontId {
    font(MONO, size)
}
#[allow(dead_code)]
pub fn mono_med(size: f32) -> FontId {
    font(MONO_MED, size)
}
pub fn mono_semi(size: f32) -> FontId {
    font(MONO_SEMI, size)
}

pub fn install_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    // Keep egui's bundled fonts as glyph fallbacks (arrows, stars, emoji).
    let fallbacks = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();

    let faces: [(&str, &'static [u8]); 6] = [
        (
            SANS,
            include_bytes!("../assets/fonts/SpaceGrotesk-Regular.ttf"),
        ),
        (
            SANS_MED,
            include_bytes!("../assets/fonts/SpaceGrotesk-Medium.ttf"),
        ),
        (
            SANS_SEMI,
            include_bytes!("../assets/fonts/SpaceGrotesk-SemiBold.ttf"),
        ),
        (
            MONO,
            include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
        ),
        (
            MONO_MED,
            include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf"),
        ),
        (
            MONO_SEMI,
            include_bytes!("../assets/fonts/JetBrainsMono-SemiBold.ttf"),
        ),
    ];
    for (name, bytes) in faces {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
        let mut family = vec![name.to_owned()];
        family.extend(fallbacks.iter().cloned());
        fonts.families.insert(FontFamily::Name(name.into()), family);
    }
    if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
        prop.insert(0, SANS.to_owned());
    }
    if let Some(m) = fonts.families.get_mut(&FontFamily::Monospace) {
        m.insert(0, MONO.to_owned());
    }
    ctx.set_fonts(fonts);
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
