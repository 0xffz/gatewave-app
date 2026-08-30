//! Favorites screen: saved provider · service · country combinations.

use egui::{Align, Color32, CornerRadius, Frame, Layout, Margin, ScrollArea, Stroke, Ui, vec2};

use super::widgets::*;
use super::{content_column, page_header};
use crate::app::{Action, App};
use crate::domain::fmt_usd4;
use crate::theme::*;

pub fn draw(ui: &mut Ui, app: &App, out: &mut Vec<Action>) {
    content_column(ui, |ui| {
        page_header(
            ui,
            "FAVORITES",
            "Saved combinations",
            Some("One click to request a number for a saved provider · service · country."),
        );
        ScrollArea::vertical()
            .id_salt("favorites")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                ui.set_width(ui.available_width());
                if app.favorites.is_empty() {
                    dashed_box(
                        ui,
                        &[
                            "No favorites yet.",
                            "Star an offer on step 4 to save it here.",
                        ],
                    );
                }
                for (i, f) in app.favorites.iter().enumerate() {
                    Frame::new()
                        .fill(ROW_BG)
                        .stroke(Stroke::new(1.0, white(0.1)))
                        .corner_radius(CornerRadius::same(9))
                        .inner_margin(Margin {
                            left: 16,
                            right: 16,
                            top: 13,
                            bottom: 13,
                        })
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = vec2(12.0, 0.0);
                                // Lay the fixed-width right side out first so the text block
                                // can truncate to whatever width is left.
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.spacing_mut().item_spacing = vec2(12.0, 0.0);
                                    let star = Btn::new("★", sans(SANS_ICON))
                                        .fg(white(0.5))
                                        .hover_fg(FG)
                                        .pad(4.0, 2.0)
                                        .show(ui);
                                    if star.clicked() {
                                        out.push(Action::RemoveFav(i));
                                    }
                                    let req = Btn::primary("Request", 12.5)
                                        .pad(14.0, 9.0)
                                        .radius(7)
                                        .show(ui);
                                    if req.clicked() {
                                        out.push(Action::RequestFav(i));
                                    }
                                    text(ui, fmt_usd4(f.price), mono(MONO_XL), FG);
                                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                        ui.spacing_mut().item_spacing = vec2(12.0, 0.0);
                                        badge(
                                            ui,
                                            &f.country_code,
                                            vec2(32.0, 24.0),
                                            5,
                                            white(0.08),
                                            Color32::TRANSPARENT,
                                            mono_semi(MONO_XS),
                                            FG,
                                        );
                                        let w = ui.available_width().max(40.0);
                                        ui.vertical(|ui| {
                                            ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
                                            text_trunc(
                                                ui,
                                                format!("{} · {}", f.service_name, f.country_name),
                                                sans_med(SANS_ROW),
                                                FG,
                                                w,
                                            );
                                            text_trunc(
                                                ui,
                                                format!(
                                                    "via {} · {}",
                                                    f.provider.name(),
                                                    f.operator
                                                ),
                                                sans(SANS_SMALL),
                                                white(0.45),
                                                w,
                                            );
                                        });
                                    });
                                });
                            });
                        });
                    ui.add_space(8.0);
                }
                ui.add_space(30.0);
            });
    });
}
