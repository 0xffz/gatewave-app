//! Top-centre snackbar overlay.

use gpui::{
    AnyElement, BoxShadow, Context, IntoElement, ParentElement, Styled, Window, div, point, px,
};
use gpui_component::IconName;

use super::Gatewave;
use super::widgets::*;
use crate::app::{Action, SnackKind};
use crate::theme::*;

impl Gatewave {
    pub(super) fn render_snack(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(snack) = &self.app.snack else {
            return div().into_any_element();
        };
        let dot_color = match snack.kind {
            SnackKind::Error => SNACK_ERROR,
            SnackKind::Success => SNACK_SUCCESS,
            SnackKind::Info => BG,
        };
        div()
            .absolute()
            .top(px(18.0))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                div()
                    .max_w(px(520.0))
                    .bg(FG)
                    .rounded(px(8.0))
                    .pl(px(16.0))
                    .pr(px(14.0))
                    .py(px(11.0))
                    .shadow(vec![BoxShadow {
                        color: black(0.5).into(),
                        offset: point(px(0.0), px(12.0)),
                        blur_radius: px(32.0),
                        spread_radius: px(0.0),
                    }])
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(10.0))
                    .child(dot(8.0, dot_color))
                    .child(
                        sans_med(SANS_BODY_LG)
                            .whitespace_normal()
                            .text_color(BG)
                            .child(snack.msg.clone()),
                    )
                    .child(
                        IconBtn::new("snack-close", IconName::Close)
                            .fg(black(0.45))
                            .hover_fg(BG)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dispatch(Action::DismissSnack, window, cx);
                            })),
                    ),
            )
            .into_any_element()
    }
}
