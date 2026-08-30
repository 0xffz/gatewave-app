//! Number Desk — egui desktop client for buying SMS-verification numbers.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod backend;
mod config;
mod domain;
mod theme;
mod ui;
mod worker;

use std::time::{Duration, Instant};

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
            theme::apply_style(&cc.egui_ctx);
            Ok(Box::new(App::new(cc)))
        }),
    )
}

impl App {
    /// One frame: drain worker results and timers, draw, apply UI actions, schedule the next repaint.
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

        // Keep animating while something moves; otherwise wake up for the next timer.
        if self.busy() {
            ctx.request_repaint_after(Duration::from_millis(33));
        } else if let Some(deadline) = self.next_deadline() {
            ctx.request_repaint_after(
                deadline
                    .saturating_duration_since(Instant::now())
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
    use std::sync::Arc;

    use sms_activate::{ActivationStatus, ApiError};

    use super::*;
    use crate::app::testing::*;
    use crate::app::{Action, Event, Screen, SnackKind};
    use crate::backend::mock::MockBackend;
    use crate::backend::{ANY_OPERATOR, ProviderKind};
    use crate::config::Config;
    use crate::domain::NumberStatus;

    fn headless() -> egui::Context {
        let ctx = egui::Context::default();
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

    fn two_frames(ctx: &egui::Context, app: &mut App) {
        run_frame(ctx, app, 0.0);
        run_frame(ctx, app, 0.05);
    }

    #[test]
    fn every_screen_renders_without_panicking() {
        let ctx = headless();

        // Step 1 with one provider connecting (skeleton in Settings) and one failed.
        let (mut app, _) = app_with(
            config_with_keys(&[ProviderKind::HeroSms, ProviderKind::FiveSim]),
            vec![
                (
                    ProviderKind::HeroSms,
                    Arc::new(MockBackend::new(ProviderKind::HeroSms)),
                ),
                (
                    ProviderKind::FiveSim,
                    Arc::new(MockBackend::new(ProviderKind::FiveSim).failing("down")),
                ),
            ],
        );
        app.screen = Screen::Settings;
        run_frame(&ctx, &mut app, 0.0); // connecting skeletons drawn before the first tick lands
        two_frames(&ctx, &mut app); // connected / failed cards + error snack
        app.screen = Screen::New;
        two_frames(&ctx, &mut app);

        // Wizard: every loading state and every loaded step.
        let (mut app, mock, _) = hero_app();
        app.apply(Action::PickProvider(ProviderKind::HeroSms));
        two_frames(&ctx, &mut app); // first frame: services skeleton
        app.apply(Action::SetSearch("tele".into()));
        two_frames(&ctx, &mut app);
        app.apply(Action::SetSearch(String::new()));
        let tg = app.services[0].clone();
        app.apply(Action::PickService(tg));
        two_frames(&ctx, &mut app);
        app.apply(Action::ToggleSort);
        two_frames(&ctx, &mut app);
        let us = app.countries[0].clone();
        app.apply(Action::PickCountry(us));
        two_frames(&ctx, &mut app);
        let tier = app.offer_groups[0].tiers[0].clone();
        app.apply(Action::PickOffer(ANY_OPERATOR.into(), tier.clone()));
        two_frames(&ctx, &mut app); // summary bar
        let fav = app.favorite_for(ANY_OPERATOR, &tier).unwrap();
        app.apply(Action::ToggleFav(fav));
        app.apply(Action::RequestNumber);
        run_frame(&ctx, &mut app, 0.1); // Requesting card (tick resolves it during the frame)
        two_frames(&ctx, &mut app); // Waiting card
        app.apply(Action::RequestNumber);
        app.tick();
        let id = app.numbers[0].id;
        app.handle_event(Event::CancelDone {
            local_id: id,
            result: Err(ApiError::EarlyCancelDenied),
        });
        two_frames(&ctx, &mut app); // "Cancel in m:ss" + info snack
        app.apply(Action::RequestNumber);
        app.tick();
        let id = app.numbers[0].id;
        app.handle_event(Event::Polled {
            local_id: id,
            result: Ok(ActivationStatus::Expired),
        });
        app.apply(Action::RequestNumber);
        app.tick();
        let id = app.numbers[0].id;
        app.handle_event(Event::Polled {
            local_id: id,
            result: Ok(ActivationStatus::Cancelled),
        });
        mock.set_status(ActivationStatus::Ok {
            code: "42 424".into(),
        });
        app.apply(Action::RequestNumber);
        app.tick();
        app.fast_forward(Duration::from_secs(6));
        assert_eq!(app.numbers[0].status, NumberStatus::Received);
        two_frames(&ctx, &mut app); // received / expired / cancelled / waiting cards + success snack
        let id = app.numbers[0].id;
        app.apply(Action::CopyCode(id));
        two_frames(&ctx, &mut app); // "Copied" label

        // Favorites (filled) and Settings (connected + disconnected inputs).
        app.apply(Action::GoScreen(Screen::Favorites));
        two_frames(&ctx, &mut app);
        app.apply(Action::GoScreen(Screen::Settings));
        app.apply(Action::SetKeyInput(
            ProviderKind::SmsBower,
            "sb_key_123".into(),
        ));
        two_frames(&ctx, &mut app);
        app.apply(Action::Disconnect(ProviderKind::HeroSms));
        two_frames(&ctx, &mut app);

        // Empty states.
        let (mut app, _) = app_with(
            Config::default(),
            vec![(
                ProviderKind::HeroSms,
                Arc::new(MockBackend::new(ProviderKind::HeroSms)),
            )],
        );
        two_frames(&ctx, &mut app);
        app.apply(Action::GoScreen(Screen::Favorites));
        two_frames(&ctx, &mut app);
        app.toast("Hero SMS: no numbers available.", SnackKind::Error);
        two_frames(&ctx, &mut app);
    }

    #[test]
    fn step_one_connect_button_jumps_to_settings() {
        let ctx = headless();
        let (mut app, _, _) = hero_app();
        // No pointer here; exercise the actions the buttons emit.
        app.apply(Action::GoScreen(Screen::Settings));
        two_frames(&ctx, &mut app);
        assert_eq!(app.screen, Screen::Settings);
        app.apply(Action::GoStep(1));
        assert_eq!(app.screen, Screen::New);
    }

    #[test]
    fn repaint_keeps_being_requested_while_waiting() {
        let ctx = headless();
        let (mut app, _, _) = hero_app();
        app.walk_to_offer(ProviderKind::HeroSms);
        app.apply(Action::RequestNumber);
        for i in 0..5 {
            let delay = run_frame(&ctx, &mut app, i as f64 * 0.05);
            assert!(delay <= Duration::from_millis(50), "frame {i}: {delay:?}");
        }
        assert_eq!(app.numbers[0].status, NumberStatus::Waiting);
        // Once nothing is live any more the app waits for its next timer instead of spinning.
        app.snack = None;
        app.numbers.clear();
        let mut delay = Duration::ZERO;
        for i in 0..4 {
            delay = run_frame(&ctx, &mut app, 1.0 + i as f64 * 0.05);
        }
        assert!(
            delay > Duration::from_millis(100),
            "idle app still repainting every {delay:?}"
        );
    }

    #[test]
    fn idle_app_does_not_spin() {
        let ctx = headless();
        let (mut app, _, _) = hero_app();
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
