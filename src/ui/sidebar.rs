//! Left sidebar: logo, navigation, step tracker, balances.

use egui::{Align, Color32, CornerRadius, Frame, Layout, Margin, Ui, vec2};

use super::widgets::*;
use crate::app::{Action, App, Screen};
use crate::domain::fmt_usd;
use crate::theme::*;

pub fn draw(ui: &mut Ui, app: &App, out: &mut Vec<Action>) {
    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;

    // Logo
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        Frame::new()
            .fill(FG)
            .corner_radius(CornerRadius::same(5))
            .inner_margin(Margin {
                left: 7,
                right: 7,
                top: 3,
                bottom: 3,
            })
            .show(ui, |ui| text_ls(ui, "N/D", mono_semi(MONO_LOGO), BG, 0.96));
        ui.add_space(8.0);
        text_ls(ui, "Number Desk", sans_med(SANS_BODY), FG, 0.26);
    });
    ui.add_space(22.0);

    // Navigation
    for (label, screen) in [
        ("New number", Screen::New),
        ("Favorites", Screen::Favorites),
        ("Settings", Screen::Settings),
    ] {
        let active = app.screen == screen;
        let r = Btn::new(label, sans_med(SANS_BODY_LG))
            .full_width()
            .align_left()
            .pad(12.0, 9.0)
            .radius(7)
            .fg(if active { BG } else { white(0.55) })
            .bg(if active { FG } else { Color32::TRANSPARENT })
            .show(ui);
        if r.clicked() {
            out.push(Action::GoScreen(screen));
        }
        ui.add_space(4.0);
    }
    ui.add_space(18.0);

    // Steps
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        text_ls(ui, "STEPS", sans(SANS_EYEBROW), white(0.35), 1.47);
    });
    ui.add_space(10.0);
    // Two text lines beside a 26 px badge; the row grows with the fonts and the text block is
    // nudged up a touch so the label sits level with the badge.
    const NUDGE_UP: f32 = 4.0;
    let block_h = line_h(ui, &sans_med(SANS_BODY)) + 2.0 + line_h(ui, &mono(MONO_SM));
    let row_h = 20.0 + (block_h + NUDGE_UP).max(26.0);
    for s in app.steps() {
        let style = RowStyle::base()
            .fill(if s.active {
                white(0.07)
            } else {
                Color32::TRANSPARENT
            })
            .border(Color32::TRANSPARENT)
            .radius(8)
            .pad(10.0, 10.0)
            .gap(11.0)
            .clickable(s.reachable);
        let has_value = s.value.is_some();
        let (resp, _) = clickable_row(ui, ("step", s.num), row_h, &style, |ui| {
            let (fill, border, fg, font) = if s.active {
                (FG, Color32::TRANSPARENT, BG, mono_semi(MONO_XS))
            } else if has_value {
                (Color32::TRANSPARENT, white(0.35), FG, mono(MONO_XS))
            } else {
                (Color32::TRANSPARENT, white(0.15), white(0.4), mono(MONO_XS))
            };
            badge(
                ui,
                &format!("0{}", s.num),
                vec2(26.0, 26.0),
                6,
                fill,
                border,
                font,
                fg,
            );
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
                text(
                    ui,
                    s.label,
                    sans_med(SANS_BODY),
                    if s.reachable { FG } else { white(0.3) },
                );
                let (val, col) = match &s.value {
                    Some(v) => (v.clone(), white(0.6)),
                    None => ("—".to_string(), white(0.25)),
                };
                text_trunc(ui, val, mono(MONO_SM), col, 150.0);
                ui.add_space(NUDGE_UP);
            });
        });
        if resp.clicked() && s.reachable {
            out.push(Action::GoStep(s.num));
        }
        ui.add_space(6.0);
    }

    // Balances, pinned to the bottom (CSS `margin-top:auto`). bottom_up adds in reverse order.
    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
        ui.add_space(2.0);
        let balances = app.balances();
        for (name, balance) in balances.iter().rev() {
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                text(ui, *name, sans(SANS_LABEL), white(0.6));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(6.0);
                    text(ui, fmt_usd(*balance), mono(MONO_MD), FG);
                });
            });
            ui.add_space(8.0);
        }
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            text_ls(ui, "BALANCES", sans(SANS_EYEBROW), white(0.35), 1.47);
        });
        ui.add_space(14.0);
        hline(ui, separator_color(ui));
    });
}
