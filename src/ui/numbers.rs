//! Right panel: active number cards in every lifecycle state.

use std::f32::consts::TAU;

use egui::{Align, CornerRadius, Frame, Layout, Margin, ScrollArea, Stroke, Ui, vec2};

use super::widgets::*;
use crate::app::{Action, App};
use crate::domain::{Number, NumberStatus, fmt_usd, mmss};
use crate::theme::*;

pub fn draw(ui: &mut Ui, app: &App, out: &mut Vec<Action>) {
    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        text_ls(ui, "ACTIVE NUMBERS", sans(10.5), white(0.35), 1.47);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(4.0);
            text(
                ui,
                format!("{:02}", app.numbers.len()),
                mono(12.0),
                white(0.5),
            );
        });
    });
    ui.add_space(12.0);
    ScrollArea::vertical()
        .id_salt("numbers")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            ui.set_width(ui.available_width());
            if app.numbers.is_empty() {
                dashed_box(ui, &["No active numbers.", "Request one on the left."]);
            }
            for n in &app.numbers {
                card(ui, app, n, out);
                ui.add_space(12.0);
            }
        });
}

fn card(ui: &mut Ui, app: &App, n: &Number, out: &mut Vec<Action>) {
    let invert = n.status == NumberStatus::Received;
    let opacity = if matches!(n.status, NumberStatus::Expired | NumberStatus::Cancelled) {
        0.55
    } else {
        1.0
    };
    let (fill, border, fg, muted, faint) = if invert {
        (FG, FG, BG, black(0.5), black(0.4))
    } else {
        (CARD_BG, white(0.1), FG, white(0.45), white(0.35))
    };
    // Copy chips: a copy icon that turns into a check mark while the copy is fresh.
    let copy_btn = |copied: bool, what: &str| {
        IconBtn::new(if copied { Icon::Check } else { Icon::Copy })
            .fg(if invert { black(0.65) } else { white(0.65) })
            .border(if invert { black(0.25) } else { white(0.18) })
            .hover_fg(if invert { BG } else { FG })
            .hover_border(if invert { black(0.5) } else { white(0.4) })
            .tooltip(if copied {
                "Copied".to_string()
            } else {
                format!("Copy {what}")
            })
    };

    Frame::new()
        .fill(op(fill, opacity))
        .stroke(Stroke::new(1.0, op(border, opacity)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin {
            left: 15,
            right: 15,
            top: 14,
            bottom: 14,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;

            // Phone line
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
                match &n.phone {
                    Some(phone) => {
                        text_ls(ui, phone, mono_semi(14.5), op(fg, opacity), 0.15);
                        let copied = app.copied_is(&format!("{}-p", n.id));
                        if copy_btn(copied, "number")
                            .opacity(opacity)
                            .show(ui)
                            .clicked()
                        {
                            out.push(Action::CopyPhone(n.id));
                        }
                    }
                    None => {
                        text_ls(ui, "+·· ··· ··· ···", mono_semi(14.5), white(0.3), 0.15);
                    }
                }
                if n.dismissible() {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let r = IconBtn::new(Icon::Close)
                            .fg(op(faint, opacity))
                            .hover_fg(op(fg, opacity))
                            .tooltip("Dismiss")
                            .show(ui);
                        if r.clicked() {
                            out.push(Action::DismissNumber(n.id));
                        }
                    });
                }
            });
            ui.add_space(3.0);
            text(ui, n.meta_line(), sans(11.5), op(muted, opacity));

            match n.status {
                NumberStatus::Requesting => {
                    ui.add_space(12.0);
                    skeleton(ui, vec2(ui.available_width() * 0.7, 14.0), 5);
                    ui.add_space(8.0);
                    text(ui, "Requesting number…", sans(11.5), white(0.4));
                }
                NumberStatus::Waiting => waiting(ui, app, n, out),
                NumberStatus::Received => {
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(10.0, 0.0);
                        if invert {
                            text_ls(
                                ui,
                                n.code.as_deref().unwrap_or(""),
                                mono_semi(20.0),
                                BG,
                                1.2,
                            );
                        } else {
                            Frame::new()
                                .fill(FG)
                                .corner_radius(CornerRadius::same(6))
                                .inner_margin(Margin {
                                    left: 10,
                                    right: 10,
                                    top: 4,
                                    bottom: 4,
                                })
                                .show(ui, |ui| {
                                    text_ls(
                                        ui,
                                        n.code.as_deref().unwrap_or(""),
                                        mono_semi(20.0),
                                        BG,
                                        1.2,
                                    )
                                });
                        }
                        let copied = app.copied_is(&format!("{}-c", n.id));
                        if copy_btn(copied, "code").show(ui).clicked() {
                            out.push(Action::CopyCode(n.id));
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            text(
                                ui,
                                fmt_usd(n.price),
                                mono(11.5),
                                if invert { black(0.4) } else { white(0.4) },
                            );
                        });
                    });
                }
                NumberStatus::Expired => {
                    ui.add_space(12.0);
                    text(
                        ui,
                        "Expired · no SMS received",
                        sans(12.0),
                        op(white(0.4), opacity),
                    );
                }
                NumberStatus::Cancelled => {
                    ui.add_space(12.0);
                    text(
                        ui,
                        format!("Cancelled · {} refunded", fmt_usd(n.price)),
                        sans(12.0),
                        op(white(0.4), opacity),
                    );
                }
            }
        });
}

fn waiting(ui: &mut Ui, app: &App, n: &Number, out: &mut Vec<Action>) {
    let now = app.now;
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        // CSS `blink 2.4s ease infinite`: opacity 1 → .25 → 1
        let t = ui.input(|i| i.time) as f32;
        let phase = (t % 2.4) / 2.4;
        let blink = 1.0 - 0.75 * (0.5 - 0.5 * (phase * TAU).cos());
        text(ui, "Waiting for SMS", sans(12.0), white(0.55 * blink));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            text(ui, mmss(n.time_left(now)), mono(12.5), FG);
        });
    });
    ui.add_space(8.0);
    progress_bar(ui, n.progress(now));
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let cancel_wait = n.cancelable_at.filter(|t| *t > now);
        let label = if n.cancel_pending {
            "Cancelling…".to_string()
        } else if let Some(t) = cancel_wait {
            format!(
                "Cancel in {}",
                mmss(t.duration_since(now).unwrap_or_default())
            )
        } else {
            "Cancel & refund".to_string()
        };
        let disabled = n.cancel_pending || cancel_wait.is_some();
        let r = Btn::new(label, sans(11.5))
            .fg(white(0.6))
            .border(white(0.16))
            .pad(11.0, 6.0)
            .radius(6)
            .disabled(disabled)
            .opacity(if disabled { 0.5 } else { 1.0 })
            .show(ui);
        if r.clicked() {
            out.push(Action::CancelNumber(n.id));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            text(ui, fmt_usd(n.price), mono(11.5), white(0.4));
        });
    });
}
