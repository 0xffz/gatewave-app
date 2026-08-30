//! Small custom widgets that reproduce the design's inline-styled elements.

use std::f32::consts::TAU;
use std::hash::Hash;

use egui::{
    Align, Align2, Color32, CornerRadius, CursorIcon, FontId, Frame, Id, Label, Layout, Margin,
    Mesh, Painter, Rect, Response, RichText, Sense, Shape, Stroke, StrokeKind, TextEdit,
    TextWrapMode, Ui, UiBuilder, Vec2, pos2, vec2,
};

use crate::theme::*;

pub fn line_h(ui: &Ui, font: &FontId) -> f32 {
    ui.fonts_mut(|f| f.row_height(font))
}

pub fn rich(s: impl Into<String>, font: FontId, color: Color32) -> RichText {
    RichText::new(s).font(font).color(color)
}

/// Single-line, non-wrapping, non-selectable text.
pub fn text(ui: &mut Ui, s: impl Into<String>, font: FontId, color: Color32) -> Response {
    ui.add(
        Label::new(rich(s, font, color))
            .selectable(false)
            .wrap_mode(TextWrapMode::Extend),
    )
}

/// Text with CSS-like letter spacing (in px).
pub fn text_ls(
    ui: &mut Ui,
    s: impl Into<String>,
    font: FontId,
    color: Color32,
    spacing: f32,
) -> Response {
    ui.add(
        Label::new(rich(s, font, color).extra_letter_spacing(spacing))
            .selectable(false)
            .wrap_mode(TextWrapMode::Extend),
    )
}

/// Text truncated with an ellipsis at `max_w`.
pub fn text_trunc(
    ui: &mut Ui,
    s: impl Into<String>,
    font: FontId,
    color: Color32,
    max_w: f32,
) -> Response {
    let h = line_h(ui, &font);
    ui.allocate_ui_with_layout(vec2(max_w, h), Layout::left_to_right(Align::Center), |ui| {
        ui.set_max_width(max_w);
        ui.add(
            Label::new(rich(s, font, color))
                .selectable(false)
                .truncate(),
        )
    })
    .inner
}

/// Wrapping paragraph text.
pub fn text_wrap(ui: &mut Ui, s: impl Into<String>, font: FontId, color: Color32) -> Response {
    ui.add(Label::new(rich(s, font, color)).selectable(false).wrap())
}

pub fn dot(ui: &mut Ui, d: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(d, d), Sense::hover());
    ui.painter().circle_filled(rect.center(), d / 2.0, color);
}

/// Small rounded box with centered text (country code, service letter, step number).
#[allow(clippy::too_many_arguments)]
pub fn badge(
    ui: &mut Ui,
    s: &str,
    size: Vec2,
    radius: u8,
    fill: Color32,
    border: Color32,
    font: FontId,
    fg: Color32,
) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect(
        rect,
        CornerRadius::same(radius),
        fill,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );
    p.text(rect.center(), Align2::CENTER_CENTER, s, font, fg);
}

/// Loading placeholder that breathes between 5 % and 10 % white.
pub fn skeleton(ui: &mut Ui, size: Vec2, radius: u8) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let t = ui.input(|i| i.time) as f32;
    let a = 0.05 + 0.05 * (0.5 + 0.5 * (t * TAU / 1.4).sin());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(radius), white(a));
}

/// 36×20 switch. Purely visual — the enclosing row handles the click.
pub fn toggle_visual(ui: &mut Ui, on: bool, id: Id) {
    let (rect, _) = ui.allocate_exact_size(vec2(36.0, 20.0), Sense::hover());
    let t = ui.ctx().animate_bool_with_time(id, on, 0.15);
    let track = lerp(white(0.14), FG, t);
    let knob = lerp(white(0.6), BG, t);
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(10), track);
    let cx = rect.left() + 2.0 + 8.0 + 16.0 * t;
    p.circle_filled(pos2(cx, rect.center().y), 8.0, knob);
}

pub fn progress_bar(ui: &mut Ui, frac: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 3.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(2), white(0.09));
    let w = rect.width() * frac.clamp(0.0, 1.0);
    if w > 0.0 {
        p.rect_filled(
            Rect::from_min_size(rect.min, vec2(w, rect.height())),
            CornerRadius::same(2),
            FG,
        );
    }
}

pub fn hline(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter()
        .hline(rect.x_range(), rect.center().y, Stroke::new(1.0, color));
}

/// Dashed empty-state box with centered lines of text.
pub fn dashed_box(ui: &mut Ui, lines: &[&str]) {
    let font = sans(12.5);
    let lh = line_h(ui, &font);
    let h = 28.0 * 2.0 + lh * lines.len() as f32;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    let p = ui.painter();
    let stroke = Stroke::new(1.0, white(0.14));
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ];
    for i in 0..4 {
        p.extend(Shape::dashed_line(
            &[corners[i], corners[(i + 1) % 4]],
            stroke,
            4.0,
            3.0,
        ));
    }
    let mut y = rect.top() + 28.0;
    for line in lines {
        p.text(
            pos2(rect.center().x, y),
            Align2::CENTER_TOP,
            line,
            font.clone(),
            white(0.35),
        );
        y += lh;
    }
}

pub fn vgradient(painter: &Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    painter.add(Shape::mesh(mesh));
}

// ---------------------------------------------------------------------------
// Hover helpers

/// Hover transition length, seconds.
pub const HOVER_ANIM: f32 = 0.12;

/// Perceived lightness (0 = black, 1 = white) of a premultiplied colour; fully transparent
/// colours count as dark (they sit on the dark app background).
fn lightness(c: Color32) -> f32 {
    let [r, g, b, a] = c.to_array();
    if a == 0 {
        return 0.0;
    }
    (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) / a as f32
}

/// Default hover emphasis: light colours move towards white, dark ones towards black.
pub fn emphasize(c: Color32, amount: f32) -> Color32 {
    if c.a() == 0 {
        return c;
    }
    let target = if lightness(c) >= 0.5 {
        Color32::WHITE
    } else {
        Color32::BLACK
    };
    lerp(c, target, amount)
}

/// Faint surface highlight suited to the text colour drawn on it.
fn hover_wash(fg: Color32) -> Color32 {
    if lightness(fg) >= 0.5 {
        white(0.06)
    } else {
        black(0.06)
    }
}

// ---------------------------------------------------------------------------
// Buttons

pub struct Btn {
    text: String,
    font: FontId,
    fg: Color32,
    bg: Color32,
    border: Color32,
    hover_fg: Option<Color32>,
    hover_bg: Option<Color32>,
    hover_border: Option<Color32>,
    pad: Vec2,
    radius: u8,
    disabled: bool,
    min_h: f32,
    width: Option<f32>,
    align: Align,
    opacity: f32,
}

impl Btn {
    pub fn new(text: impl Into<String>, font: FontId) -> Self {
        Self {
            text: text.into(),
            font,
            fg: FG,
            bg: Color32::TRANSPARENT,
            border: Color32::TRANSPARENT,
            hover_fg: None,
            hover_bg: None,
            hover_border: None,
            pad: vec2(12.0, 8.0),
            radius: 7,
            disabled: false,
            min_h: 0.0,
            width: None,
            align: Align::Center,
            opacity: 1.0,
        }
    }

    /// Light filled button (`background:#f2f2f0;color:#0b0b0c`, hover `#fff`).
    pub fn primary(text: impl Into<String>, size: f32) -> Self {
        Self::new(text, sans_semi(size))
            .fg(BG)
            .bg(FG)
            .hover_bg(WHITE)
    }

    pub fn fg(mut self, c: Color32) -> Self {
        self.fg = c;
        self
    }
    pub fn bg(mut self, c: Color32) -> Self {
        self.bg = c;
        self
    }
    pub fn border(mut self, c: Color32) -> Self {
        self.border = c;
        self
    }
    pub fn hover_fg(mut self, c: Color32) -> Self {
        self.hover_fg = Some(c);
        self
    }
    pub fn hover_bg(mut self, c: Color32) -> Self {
        self.hover_bg = Some(c);
        self
    }
    pub fn hover_border(mut self, c: Color32) -> Self {
        self.hover_border = Some(c);
        self
    }
    pub fn pad(mut self, x: f32, y: f32) -> Self {
        self.pad = vec2(x, y);
        self
    }
    pub fn radius(mut self, r: u8) -> Self {
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
        self.width = Some(f32::INFINITY);
        self
    }
    pub fn align_left(mut self) -> Self {
        self.align = Align::Min;
        self
    }
    pub fn opacity(mut self, o: f32) -> Self {
        self.opacity = o;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let galley =
            ui.painter()
                .layout_no_wrap(self.text.clone(), self.font.clone(), Color32::PLACEHOLDER);
        let w = match self.width {
            Some(w) if w.is_infinite() => ui.available_width(),
            Some(w) => w,
            None => galley.size().x + 2.0 * self.pad.x,
        };
        let h = (galley.size().y + 2.0 * self.pad.y).max(self.min_h);
        let sense = if self.disabled {
            Sense::hover()
        } else {
            Sense::click()
        };
        let (rect, mut resp) = ui.allocate_exact_size(vec2(w, h), sense);
        let hovered = !self.disabled && resp.hovered();
        // Hover state fades in/out; explicit hover colours win, otherwise text brightens, a
        // transparent background gets a faint wash and borders move towards the text colour.
        let t = if self.disabled {
            0.0
        } else {
            ui.ctx()
                .animate_bool_with_time(resp.id, hovered, HOVER_ANIM)
        };
        let hover_fg = self.hover_fg.unwrap_or_else(|| emphasize(self.fg, 0.6));
        let hover_bg = self.hover_bg.unwrap_or_else(|| {
            if self.bg.a() == 0 {
                hover_wash(self.fg)
            } else {
                emphasize(self.bg, 0.5)
            }
        });
        let hover_border = self.hover_border.unwrap_or_else(|| {
            if self.border.a() == 0 {
                Color32::TRANSPARENT
            } else {
                lerp(self.border, self.fg, 0.5)
            }
        });
        let mut fg = lerp(self.fg, hover_fg, t);
        let mut bg = lerp(self.bg, hover_bg, t);
        let border = lerp(self.border, hover_border, t);
        if hovered && resp.is_pointer_button_down_on() {
            // Press: dip halfway back towards the resting colours.
            fg = lerp(fg, self.fg, 0.5);
            bg = lerp(bg, self.bg, 0.5);
        }
        if ui.is_rect_visible(rect) {
            let p = ui.painter();
            let stroke = if border.a() == 0 {
                Stroke::NONE
            } else {
                Stroke::new(1.0, op(border, self.opacity))
            };
            p.rect(
                rect,
                CornerRadius::same(self.radius),
                op(bg, self.opacity),
                stroke,
                StrokeKind::Inside,
            );
            let pos = match self.align {
                Align::Min => pos2(
                    rect.left() + self.pad.x,
                    rect.center().y - galley.size().y / 2.0,
                ),
                _ => rect.center() - galley.size() / 2.0,
            };
            p.galley(pos, galley, op(fg, self.opacity));
        }
        if !self.disabled {
            resp = resp.on_hover_cursor(CursorIcon::PointingHand);
        }
        resp
    }
}

// ---------------------------------------------------------------------------
// Icon buttons

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    /// Two overlapping squares.
    Copy,
    /// A check mark (shown while a copy is fresh).
    Check,
    /// A small ×.
    Close,
}

/// Small bordered button showing a painted icon (same footprint as the design's `Copy` chip).
pub struct IconBtn {
    icon: Icon,
    fg: Color32,
    border: Color32,
    hover_fg: Option<Color32>,
    hover_border: Option<Color32>,
    opacity: f32,
    tooltip: Option<String>,
}

impl IconBtn {
    pub fn new(icon: Icon) -> Self {
        Self {
            icon,
            fg: FG,
            border: Color32::TRANSPARENT,
            hover_fg: None,
            hover_border: None,
            opacity: 1.0,
            tooltip: None,
        }
    }
    pub fn fg(mut self, c: Color32) -> Self {
        self.fg = c;
        self
    }
    pub fn border(mut self, c: Color32) -> Self {
        self.border = c;
        self
    }
    pub fn hover_fg(mut self, c: Color32) -> Self {
        self.hover_fg = Some(c);
        self
    }
    pub fn hover_border(mut self, c: Color32) -> Self {
        self.hover_border = Some(c);
        self
    }
    pub fn opacity(mut self, o: f32) -> Self {
        self.opacity = o;
        self
    }
    pub fn tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 20.0), Sense::click());
        let hovered = resp.hovered();
        let t = ui
            .ctx()
            .animate_bool_with_time(resp.id, hovered, HOVER_ANIM);
        let hover_fg = self.hover_fg.unwrap_or_else(|| emphasize(self.fg, 0.6));
        let hover_border = self.hover_border.unwrap_or_else(|| {
            if self.border.a() == 0 {
                Color32::TRANSPARENT
            } else {
                lerp(self.border, self.fg, 0.5)
            }
        });
        let mut fg = op(lerp(self.fg, hover_fg, t), self.opacity);
        if hovered && resp.is_pointer_button_down_on() {
            fg = lerp(fg, op(self.fg, self.opacity), 0.5);
        }
        let border = lerp(self.border, hover_border, t);
        let wash = lerp(Color32::TRANSPARENT, hover_wash(self.fg), t);
        if ui.is_rect_visible(rect) {
            let p = ui.painter();
            if wash.a() > 0 {
                p.rect_filled(rect, CornerRadius::same(5), op(wash, self.opacity));
            }
            if border.a() != 0 {
                p.rect_stroke(
                    rect,
                    CornerRadius::same(5),
                    Stroke::new(1.0, op(border, self.opacity)),
                    StrokeKind::Inside,
                );
            }
            let c = rect.center();
            let stroke = Stroke::new(1.3, fg);
            match self.icon {
                Icon::Copy => {
                    // Front square, fully drawn.
                    let front = Rect::from_center_size(c + vec2(1.5, 1.5), vec2(7.5, 7.5));
                    p.rect_stroke(front, CornerRadius::same(1), stroke, StrokeKind::Middle);
                    // Back square: only its top and left edges, stopping short of the front one.
                    let back = Rect::from_center_size(c - vec2(1.5, 1.5), vec2(7.5, 7.5));
                    p.line_segment([back.left_bottom(), back.left_top()], stroke);
                    p.line_segment([back.left_top(), back.right_top()], stroke);
                }
                Icon::Check => {
                    let pts = [
                        c + vec2(-4.5, 0.0),
                        c + vec2(-1.5, 3.0),
                        c + vec2(4.5, -3.5),
                    ];
                    p.line_segment([pts[0], pts[1]], stroke);
                    p.line_segment([pts[1], pts[2]], stroke);
                }
                Icon::Close => {
                    p.line_segment([c + vec2(-3.5, -3.5), c + vec2(3.5, 3.5)], stroke);
                    p.line_segment([c + vec2(-3.5, 3.5), c + vec2(3.5, -3.5)], stroke);
                }
            }
        }
        let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
        match self.tooltip {
            Some(t) => resp.on_hover_text(t),
            None => resp,
        }
    }
}

// ---------------------------------------------------------------------------
// Clickable rows

pub struct RowStyle {
    pub fill: Color32,
    pub border: Color32,
    pub radius: u8,
    pub pad: Vec2,
    pub gap: f32,
    pub width: Option<f32>,
    pub opacity: f32,
    pub clickable: bool,
}

impl RowStyle {
    /// The design's `rowBase`: `#101012` fill, 10 % border, radius 9, padding 13/16, gap 12.
    pub fn base() -> Self {
        Self {
            fill: ROW_BG,
            border: white(0.1),
            radius: 9,
            pad: vec2(16.0, 13.0),
            gap: 12.0,
            width: None,
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

    pub fn pad(mut self, x: f32, y: f32) -> Self {
        self.pad = vec2(x, y);
        self
    }
    pub fn radius(mut self, r: u8) -> Self {
        self.radius = r;
        self
    }
    pub fn gap(mut self, g: f32) -> Self {
        self.gap = g;
        self
    }
    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }
    pub fn opacity(mut self, o: f32) -> Self {
        self.opacity = o;
        self
    }
    pub fn fill(mut self, c: Color32) -> Self {
        self.fill = c;
        self
    }
    pub fn border(mut self, c: Color32) -> Self {
        self.border = c;
        self
    }
    pub fn clickable(mut self, c: bool) -> Self {
        self.clickable = c;
        self
    }
}

/// A fixed-height row that is itself clickable but may contain buttons.
///
/// The row's interaction is registered *before* the children so inner buttons
/// win the click (egui gives ties to the last registered widget).
pub fn clickable_row<R>(
    ui: &mut Ui,
    salt: impl Hash + std::fmt::Debug,
    height: f32,
    style: &RowStyle,
    content: impl FnOnce(&mut Ui) -> R,
) -> (Response, R) {
    let w = style.width.unwrap_or(ui.available_width());
    let sense = if style.clickable {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, mut resp) = ui.allocate_exact_size(vec2(w, height), sense);
    let (mut fill, mut border) = (style.fill, style.border);
    if style.clickable {
        resp = resp.on_hover_cursor(CursorIcon::PointingHand);
        // Rows lift slightly on hover: dark rows get a faint wash and a brighter border, light
        // (selected) rows go a touch whiter.
        let t = ui
            .ctx()
            .animate_bool_with_time(resp.id, resp.hovered(), HOVER_ANIM);
        let (hover_fill, hover_border) = if lightness(style.fill) >= 0.5 {
            (lerp(style.fill, Color32::WHITE, 0.6), style.border)
        } else {
            let b = if style.border.a() == 0 {
                style.border
            } else {
                lerp(style.border, white(0.28), 1.0)
            };
            (lerp(style.fill, Color32::WHITE, 0.04), b)
        };
        fill = lerp(style.fill, hover_fill, t);
        border = lerp(style.border, hover_border, t);
    }
    if ui.is_rect_visible(rect) {
        let stroke = if border.a() == 0 {
            Stroke::NONE
        } else {
            Stroke::new(1.0, op(border, style.opacity))
        };
        ui.painter().rect(
            rect,
            CornerRadius::same(style.radius),
            op(fill, style.opacity),
            stroke,
            StrokeKind::Inside,
        );
    }
    let inner = Rect::from_min_max(rect.min + style.pad, rect.max - style.pad);
    let mut child = ui.new_child(
        UiBuilder::new()
            .id_salt(salt)
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.spacing_mut().item_spacing = vec2(style.gap, 0.0);
    let r = content(&mut child);
    (resp, r)
}

// ---------------------------------------------------------------------------
// Text input

pub struct InputStyle {
    pub bg: Color32,
    pub border: Color32,
    pub focus_border: Color32,
    pub radius: u8,
    pub pad: Vec2,
}

pub fn input(
    ui: &mut Ui,
    id: Id,
    text: &mut String,
    hint: &str,
    font: FontId,
    style: InputStyle,
) -> Response {
    let focused = ui.memory(|m| m.has_focus(id));
    let border = if focused {
        style.focus_border
    } else {
        style.border
    };
    Frame::new()
        .fill(style.bg)
        .stroke(Stroke::new(1.0, border))
        .corner_radius(CornerRadius::same(style.radius))
        .inner_margin(Margin {
            left: style.pad.x as i8,
            right: style.pad.x as i8,
            top: style.pad.y as i8,
            bottom: style.pad.y as i8,
        })
        .show(ui, |ui| {
            ui.add(
                TextEdit::singleline(text)
                    .id(id)
                    .frame(Frame::NONE)
                    .background_color(Color32::TRANSPARENT)
                    .hint_text(RichText::new(hint).font(font.clone()).color(white(0.3)))
                    .font(font)
                    .text_color(FG)
                    .desired_width(f32::INFINITY)
                    .margin(Margin::same(0))
                    .vertical_align(Align::Center),
            )
        })
        .inner
}

/// Height an `input()` with the given font and vertical padding will occupy.
pub fn input_height(ui: &Ui, font: &FontId, pad_y: f32) -> f32 {
    line_h(ui, font) + 2.0 * pad_y + 2.0
}
