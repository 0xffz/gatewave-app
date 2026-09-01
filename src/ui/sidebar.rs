//! Left sidebar: logo, navigation, step tracker, balances.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window, div, px,
};

use super::Gatewave;
use super::widgets::*;
use crate::app::{Action, Screen};
use crate::domain::fmt_usd;
use crate::theme::*;

impl Gatewave {
    pub(super) fn render_sidebar(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            // Logo
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .pl(px(6.0))
                    .child(
                        div()
                            .flex_none()
                            .bg(FG)
                            .rounded(px(5.0))
                            .px(px(7.0))
                            .py(px(3.0))
                            .child(mono_semi(MONO_LOGO).text_color(BG).child("GW")),
                    )
                    .child(
                        div()
                            .pl(px(8.0))
                            .child(sans_med(SANS_BODY).text_color(FG).child("Gatewave")),
                    ),
            )
            .child(div().h(px(22.0)))
            // Navigation
            .children(
                [
                    ("New number", Screen::New),
                    ("Favorites", Screen::Favorites),
                    ("Settings", Screen::Settings),
                ]
                .map(|(label, screen)| {
                    let active = self.app.screen == screen;
                    div().pb(px(4.0)).child(
                        Btn::new(label, label)
                            .sans_med(SANS_BODY_LG)
                            .full_width()
                            .align_left()
                            .pad(12.0, 9.0)
                            .radius(7.0)
                            .fg(if active { BG } else { white(0.55) })
                            .bg(if active { FG } else { TRANSPARENT })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.dispatch(Action::GoScreen(screen), window, cx);
                            })),
                    )
                }),
            )
            .child(div().h(px(18.0)))
            // Steps
            .child(div().pl(px(6.0)).pb(px(10.0)).child(eyebrow("STEPS")))
            .children(self.app.steps().map(|s| {
                let style = RowStyle::base()
                    .fill(if s.active { FG } else { TRANSPARENT })
                    .border(TRANSPARENT)
                    .radius(7.0)
                    .pad_x(12.0)
                    .gap(11.0)
                    .clickable(s.reachable && !s.active);
                let (label, value, badge_fill, badge_border, badge_fg) = if s.active {
                    (BG, black(0.55), BG, TRANSPARENT, FG)
                } else if s.reachable {
                    (
                        white(0.55),
                        white(0.4),
                        TRANSPARENT,
                        white(0.22),
                        white(0.55),
                    )
                } else {
                    (
                        white(0.3),
                        white(0.22),
                        TRANSPARENT,
                        white(0.12),
                        white(0.3),
                    )
                };
                let clickable = s.reachable && !s.active;
                let num = s.num;
                div().pb(px(6.0)).child(
                    row(("step", s.num as u32), 56.0, &style)
                        .child(
                            badge_frame(26.0, 26.0, 6.0, badge_fill, badge_border).child(
                                if s.active {
                                    mono_semi(MONO_XS)
                                } else {
                                    mono(MONO_XS)
                                }
                                .text_color(badge_fg)
                                .child(format!("0{}", s.num)),
                            ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .min_w(px(0.0))
                                .child(
                                    sans_med(SANS_BODY)
                                        .text_color(label)
                                        .when(clickable, |d| {
                                            d.group_hover(ROW_GROUP, |st| st.text_color(FG))
                                        })
                                        .child(s.label),
                                )
                                .child(
                                    mono(MONO_SM)
                                        .text_color(value)
                                        .max_w(px(150.0))
                                        .truncate()
                                        .when(clickable, |d| {
                                            d.group_hover(ROW_GROUP, |st| st.text_color(white(0.6)))
                                        })
                                        .child(s.value.clone().unwrap_or_else(|| "—".to_string())),
                                ),
                        )
                        .when(clickable, |d| {
                            d.on_click(cx.listener(move |this, _, window, cx| {
                                this.dispatch(Action::GoStep(num), window, cx);
                            }))
                        }),
                )
            }))
            // Balances, pinned to the bottom (CSS `margin-top:auto`).
            .child(div().flex_1())
            .child(hline(white(0.08)))
            .child(
                div()
                    .pl(px(6.0))
                    .pt(px(14.0))
                    .pb(px(8.0))
                    .child(eyebrow("BALANCES")),
            )
            .children(self.app.balances().into_iter().map(|(name, balance)| {
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px(px(6.0))
                    .pb(px(8.0))
                    .child(sans(SANS_LABEL).text_color(white(0.6)).child(name))
                    .child(mono(MONO_MD).text_color(FG).child(fmt_usd(balance)))
            }))
            .into_any_element()
    }
}
