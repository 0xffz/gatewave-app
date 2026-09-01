//! Favorites screen: saved provider · service · country combinations.

use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, Styled, Window, div, px,
};
use gpui_component::scroll::ScrollableElement as _;

use super::widgets::*;
use super::{Gatewave, page_header};
use crate::app::Action;
use crate::domain::fmt_usd4;
use crate::theme::*;

impl Gatewave {
    pub(super) fn render_favorites(
        &mut self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut list = div().flex().flex_col().w_full().pb(px(30.0));
        if self.app.favorites.is_empty() {
            list = list.child(dashed_box(&[
                "No favorites yet.",
                "Star an offer on step 4 to save it here.",
            ]));
        }
        list = list.children(self.app.favorites.iter().enumerate().map(|(i, f)| {
            div()
                .mb(px(8.0))
                .w_full()
                .bg(ROW_BG)
                .border_1()
                .border_color(white(0.1))
                .rounded(px(9.0))
                .px(px(16.0))
                .py(px(13.0))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(12.0))
                // Country badge.
                .child(
                    badge_frame(32.0, 24.0, 5.0, white(0.08), TRANSPARENT).child(
                        mono_semi(MONO_XS)
                            .text_color(FG)
                            .child(f.country_code.clone()),
                    ),
                )
                // Service · country plus the provider · operator meta line; truncates to
                // whatever width the fixed right side leaves over.
                .child(
                    div()
                        .flex_1()
                        .min_w(px(40.0))
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            sans_med(SANS_ROW)
                                .w_full()
                                .truncate()
                                .text_color(FG)
                                .child(format!("{} · {}", f.service_name, f.country_name)),
                        )
                        .child(
                            sans(SANS_SMALL)
                                .w_full()
                                .truncate()
                                .text_color(white(0.45))
                                .child(format!("via {} · {}", f.provider.name(), f.operator)),
                        ),
                )
                .child(mono(MONO_XL).text_color(FG).child(fmt_usd4(f.price)))
                .child(
                    Btn::primary(("request-fav", i as u32), "Request", SANS_LABEL)
                        .pad(14.0, 9.0)
                        .radius(7.0)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.dispatch(Action::RequestFav(i), window, cx);
                        })),
                )
                .child(
                    IconBtn::new(("remove-fav", i as u32), star_icon(true))
                        .fg(white(0.5))
                        .hover_fg(FG)
                        .tooltip("Remove from favorites")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.dispatch(Action::RemoveFav(i), window, cx);
                        })),
                )
        }));
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(page_header(
                "FAVORITES",
                "Saved combinations",
                Some("One click to request a number for a saved provider · service · country."),
            ))
            .child(
                div()
                    .id("favorites")
                    .flex_1()
                    .min_h(px(0.0))
                    .child(list)
                    .overflow_y_scrollbar(),
            )
            .into_any_element()
    }
}
