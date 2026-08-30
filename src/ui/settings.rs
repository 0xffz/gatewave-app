//! Settings screen: behaviour toggles and provider API keys.

use egui::{Align, Color32, CornerRadius, Frame, Id, Layout, Margin, ScrollArea, Stroke, Ui, vec2};

use super::widgets::*;
use super::{content_column, page_header};
use crate::app::{Action, App};
use crate::domain::{PREF_DEFS, fmt_usd, masked_key};
use crate::theme::*;

pub fn draw(ui: &mut Ui, app: &App, out: &mut Vec<Action>) {
    content_column(ui, |ui| {
        page_header(
            ui,
            "SETTINGS",
            "Providers",
            Some("Connect a provider with its API key to request numbers through it."),
        );
        ScrollArea::vertical()
            .id_salt("settings")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                ui.set_width(ui.available_width());
                behaviour(ui, app, out);
                ui.add_space(28.0);
                providers(ui, app, out);
                ui.add_space(30.0);
            });
    });
}

fn behaviour(ui: &mut Ui, app: &App, out: &mut Vec<Action>) {
    text_ls(ui, "BEHAVIOR", sans(SANS_EYEBROW), white(0.35), 1.47);
    ui.add_space(10.0);
    let row_h = 26.0 + line_h(ui, &sans_med(SANS_BODY_LG)) + 2.0 + line_h(ui, &sans(SANS_SMALL));
    Frame::new()
        .fill(ROW_BG)
        .stroke(Stroke::new(1.0, white(0.1)))
        .corner_radius(CornerRadius::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            let last = PREF_DEFS.len() - 1;
            for (i, (key, label, hint)) in PREF_DEFS.iter().enumerate() {
                let style = RowStyle::base()
                    .fill(Color32::TRANSPARENT)
                    .border(Color32::TRANSPARENT)
                    .radius(0)
                    .pad(16.0, 13.0)
                    .gap(14.0);
                let on = app.prefs.get(*key);
                let (resp, _) = clickable_row(ui, ("pref", i), row_h, &style, |ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
                        text(ui, *label, sans_med(SANS_BODY_LG), FG);
                        text(ui, *hint, sans(SANS_SMALL), white(0.45));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        toggle_visual(ui, on, Id::new(("toggle", i)));
                    });
                });
                if resp.clicked() {
                    out.push(Action::TogglePref(*key));
                }
                if i < last {
                    hline(ui, separator_color(ui));
                }
            }
        });
}

fn providers(ui: &mut Ui, app: &App, out: &mut Vec<Action>) {
    text_ls(ui, "PROVIDERS", sans(SANS_EYEBROW), white(0.35), 1.47);
    ui.add_space(10.0);
    let key_font = mono(MONO_LG);
    let input_h = input_height(ui, &key_font, 10.0);
    for p in &app.providers {
        let kind = p.kind;
        Frame::new()
            .fill(ROW_BG)
            .stroke(Stroke::new(1.0, white(0.1)))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin {
                left: 18,
                right: 18,
                top: 16,
                bottom: 16,
            })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(10.0, 0.0);
                    dot(ui, 7.0, if p.connected { GREEN } else { white(0.2) });
                    text(ui, p.name(), sans_semi(SANS_ROW_LG), FG);
                    if p.connected {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing = vec2(10.0, 0.0);
                            let r = Btn::new("Disconnect", sans(SANS_CAPTION))
                                .fg(white(0.4))
                                .hover_fg(RED_HOVER)
                                .pad(2.0, 2.0)
                                .show(ui);
                            if r.clicked() {
                                out.push(Action::Disconnect(kind));
                            }
                            if let Some(balance) = p.balance {
                                text(ui, fmt_usd(balance), mono(MONO_LG), op(FG, 0.75));
                            }
                        });
                    }
                });
                ui.add_space(12.0);
                if p.connected {
                    text(
                        ui,
                        format!("API key · {}", masked_key(p.key.as_deref().unwrap_or(""))),
                        mono(MONO_MD),
                        white(0.4),
                    );
                } else if p.connecting {
                    skeleton(ui, vec2(ui.available_width(), 38.0), 7);
                } else {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
                            let r = Btn::primary("Connect", 13.0)
                                .pad(16.0, 0.0)
                                .radius(7)
                                .min_height(input_h)
                                .show(ui);
                            if r.clicked() {
                                out.push(Action::Connect(kind));
                            }
                            let mut v = app.key_inputs.get(&kind).cloned().unwrap_or_default();
                            let r = input(
                                ui,
                                Id::new(("api-key", kind)),
                                &mut v,
                                kind.key_hint(),
                                key_font.clone(),
                                InputStyle {
                                    bg: BG,
                                    border: white(0.14),
                                    focus_border: white(0.4),
                                    radius: 7,
                                    pad: vec2(12.0, 10.0),
                                },
                            );
                            if r.changed() {
                                out.push(Action::SetKeyInput(kind, v));
                            }
                        });
                    });
                }
            });
        ui.add_space(10.0);
    }
}
