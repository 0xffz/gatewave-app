//! Root gpui view: three-column layout (sidebar · main content · active numbers), the snackbar
//! overlay, the shared text inputs and the event pump that replaces egui's repaint scheduling.

pub mod widgets;

mod favorites;
mod numbers;
mod settings;
mod sidebar;
mod snack;
mod wizard;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::channel::mpsc;
use futures::{StreamExt, select_biased};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AppContext, ClipboardItem, Context, Entity, IntoElement, ParentElement, Render, Styled,
    Subscription, Task, Timer, Window, div, px,
};
use gpui_component::input::{InputEvent, InputState};

use crate::app::{Action, App, Screen};
use crate::backend::ProviderKind;
use crate::sound;
use crate::theme::*;

pub const SIDEBAR_W: f32 = 264.0;
pub const NUMBERS_W: f32 = 372.0;

/// The root view: owns the [`App`] state machine, the shared text-input entities and the pump.
pub struct Gatewave {
    pub app: App,
    /// Wizard search box (all four steps share it; `App::search` is the source of truth).
    pub search_input: Entity<InputState>,
    /// Settings API-key boxes, one per provider slot.
    pub key_inputs: HashMap<ProviderKind, Entity<InputState>>,
    _pump: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl Gatewave {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (wake_tx, wake_rx) = mpsc::unbounded::<()>();
        let wake: crate::worker::Wake = Arc::new(move || {
            let _ = wake_tx.unbounded_send(());
        });
        Self::with_app(App::new(Some(wake)), wake_rx, window, cx)
    }

    fn with_app(
        app: App,
        wake_rx: mpsc::UnboundedReceiver<()>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(app.search_placeholder()));
        let mut subscriptions = vec![cx.subscribe_in(
            &search_input,
            window,
            |this: &mut Self, state, ev: &InputEvent, window, cx| {
                if matches!(ev, InputEvent::Change) {
                    let value = state.read(cx).value().to_string();
                    if this.app.search != value {
                        this.dispatch(Action::SetSearch(value), window, cx);
                    }
                }
            },
        )];

        let mut key_inputs = HashMap::new();
        for kind in ProviderKind::ALL {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder(kind.key_hint()));
            subscriptions.push(cx.subscribe_in(
                &input,
                window,
                move |this: &mut Self, state, ev: &InputEvent, window, cx| {
                    if matches!(ev, InputEvent::Change) {
                        let value = state.read(cx).value().to_string();
                        if this.app.key_inputs.get(&kind).map(String::as_str)
                            != Some(value.as_str())
                        {
                            this.dispatch(Action::SetKeyInput(kind, value), window, cx);
                        }
                    }
                },
            ));
            key_inputs.insert(kind, input);
        }

        let pump = cx.spawn_in(window, async move |this, cx| {
            let mut wake_rx = wake_rx;
            loop {
                let delay = match this.update_in(cx, |gw, window, cx| gw.tick(window, cx)) {
                    Ok(delay) => delay,
                    Err(_) => break,
                };
                match delay {
                    Some(delay) => {
                        select_biased! {
                            _ = wake_rx.next() => {}
                            _ = futures::FutureExt::fuse(Timer::after(delay)) => {}
                        }
                    }
                    None => {
                        if wake_rx.next().await.is_none() {
                            break;
                        }
                    }
                }
            }
        });

        Self {
            app,
            search_input,
            key_inputs,
            _pump: pump,
            _subscriptions: subscriptions,
        }
    }

    /// Applies a UI action and flushes its side effects.
    pub fn dispatch(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        self.app.apply(action);
        self.after_mutate(window, cx);
    }

    /// One pump step: drain timers and worker results, then schedule the next wake-up
    /// via [`App::wake_delay`].
    fn tick(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<Duration> {
        self.app.tick();
        self.after_mutate(window, cx);
        self.app.wake_delay(Instant::now())
    }

    /// Clipboard, chime and input synchronisation after any state mutation.
    fn after_mutate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.app.take_clipboard() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        if self.app.take_chime() {
            sound::chime();
        }
        self.sync_inputs(window, cx);
        cx.notify();
    }

    /// `App` owns the canonical text; push it back into the input entities when an action
    /// changed it (e.g. `GoStep` clears the search).
    fn sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let placeholder = self.app.search_placeholder();
        let search = self.app.search.clone();
        self.search_input.update(cx, |state, cx| {
            if state.value().as_ref() != search {
                state.set_value(search, window, cx);
            }
            state.set_placeholder(placeholder, window, cx);
        });
        for (kind, input) in &self.key_inputs {
            let text = self.app.key_inputs.get(kind).cloned().unwrap_or_default();
            input.update(cx, |state, cx| {
                if state.value().as_ref() != text {
                    state.set_value(text, window, cx);
                }
            });
        }
    }
}

impl Render for Gatewave {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let center = match self.app.screen {
            Screen::New => self.render_wizard(window, cx),
            Screen::Favorites => self.render_favorites(window, cx),
            Screen::Settings => self.render_settings(window, cx),
        };
        div()
            .size_full()
            .relative()
            .flex()
            .flex_row()
            .bg(BG)
            .font_family(SANS_FAMILY)
            .text_color(FG)
            .text_size(px(SANS_BODY))
            .child(
                div()
                    .flex_none()
                    .w(px(SIDEBAR_W))
                    .h_full()
                    .px(px(18.0))
                    .py(px(22.0))
                    .border_r_1()
                    .border_color(white(0.08))
                    .child(self.render_sidebar(window, cx)),
            )
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .min_w(px(0.0))
                    .px(px(34.0))
                    .child(center),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(NUMBERS_W))
                    .h_full()
                    .px(px(18.0))
                    .py(px(22.0))
                    .bg(RIGHT_BG)
                    .border_l_1()
                    .border_color(white(0.08))
                    .child(self.render_numbers(window, cx)),
            )
            .child(self.render_snack(window, cx))
    }
}

/// "FAVORITES" / "SETTINGS" eyebrow + h1 + optional paragraph.
pub fn page_header(eyebrow_text: &str, title: &str, para: Option<&str>) -> gpui::Div {
    use widgets::*;
    div()
        .flex()
        .flex_col()
        .pt(px(30.0))
        .child(
            mono(MONO_XS)
                .text_color(white(0.4))
                .child(eyebrow_text.to_string()),
        )
        .child(
            div().pt(px(6.0)).child(
                sans_semi(SANS_TITLE)
                    .text_color(FG)
                    .child(title.to_string()),
            ),
        )
        .when_some(para, |d, p| {
            d.child(
                div().pt(px(6.0)).pb(px(24.0)).child(
                    sans(SANS_BODY)
                        .whitespace_normal()
                        .text_color(white(0.45))
                        .child(p.to_string()),
                ),
            )
        })
}
