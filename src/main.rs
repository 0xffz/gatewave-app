//! Gatewave — gpui desktop client for buying SMS-verification numbers.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod backend;
mod config;
mod domain;
mod sound;
mod theme;
mod ui;
mod worker;

use std::borrow::Cow;

use gpui::{
    App, AppContext, Application, AssetSource, Bounds, Result, SharedString, TitlebarOptions,
    WindowBounds, WindowOptions, px, size,
};
use gpui_component::Root;

/// gpui-component's stock lucide icons plus Gatewave's own additions.
struct GatewaveAssets;

impl AssetSource for GatewaveAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == "icons/star-filled.svg" {
            return Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/star-filled.svg"
            ))));
        }
        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        gpui_component_assets::Assets.list(path)
    }
}

fn main() {
    Application::new()
        .with_assets(GatewaveAssets)
        .run(|cx: &mut App| {
            gpui_component::init(cx);
            theme::init(cx);
            cx.activate(true);

            let bounds = Bounds::centered(None, size(px(1380.0), px(860.0)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(1100.0), px(640.0))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Gatewave".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            cx.open_window(options, |window, cx| {
                let view = cx.new(|cx| ui::Gatewave::new(window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("open the main window");
        });
}
