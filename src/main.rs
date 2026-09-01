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
    App, AppContext, Application, AssetSource, Bounds, KeyBinding, Menu, MenuItem, OsAction,
    Result, SharedString, SystemMenuType, TitlebarOptions, WindowBounds, WindowOptions, actions,
    px, size,
};
use gpui_component::Root;
use gpui_component::input;

actions!(gatewave, [Quit, Hide, HideOthers, ShowAll, Minimize, Zoom]);

/// The standard macOS menu bar: application, Edit (routed to the focused text input) and
/// Window menus. Other platforms ignore menus they do not support.
fn init_menus(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &Minimize, cx| {
        if let Some(window) = cx.active_window() {
            window
                .update(cx, |_, window, _| window.minimize_window())
                .ok();
        }
    });
    cx.on_action(|_: &Zoom, cx| {
        if let Some(window) = cx.active_window() {
            window.update(cx, |_, window, _| window.zoom_window()).ok();
        }
    });
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("alt-cmd-h", HideOthers, None),
        KeyBinding::new("cmd-m", Minimize, None),
    ]);
    cx.set_menus(vec![
        Menu {
            name: "Gatewave".into(),
            items: vec![
                MenuItem::action("Hide Gatewave", Hide),
                MenuItem::action("Hide Others", HideOthers),
                MenuItem::action("Show All", ShowAll),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Quit Gatewave", Quit),
            ],
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::os_action("Undo", input::Undo, OsAction::Undo),
                MenuItem::os_action("Redo", input::Redo, OsAction::Redo),
                MenuItem::separator(),
                MenuItem::os_action("Cut", input::Cut, OsAction::Cut),
                MenuItem::os_action("Copy", input::Copy, OsAction::Copy),
                MenuItem::os_action("Paste", input::Paste, OsAction::Paste),
                MenuItem::separator(),
                MenuItem::os_action("Select All", input::SelectAll, OsAction::SelectAll),
            ],
        },
        Menu {
            name: "Window".into(),
            items: vec![
                MenuItem::action("Minimize", Minimize),
                MenuItem::action("Zoom", Zoom),
            ],
        },
    ]);
}

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
            init_menus(cx);
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
