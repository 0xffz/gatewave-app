//! Settings screen: behaviour toggles and provider API keys.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use gpui_component::input::Input;
use gpui_component::scroll::ScrollableElement as _;
use gpui_component::skeleton::Skeleton;
use gpui_component::switch::Switch;

use super::widgets::*;
use super::{Gatewave, page_header};
use crate::app::Action;
use crate::domain::{PREF_DEFS, fmt_usd, masked_key};
use crate::theme::*;

/// Height of the API-key input (mono 12.5 line + 2 × 10 padding), shared with the
/// Connect button so the pair lines up.
const KEY_INPUT_H: f32 = 40.0;

impl Gatewave {
    pub(super) fn render_settings(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let content = div()
            .flex()
            .flex_col()
            .w_full()
            .pb(px(30.0))
            .child(self.settings_behaviour(cx))
            .child(div().h(px(28.0)))
            .child(self.settings_providers(cx));
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(page_header(
                "SETTINGS",
                "Providers",
                Some("Connect a provider with its API key to request numbers through it."),
            ))
            .child(
                div()
                    .id("settings")
                    .flex_1()
                    .min_h(px(0.0))
                    .child(content)
                    .overflow_y_scrollbar(),
            )
            .into_any_element()
    }

    /// Preference toggles: one card, a separator between rows. The whole row is the click
    /// target; the switch is purely visual (no handler, so the click falls through).
    fn settings_behaviour(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let last = PREF_DEFS.len() - 1;
        div()
            .flex()
            .flex_col()
            .w_full()
            .child(div().pb(px(10.0)).child(eyebrow("BEHAVIOR")))
            .child(
                div()
                    .w_full()
                    .bg(ROW_BG)
                    .border_1()
                    .border_color(white(0.1))
                    .rounded(px(10.0))
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .children(
                        PREF_DEFS
                            .iter()
                            .enumerate()
                            .flat_map(|(i, (key, label, hint))| {
                                let key = *key;
                                let on = self.app.prefs.get(key);
                                let mut out: Vec<AnyElement> = vec![
                                    div()
                                        .id(("pref", i as u32))
                                        .group(ROW_GROUP)
                                        .w_full()
                                        .px(px(16.0))
                                        .py(px(13.0))
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap(px(14.0))
                                        .cursor_pointer()
                                        .hover(|s| s.bg(white(0.06)))
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w(px(0.0))
                                                .flex()
                                                .flex_col()
                                                .gap(px(2.0))
                                                .child(
                                                    sans_med(SANS_BODY_LG)
                                                        .text_color(FG)
                                                        .child(*label),
                                                )
                                                .child(
                                                    sans(SANS_SMALL)
                                                        .text_color(white(0.45))
                                                        .child(*hint),
                                                ),
                                        )
                                        .child(Switch::new(("toggle", i as u32)).checked(on))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.dispatch(Action::TogglePref(key), window, cx);
                                        }))
                                        .into_any_element(),
                                ];
                                if i < last {
                                    out.push(hline(white(0.08)).into_any_element());
                                }
                                out
                            }),
                    ),
            )
            .into_any_element()
    }

    /// One card per provider slot: status dot + name, then the masked key (connected),
    /// a loading skeleton (connecting) or the API-key input with a Connect button.
    fn settings_providers(&mut self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .w_full()
            .child(div().pb(px(10.0)).child(eyebrow("PROVIDERS")))
            .children(self.app.providers.iter().enumerate().map(|(idx, p)| {
                let kind = p.kind;
                let header = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(10.0))
                    .child(dot(7.0, if p.connected { GREEN } else { white(0.2) }))
                    .child(sans_semi(SANS_ROW_LG).text_color(FG).child(kind.name()))
                    .when(p.connected, |d| {
                        d.child(div().flex_1())
                            .when_some(p.balance, |d, balance| {
                                d.child(
                                    mono(MONO_LG)
                                        .text_color(op(FG, 0.75))
                                        .child(fmt_usd(balance)),
                                )
                            })
                            .child(
                                Btn::new(("disconnect", idx as u32), "Disconnect")
                                    .sans(SANS_CAPTION)
                                    .fg(white(0.4))
                                    .hover_fg(RED_HOVER)
                                    .pad(2.0, 2.0)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.dispatch(Action::Disconnect(kind), window, cx);
                                    })),
                            )
                    });
                let body: AnyElement = if p.connected {
                    mono(MONO_MD)
                        .text_color(white(0.4))
                        .child(format!(
                            "API key · {}",
                            masked_key(p.key.as_deref().unwrap_or(""))
                        ))
                        .into_any_element()
                } else if p.connecting {
                    Skeleton::new()
                        .w_full()
                        .h(px(38.0))
                        .rounded(px(7.0))
                        .into_any_element()
                } else {
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            Input::new(&self.key_inputs[&kind])
                                .flex_1()
                                .h(px(KEY_INPUT_H))
                                .rounded(px(7.0))
                                .px(px(12.0))
                                .py(px(10.0))
                                .font_family(MONO_FAMILY)
                                .text_size(px(MONO_LG)),
                        )
                        .child(
                            Btn::primary(("connect", idx as u32), "Connect", 13.0)
                                .pad(16.0, 0.0)
                                .radius(7.0)
                                .min_height(KEY_INPUT_H)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.dispatch(Action::Connect(kind), window, cx);
                                })),
                        )
                        .into_any_element()
                };
                div()
                    .mb(px(10.0))
                    .w_full()
                    .bg(ROW_BG)
                    .border_1()
                    .border_color(white(0.1))
                    .rounded(px(10.0))
                    .px(px(18.0))
                    .py(px(16.0))
                    .flex()
                    .flex_col()
                    .child(header)
                    .child(div().h(px(12.0)))
                    .child(body)
            }))
            .into_any_element()
    }
}
