//! "New number" screen: 4-step wizard (provider → service → country → offer).

use egui::{
    Align, Color32, CornerRadius, Frame, Id, Layout, Margin, Rect, ScrollArea, Stroke, Ui, vec2,
};

use super::content_column;
use super::widgets::*;
use crate::app::{Action, App, Screen, SortDir};
use crate::model::*;
use crate::theme::*;

pub fn draw(ui: &mut Ui, app: &App, out: &mut Vec<Action>) {
    content_column(ui, |ui| {
        header(ui, app, out);

        let bar_h = if app.step == 4 && !app.loading_offers && app.offer.is_some() {
            Some(summary_bar_height(ui))
        } else {
            None
        };
        let scroll_h = (ui.available_height() - bar_h.unwrap_or(0.0)).max(0.0);
        ScrollArea::vertical()
            .id_salt(("wizard", app.step))
            .auto_shrink([false, false])
            .max_height(scroll_h)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                ui.set_width(ui.available_width());
                match app.step {
                    1 => step_providers(ui, app, out),
                    2 => step_services(ui, app, out),
                    3 => step_countries(ui, app, out),
                    _ => step_offers(ui, app, out),
                }
                ui.add_space(if bar_h.is_some() { 8.0 } else { 30.0 });
            });
        if bar_h.is_some() {
            summary_bar(ui, app, out);
        }
    });
}

fn header(ui: &mut Ui, app: &App, out: &mut Vec<Action>) {
    ui.add_space(30.0);
    text_ls(
        ui,
        format!("STEP {} / 4", app.step),
        mono(11.0),
        white(0.4),
        1.32,
    );
    ui.add_space(6.0);
    text_ls(ui, app.step_title(), sans_semi(22.0), FG, -0.22);
    ui.add_space(16.0);

    let font = sans(13.5);
    let input_h = input_height(ui, &font, 11.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
            if app.step == 3 && !app.loading_countries {
                let arrow = match app.sort_dir {
                    Some(SortDir::Desc) => "↓",
                    Some(SortDir::Asc) => "↑",
                    None => "↕",
                };
                let r = Btn::new(format!("Price {arrow}"), sans(11.5))
                    .fg(white(0.6))
                    .border(white(0.14))
                    .hover_fg(FG)
                    .hover_border(white(0.4))
                    .pad(14.0, 0.0)
                    .radius(8)
                    .min_height(input_h)
                    .show(ui);
                if r.clicked() {
                    out.push(Action::ToggleSort);
                }
            }
            let mut s = app.search.clone();
            let r = input(
                ui,
                Id::new("search"),
                &mut s,
                app.search_placeholder(),
                font.clone(),
                InputStyle {
                    bg: ROW_BG,
                    border: white(0.12),
                    focus_border: white(0.4),
                    radius: 8,
                    pad: vec2(14.0, 11.0),
                },
            );
            if r.changed() {
                out.push(Action::SetSearch(s));
            }
        });
    });
    ui.add_space(14.0 + 8.0);
}

fn skeleton_list(ui: &mut Ui, h: f32) {
    for _ in 0..8 {
        skeleton(ui, vec2(ui.available_width(), h), 8);
        ui.add_space(8.0);
    }
}

// ---------------------------------------------------------------------------
// Step 1

fn step_providers(ui: &mut Ui, app: &App, out: &mut Vec<Action>) {
    let btn_h = line_h(ui, &sans(11.5)) + 10.0;
    let row_h = 26.0 + line_h(ui, &sans_med(14.5)).max(btn_h);
    for p in app.provider_rows() {
        let selected = app.provider.as_deref() == Some(p.name.as_str());
        let fg = if selected { BG } else { FG };
        let opacity = if !p.connected && !selected { 0.55 } else { 1.0 };
        let style = if selected {
            RowStyle::selected()
        } else {
            RowStyle::base()
        }
        .opacity(opacity);
        let name = p.name.clone();
        let (resp, connect_clicked) =
            clickable_row(ui, ("provider", &p.name), row_h, &style, |ui| {
                dot(
                    ui,
                    7.0,
                    op(if p.connected { GREEN } else { white(0.2) }, opacity),
                );
                text(ui, &p.name, sans_med(14.5), op(fg, opacity));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if p.connected {
                        text(ui, fmt_usd(p.balance), mono(12.5), op(fg, 0.75));
                        false
                    } else {
                        Btn::new("Connect →", sans(11.5))
                            .fg(white(0.6))
                            .border(white(0.2))
                            .hover_fg(FG)
                            .hover_border(white(0.5))
                            .pad(10.0, 5.0)
                            .radius(6)
                            .opacity(opacity)
                            .show(ui)
                            .clicked()
                    }
                })
                .inner
            });
        if connect_clicked {
            out.push(Action::GoScreen(Screen::Settings));
        } else if resp.clicked() {
            out.push(Action::PickProvider(name));
        }
        ui.add_space(8.0);
    }
}

// ---------------------------------------------------------------------------
// Step 2

fn step_services(ui: &mut Ui, app: &App, out: &mut Vec<Action>) {
    let gap = 8.0;
    let tile_w = (ui.available_width() - gap) / 2.0;
    if app.loading_services {
        for _ in 0..4 {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(gap, 0.0);
                skeleton(ui, vec2(tile_w, 56.0), 8);
                skeleton(ui, vec2(tile_w, 56.0), 8);
            });
            ui.add_space(gap);
        }
        return;
    }
    let tile_h = 24.0 + 30.0;
    for pair in app.service_rows().chunks(2) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(gap, 0.0);
            for s in pair {
                let selected = app.service.as_deref() == Some(*s);
                let style = if selected {
                    RowStyle::selected()
                } else {
                    RowStyle::base()
                }
                .pad(14.0, 12.0)
                .width(tile_w);
                let (resp, _) = clickable_row(ui, ("service", s), tile_h, &style, |ui| {
                    let (fill, fg) = if selected {
                        (BG, FG)
                    } else {
                        (white(0.08), FG)
                    };
                    badge(
                        ui,
                        &s[..1],
                        vec2(30.0, 30.0),
                        7,
                        fill,
                        Color32::TRANSPARENT,
                        mono_semi(13.0),
                        fg,
                    );
                    text(ui, *s, sans_med(14.0), if selected { BG } else { FG });
                });
                if resp.clicked() {
                    out.push(Action::PickService(s.to_string()));
                }
            }
        });
        ui.add_space(gap);
    }
}

// ---------------------------------------------------------------------------
// Step 3

fn step_countries(ui: &mut Ui, app: &App, out: &mut Vec<Action>) {
    if app.loading_countries {
        skeleton_list(ui, 50.0);
        return;
    }
    let row_h = 24.0 + line_h(ui, &sans_med(14.0)).max(24.0);
    for c in app.country_rows() {
        let selected = app.country.is_some_and(|sel| sel.code == c.code);
        let fg = if selected { BG } else { FG };
        let style = if selected {
            RowStyle::selected()
        } else {
            RowStyle::base()
        }
        .pad(14.0, 12.0);
        let (resp, _) = clickable_row(ui, ("country", c.code), row_h, &style, |ui| {
            let (fill, badge_fg) = if selected {
                (BG, FG)
            } else {
                (white(0.08), FG)
            };
            badge(
                ui,
                c.code,
                vec2(32.0, 24.0),
                5,
                fill,
                Color32::TRANSPARENT,
                mono_semi(11.0),
                badge_fg,
            );
            text(ui, c.name, sans_med(14.0), fg);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.allocate_ui_with_layout(
                    vec2(56.0, ui.available_height()),
                    Layout::right_to_left(Align::Center),
                    |ui| {
                        text(ui, fmt_usd(app.country_price(&c)), mono(13.0), fg);
                    },
                );
                text(
                    ui,
                    c.dial,
                    mono(12.0),
                    if selected { black(0.45) } else { white(0.45) },
                );
            });
        });
        if resp.clicked() {
            out.push(Action::PickCountry(c));
        }
        ui.add_space(8.0);
    }
}

// ---------------------------------------------------------------------------
// Step 4

fn step_offers(ui: &mut Ui, app: &App, out: &mut Vec<Action>) {
    if app.loading_offers {
        skeleton_list(ui, 44.0);
        return;
    }
    let (Some(provider), Some(service), Some(country)) = (&app.provider, &app.service, app.country)
    else {
        return;
    };
    let tier_h = 22.0 + line_h(ui, &mono_semi(13.5)).max(line_h(ui, &sans(14.0)) + 4.0);
    for g in app.offer_groups() {
        ui.horizontal(|ui| {
            ui.add_space(2.0);
            text(ui, &g.name, sans_semi(13.0), FG);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(2.0);
                text(
                    ui,
                    format!("{} available", fmt_thousands(g.total)),
                    mono(11.0),
                    white(0.4),
                );
            });
        });
        ui.add_space(8.0);
        for (k, t) in g.tiers.iter().enumerate() {
            let selected = app
                .offer
                .as_ref()
                .is_some_and(|o| o.operator == g.name && o.price == t.price);
            let fav = Favorite {
                provider: provider.clone(),
                service: service.clone(),
                country,
                operator: g.name.clone(),
                price: t.price,
            };
            let is_fav = app.is_fav(&fav);
            let style = if selected {
                RowStyle::selected()
            } else {
                RowStyle::base()
            }
            .pad(14.0, 11.0)
            .radius(8);
            let (resp, star_clicked) =
                clickable_row(ui, ("tier", &g.name, k), tier_h, &style, |ui| {
                    dot(ui, 6.0, if selected { BG } else { white(0.3) });
                    text(
                        ui,
                        fmt_usd4(t.price),
                        mono_semi(13.5),
                        if selected { BG } else { FG },
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing = vec2(12.0, 0.0);
                        let star = Btn::new(if is_fav { "★" } else { "☆" }, sans(14.0))
                            .fg(if selected { black(0.6) } else { white(0.5) })
                            .pad(4.0, 2.0)
                            .show(ui)
                            .clicked();
                        text(
                            ui,
                            format!("{} numbers", fmt_thousands(t.count)),
                            sans(11.5),
                            if selected { black(0.55) } else { white(0.45) },
                        );
                        star
                    })
                    .inner
                });
            if star_clicked {
                out.push(Action::ToggleFav(fav));
            } else if resp.clicked() {
                out.push(Action::PickOffer(Offer {
                    operator: g.name.clone(),
                    price: t.price,
                }));
            }
            ui.add_space(8.0);
        }
        ui.add_space(12.0);
    }
}

fn summary_bar_height(ui: &Ui) -> f32 {
    let text_block = line_h(ui, &sans_med(13.5)) + 2.0 + line_h(ui, &sans(11.5));
    let button = line_h(ui, &sans_semi(13.5)) + 22.0;
    24.0 + text_block.max(button) + 28.0 + 2.0 + 4.0
}

fn summary_bar(ui: &mut Ui, app: &App, out: &mut Vec<Action>) {
    let Some((line, via, price)) = app.summary() else {
        return;
    };
    // Fade the list out above the bar (CSS `linear-gradient(transparent, #0b0b0c 30%)`).
    let fade_rect = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), 24.0));
    vgradient(ui.painter(), fade_rect, Color32::TRANSPARENT, BG);
    ui.add_space(24.0);
    Frame::new()
        .fill(SUMMARY_BG)
        .stroke(Stroke::new(1.0, white(0.1)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin {
            left: 16,
            right: 16,
            top: 14,
            bottom: 14,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(14.0, 0.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing = vec2(14.0, 0.0);
                    let r = Btn::primary("Request number", 13.5)
                        .pad(18.0, 11.0)
                        .radius(8)
                        .show(ui);
                    if r.clicked() {
                        out.push(Action::RequestNumber);
                    }
                    text(ui, price, mono_semi(15.0), FG);
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        let w = ui.available_width();
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
                            text_trunc(ui, line, sans_med(13.5), FG, w);
                            text(ui, via, sans(11.5), white(0.45));
                        });
                    });
                });
            });
        });
    ui.add_space(4.0);
}
