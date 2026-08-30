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

/// Default text multiplier over the design sizes.
pub const DEFAULT_FONT_SCALE: f32 = 1.0;

static FONT_SCALE_BITS: AtomicU32 = AtomicU32::new(DEFAULT_FONT_SCALE.to_bits());

/// Multiplier applied to every text size (adjustable from the debug panel).
pub fn font_scale() -> f32 {
    f32::from_bits(FONT_SCALE_BITS.load(Ordering::Relaxed))
}

#[cfg_attr(not(debug_assertions), allow(dead_code))]
pub fn set_font_scale(scale: f32) {
    FONT_SCALE_BITS.store(scale.clamp(0.5, 2.5).to_bits(), Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// Text sizes. The design's CSS sizes read small in a native window, so every value is the
// design size + 2 px. Proportional (`SANS_*`) and monospace (`MONO_*`) scales are kept apart.

/// 10.5 — section eyebrows (STEPS, BALANCES, ACTIVE NUMBERS).
pub const SANS_EYEBROW: f32 = 12.5;
/// 11.5 — hints, meta lines, small buttons (Connect, Cancel).
pub const SANS_SMALL: f32 = 13.5;
/// 12 — secondary text (Disconnect, "Waiting for SMS", expired notes).
pub const SANS_CAPTION: f32 = 14.0;
/// 12.5 — balances, empty-state boxes, the Request button in favorites.
pub const SANS_LABEL: f32 = 14.5;
/// 13 — sidebar step labels, paragraphs, offer group names.
pub const SANS_BODY: f32 = 15.0;
/// 13.5 — navigation, inputs, preference labels, snackbar, primary buttons.
pub const SANS_BODY_LG: f32 = 15.5;
/// 14 — list rows (services, countries, favorites) and the offer star.
pub const SANS_ROW: f32 = 16.0;
/// 14.5 — provider names.
pub const SANS_ROW_LG: f32 = 16.5;
/// 22 — page titles.
pub const SANS_TITLE: f32 = 24.0;

/// 11 — badges, step numbers, "STEP 1 / 4", available counts.
pub const MONO_XS: f32 = 13.0;
/// 11.5 — step values, prices on number cards.
pub const MONO_SM: f32 = 13.5;
/// 12 — balances, dial codes, masked API keys, counters.
pub const MONO_MD: f32 = 14.0;
/// 12.5 — provider balance in rows, countdown, API key input.
pub const MONO_LG: f32 = 14.5;
/// 13 — prices, badge letters.
pub const MONO_XL: f32 = 15.0;
/// 13.5 — offer tier prices.
pub const MONO_PRICE: f32 = 15.5;
/// 14.5 — phone numbers.
pub const MONO_PHONE: f32 = 16.5;
/// 15 — the summary-bar total.
pub const MONO_TOTAL: f32 = 17.0;
/// 16 — the N/D logo.
pub const MONO_LOGO: f32 = 18.0;
/// 20 — the received code.
pub const MONO_CODE: f32 = 22.0;

/// The size a design constant renders at.
pub fn text_size(size: f32) -> f32 {
    (size * font_scale()).max(1.0)
}

/// Proportional text — egui's default (Ubuntu-Light). The design's weight variants collapse onto
/// the single bundled weight.
pub fn sans(size: f32) -> FontId {
    FontId::new(text_size(size), FontFamily::Proportional)
}
pub fn sans_med(size: f32) -> FontId {
    sans(size)
}
pub fn sans_semi(size: f32) -> FontId {
    sans(size)
}
/// Monospace text — egui's default (Hack).
pub fn mono(size: f32) -> FontId {
    FontId::new(text_size(size), FontFamily::Monospace)
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
