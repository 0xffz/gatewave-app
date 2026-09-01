//! Palette, typography and gpui-component theme wiring for Gatewave.

use gpui::{App, Rgba};
use gpui_component::theme::{Theme, ThemeMode};

/// `#0b0b0c` — the app background.
pub const BG: Rgba = c(0x0b0b0c);
/// `#f2f2f0` — the main foreground.
pub const FG: Rgba = c(0xf2f2f0);
pub const WHITE: Rgba = c(0xffffff);
pub const ROW_BG: Rgba = c(0x101012);
pub const RIGHT_BG: Rgba = c(0x0d0d0f);
pub const CARD_BG: Rgba = c(0x121214);
pub const SUMMARY_BG: Rgba = c(0x151517);
pub const GREEN: Rgba = c(0x8affc1);
/// "Connected" green for light (inverted) surfaces, where the mint `GREEN` washes out.
pub const GREEN_ON_LIGHT: Rgba = c(0x179c63);
pub const SNACK_ERROR: Rgba = c(0xff5c5c);
pub const SNACK_SUCCESS: Rgba = c(0x1fbf7a);
pub const RED_HOVER: Rgba = c(0xff7a7a);
pub const TRANSPARENT: Rgba = Rgba {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.0,
};

/// `#rrggbb` at full opacity.
const fn c(hex: u32) -> Rgba {
    Rgba {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

/// `rgba(255,255,255,a)`
pub const fn white(a: f32) -> Rgba {
    Rgba {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a,
    }
}

/// `rgba(0,0,0,a)`
pub const fn black(a: f32) -> Rgba {
    Rgba {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a,
    }
}

/// CSS `opacity` approximation: scales the alpha channel.
pub fn op(c: Rgba, opacity: f32) -> Rgba {
    Rgba {
        a: c.a * opacity,
        ..c
    }
}

/// Component-wise interpolation in (non-premultiplied) sRGB.
pub fn lerp(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let t = t.clamp(0.0, 1.0);
    let l = |x: f32, y: f32| x + (y - x) * t;
    Rgba {
        r: l(a.r, b.r),
        g: l(a.g, b.g),
        b: l(a.b, b.b),
        a: l(a.a, b.a),
    }
}

// ---------------------------------------------------------------------------
// Text sizes, straight from the design's CSS. Proportional (`SANS_*`) and monospace (`MONO_*`)
// scales are kept apart.

/// section eyebrows (STEPS, BALANCES, ACTIVE NUMBERS).
pub const SANS_EYEBROW: f32 = 10.5;
/// hints, meta lines, small buttons (Connect, Cancel).
pub const SANS_SMALL: f32 = 11.5;
/// secondary text (Disconnect, "Waiting for SMS", expired notes).
pub const SANS_CAPTION: f32 = 12.0;
/// balances, empty-state boxes, the Request button in favorites.
pub const SANS_LABEL: f32 = 12.5;
/// sidebar step labels, paragraphs, offer group names.
pub const SANS_BODY: f32 = 13.0;
/// navigation, inputs, preference labels, snackbar, primary buttons.
pub const SANS_BODY_LG: f32 = 13.5;
/// list rows (services, countries, favorites) and the offer star.
pub const SANS_ROW: f32 = 14.0;
/// provider names.
pub const SANS_ROW_LG: f32 = 14.5;
/// page titles.
pub const SANS_TITLE: f32 = 22.0;

/// badges, step numbers, "STEP 1 / 4", available counts.
pub const MONO_XS: f32 = 11.0;
/// step values, prices on number cards.
pub const MONO_SM: f32 = 11.5;
/// balances, dial codes, masked API keys, counters.
pub const MONO_MD: f32 = 12.0;
/// provider balance in rows, countdown, API key input.
pub const MONO_LG: f32 = 12.5;
/// prices, badge letters.
pub const MONO_XL: f32 = 13.0;
/// offer tier prices.
pub const MONO_PRICE: f32 = 13.5;
/// phone numbers.
pub const MONO_PHONE: f32 = 14.5;
/// the summary-bar total.
pub const MONO_TOTAL: f32 = 15.0;
/// the GW logo.
pub const MONO_LOGO: f32 = 16.0;
/// the received code.
pub const MONO_CODE: f32 = 20.0;

// ---------------------------------------------------------------------------
// Fonts: IBM Plex Sans (regular / medium / semi-bold) and IBM Plex Mono (regular / medium), OFL.

pub const SANS_FAMILY: &str = "IBM Plex Sans";
pub const MONO_FAMILY: &str = "IBM Plex Mono";

/// Registers the embedded app faces with gpui's text system.
fn install_fonts(cx: &mut App) {
    let faces: Vec<std::borrow::Cow<'static, [u8]>> = vec![
        (include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf") as &[u8]).into(),
        (include_bytes!("../assets/fonts/IBMPlexSans-Medium.ttf") as &[u8]).into(),
        (include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf") as &[u8]).into(),
        (include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf") as &[u8]).into(),
        (include_bytes!("../assets/fonts/IBMPlexMono-Medium.ttf") as &[u8]).into(),
    ];
    cx.text_system()
        .add_fonts(faces)
        .expect("embedded IBM Plex faces load");
}

/// Fonts + dark mode + palette overrides so gpui-component widgets (inputs, switches,
/// skeletons, scrollbars) blend with the design.
pub fn init(cx: &mut App) {
    install_fonts(cx);
    Theme::change(ThemeMode::Dark, None, cx);
    let theme = Theme::global_mut(cx);
    theme.font_family = SANS_FAMILY.into();
    theme.mono_font_family = MONO_FAMILY.into();
    theme.background = BG.into();
    theme.foreground = FG.into();
    theme.border = white(0.1).into();
    theme.input = white(0.14).into();
    theme.ring = white(0.3).into();
    theme.caret = FG.into();
    theme.muted = ROW_BG.into();
    theme.muted_foreground = white(0.45).into();
    theme.primary = FG.into();
    theme.primary_hover = WHITE.into();
    theme.primary_active = FG.into();
    theme.primary_foreground = BG.into();
    theme.accent = white(0.06).into();
    theme.accent_foreground = FG.into();
    theme.secondary = ROW_BG.into();
    theme.secondary_foreground = FG.into();
    theme.popover = CARD_BG.into();
    theme.popover_foreground = FG.into();
    theme.danger = SNACK_ERROR.into();
    theme.success = SNACK_SUCCESS.into();
}
