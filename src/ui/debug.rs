//! Floating developer panel (debug builds only): tweak text size, UI zoom and egui's style at
//! runtime while styling. Toggle with **F12**.

use egui::epaint::Shadow;
use egui::{Context, CornerRadius, Frame, Key, Margin, Stroke, Window, vec2};

use crate::theme::{self, CARD_BG, black, white};

pub struct DebugPanel {
    open: bool,
    font_scale: f32,
    zoom: f32,
    debug_on_hover: bool,
}

impl Default for DebugPanel {
    fn default() -> Self {
        Self {
            // Start open when GATEWAVE_DEBUG_PANEL=1 (handy for screenshots).
            open: std::env::var("GATEWAVE_DEBUG_PANEL").is_ok_and(|v| v == "1"),
            font_scale: theme::font_scale(),
            zoom: 1.0,
            debug_on_hover: false,
        }
    }
}

impl DebugPanel {
    pub fn show(&mut self, ctx: &Context) {
        if ctx.input(|i| i.key_pressed(Key::F12)) {
            self.open = !self.open;
        }
        if !self.open {
            return;
        }
        let mut open = self.open;
        Window::new("Debug · F12")
            .open(&mut open)
            .default_pos((40.0, 60.0))
            .default_width(320.0)
            .resizable(true)
            .frame(
                Frame::new()
                    .fill(CARD_BG)
                    .stroke(Stroke::new(1.0, white(0.15)))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::same(14))
                    .shadow(Shadow {
                        offset: [0, 8],
                        blur: 24,
                        spread: 0,
                        color: black(0.5),
                    }),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                ui.spacing_mut().slider_width = 180.0;

                ui.label("Text size");
                let mut changed = false;
                ui.horizontal(|ui| {
                    changed |= ui
                        .add(
                            egui::Slider::new(&mut self.font_scale, 0.7..=1.6)
                                .step_by(0.05)
                                .suffix("×"),
                        )
                        .changed();
                    if ui.small_button("reset").clicked() {
                        self.font_scale = theme::DEFAULT_FONT_SCALE;
                        changed = true;
                    }
                });
                if changed {
                    theme::set_font_scale(self.font_scale);
                }

                ui.label("UI zoom (everything, incl. spacing)");
                ui.horizontal(|ui| {
                    let mut zoom_changed = ui
                        .add(egui::Slider::new(&mut self.zoom, 0.75..=1.75).step_by(0.05))
                        .changed();
                    if ui.small_button("1.0").clicked() {
                        self.zoom = 1.0;
                        zoom_changed = true;
                    }
                    if zoom_changed {
                        ctx.set_zoom_factor(self.zoom);
                    }
                });

                if ui
                    .checkbox(&mut self.debug_on_hover, "Show layout rects on hover")
                    .changed()
                {
                    ctx.set_debug_on_hover(self.debug_on_hover);
                }

                ui.separator();
                ui.collapsing("egui style", |ui| ctx.style_ui(ui, egui::Theme::Dark));
                ui.collapsing("Inspection", |ui| ctx.inspection_ui(ui));
            });
        self.open = open;
    }
}
