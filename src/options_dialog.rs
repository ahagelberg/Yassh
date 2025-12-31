use crate::config::{AppConfig, Theme};
use egui::{Align2, Area, Color32, Order, Ui, Window};

// Dialog constants
const OVERLAY_COLOR: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 180);

pub struct OptionsDialog {
    visible: bool,
    config: AppConfig,
}

impl Default for OptionsDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl OptionsDialog {
    pub fn new() -> Self {
        Self {
            visible: false,
            config: AppConfig::default(),
        }
    }

    pub fn open(&mut self, config: AppConfig) {
        self.config = config;
        self.visible = true;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<OptionsResult> {
        if !self.visible {
            return None;
        }
        let mut result = None;
        // Draw modal overlay
        Area::new(egui::Id::new("options_modal_overlay"))
            .order(Order::Middle)
            .anchor(Align2::LEFT_TOP, [0.0, 0.0])
            .show(ctx, |ui| {
                let screen_rect = ctx.content_rect();
                ui.allocate_response(screen_rect.size(), egui::Sense::click());
                ui.painter().rect_filled(screen_rect, 0.0, OVERLAY_COLOR);
            });
        Window::new("Options")
            .collapsible(false)
            .resizable(false)
            .order(Order::Foreground)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .min_width(400.0)
            .show(ctx, |ui| {
                result = self.show_content(ui);
            });
        result
    }

    fn show_content(&mut self, ui: &mut Ui) -> Option<OptionsResult> {
        let mut result = None;
        ui.heading("Appearance");
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label("Theme:");
            egui::ComboBox::from_id_salt("theme_select")
                .selected_text(match self.config.theme {
                    Theme::Dark => "Dark",
                    Theme::Light => "Light",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.config.theme, Theme::Dark, "Dark");
                    ui.selectable_value(&mut self.config.theme, Theme::Light, "Light");
                });
        });
        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);
        // Check for Enter key to submit
        let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                result = Some(OptionsResult::Cancelled);
                self.visible = false;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("OK").clicked() || enter_pressed {
                    result = Some(OptionsResult::Saved(self.config.clone()));
                    self.visible = false;
                }
            });
        });
        result
    }
}

pub enum OptionsResult {
    Saved(AppConfig),
    Cancelled,
}

