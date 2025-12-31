use crate::ssh::connection::ConnectionState;
use egui::{Color32, Ui, Vec2};
use uuid::Uuid;

fn brighten_color(color: Color32, amount: f32) -> Color32 {
    let r = (color.r() as f32 + (255.0 - color.r() as f32) * amount) as u8;
    let g = (color.g() as f32 + (255.0 - color.g() as f32) * amount) as u8;
    let b = (color.b() as f32 + (255.0 - color.b() as f32) * amount) as u8;
    Color32::from_rgb(r, g, b)
}

fn darken_color(color: Color32, amount: f32) -> Color32 {
    let r = (color.r() as f32 * (1.0 - amount)) as u8;
    let g = (color.g() as f32 * (1.0 - amount)) as u8;
    let b = (color.b() as f32 * (1.0 - amount)) as u8;
    Color32::from_rgb(r, g, b)
}

// Tab bar constants
const TAB_HEIGHT: f32 = 32.0;
const TAB_MIN_WIDTH: f32 = 100.0;
const TAB_MAX_WIDTH: f32 = 200.0;
const TAB_PADDING: f32 = 8.0;
const CLOSE_BUTTON_SIZE: f32 = 16.0;
const STATUS_INDICATOR_SIZE: f32 = 8.0;
const TAB_SPACING: f32 = 2.0;
const INACTIVE_TAB_ALPHA: u8 = 80;
const ACTIVE_BRIGHTEN_AMOUNT: f32 = 0.3;
const ACTIVE_DARKEN_AMOUNT: f32 = 0.3;
const DROP_INDICATOR_WIDTH: f32 = 3.0;

#[derive(Clone)]
pub enum TabAction {
    Select(Uuid),
    Close(Uuid),
    Reconnect(Uuid),
    EditSettings(Uuid),
    Reorder { dragged_id: Uuid, target_index: usize },
    None,
}

pub struct TabBar {
    hovered_close: Option<Uuid>,
    dragging: Option<Uuid>,
    drop_target_index: Option<usize>,
}

impl Default for TabBar {
    fn default() -> Self {
        Self::new()
    }
}

impl TabBar {
    pub fn new() -> Self {
        Self {
            hovered_close: None,
            dragging: None,
            drop_target_index: None,
        }
    }

    pub fn show_with_data(
        &mut self,
        ui: &mut Ui,
        sessions: &[(Uuid, String, ConnectionState, Color32)],
        active_id: Option<Uuid>,
    ) -> TabAction {
        let mut action = TabAction::None;
        let mut tab_rects: Vec<(Uuid, egui::Rect)> = Vec::new();
        ui.horizontal(|ui| {
            ui.set_height(TAB_HEIGHT);
            ui.spacing_mut().item_spacing.x = TAB_SPACING;
            for (index, (id, title, state, accent)) in sessions.iter().enumerate() {
                let is_active = active_id == Some(*id);
                let (tab_action, rect) = self.show_tab(ui, *id, title, state, *accent, is_active, index);
                tab_rects.push((*id, rect));
                match tab_action {
                    TabAction::None => {}
                    other => action = other,
                }
            }
        });
        // Handle drag state and calculate drop target
        if self.dragging.is_some() {
            if let Some(pointer_pos) = ui.ctx().pointer_interact_pos() {
                // Find which tab the pointer is over
                let mut new_drop_index = None;
                for (index, (_id, rect)) in tab_rects.iter().enumerate() {
                    if pointer_pos.y >= rect.min.y && pointer_pos.y <= rect.max.y {
                        let mid_x = rect.center().x;
                        if pointer_pos.x < mid_x {
                            new_drop_index = Some(index);
                            break;
                        } else if pointer_pos.x >= mid_x {
                            new_drop_index = Some(index + 1);
                        }
                    }
                }
                self.drop_target_index = new_drop_index;
            }
            // Draw drop indicator
            if let Some(drop_index) = self.drop_target_index {
                let indicator_x = if drop_index < tab_rects.len() {
                    tab_rects[drop_index].1.min.x - TAB_SPACING / 2.0
                } else if !tab_rects.is_empty() {
                    tab_rects.last().unwrap().1.max.x + TAB_SPACING / 2.0
                } else {
                    0.0
                };
                if !tab_rects.is_empty() {
                    let indicator_rect = egui::Rect::from_min_size(
                        egui::pos2(indicator_x - DROP_INDICATOR_WIDTH / 2.0, tab_rects[0].1.min.y),
                        Vec2::new(DROP_INDICATOR_WIDTH, TAB_HEIGHT),
                    );
                    ui.painter().rect_filled(
                        indicator_rect,
                        2,
                        ui.visuals().selection.bg_fill,
                    );
                }
            }
            // Check if drag ended
            if !ui.ctx().input(|i| i.pointer.any_down()) {
                if let (Some(dragged_id), Some(target_index)) = (self.dragging, self.drop_target_index) {
                    // Find the current index of the dragged tab
                    if let Some(current_index) = tab_rects.iter().position(|(id, _)| *id == dragged_id) {
                        // Only reorder if actually moving to a different position
                        if target_index != current_index && target_index != current_index + 1 {
                            action = TabAction::Reorder { dragged_id, target_index };
                        }
                    }
                }
                self.dragging = None;
                self.drop_target_index = None;
            }
        }
        action
    }

    fn show_tab(
        &mut self,
        ui: &mut Ui,
        id: Uuid,
        title: &str,
        state: &ConnectionState,
        accent: Color32,
        is_active: bool,
        _index: usize,
    ) -> (TabAction, egui::Rect) {
        let mut action = TabAction::None;
        let display_title = self.truncate_title(title, TAB_MAX_WIDTH - TAB_PADDING * 2.0 - CLOSE_BUTTON_SIZE, ui);
        let status_color = match state {
            ConnectionState::Connected => Color32::from_rgb(0, 200, 83),
            ConnectionState::Connecting => Color32::from_rgb(255, 193, 7),
            ConnectionState::Disconnected => Color32::from_rgb(158, 158, 158),
            ConnectionState::Error(_) => Color32::from_rgb(244, 67, 54),
        };
        let is_dark_mode = ui.visuals().dark_mode;
        let is_being_dragged = self.dragging == Some(id);
        let bg_color = if is_being_dragged {
            // Dragged tab: more transparent
            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 40)
        } else if is_active {
            // Active tab: solid accent color, brightened/darkened based on theme
            if is_dark_mode {
                brighten_color(accent, ACTIVE_BRIGHTEN_AMOUNT)
            } else {
                darken_color(accent, ACTIVE_DARKEN_AMOUNT)
            }
        } else {
            // Inactive tabs: accent color with transparency
            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), INACTIVE_TAB_ALPHA)
        };
        let text_color = if is_active {
            // Contrast text for active tab
            if is_dark_mode {
                Color32::WHITE
            } else {
                Color32::BLACK
            }
        } else {
            ui.visuals().widgets.inactive.text_color()
        };
        let desired_width = (ui.fonts_mut(|f| f.glyph_width(&egui::FontId::default(), 'M')) * display_title.len() as f32
            + TAB_PADDING * 2.0
            + STATUS_INDICATOR_SIZE
            + CLOSE_BUTTON_SIZE
            + TAB_SPACING * 2.0)
            .clamp(TAB_MIN_WIDTH, TAB_MAX_WIDTH);
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(desired_width, TAB_HEIGHT),
            egui::Sense::click_and_drag(),
        );
        // Handle drag start
        if response.drag_started() {
            self.dragging = Some(id);
        }
        if response.clicked() {
            action = TabAction::Select(id);
        }
        // Switch to tab on right-click as well
        if response.secondary_clicked() {
            action = TabAction::Select(id);
        }
        // Context menu on right-click (only if not dragging)
        if self.dragging.is_none() {
            response.context_menu(|ui| {
                if ui.button("Reconnect").clicked() {
                    action = TabAction::Reconnect(id);
                    ui.close();
                }
                if ui.button("Edit Settings...").clicked() {
                    action = TabAction::EditSettings(id);
                    ui.close();
                }
                ui.separator();
                if ui.button("Close").clicked() {
                    action = TabAction::Close(id);
                    ui.close();
                }
            });
        }
        let painter = ui.painter();
        // Round only top corners for proper tab appearance
        painter.rect_filled(rect, egui::CornerRadius {
            nw: 6,
            ne: 6,
            sw: 0,
            se: 0,
        }, bg_color);
        if is_active {
            // Accent border at top of active tab
            let accent_rect = egui::Rect::from_min_size(
                rect.min,
                Vec2::new(rect.width(), 3.0),
            );
            painter.rect_filled(accent_rect, egui::CornerRadius {
                nw: 6,
                ne: 6,
                sw: 0,
                se: 0,
            }, accent);
        }
        let status_center = egui::pos2(
            rect.min.x + TAB_PADDING + STATUS_INDICATOR_SIZE / 2.0,
            rect.center().y,
        );
        painter.circle_filled(status_center, STATUS_INDICATOR_SIZE / 2.0, status_color);
        let text_pos = egui::pos2(
            status_center.x + STATUS_INDICATOR_SIZE / 2.0 + TAB_SPACING * 2.0,
            rect.center().y,
        );
        painter.text(
            text_pos,
            egui::Align2::LEFT_CENTER,
            &display_title,
            egui::FontId::default(),
            text_color,
        );
        let close_rect = egui::Rect::from_center_size(
            egui::pos2(rect.max.x - TAB_PADDING - CLOSE_BUTTON_SIZE / 2.0, rect.center().y),
            Vec2::splat(CLOSE_BUTTON_SIZE),
        );
        let close_response = ui.interact(close_rect, ui.id().with(("close", id)), egui::Sense::click());
        let close_hovered = close_response.hovered();
        if close_hovered {
            self.hovered_close = Some(id);
        } else if self.hovered_close == Some(id) {
            self.hovered_close = None;
        }
        let close_color = if close_hovered {
            Color32::from_rgb(244, 67, 54)
        } else {
            text_color.gamma_multiply(0.6)
        };
        painter.text(
            close_rect.center(),
            egui::Align2::CENTER_CENTER,
            "×",
            egui::FontId::new(14.0, egui::FontFamily::Proportional),
            close_color,
        );
        if close_response.clicked() {
            action = TabAction::Close(id);
        }
        (action, rect)
    }

    fn truncate_title(&self, title: &str, max_width: f32, ui: &Ui) -> String {
        let char_width = ui.fonts_mut(|f| f.glyph_width(&egui::FontId::default(), 'M'));
        let max_chars = (max_width / char_width).floor() as usize;
        if title.len() <= max_chars {
            title.to_string()
        } else if max_chars > 3 {
            format!("{}...", &title[..max_chars - 3])
        } else {
            title.chars().take(max_chars).collect()
        }
    }
}
