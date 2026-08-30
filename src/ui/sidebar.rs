//! Left sidebar: logo, navigation, step tracker, balances.

use egui::{Align, Color32, CornerRadius, Frame, Layout, Margin, Ui, vec2};

use super::widgets::*;
use crate::app::{Action, App, Screen};
use crate::model::fmt_usd;
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
            .show(ui, |ui| text_ls(ui, "N/D", mono_semi(16.0), BG, 0.96));
        ui.add_space(8.0);
        text_ls(ui, "Number Desk", sans_med(13.0), FG, 0.26);
    });
    ui.add_space(22.0);

    // Navigation
    for (label, screen) in [
        ("New number", Screen::New),
        ("Favorites", Screen::Favorites),
        ("Settings", Screen::Settings),
    ] {
        let active = app.screen == screen;
        let r = Btn::new(label, sans_med(13.5))
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
        text_ls(ui, "STEPS", sans(10.5), white(0.35), 1.47);
    });
    ui.add_space(10.0);
    let row_h = 20.0 + 26.0;
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
                (FG, Color32::TRANSPARENT, BG, mono_semi(11.0))
            } else if has_value {
                (Color32::TRANSPARENT, white(0.35), FG, mono(11.0))
            } else {
                (Color32::TRANSPARENT, white(0.15), white(0.4), mono(11.0))
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
                    sans_med(13.0),
                    if s.reachable { FG } else { white(0.3) },
                );
                let (val, col) = match &s.value {
                    Some(v) => (v.clone(), white(0.6)),
                    None => ("—".to_string(), white(0.25)),
                };
                text_trunc(ui, val, mono(11.5), col, 150.0);
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
        let balances: Vec<_> = app.balances().collect();
        for p in balances.iter().rev() {
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                text(ui, &p.name, sans(12.5), white(0.6));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(6.0);
                    text(ui, fmt_usd(p.balance), mono(12.0), FG);
                });
            });
            ui.add_space(8.0);
        }
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            text_ls(ui, "BALANCES", sans(10.5), white(0.35), 1.47);
        });
        ui.add_space(14.0);
        hline(ui, white(0.08));
    });
}
