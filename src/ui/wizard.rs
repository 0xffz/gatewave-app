//! "New number" screen: 4-step wizard (provider → service → country → offer).

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Div, Focusable, InteractiveElement, IntoElement, ParentElement,
    SharedString, Stateful, StatefulInteractiveElement, Styled, Window, div, linear_color_stop,
    linear_gradient, px,
};
use gpui_component::input::Input;
use gpui_component::scroll::ScrollableElement as _;
use gpui_component::skeleton::Skeleton;

use super::Gatewave;
use super::widgets::*;
use crate::app::{Action, Screen, SortDir};
use crate::domain::{fmt_thousands, fmt_usd, fmt_usd4};
use crate::theme::*;

/// Search box height: the body-lg line plus 11 px vertical padding and the 1 px borders.
const INPUT_H: f32 = 42.0;
/// Step 1 rows: 26 px chrome + the taller of the provider name and the Connect button.
const PROVIDER_ROW_H: f32 = 50.0;
/// Step 2 tiles: 24 px padding + the 30 px initial badge.
const SERVICE_TILE_H: f32 = 54.0;
/// Step 3 rows: 24 px padding + the 24 px country badge.
const COUNTRY_ROW_H: f32 = 48.0;
/// Step 4 tier rows: 22 px padding + the "N numbers" line.
const TIER_H: f32 = 44.0;

impl Gatewave {
    pub(super) fn render_wizard(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let has_bar = self.app.step == 4 && !self.app.loading_offers && self.app.offer.is_some();
        let bottom = if has_bar { 8.0 } else { 30.0 };
        // Four distinct `overflow_y_scrollbar` call sites so each step keeps its own scroll
        // offset (the egui original salts the ScrollArea id with the step).
        let list = match self.app.step {
            1 => list_frame(1, self.step_providers(cx), bottom).overflow_y_scrollbar(),
            2 => list_frame(2, self.step_services(cx), bottom).overflow_y_scrollbar(),
            3 => list_frame(3, self.step_countries(cx), bottom).overflow_y_scrollbar(),
            _ => list_frame(4, self.step_offers(cx), bottom).overflow_y_scrollbar(),
        };
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(self.wizard_header(window, cx))
            .child(list)
            .when(has_bar, |d| d.child(self.wizard_summary_bar(cx)))
            .into_any_element()
    }

    /// "STEP x / 4" eyebrow, the step title and the search row (plus the price-sort toggle
    /// on step 3).
    fn wizard_header(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let focused = self
            .search_input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let step = self.app.step;
        div()
            .flex_none()
            .flex()
            .flex_col()
            .pt(px(30.0))
            .child(
                mono(MONO_XS)
                    .text_color(white(0.4))
                    .child(format!("STEP {step} / 4")),
            )
            .child(
                div().pt(px(6.0)).child(
                    sans_semi(SANS_TITLE)
                        .text_color(FG)
                        .child(self.app.step_title()),
                ),
            )
            .child(
                div()
                    .pt(px(16.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .h(px(INPUT_H))
                            .px(px(14.0))
                            .rounded(px(8.0))
                            .bg(ROW_BG)
                            .border_1()
                            .border_color(if focused { white(0.4) } else { white(0.12) })
                            .flex()
                            .items_center()
                            .child(
                                Input::new(&self.search_input)
                                    .appearance(false)
                                    .w_full()
                                    .font_family(SANS_FAMILY)
                                    .text_size(px(SANS_BODY_LG)),
                            ),
                    )
                    .when(step == 3 && !self.app.loading_countries, |d| {
                        let arrow = match self.app.sort_dir {
                            Some(SortDir::Desc) => "↓",
                            Some(SortDir::Asc) => "↑",
                            None => "↕",
                        };
                        d.child(
                            Btn::new("sort", format!("Price {arrow}"))
                                .mono(MONO_SM)
                                .fg(white(0.6))
                                .border(white(0.14))
                                .hover_fg(FG)
                                .hover_border(white(0.4))
                                .pad(14.0, 0.0)
                                .radius(8.0)
                                .min_height(INPUT_H)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.dispatch(Action::ToggleSort, window, cx);
                                })),
                        )
                    }),
            )
            .child(div().h(px(22.0)))
    }

    // -----------------------------------------------------------------------
    // Step 1

    fn step_providers(&self, cx: &mut Context<Self>) -> Div {
        let selected_kind = self.app.provider;
        div().flex().flex_col().gap(px(8.0)).pb(px(8.0)).children(
            self.app.provider_rows().into_iter().map(|p| {
                let kind = p.kind;
                let selected = selected_kind == Some(kind);
                let fg = if selected { BG } else { FG };
                let opacity = if !p.connected && !selected { 0.55 } else { 1.0 };
                let style = if selected {
                    RowStyle::selected()
                } else {
                    RowStyle::base()
                }
                .opacity(opacity);
                // The mint green is tuned for dark rows; the selected row is light, so use
                // the darker "connected" green there.
                let dot_color = match (p.connected, selected) {
                    (true, true) => GREEN_ON_LIGHT,
                    (true, false) => GREEN,
                    (false, _) => white(0.2),
                };
                let right: AnyElement = if p.connected {
                    match p.balance {
                        Some(balance) => mono(MONO_LG)
                            .text_color(op(fg, 0.75))
                            .child(fmt_usd(balance))
                            .into_any_element(),
                        None => div().into_any_element(),
                    }
                } else if p.connecting {
                    sans(SANS_SMALL)
                        .text_color(op(white(0.6), opacity))
                        .child("Connecting…")
                        .into_any_element()
                } else {
                    Btn::new(("connect", kind as u32), "Connect ›")
                        .sans(SANS_SMALL)
                        .fg(white(0.6))
                        .border(white(0.2))
                        .hover_fg(FG)
                        .hover_border(white(0.5))
                        .pad(10.0, 5.0)
                        .radius(6.0)
                        .opacity(opacity)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.dispatch(Action::GoScreen(Screen::Settings), window, cx);
                        }))
                        .into_any_element()
                };
                row(("provider", kind as u32), PROVIDER_ROW_H, &style)
                    .child(dot(7.0, op(dot_color, opacity)))
                    .child(
                        sans_med(SANS_ROW_LG)
                            .text_color(op(fg, opacity))
                            .child(p.name()),
                    )
                    .child(div().flex_1())
                    .child(right)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.dispatch(Action::PickProvider(kind), window, cx);
                    }))
            }),
        )
    }

    // -----------------------------------------------------------------------
    // Step 2

    fn step_services(&self, cx: &mut Context<Self>) -> Div {
        if self.app.loading_services {
            return div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .pb(px(8.0))
                .children((0..4).map(|_| {
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(8.0))
                        .child(Skeleton::new().flex_1().h(px(56.0)).rounded(px(8.0)))
                        .child(Skeleton::new().flex_1().h(px(56.0)).rounded(px(8.0)))
                }));
        }
        let rows = self.app.service_rows();
        if rows.is_empty() {
            return div()
                .flex()
                .flex_col()
                .child(dashed_box(&["No services match.", "Try another search."]));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .pb(px(8.0))
            .children(rows.chunks(2).map(|pair| {
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.0))
                    .children(pair.iter().map(|s| {
                        let selected = self
                            .app
                            .service
                            .as_ref()
                            .is_some_and(|sel| sel.code == s.code);
                        let style = if selected {
                            RowStyle::selected()
                        } else {
                            RowStyle::base()
                        }
                        .pad_x(14.0);
                        let (fill, badge_fg) = if selected {
                            (BG, FG)
                        } else {
                            (white(0.08), FG)
                        };
                        let initial: String = s
                            .name
                            .chars()
                            .next()
                            .map(|c| c.to_uppercase().collect())
                            .unwrap_or_default();
                        let svc = (*s).clone();
                        div().flex_1().child(
                            row(
                                SharedString::from(format!("service-{}", s.code.as_str())),
                                SERVICE_TILE_H,
                                &style,
                            )
                            .child(
                                badge_frame(30.0, 30.0, 7.0, fill, TRANSPARENT)
                                    .child(mono_semi(MONO_XL).text_color(badge_fg).child(initial)),
                            )
                            .child(
                                sans_med(SANS_ROW)
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_color(if selected { BG } else { FG })
                                    .child(s.name.clone()),
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.dispatch(Action::PickService(svc.clone()), window, cx);
                                },
                            )),
                        )
                    }))
                    .when(pair.len() == 1, |d| d.child(div().flex_1()))
            }))
    }

    // -----------------------------------------------------------------------
    // Step 3

    fn step_countries(&self, cx: &mut Context<Self>) -> Div {
        if self.app.loading_countries {
            return skeleton_list(50.0);
        }
        let rows = self.app.country_rows();
        if rows.is_empty() {
            return div().flex().flex_col().child(dashed_box(&[
                "No countries for this service.",
                "Try another service.",
            ]));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .pb(px(8.0))
            .children(rows.into_iter().map(|c| {
                let selected = self
                    .app
                    .country
                    .as_ref()
                    .is_some_and(|sel| sel.key == c.key);
                let fg = if selected { BG } else { FG };
                // Sold out here right now: shown, but faded.
                let opacity = if c.count == 0 && !selected { 0.55 } else { 1.0 };
                let style = if selected {
                    RowStyle::selected()
                } else {
                    RowStyle::base()
                }
                .pad_x(14.0)
                .opacity(opacity);
                let (fill, badge_fg) = if selected {
                    (BG, FG)
                } else {
                    (white(0.08), FG)
                };
                let country = c.clone();
                row(
                    SharedString::from(format!("country-{}", c.key)),
                    COUNTRY_ROW_H,
                    &style,
                )
                .child(
                    badge_frame(32.0, 24.0, 5.0, op(fill, opacity), TRANSPARENT).child(
                        mono_semi(MONO_XS)
                            .text_color(op(badge_fg, opacity))
                            .child(c.code.clone()),
                    ),
                )
                // Keep price and dial code intact; the country name gives way when space
                // is short.
                .child(
                    sans_med(SANS_ROW)
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .text_color(op(fg, opacity))
                        .child(c.name.clone()),
                )
                .when_some(c.dial.clone(), |r, dial| {
                    r.child(
                        mono(MONO_MD)
                            .text_color(op(
                                if selected { black(0.45) } else { white(0.45) },
                                opacity,
                            ))
                            .child(dial),
                    )
                })
                .child(
                    div().flex_none().w(px(56.0)).flex().justify_end().child(
                        mono(MONO_XL)
                            .text_color(op(fg, opacity))
                            .child(fmt_usd(c.price)),
                    ),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.dispatch(Action::PickCountry(country.clone()), window, cx);
                }))
            }))
    }

    // -----------------------------------------------------------------------
    // Step 4

    fn step_offers(&self, cx: &mut Context<Self>) -> Div {
        if self.app.loading_offers {
            return skeleton_list(44.0);
        }
        let groups = self.app.offer_rows();
        if groups.is_empty() {
            return div().flex().flex_col().child(dashed_box(&[
                "No offers right now.",
                "Try another country.",
            ]));
        }
        div()
            .flex()
            .flex_col()
            .children(groups.into_iter().enumerate().map(|(gi, g)| {
                div()
                    .flex()
                    .flex_col()
                    .pb(px(20.0))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .px(px(2.0))
                            .child(
                                sans_semi(SANS_BODY)
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_color(FG)
                                    .child(g.name.clone()),
                            )
                            .child(
                                mono(MONO_XS)
                                    .text_color(white(0.4))
                                    .child(format!("{} available", fmt_thousands(g.total))),
                            ),
                    )
                    .child(div().h(px(8.0)))
                    .child(
                        div().flex().flex_col().gap(px(8.0)).children(
                            g.tiers
                                .iter()
                                .enumerate()
                                .map(|(k, t)| self.offer_tier(gi, g.name.as_str(), k, t, cx)),
                        ),
                    )
            }))
    }

    fn offer_tier(
        &self,
        gi: usize,
        group: &str,
        k: usize,
        t: &crate::backend::OfferTier,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let selected = self
            .app
            .offer
            .as_ref()
            .is_some_and(|(sel_group, tier)| *sel_group == group && tier == t);
        let fav = self.app.favorite_for(group, t);
        let is_fav = fav.as_ref().is_some_and(|f| self.app.is_fav(f));
        let style = if selected {
            RowStyle::selected()
        } else {
            RowStyle::base()
        }
        .pad_x(14.0)
        .radius(8.0);
        let star = IconBtn::new(
            SharedString::from(format!("star-{gi}-{k}")),
            star_icon(is_fav),
        )
        .fg(if selected { black(0.6) } else { white(0.5) })
        .hover_fg(if selected { BG } else { FG })
        .tooltip(if is_fav {
            "Remove from favorites"
        } else {
            "Add to favorites"
        });
        let star = match fav {
            Some(fav) => star.on_click(cx.listener(move |this, _, window, cx| {
                this.dispatch(Action::ToggleFav(fav.clone()), window, cx);
            })),
            None => star,
        };
        let group_name = group.to_string();
        let tier = t.clone();
        row(SharedString::from(format!("tier-{gi}-{k}")), TIER_H, &style)
            .child(dot(6.0, if selected { BG } else { white(0.3) }))
            .child(
                mono_semi(MONO_PRICE)
                    .text_color(if selected { BG } else { FG })
                    .child(fmt_usd4(t.price)),
            )
            .child(div().flex_1())
            .child(
                sans(SANS_SMALL)
                    .text_color(if selected { black(0.55) } else { white(0.45) })
                    .child(format!("{} numbers", fmt_thousands(t.count))),
            )
            .child(star)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.dispatch(
                    Action::PickOffer(group_name.clone(), tier.clone()),
                    window,
                    cx,
                );
            }))
    }

    // -----------------------------------------------------------------------
    // Summary bar

    /// Sticky request bar below the list: fade strip, then the summary card.
    fn wizard_summary_bar(&self, cx: &mut Context<Self>) -> Div {
        let Some((line, via, price)) = self.app.summary() else {
            return div();
        };
        div()
            .flex_none()
            .flex()
            .flex_col()
            // Fade the list out above the bar (CSS `linear-gradient(transparent, #0b0b0c 30%)`).
            .child(div().h(px(24.0)).w_full().bg(linear_gradient(
                180.0,
                linear_color_stop(TRANSPARENT, 0.0),
                linear_color_stop(BG, 1.0),
            )))
            .child(
                div()
                    .w_full()
                    .bg(SUMMARY_BG)
                    .border_1()
                    .border_color(white(0.1))
                    .rounded(px(10.0))
                    .px(px(16.0))
                    .py(px(14.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(14.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(sans_med(SANS_BODY_LG).truncate().text_color(FG).child(line))
                            .child(sans(SANS_SMALL).text_color(white(0.45)).child(via)),
                    )
                    .child(mono_semi(MONO_TOTAL).text_color(FG).child(price))
                    .child(
                        Btn::primary("request-number", "Request number", SANS_BODY_LG)
                            .pad(18.0, 11.0)
                            .radius(8.0)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dispatch(Action::RequestNumber, window, cx);
                            })),
                    ),
            )
            .child(div().h(px(4.0)))
    }
}

/// The scrollable list region: styles here are hoisted onto the `Scrollable` wrapper, so the
/// actual rows live in a nested column.
fn list_frame(step: u8, content: Div, bottom: f32) -> Stateful<Div> {
    div()
        .id(("wizard", step as u32))
        .flex_1()
        .min_h(px(0.0))
        .w_full()
        .child(
            div()
                .flex()
                .flex_col()
                .w_full()
                .child(content)
                .child(div().h(px(bottom))),
        )
}

/// Eight full-width loading placeholders (countries: 50 px, offers: 44 px).
fn skeleton_list(h: f32) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .pb(px(8.0))
        .children((0..8).map(move |_| Skeleton::new().w_full().h(px(h)).rounded(px(8.0))))
}
