//! Three-column layout: sidebar · main content · active numbers, plus the snackbar overlay.

#[cfg(debug_assertions)]
pub mod debug;
pub mod widgets;

mod favorites;
mod numbers;
mod settings;
mod sidebar;
mod snack;
mod wizard;

use egui::{CentralPanel, Frame, Margin, Panel, Ui};

use crate::app::{Action, App, Screen};
use crate::theme::*;

pub const SIDEBAR_W: f32 = 264.0;
pub const NUMBERS_W: f32 = 372.0;

pub fn draw(ui: &mut Ui, app: &App) -> Vec<Action> {
    let mut out = Vec::new();

    Panel::left("sidebar")
        .exact_size(SIDEBAR_W)
        .resizable(false)
        .frame(Frame::new().fill(BG).inner_margin(Margin {
            left: 18,
            right: 18,
            top: 22,
            bottom: 22,
        }))
        .show(ui, |ui| sidebar::draw(ui, app, &mut out));

    Panel::right("numbers")
        .exact_size(NUMBERS_W)
        .resizable(false)
        .frame(Frame::new().fill(RIGHT_BG).inner_margin(Margin {
            left: 18,
            right: 18,
            top: 22,
            bottom: 22,
        }))
        .show(ui, |ui| numbers::draw(ui, app, &mut out));

    CentralPanel::default()
        .frame(Frame::new().fill(BG).inner_margin(Margin {
            left: 34,
            right: 34,
            top: 0,
            bottom: 0,
        }))
        .show(ui, |ui| match app.screen {
            Screen::New => wizard::draw(ui, app, &mut out),
            Screen::Favorites => favorites::draw(ui, app, &mut out),
            Screen::Settings => settings::draw(ui, app, &mut out),
        });

    snack::draw(ui.ctx(), app, &mut out);
    out
}

/// Runs `f` inside a top-down column filling the centre panel's width.
pub fn content_column(ui: &mut Ui, f: impl FnOnce(&mut egui::Ui)) {
    let w = ui.available_width();
    ui.allocate_ui_with_layout(
        egui::vec2(w, ui.available_height()),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_width(w);
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            f(ui);
        },
    );
}

/// "FAVORITES" / "SETTINGS" eyebrow + h1 + optional paragraph.
pub fn page_header(ui: &mut Ui, eyebrow: &str, title: &str, para: Option<&str>) {
    ui.add_space(30.0);
    widgets::text_ls(ui, eyebrow, mono(MONO_XS), white(0.4), 1.32);
    ui.add_space(6.0);
    widgets::text_ls(ui, title, sans_semi(SANS_TITLE), FG, -0.22);
    if let Some(p) = para {
        ui.add_space(6.0);
        widgets::text_wrap(ui, p, sans(SANS_BODY), white(0.45));
        ui.add_space(24.0);
    }
}
