//! Top-centre snackbar.

use egui::epaint::Shadow;
use egui::{Align, Align2, Area, Context, CornerRadius, Frame, Id, Layout, Margin, Order, vec2};

use super::widgets::*;
use crate::app::{Action, App, SnackKind};
use crate::theme::*;

pub fn draw(ctx: &Context, app: &App, out: &mut Vec<Action>) {
    let Some(snack) = &app.snack else { return };
    let dot_color = match snack.kind {
        SnackKind::Error => SNACK_ERROR,
        SnackKind::Success => SNACK_SUCCESS,
        SnackKind::Info => BG,
    };
    Area::new(Id::new("snackbar"))
        .order(Order::Foreground)
        .anchor(Align2::CENTER_TOP, vec2(0.0, 18.0))
        .interactable(true)
        .show(ctx, |ui| {
            Frame::new()
                .fill(FG)
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin {
                    left: 16,
                    right: 14,
                    top: 11,
                    bottom: 11,
                })
                .shadow(Shadow {
                    offset: [0, 12],
                    blur: 32,
                    spread: 0,
                    color: black(0.5),
                })
                .show(ui, |ui| {
                    ui.set_max_width(520.0);
                    ui.spacing_mut().item_spacing = vec2(10.0, 0.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing = vec2(10.0, 0.0);
                        let r = Btn::new("✕", mono(15.0))
                            .fg(black(0.45))
                            .pad(2.0, 2.0)
                            .show(ui);
                        if r.clicked() {
                            out.push(Action::DismissSnack);
                        }
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing = vec2(10.0, 0.0);
                            dot(ui, 8.0, dot_color);
                            text_wrap(ui, &snack.msg, sans_med(13.5), BG);
                        });
                    });
                });
        });
}
