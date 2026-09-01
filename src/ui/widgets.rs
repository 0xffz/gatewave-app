//! Small gpui building blocks that reproduce the design's inline-styled elements.

use gpui::prelude::FluentBuilder;
use gpui::{
    App, ClickEvent, Div, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, Rgba, SharedString, Stateful, StatefulInteractiveElement, Styled, Window, div, px,
    relative,
};
use gpui_component::{Icon, IconName};

use crate::theme::*;

/// Group name rows advertise so children can restyle on row hover
/// (`.group_hover(ROW_GROUP, …)`).
pub const ROW_GROUP: &str = "row";

// ---------------------------------------------------------------------------
// Text builders: a div pre-styled with the design's face and size. Callers add
// `.text_color(…)` and `.child(…)`. Single-line by default; call
// `.whitespace_normal()` for wrapping paragraphs.

fn face(family: &'static str, weight: FontWeight, size: f32) -> Div {
    div()
        .font_family(family)
        .font_weight(weight)
        .text_size(px(size))
        .whitespace_nowrap()
}

/// Proportional text, regular weight.
pub fn sans(size: f32) -> Div {
    face(SANS_FAMILY, FontWeight::NORMAL, size)
}
pub fn sans_med(size: f32) -> Div {
    face(SANS_FAMILY, FontWeight::MEDIUM, size)
}
pub fn sans_semi(size: f32) -> Div {
    face(SANS_FAMILY, FontWeight::SEMIBOLD, size)
}
/// Monospace text, regular weight.
pub fn mono(size: f32) -> Div {
    face(MONO_FAMILY, FontWeight::NORMAL, size)
}
/// Monospace text, medium weight (the heaviest embedded Plex Mono face).
pub fn mono_semi(size: f32) -> Div {
    face(MONO_FAMILY, FontWeight::MEDIUM, size)
}

/// Section eyebrow ("STEPS", "BALANCES", "ACTIVE NUMBERS").
pub fn eyebrow(s: impl Into<SharedString>) -> Div {
    sans(SANS_EYEBROW).text_color(white(0.35)).child(s.into())
}

// ---------------------------------------------------------------------------
// Paint primitives

pub fn dot(d: f32, color: Rgba) -> Div {
    div().flex_none().size(px(d)).rounded_full().bg(color)
}

/// Small rounded box with centered text (country code, service letter, step number).
/// The text child comes from a text builder, e.g. `badge_frame(…).child(mono(MONO_XS)…)`.
pub fn badge_frame(w: f32, h: f32, radius: f32, fill: Rgba, border: Rgba) -> Div {
    div()
        .flex_none()
        .w(px(w))
        .h(px(h))
        .rounded(px(radius))
        .bg(fill)
        .border_1()
        .border_color(border)
        .flex()
        .items_center()
        .justify_center()
}

pub fn hline(color: Rgba) -> Div {
    div().w_full().h(px(1.0)).bg(color)
}

/// Dashed empty-state box with centered lines of text.
pub fn dashed_box(lines: &[&str]) -> Div {
    div()
        .w_full()
        .py(px(28.0))
        .border_1()
        .border_dashed()
        .border_color(white(0.14))
        .flex()
        .flex_col()
        .items_center()
        .children(lines.iter().map(|line| {
            sans(SANS_LABEL)
                .text_color(white(0.35))
                .child(SharedString::from(line.to_string()))
        }))
}

/// 3 px track with a light fill from the left.
pub fn progress_bar(frac: f32) -> Div {
    div()
        .w_full()
        .h(px(3.0))
        .rounded(px(2.0))
        .bg(white(0.09))
        .child(
            div()
                .h_full()
                .rounded(px(2.0))
                .bg(FG)
                .w(relative(frac.clamp(0.0, 1.0))),
        )
}

/// Loading placeholder block (gpui-component's skeleton pulses on its own).
pub fn skeleton(w: f32, h: f32, radius: f32) -> impl IntoElement {
    gpui_component::skeleton::Skeleton::new()
        .w(px(w))
        .h(px(h))
        .rounded(px(radius))
}

// ---------------------------------------------------------------------------
// Hover color helpers

/// Perceived lightness (0 = black, 1 = white); fully transparent colours count as dark
/// (they sit on the dark app background).
fn lightness(c: Rgba) -> f32 {
    if c.a == 0.0 {
        return 0.0;
    }
    0.299 * c.r + 0.587 * c.g + 0.114 * c.b
}

/// Default hover emphasis: light colours move towards white, dark ones towards black.
pub fn emphasize(c: Rgba, amount: f32) -> Rgba {
    if c.a == 0.0 {
        return c;
    }
    let target = if lightness(c) >= 0.5 {
        WHITE
    } else {
        black(1.0)
    };
    lerp(c, target, amount)
}

/// Faint surface highlight suited to the text colour drawn on it.
fn hover_wash(fg: Rgba) -> Rgba {
    if lightness(fg) >= 0.5 {
        white(0.06)
    } else {
        black(0.06)
    }
}

// ---------------------------------------------------------------------------
// Buttons

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// The design's text button: quiet colours that brighten on hover.
#[derive(IntoElement)]
pub struct Btn {
    id: ElementId,
    text: SharedString,
    family: &'static str,
    weight: FontWeight,
    size: f32,
    fg: Rgba,
    bg: Rgba,
    border: Rgba,
    hover_fg: Option<Rgba>,
    hover_bg: Option<Rgba>,
    hover_border: Option<Rgba>,
    pad: (f32, f32),
    radius: f32,
    disabled: bool,
    min_h: f32,
    full_width: bool,
    align_left: bool,
    opacity: f32,
    on_click: Option<ClickHandler>,
}

impl Btn {
    pub fn new(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            family: SANS_FAMILY,
            weight: FontWeight::MEDIUM,
            size: SANS_BODY_LG,
            fg: FG,
            bg: TRANSPARENT,
            border: TRANSPARENT,
            hover_fg: None,
            hover_bg: None,
            hover_border: None,
            pad: (12.0, 8.0),
            radius: 7.0,
            disabled: false,
            min_h: 0.0,
            full_width: false,
            align_left: false,
            opacity: 1.0,
            on_click: None,
        }
    }

    /// Light filled button (`background:#f2f2f0;color:#0b0b0c`, hover `#fff`).
    pub fn primary(id: impl Into<ElementId>, text: impl Into<SharedString>, size: f32) -> Self {
        Self::new(id, text)
            .font(SANS_FAMILY, FontWeight::SEMIBOLD, size)
            .fg(BG)
            .bg(FG)
            .hover_bg(WHITE)
    }

    pub fn font(mut self, family: &'static str, weight: FontWeight, size: f32) -> Self {
        self.family = family;
        self.weight = weight;
        self.size = size;
        self
    }
    pub fn sans(self, size: f32) -> Self {
        self.font(SANS_FAMILY, FontWeight::NORMAL, size)
    }
    pub fn sans_med(self, size: f32) -> Self {
        self.font(SANS_FAMILY, FontWeight::MEDIUM, size)
    }
    pub fn mono(self, size: f32) -> Self {
        self.font(MONO_FAMILY, FontWeight::NORMAL, size)
    }
    pub fn fg(mut self, c: Rgba) -> Self {
        self.fg = c;
        self
    }
    pub fn bg(mut self, c: Rgba) -> Self {
        self.bg = c;
        self
    }
    pub fn border(mut self, c: Rgba) -> Self {
        self.border = c;
        self
    }
    pub fn hover_fg(mut self, c: Rgba) -> Self {
        self.hover_fg = Some(c);
        self
    }
    pub fn hover_bg(mut self, c: Rgba) -> Self {
        self.hover_bg = Some(c);
        self
    }
    pub fn hover_border(mut self, c: Rgba) -> Self {
        self.hover_border = Some(c);
        self
    }
    pub fn pad(mut self, x: f32, y: f32) -> Self {
        self.pad = (x, y);
        self
    }
    pub fn radius(mut self, r: f32) -> Self {
        self.radius = r;
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
    pub fn min_height(mut self, h: f32) -> Self {
        self.min_h = h;
        self
    }
    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }
    pub fn align_left(mut self) -> Self {
        self.align_left = true;
        self
    }
    pub fn opacity(mut self, o: f32) -> Self {
        self.opacity = o;
        self
    }
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }
}

impl RenderOnce for Btn {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let hover_fg = self.hover_fg.unwrap_or_else(|| emphasize(self.fg, 0.6));
        let hover_bg = self.hover_bg.unwrap_or_else(|| {
            if self.bg.a == 0.0 {
                hover_wash(self.fg)
            } else {
                emphasize(self.bg, 0.5)
            }
        });
        let hover_border = self.hover_border.unwrap_or_else(|| {
            if self.border.a == 0.0 {
                TRANSPARENT
            } else {
                lerp(self.border, self.fg, 0.5)
            }
        });
        let o = self.opacity;
        div()
            .id(self.id)
            .flex()
            .flex_none()
            .items_center()
            .when(self.align_left, |d| d.justify_start())
            .when(!self.align_left, |d| d.justify_center())
            .when(self.full_width, |d| d.w_full())
            .px(px(self.pad.0))
            .py(px(self.pad.1))
            .when(self.min_h > 0.0, |d| d.min_h(px(self.min_h)))
            .rounded(px(self.radius))
            .bg(op(self.bg, o))
            .border_1()
            .border_color(op(self.border, o))
            .font_family(self.family)
            .font_weight(self.weight)
            .text_size(px(self.size))
            .text_color(op(self.fg, o))
            .whitespace_nowrap()
            .child(self.text)
            .when(!self.disabled, |d| {
                d.cursor_pointer()
                    .hover(|s| {
                        s.bg(op(hover_bg, o))
                            .border_color(op(hover_border, o))
                            .text_color(op(hover_fg, o))
                    })
                    .active(|s| {
                        s.bg(op(lerp(hover_bg, self.bg, 0.5), o))
                            .text_color(op(lerp(hover_fg, self.fg, 0.5), o))
                    })
            })
            .when_some(self.on_click.filter(|_| !self.disabled), |d, f| {
                d.on_click(move |ev, window, cx| {
                    cx.stop_propagation();
                    f(ev, window, cx)
                })
            })
    }
}

// ---------------------------------------------------------------------------
// Icon buttons

/// Small 26×20 chip with a lucide icon (copy / check / close / star).
#[derive(IntoElement)]
pub struct IconBtn {
    id: ElementId,
    icon: Icon,
    fg: Rgba,
    border: Rgba,
    hover_fg: Option<Rgba>,
    hover_border: Option<Rgba>,
    opacity: f32,
    tooltip: Option<SharedString>,
    height: f32,
    on_click: Option<ClickHandler>,
}

impl IconBtn {
    pub fn new(id: impl Into<ElementId>, icon: impl Into<Icon>) -> Self {
        Self {
            id: id.into(),
            icon: icon.into(),
            fg: FG,
            border: TRANSPARENT,
            hover_fg: None,
            hover_border: None,
            opacity: 1.0,
            tooltip: None,
            height: 20.0,
            on_click: None,
        }
    }
    pub fn fg(mut self, c: Rgba) -> Self {
        self.fg = c;
        self
    }
    pub fn border(mut self, c: Rgba) -> Self {
        self.border = c;
        self
    }
    pub fn hover_fg(mut self, c: Rgba) -> Self {
        self.hover_fg = Some(c);
        self
    }
    pub fn hover_border(mut self, c: Rgba) -> Self {
        self.hover_border = Some(c);
        self
    }
    pub fn opacity(mut self, o: f32) -> Self {
        self.opacity = o;
        self
    }
    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = Some(text.into());
        self
    }
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }
}

impl RenderOnce for IconBtn {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let hover_fg = self.hover_fg.unwrap_or_else(|| emphasize(self.fg, 0.6));
        let hover_border = self.hover_border.unwrap_or_else(|| {
            if self.border.a == 0.0 {
                TRANSPARENT
            } else {
                lerp(self.border, self.fg, 0.5)
            }
        });
        let o = self.opacity;
        let wash = hover_wash(self.fg);
        div()
            .id(self.id)
            .flex_none()
            .h(px(self.height))
            .flex()
            .items_center()
            .child(
                div()
                    .w(px(26.0))
                    .h(px(20.0))
                    .rounded(px(5.0))
                    .border_1()
                    .border_color(op(self.border, o))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(op(self.fg, o))
                    .group_hover(ICON_BTN_GROUP, |s| {
                        s.bg(op(wash, o))
                            .border_color(op(hover_border, o))
                            .text_color(op(hover_fg, o))
                    })
                    .child(self.icon.size(px(13.0))),
            )
            .group(ICON_BTN_GROUP)
            .cursor_pointer()
            .when_some(self.tooltip, |d, text| {
                d.tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(text.clone()).build(window, cx)
                })
            })
            .when_some(self.on_click, |d, f| {
                d.on_click(move |ev, window, cx| {
                    cx.stop_propagation();
                    f(ev, window, cx)
                })
            })
    }
}

/// Five-point star: filled (favorited, `assets/icons/star-filled.svg`) or lucide outline.
pub fn star_icon(filled: bool) -> Icon {
    if filled {
        Icon::default().path("icons/star-filled.svg")
    } else {
        Icon::new(IconName::Star)
    }
}

const ICON_BTN_GROUP: &str = "icon-btn";

// ---------------------------------------------------------------------------
// Clickable rows

#[derive(Clone)]
pub struct RowStyle {
    pub fill: Rgba,
    pub border: Rgba,
    pub radius: f32,
    pub pad_x: f32,
    pub gap: f32,
    pub opacity: f32,
    pub clickable: bool,
}

impl RowStyle {
    /// The design's `rowBase`: `#101012` fill, 10 % border, radius 9, padding 16, gap 12.
    pub fn base() -> Self {
        Self {
            fill: ROW_BG,
            border: white(0.1),
            radius: 9.0,
            pad_x: 16.0,
            gap: 12.0,
            opacity: 1.0,
            clickable: true,
        }
    }

    /// Inverted (selected) variant: light fill, dark text.
    pub fn selected() -> Self {
        Self {
            fill: FG,
            border: FG,
            ..Self::base()
        }
    }

    pub fn fill(mut self, c: Rgba) -> Self {
        self.fill = c;
        self
    }
    pub fn border(mut self, c: Rgba) -> Self {
        self.border = c;
        self
    }
    pub fn radius(mut self, r: f32) -> Self {
        self.radius = r;
        self
    }
    pub fn pad_x(mut self, x: f32) -> Self {
        self.pad_x = x;
        self
    }
    pub fn gap(mut self, g: f32) -> Self {
        self.gap = g;
        self
    }
    pub fn opacity(mut self, o: f32) -> Self {
        self.opacity = o;
        self
    }
    pub fn clickable(mut self, c: bool) -> Self {
        self.clickable = c;
        self
    }
}

/// A fixed-height row that is itself clickable but may contain buttons. Children can follow
/// the hover state with `.group_hover(ROW_GROUP, …)`. Attach the action with `.on_click(…)`.
pub fn row(id: impl Into<ElementId>, height: f32, style: &RowStyle) -> Stateful<Div> {
    let o = style.opacity;
    let (hover_fill, hover_border) = if lightness(style.fill) >= 0.5 {
        (lerp(style.fill, WHITE, 0.6), style.border)
    } else {
        let b = if style.border.a == 0.0 {
            style.border
        } else {
            white(0.28)
        };
        (lerp(style.fill, WHITE, 0.06), b)
    };
    div()
        .id(id)
        .group(ROW_GROUP)
        .w_full()
        .h(px(height))
        .px(px(style.pad_x))
        .rounded(px(style.radius))
        .bg(op(style.fill, o))
        .border_1()
        .border_color(op(style.border, o))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(style.gap))
        .when(style.clickable, |d| {
            d.cursor_pointer()
                .hover(move |s| s.bg(op(hover_fill, o)).border_color(op(hover_border, o)))
        })
}
