//! Number Desk — egui mock-up of the SMS-number client design.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod model;
mod sim;
mod theme;
mod ui;

use std::time::Duration;

use app::App;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Number Desk")
            .with_app_id("number-desk")
            .with_inner_size([1380.0, 860.0])
            .with_min_inner_size([1100.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Number Desk",
        options,
        Box::new(|cc| {
            theme::install_fonts(&cc.egui_ctx);
            theme::apply_style(&cc.egui_ctx);
            let mut app = App::new();
            if let Ok(scene) = std::env::var("NUMBER_DESK_SCENE") {
                app.apply_scene(&scene);
            }
            Ok(Box::new(app))
        }),
    )
}

impl App {
    /// One frame: advance the simulation, draw, apply UI actions, schedule the next repaint.
    fn frame(&mut self, ui: &mut egui::Ui) {
        self.tick();

        let ctx = ui.ctx().clone();
        let actions = ui::draw(ui, self);
        for action in actions {
            self.apply(action);
        }
        if let Some(text) = self.take_clipboard() {
            ctx.copy_text(text);
        }

        // Keep animating while something moves; otherwise wake up for the next simulated callback.
        if self.animating() {
            ctx.request_repaint_after(Duration::from_millis(33));
        } else if let Some(deadline) = self.next_deadline() {
            ctx.request_repaint_after(
                deadline
                    .saturating_duration_since(self.now)
                    .max(Duration::from_millis(16)),
            );
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frame(ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headless() -> egui::Context {
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        theme::apply_style(&ctx);
        ctx
    }

    /// Runs one frame at `time` seconds and returns the repaint delay egui was asked for.
    fn run_frame(ctx: &egui::Context, app: &mut App, time: f64) -> Duration {
        let input = egui::RawInput {
            time: Some(time),
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1380.0, 860.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| app.frame(ui));
        // No GPU here: acknowledge font-atlas uploads so epaint's drop guard is happy.
        out.textures_delta.clear();
        out.viewport_output
            .get(&egui::ViewportId::ROOT)
            .map(|v| v.repaint_delay)
            .unwrap_or(Duration::MAX)
    }

    const SCENES: [&str; 11] = [
        "",
        "step2",
        "step3",
        "step4",
        "offer",
        "requesting",
        "favorites",
        "settings",
        "connecting",
        "snack",
        "empty",
    ];

    #[test]
    fn every_scene_renders_without_panicking() {
        let ctx = headless();
        for scene in SCENES {
            let mut app = App::new();
            app.apply_scene(scene);
            run_frame(&ctx, &mut app, 0.0);
            run_frame(&ctx, &mut app, 0.05);
        }
    }

    #[test]
    fn repaint_keeps_being_requested_while_a_request_is_in_flight() {
        let ctx = headless();
        let mut app = App::new();
        app.apply_scene("requesting");
        for i in 0..5 {
            let delay = run_frame(&ctx, &mut app, i as f64 * 0.05);
            assert!(delay <= Duration::from_millis(50), "frame {i}: {delay:?}");
        }
    }

    #[test]
    fn idle_app_does_not_spin() {
        let ctx = headless();
        let mut app = App::new();
        app.numbers.clear();
        let mut delay = Duration::ZERO;
        for i in 0..4 {
            delay = run_frame(&ctx, &mut app, i as f64 * 0.05);
        }
        assert!(
            delay > Duration::from_secs(1),
            "idle app still repainting every {delay:?}"
        );
    }
}
