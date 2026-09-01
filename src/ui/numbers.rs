//! Right panel: active number cards in every lifecycle state.

use std::f32::consts::TAU;
use std::time::Duration;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    Animation, AnimationExt as _, AnyElement, Context, InteractiveElement, IntoElement,
    ParentElement, Styled, Window, div, px,
};
use gpui_component::IconName;
use gpui_component::scroll::ScrollableElement as _;

use super::widgets::*;
use super::{Gatewave, NUMBERS_W};
use crate::app::{Action, App};
use crate::domain::{Number, NumberStatus, fmt_usd, mmss, phone_display};
use crate::theme::*;

impl Gatewave {
    pub(super) fn render_numbers(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let empty = self.app.numbers.is_empty();
        let cards: Vec<AnyElement> = self
            .app
            .numbers
            .iter()
            .map(|n| card(&self.app, n, cx))
            .collect();
        div()
            .flex()
            .flex_col()
            .size_full()
            // Header: eyebrow + zero-padded count.
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .px(px(4.0))
                    .child(eyebrow("ACTIVE NUMBERS"))
                    .child(div().flex_1())
                    .child(
                        mono(MONO_MD)
                            .text_color(white(0.5))
                            .child(format!("{:02}", self.app.numbers.len())),
                    ),
            )
            .child(div().h(px(12.0)))
            .child(
                div()
                    .id("numbers")
                    .flex_1()
                    .min_h(px(0.0))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .when(empty, |d| {
                                d.child(dashed_box(&[
                                    "No active numbers.",
                                    "Request one on the left.",
                                ]))
                            })
                            .children(cards),
                    )
                    .overflow_y_scrollbar(),
            )
            .into_any_element()
    }
}

fn card(app: &App, n: &Number, cx: &mut Context<Gatewave>) -> AnyElement {
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
    let id = n.id;

    // Copy chips: a copy icon that turns into a green check mark while the copy is fresh.
    let copy_chip = |copied: bool, what: &str, chip_id: (&'static str, u32)| {
        IconBtn::new(
            chip_id,
            if copied {
                IconName::Check
            } else {
                IconName::Copy
            },
        )
        .fg(if copied {
            if invert { GREEN_ON_LIGHT } else { GREEN }
        } else if invert {
            black(0.65)
        } else {
            white(0.65)
        })
        .border(if invert { black(0.25) } else { white(0.18) })
        .hover_fg(if invert { BG } else { FG })
        .hover_border(if invert { black(0.5) } else { white(0.4) })
        .tooltip(if copied {
            "Copied".to_string()
        } else {
            format!("Copy {what}")
        })
    };

    // Phone line: number + copy chip (or placeholder), dismiss × on the right.
    let phone_line = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .map(|d| match &n.phone {
            Some(phone) => d
                .child(
                    mono_semi(MONO_PHONE)
                        .text_color(op(fg, opacity))
                        .child(phone_display(phone)),
                )
                .child(
                    copy_chip(
                        app.copied_is(&format!("{id}-p")),
                        "number",
                        ("copy-phone", id),
                    )
                    .opacity(opacity)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.dispatch(Action::CopyPhone(id), window, cx);
                    })),
                ),
            None => d.child(
                mono_semi(MONO_PHONE)
                    .text_color(white(0.3))
                    .child("+·· ··· ··· ···"),
            ),
        })
        .when(n.dismissible(), |d| {
            d.child(div().flex_1()).child(
                IconBtn::new(("dismiss", id), IconName::Close)
                    .fg(op(faint, opacity))
                    .hover_fg(op(fg, opacity))
                    .tooltip("Dismiss")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.dispatch(Action::DismissNumber(id), window, cx);
                    })),
            )
        });

    let body: AnyElement = match n.status {
        NumberStatus::Requesting => {
            // 70 % of the card's fixed inner width (panel − panel padding − card padding − border).
            let skel_w = (NUMBERS_W - 2.0 * 18.0 - 2.0 * 15.0 - 2.0) * 0.7;
            div()
                .pt(px(12.0))
                .flex()
                .flex_col()
                .child(skeleton(skel_w, 14.0, 5.0))
                .child(div().h(px(8.0)))
                .child(
                    sans(SANS_SMALL)
                        .text_color(white(0.4))
                        .child("Requesting number…"),
                )
                .into_any_element()
        }
        NumberStatus::Waiting => waiting(app, n, cx),
        NumberStatus::Received => div()
            .pt(px(12.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(10.0))
            .child(
                mono_semi(MONO_CODE)
                    .text_color(BG)
                    .child(n.code.clone().unwrap_or_default()),
            )
            .child(
                copy_chip(app.copied_is(&format!("{id}-c")), "code", ("copy-code", id)).on_click(
                    cx.listener(move |this, _, window, cx| {
                        this.dispatch(Action::CopyCode(id), window, cx);
                    }),
                ),
            )
            .child(div().flex_1())
            .child(mono(MONO_SM).text_color(black(0.4)).child(fmt_usd(n.price)))
            .into_any_element(),
        NumberStatus::Expired => div()
            .pt(px(12.0))
            .child(
                sans(SANS_CAPTION)
                    .text_color(op(white(0.4), opacity))
                    .child("Expired · no SMS received"),
            )
            .into_any_element(),
        NumberStatus::Cancelled => div()
            .pt(px(12.0))
            .child(
                sans(SANS_CAPTION)
                    .text_color(op(white(0.4), opacity))
                    .child(format!("Cancelled · {} refunded", fmt_usd(n.price))),
            )
            .into_any_element(),
    };

    div()
        .w_full()
        .mb(px(12.0))
        .bg(op(fill, opacity))
        .border_1()
        .border_color(op(border, opacity))
        .rounded(px(10.0))
        .px(px(15.0))
        .pt(px(14.0))
        .pb(px(14.0))
        .child(phone_line)
        .child(div().h(px(3.0)))
        .child(
            sans(SANS_SMALL)
                .text_color(op(muted, opacity))
                .child(n.meta_line()),
        )
        .child(body)
        .into_any_element()
}

/// Blinking status line, countdown, progress bar and the cancel/refund row.
fn waiting(app: &App, n: &Number, cx: &mut Context<Gatewave>) -> AnyElement {
    let now = app.now;
    let id = n.id;
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
    div()
        .pt(px(12.0))
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .child(
                    // CSS `blink 2.4s ease infinite`: opacity 1 → .25 → 1.
                    sans(SANS_CAPTION)
                        .text_color(white(0.55))
                        .child("Waiting for SMS")
                        .with_animation(
                            ("blink", id),
                            Animation::new(Duration::from_millis(2400)).repeat(),
                            |el, delta| {
                                let blink = 1.0 - 0.75 * (0.5 - 0.5 * (delta * TAU).cos());
                                el.text_color(white(0.55 * blink))
                            },
                        ),
                )
                .child(div().flex_1())
                .child(mono(MONO_LG).text_color(FG).child(mmss(n.time_left(now)))),
        )
        .child(div().h(px(8.0)))
        .child(progress_bar(n.progress(now)))
        .child(div().h(px(10.0)))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .child(
                    Btn::new(("cancel", id), label)
                        .sans(SANS_SMALL)
                        .fg(white(0.6))
                        .border(white(0.16))
                        .pad(11.0, 6.0)
                        .radius(6.0)
                        .disabled(disabled)
                        .opacity(if disabled { 0.5 } else { 1.0 })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.dispatch(Action::CancelNumber(id), window, cx);
                        })),
                )
                .child(div().flex_1())
                .child(mono(MONO_SM).text_color(white(0.4)).child(fmt_usd(n.price))),
        )
        .into_any_element()
}
