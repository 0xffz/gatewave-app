//! "New number" screen: 4-step wizard (provider → service → country → offer).

use egui::{
    Align, Color32, CornerRadius, Frame, Id, Layout, Margin, Rect, ScrollArea, Stroke, Ui, vec2,
};

use super::content_column;
use super::widgets::*;
use crate::app::{Action, App, Screen, SortDir};
use crate::domain::{fmt_thousands, fmt_usd, fmt_usd4};
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
        mono(MONO_XS),
        white(0.4),
        1.32,
    );
    ui.add_space(6.0);
    text_ls(ui, app.step_title(), sans_semi(SANS_TITLE), FG, -0.22);
    ui.add_space(16.0);

    let font = sans(SANS_BODY_LG);
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
                let r = Btn::new(format!("Price {arrow}"), mono(MONO_SM))
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

/// Lists can run to a thousand rows: rows scrolled out of view only reserve their space.
fn row_visible(ui: &Ui, h: f32) -> bool {
    let rect = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), h));
    ui.is_rect_visible(rect)
}

fn empty_hint(ui: &mut Ui, lines: &[&str]) {
    dashed_box(ui, lines);
}

// ---------------------------------------------------------------------------
// Step 1

fn step_providers(ui: &mut Ui, app: &App, out: &mut Vec<Action>) {
    let btn_h = line_h(ui, &sans(SANS_SMALL)) + 10.0;
    let row_h = 26.0 + line_h(ui, &sans_med(SANS_ROW_LG)).max(btn_h);
    for p in app.provider_rows() {
        let kind = p.kind;
        let selected = app.provider == Some(kind);
        let fg = if selected { BG } else { FG };
        let opacity = if !p.connected && !selected { 0.55 } else { 1.0 };
        let style = if selected {
            RowStyle::selected()
        } else {
            RowStyle::base()
        }
        .opacity(opacity);
        let (resp, connect_clicked) = clickable_row(ui, ("provider", kind), row_h, &style, |ui| {
            // The mint green is tuned for dark rows; the selected row is light, so use the
            // darker "connected" green there.
            let dot_color = match (p.connected, selected) {
                (true, true) => GREEN_ON_LIGHT,
                (true, false) => GREEN,
                (false, _) => white(0.2),
            };
            dot(ui, 7.0, op(dot_color, opacity));
            text(ui, p.name(), sans_med(SANS_ROW_LG), op(fg, opacity));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if p.connected {
                    if let Some(balance) = p.balance {
                        text(ui, fmt_usd(balance), mono(MONO_LG), op(fg, 0.75));
                    }
                    false
                } else if p.connecting {
                    text(ui, "Connecting…", sans(SANS_SMALL), op(white(0.6), opacity));
                    false
                } else {
                    Btn::new("Connect ›", sans(SANS_SMALL))
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
            out.push(Action::PickProvider(kind));
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
    let rows = app.service_rows();
    if rows.is_empty() {
        empty_hint(ui, &["No services match.", "Try another search."]);
        return;
    }
    let tile_h = 24.0 + 30.0;
    for pair in rows.chunks(2) {
        if !row_visible(ui, tile_h) {
            ui.allocate_space(vec2(ui.available_width(), tile_h));
            ui.add_space(gap);
            continue;
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(gap, 0.0);
            for s in pair {
                let selected = app.service.as_ref().is_some_and(|sel| sel.code == s.code);
                let style = if selected {
                    RowStyle::selected()
                } else {
                    RowStyle::base()
                }
                .pad(14.0, 12.0)
                .width(tile_w);
                let initial: String = s
                    .name
                    .chars()
                    .next()
                    .map(|c| c.to_uppercase().collect())
                    .unwrap_or_default();
                let (resp, _) =
                    clickable_row(ui, ("service", s.code.as_str()), tile_h, &style, |ui| {
                        let (fill, fg) = if selected {
                            (BG, FG)
                        } else {
                            (white(0.08), FG)
                        };
                        badge(
                            ui,
                            &initial,
                            vec2(30.0, 30.0),
                            7,
                            fill,
                            Color32::TRANSPARENT,
                            mono_semi(MONO_XL),
                            fg,
                        );
                        let w = ui.available_width();
                        text_trunc(
                            ui,
                            &s.name,
                            sans_med(SANS_ROW),
                            if selected { BG } else { FG },
                            w,
                        );
                    });
                if resp.clicked() {
                    out.push(Action::PickService((*s).clone()));
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
    let rows = app.country_rows();
    if rows.is_empty() {
        empty_hint(
            ui,
            &["No countries for this service.", "Try another service."],
        );
        return;
    }
    let row_h = 24.0 + line_h(ui, &sans_med(SANS_ROW)).max(24.0);
    for c in rows {
        if !row_visible(ui, row_h) {
            ui.allocate_space(vec2(ui.available_width(), row_h));
            ui.add_space(8.0);
            continue;
        }
        let selected = app.country.as_ref().is_some_and(|sel| sel.key == c.key);
        let fg = if selected { BG } else { FG };
        // Sold out here right now: shown, but faded.
        let opacity = if c.count == 0 && !selected { 0.55 } else { 1.0 };
        let style = if selected {
            RowStyle::selected()
        } else {
            RowStyle::base()
        }
        .pad(14.0, 12.0)
        .opacity(opacity);
        let (resp, _) = clickable_row(ui, ("country", &c.key), row_h, &style, |ui| {
            let (fill, badge_fg) = if selected {
                (BG, FG)
            } else {
                (white(0.08), FG)
            };
            badge(
                ui,
                &c.code,
                vec2(32.0, 24.0),
                5,
                op(fill, opacity),
                Color32::TRANSPARENT,
                mono_semi(MONO_XS),
                op(badge_fg, opacity),
            );
            // Keep price and dial code intact; the country name gives way when space is short.
            let reserved = 56.0 + 12.0 + if c.dial.is_some() { 48.0 } else { 0.0 };
            let name_w = (ui.available_width() - reserved).max(40.0);
            text_trunc(ui, &c.name, sans_med(SANS_ROW), op(fg, opacity), name_w);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.allocate_ui_with_layout(
                    vec2(56.0, ui.available_height()),
                    Layout::right_to_left(Align::Center),
                    |ui| {
                        text(ui, fmt_usd(c.price), mono(MONO_XL), op(fg, opacity));
                    },
                );
                if let Some(dial) = &c.dial {
                    text(
                        ui,
                        dial,
                        mono(MONO_MD),
                        op(if selected { black(0.45) } else { white(0.45) }, opacity),
                    );
                }
            });
        });
        if resp.clicked() {
            out.push(Action::PickCountry(c.clone()));
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
    let groups = app.offer_rows();
    if groups.is_empty() {
        empty_hint(ui, &["No offers right now.", "Try another country."]);
        return;
    }
    let tier_h = 22.0 + line_h(ui, &mono_semi(MONO_PRICE)).max(line_h(ui, &sans(SANS_ROW)) + 4.0);
    for g in groups {
        ui.horizontal(|ui| {
            ui.add_space(2.0);
            text(ui, &g.name, sans_semi(SANS_BODY), FG);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(2.0);
                text(
                    ui,
                    format!("{} available", fmt_thousands(g.total)),
                    mono(MONO_XS),
                    white(0.4),
                );
            });
        });
        ui.add_space(8.0);
        for (k, t) in g.tiers.iter().enumerate() {
            let selected = app
                .offer
                .as_ref()
                .is_some_and(|(group, tier)| *group == g.name && tier == t);
            let fav = app.favorite_for(&g.name, t);
            let is_fav = fav.as_ref().is_some_and(|f| app.is_fav(f));
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
                        mono_semi(MONO_PRICE),
                        if selected { BG } else { FG },
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing = vec2(12.0, 0.0);
                        let star = IconBtn::new(Icon::Star { filled: is_fav })
                            .fg(if selected { black(0.6) } else { white(0.5) })
                            .hover_fg(if selected { BG } else { FG })
                            .tooltip(if is_fav {
                                "Remove from favorites"
                            } else {
                                "Add to favorites"
                            })
                            .show(ui)
                            .clicked();
                        text(
                            ui,
                            format!("{} numbers", fmt_thousands(t.count)),
                            sans(SANS_SMALL),
                            if selected { black(0.55) } else { white(0.45) },
                        );
                        star
                    })
                    .inner
                });
            if star_clicked {
                if let Some(fav) = fav {
                    out.push(Action::ToggleFav(fav));
                }
            } else if resp.clicked() {
                out.push(Action::PickOffer(g.name.clone(), t.clone()));
            }
            ui.add_space(8.0);
        }
        ui.add_space(12.0);
    }
}

fn summary_bar_height(ui: &Ui) -> f32 {
    let text_block = line_h(ui, &sans_med(SANS_BODY_LG)) + 2.0 + line_h(ui, &sans(SANS_SMALL));
    let button = line_h(ui, &sans_semi(SANS_BODY_LG)) + 22.0;
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
                    text(ui, price, mono_semi(MONO_TOTAL), FG);
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        let w = ui.available_width();
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
                            text_trunc(ui, line, sans_med(SANS_BODY_LG), FG, w);
                            text(ui, via, sans(SANS_SMALL), white(0.45));
                        });
                    });
                });
            });
        });
    ui.add_space(4.0);
}
