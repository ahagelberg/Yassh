use crate::config::{AppConfig, BellNotification, SessionFolder, Theme, ThemeColors};
use crate::config_dialog::{ConfigDialog, DialogMode, DialogResult};
use crate::input::{InputHandler, InputResult};
use crate::options_dialog::{OptionsDialog, OptionsResult};
use crate::persistence::{
    load_app_config, load_open_sessions, save_app_config, save_open_sessions, PersistenceManager,
};
use crate::selection::SelectionManager;
use crate::session_manager::{SessionManagerAction, SessionManagerUi};
use crate::ssh::manager::SessionManager;
use crate::tabs::{TabAction, TabBar};
use arboard::Clipboard;
use egui::{CentralPanel, Color32, Context, TopBottomPanel, FontDefinitions, FontData, FontFamily};
use uuid::Uuid;
use std::fs::OpenOptions;
use std::io::Write as IoWrite;
use std::sync::Arc;
use font_kit::source::SystemSource;
use font_kit::properties::{Properties, Style, Weight};
use font_kit::family_name::FamilyName;

// App layout constants
const MIN_SIDEBAR_WIDTH: f32 = 170.0;
const MAX_SIDEBAR_WIDTH: f32 = 400.0;
const DEFAULT_SIDEBAR_WIDTH: f32 = 130.0;
const WINDOW_BORDER_WIDTH: f32 = 1.0;

// Title bar constants
const TITLE_BAR_HEIGHT: f32 = 30.0;
const TITLE_BAR_SIDE_MARGIN: f32 = 12.0;
const ICON_SIZE: f32 = 20.0;
const ICON_PADDING: f32 = 8.0;
const BUTTON_WIDTH: f32 = 48.0;
const CLOSE_X_SIZE: f32 = 6.0;
const CLOSE_X_SIZE_HOVER: f32 = 8.0;
const MAX_ICON_SIZE: f32 = 6.0;
const MIN_ICON_SIZE: f32 = 6.0;

pub struct YasshApp {
    app_config: AppConfig,
    persistence: PersistenceManager,
    session_manager: SessionManager,
    session_manager_ui: SessionManagerUi,
    tab_bar: TabBar,
    config_dialog: ConfigDialog,
    options_dialog: OptionsDialog,
    input_handler: InputHandler,
    selection_managers: std::collections::HashMap<Uuid, SelectionManager>,
    clipboard: Option<Clipboard>,
    sidebar_visible: bool,
    folder_rename_dialog: Option<(Uuid, String)>,
    confirm_delete_session: Option<Uuid>,
    confirm_delete_folder: Option<Uuid>,
    terminal_focus_id: egui::Id,
    show_about_dialog: bool,
    theme_applied: bool,
    frame_count: u64,
    last_sidebar_width: f32,
    current_font: String,
}


fn debug_log(msg: &str) {
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("yassh_debug.log")
    {
        let _ = writeln!(file, "{}", msg);
        let _ = file.flush();
    }
}

const TERMINAL_FONT_NAME: &str = "terminal_mono";

fn load_system_font(font_name: &str) -> Option<Vec<u8>> {
    let source = SystemSource::new();
    // Try to find the font by family name
    let handle = source.select_best_match(
        &[FamilyName::Title(font_name.to_string())],
        &Properties::new().weight(Weight::NORMAL).style(Style::Normal),
    ).ok()?;
    let font = handle.load().ok()?;
    font.copy_font_data().map(|arc| (*arc).clone())
}

fn setup_terminal_font(ctx: &Context, font_name: &str) {
    let mut fonts = FontDefinitions::default();
    // Try to load the requested font
    if let Some(font_data) = load_system_font(font_name) {
        fonts.font_data.insert(
            TERMINAL_FONT_NAME.to_owned(),
            Arc::new(FontData::from_owned(font_data)),
        );
        // Add the terminal font as the first priority for monospace
        fonts.families
            .entry(FontFamily::Monospace)
            .or_default()
            .insert(0, TERMINAL_FONT_NAME.to_owned());
        log::info!("Loaded terminal font: {}", font_name);
    } else {
        log::warn!("Could not load font '{}', using default monospace", font_name);
    }
    ctx.set_fonts(fonts);
}


impl YasshApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let app_config = load_app_config();
        // Apply theme immediately on startup
        Self::apply_theme(&cc.egui_ctx, app_config.theme);
        let mut persistence = PersistenceManager::new();
        let _ = persistence.load();
        let clipboard = Clipboard::new().ok();
        // Load default font
        let default_font = String::from("Consolas");
        setup_terminal_font(&cc.egui_ctx, &default_font);
        let mut app = Self {
            app_config,
            persistence,
            session_manager: SessionManager::new(),
            session_manager_ui: SessionManagerUi::new(),
            tab_bar: TabBar::new(),
            config_dialog: ConfigDialog::new(),
            options_dialog: OptionsDialog::new(),
            input_handler: InputHandler::new(),
            selection_managers: std::collections::HashMap::new(),
            clipboard,
            sidebar_visible: true,
            folder_rename_dialog: None,
            confirm_delete_session: None,
            confirm_delete_folder: None,
            terminal_focus_id: egui::Id::new("terminal_input_focus"),
            show_about_dialog: false,
            theme_applied: false,
            frame_count: 0,
            last_sidebar_width: DEFAULT_SIDEBAR_WIDTH,
            current_font: default_font,
        };
        // Restore open sessions
        if let Ok(open_ids) = load_open_sessions() {
            debug_log(&format!("[DEBUG APP] Restoring {} open sessions: {:?}", open_ids.len(), open_ids));
            for id in open_ids {
                debug_log(&format!("[DEBUG APP] Restoring session {}", id));
                if let Some(config) = app.persistence.get_session(id).cloned() {
                    let session_id = app.session_manager.add_session(config);
                    app.session_manager.connect_session(session_id);
                }
            }
        }
        app
    }

    fn apply_theme(ctx: &Context, theme: Theme) {
        let visuals = match theme {
            Theme::Dark => egui::Visuals::dark(),
            Theme::Light => egui::Visuals::light(),
            Theme::DarkBlue => {
                let mut visuals = egui::Visuals::dark();
                visuals.panel_fill = Color32::from_rgb(25, 30, 40);
                visuals.window_fill = Color32::from_rgb(20, 25, 35);
                visuals
            },
            Theme::LightBlue => {
                let mut visuals = egui::Visuals::light();
                visuals.panel_fill = Color32::from_rgb(240, 245, 255);
                visuals.window_fill = Color32::from_rgb(250, 252, 255);
                visuals
            },
            Theme::DarkGreen => {
                let mut visuals = egui::Visuals::dark();
                visuals.panel_fill = Color32::from_rgb(20, 35, 25);
                visuals.window_fill = Color32::from_rgb(15, 30, 20);
                visuals
            },
        };
        ctx.set_visuals(visuals);
    }

    fn draw_modal_overlay(ctx: &Context, id: &str) {
        egui::Area::new(egui::Id::new(id))
            .fixed_pos(egui::pos2(0.0, 0.0))
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                let screen = ctx.content_rect();
                ui.allocate_response(screen.size(), egui::Sense::click());
                ui.painter().rect_filled(
                    screen,
                    0.0,
                    egui::Color32::from_black_alpha(128),
                );
            });
    }

    fn handle_session_manager_action(&mut self, action: SessionManagerAction) {
        match action {
            SessionManagerAction::Connect(id) => {
                if let Some(config) = self.persistence.get_session(id).cloned() {
                    let session_id = self.session_manager.add_session(config);
                    self.session_manager.connect_session(session_id);
                    self.session_manager.set_active(session_id);
                }
            }
            SessionManagerAction::Edit(id) => {
                if let Some(config) = self.persistence.get_session(id).cloned() {
                    self.config_dialog.open_edit(config);
                }
            }
            SessionManagerAction::Duplicate(id) => {
                let _ = self.persistence.duplicate_session(id);
                let _ = self.persistence.save();
            }
            SessionManagerAction::Delete(id) => {
                self.confirm_delete_session = Some(id);
            }
            SessionManagerAction::NewSession => {
                self.config_dialog.open_new();
            }
            SessionManagerAction::NewSessionInFolder(folder_id) => {
                self.config_dialog.open_new_in_folder(folder_id);
            }
            SessionManagerAction::NewFolder => {
                let folder = SessionFolder::new(String::from("New Folder"));
                let folder_id = folder.id;
                self.persistence.add_folder(folder);
                let _ = self.persistence.save();
                self.folder_rename_dialog = Some((folder_id, String::from("New Folder")));
            }
            SessionManagerAction::EditFolder(id) => {
                if let Some(folder) = self.persistence.get_folder(id) {
                    self.folder_rename_dialog = Some((id, folder.name.clone()));
                }
            }
            SessionManagerAction::DeleteFolder(id) => {
                self.confirm_delete_folder = Some(id);
            }
            SessionManagerAction::MoveSession { session_id, folder_id } => {
                self.persistence.move_session_to_folder(session_id, folder_id);
                let _ = self.persistence.save();
            }
            SessionManagerAction::MoveSessionRelative { session_id, target_id, before } => {
                self.persistence.move_session_relative(session_id, target_id, before);
                let _ = self.persistence.save();
            }
            SessionManagerAction::ReorderSession { session_id, target_id, before } => {
                self.persistence.reorder_session(session_id, target_id, before);
                let _ = self.persistence.save();
            }
            SessionManagerAction::MoveFolder { folder_id, parent_id } => {
                self.persistence.move_folder_to_parent(folder_id, parent_id);
                let _ = self.persistence.save();
            }
        }
    }

    fn handle_dialog_result(&mut self, result: DialogResult) {
        match result {
            DialogResult::Confirmed(config) => {
                match self.config_dialog.mode {
                    DialogMode::New => {
                        self.persistence.add_session(config);
                        let _ = self.persistence.save();
                    }
                    DialogMode::Edit(_id) => {
                        self.persistence.update_session(config.clone());
                        let _ = self.persistence.save();
                    }
                    DialogMode::EditConnection(connection_id) => {
                        // Only update the specific open connection, not the stored session
                        if let Some(session) = self.session_manager.get_session_mut(connection_id) {
                            session.update_config(config);
                        }
                    }
                    DialogMode::QuickConnect => {
                        let session_id = self.session_manager.add_session(config);
                        self.session_manager.connect_session(session_id);
                        self.session_manager.set_active(session_id);
                    }
                }
            }
            DialogResult::Cancelled => {}
        }
    }

    fn handle_options_result(&mut self, ctx: &Context, result: OptionsResult) {
        match result {
            OptionsResult::Saved(config) => {
                let theme_changed = self.app_config.theme != config.theme;
                self.app_config = config;
                if theme_changed {
                    Self::apply_theme(ctx, self.app_config.theme);
                }
                let _ = save_app_config(&self.app_config);
            }
            OptionsResult::Cancelled => {}
        }
    }

    fn handle_tab_action(&mut self, action: TabAction) {
        match action {
            TabAction::Select(id) => {
                self.session_manager.set_active(id);
            }
            TabAction::Close(id) => {
                self.session_manager.close_session(id);
                self.selection_managers.remove(&id);
            }
            TabAction::Reconnect(id) => {
                if let Some(session) = self.session_manager.get_session_mut(id) {
                    session.disconnect();
                    session.connect();
                }
            }
            TabAction::EditSettings(id) => {
                if let Some(session) = self.session_manager.get_session(id) {
                    // Edit the connection's runtime settings, not the stored session
                    self.config_dialog.open_edit_connection(id, session.config.clone());
                }
            }
            TabAction::Reorder { dragged_id, target_index } => {
                self.session_manager.reorder_session(dragged_id, target_index);
            }
            TabAction::None => {}
        }
    }

    fn copy_selection(&mut self) {
        let Some(session) = self.session_manager.active_session() else {
            return;
        };
        let session_id = session.id;
        let Some(sel_mgr) = self.selection_managers.get(&session_id) else {
            return;
        };
        if let Some(text) = sel_mgr.get_text(session.emulator.buffer()) {
            if let Some(clipboard) = &mut self.clipboard {
                let _ = clipboard.set_text(&text);
            }
        }
    }

    fn paste(&mut self) {
        let Some(clipboard) = &mut self.clipboard else {
            return;
        };
        let Ok(text) = clipboard.get_text() else {
            return;
        };
        let Some(session) = self.session_manager.active_session() else {
            return;
        };
        let bracketed = session.emulator.bracketed_paste();
        let data = if bracketed {
            format!("\x1b[200~{}\x1b[201~", text)
        } else {
            text
        };
        session.send(data.as_bytes());
    }

    fn handle_bell(&mut self, notification: BellNotification) {
        match notification {
            BellNotification::Sound => {
                #[cfg(windows)]
                {
                    use std::process::Command;
                    let _ = Command::new("powershell")
                        .args(["-c", "[console]::beep(800,200)"])
                        .spawn();
                }
            }
            BellNotification::BlinkScreen | BellNotification::BlinkLine => {}
            BellNotification::None => {}
        }
    }

    fn show_menu_bar(&mut self, ctx: &Context) {
        let theme_colors = ThemeColors::for_theme(self.app_config.theme);
        
        TopBottomPanel::top("menu_bar")
            .frame(egui::Frame::NONE.fill(theme_colors.title_bar_bg))
            .resizable(false)
            .height_range(std::ops::RangeInclusive::new(TITLE_BAR_HEIGHT, TITLE_BAR_HEIGHT))
            .show(ctx, |ui| {
                // Make the title bar draggable
                let title_bar_response = ui.interact(ui.available_rect_before_wrap(), ui.id().with("title_bar"), egui::Sense::drag());
                if title_bar_response.dragged() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                ui.set_height(TITLE_BAR_HEIGHT);
                ui.horizontal(|ui| {
                    // Add left margin manually (not using inner_margin to avoid reducing available width)
                    ui.add_space(TITLE_BAR_SIDE_MARGIN);
                    // Program icon on the left
                    let icon_rect = ui.allocate_rect(
                        egui::Rect::from_min_size(ui.cursor().left_top(), egui::Vec2::new(ICON_SIZE, ICON_SIZE)),
                        egui::Sense::click()
                    );
                    // Draw a simple terminal icon (two horizontal lines representing a terminal window)
                    let painter = ui.painter();
                    let icon_color = theme_colors.title_bar_icon;
                    let icon_center = icon_rect.rect.center();
                    // Draw terminal window frame
                    painter.rect_stroke(
                        icon_rect.rect,
                        0.0,
                        egui::Stroke::new(1.5, icon_color),
                        egui::StrokeKind::Outside,
                    );
                    // Draw terminal prompt lines
                    let line_y1 = icon_center.y - 3.0;
                    let line_y2 = icon_center.y + 3.0;
                    let line_x_start = icon_rect.rect.left() + 3.0;
                    let line_x_end = icon_rect.rect.right() - 3.0;
                    painter.line_segment(
                        [egui::Pos2::new(line_x_start, line_y1), egui::Pos2::new(line_x_end, line_y1)],
                        egui::Stroke::new(1.0, icon_color),
                    );
                    painter.line_segment(
                        [egui::Pos2::new(line_x_start, line_y2), egui::Pos2::new(line_x_end * 0.6, line_y2)],
                        egui::Stroke::new(1.0, icon_color),
                    );
                    ui.add_space(ICON_PADDING);
                    // Menu bar centered vertically with padding
                    ui.vertical(|ui| {
                        ui.add_space(4.0);
                        egui::MenuBar::new().ui(ui, |ui| {
                        // File menu
                        ui.menu_button("File", |ui| {
                            if ui.button("New Session...").clicked() {
                                self.config_dialog.open_new();
                                ui.close();
                            }
                            if ui.button("Quick Connect...").clicked() {
                                self.config_dialog.open_quick_connect();
                                ui.close();
                            }
                            ui.separator();
                            if ui.button("New Folder").clicked() {
                                let folder = SessionFolder::new(String::from("New Folder"));
                                let folder_id = folder.id;
                                self.persistence.add_folder(folder);
                                let _ = self.persistence.save();
                                self.folder_rename_dialog = Some((folder_id, String::from("New Folder")));
                                ui.close();
                            }
                            ui.separator();
                            if ui.button("Exit").clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        });
                        // Edit menu
                        ui.menu_button("Edit", |ui| {
                            let has_active = self.session_manager.active_session().is_some();
                            if ui.add_enabled(has_active, egui::Button::new("Copy")).clicked() {
                                self.copy_selection();
                                ui.close();
                            }
                            if ui.add_enabled(has_active, egui::Button::new("Paste")).clicked() {
                                self.paste();
                                ui.close();
                            }
                        });
                        // View menu
                        ui.menu_button("View", |ui| {
                            let sidebar_text = if self.sidebar_visible { "Hide Sidebar" } else { "Show Sidebar" };
                            if ui.button(sidebar_text).clicked() {
                                self.sidebar_visible = !self.sidebar_visible;
                                ui.close();
                            }
                        });
                        // Session menu
                        ui.menu_button("Session", |ui| {
                            let has_active = self.session_manager.active_session().is_some();
                            if ui.add_enabled(has_active, egui::Button::new("Reconnect")).clicked() {
                                if let Some(session) = self.session_manager.active_session_mut() {
                                    session.disconnect();
                                    session.connect();
                                }
                                ui.close();
                            }
                            if ui.add_enabled(has_active, egui::Button::new("Disconnect")).clicked() {
                                if let Some(session) = self.session_manager.active_session_mut() {
                                    session.disconnect();
                                }
                                ui.close();
                            }
                            ui.separator();
                            if ui.add_enabled(has_active, egui::Button::new("Edit Connection Settings...")).clicked() {
                                if let Some(session) = self.session_manager.active_session() {
                                    // Edit the connection's runtime settings, not the stored session
                                    let id = session.id;
                                    self.config_dialog.open_edit_connection(id, session.config.clone());
                                }
                                ui.close();
                            }
                            ui.separator();
                            if ui.add_enabled(has_active, egui::Button::new("Close Tab")).clicked() {
                                if let Some(session) = self.session_manager.active_session() {
                                    let id = session.id;
                                    self.session_manager.close_session(id);
                                    self.selection_managers.remove(&id);
                                }
                                ui.close();
                            }
                        });
                        // Options menu
                        ui.menu_button("Options", |ui| {
                            if ui.button("Preferences...").clicked() {
                                self.options_dialog.open(self.app_config.clone());
                                ui.close();
                            }
                        });
                        // Help menu
                        ui.menu_button("Help", |ui| {
                            if ui.button("About Yassh").clicked() {
                                self.show_about_dialog = true;
                                ui.close();
                            }
                        });
                        });
                    });
                    // Window controls on the right
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Close button
                        let close_response = ui.add_sized(
                            [BUTTON_WIDTH, TITLE_BAR_HEIGHT],
                            egui::Button::new("").fill(Color32::TRANSPARENT).frame(false)
                        );
                        if close_response.clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        // Draw X - ensure it's centered in the button
                        let button_rect = close_response.rect;
                        let center = button_rect.center();
                        if close_response.hovered() {
                            ui.painter().rect_filled(
                                button_rect,
                                0.0,
                                theme_colors.close_button_hover
                            );
                            // Draw X on hover
                            ui.painter().line_segment(
                                [egui::Pos2::new(center.x - CLOSE_X_SIZE_HOVER, center.y - CLOSE_X_SIZE_HOVER), egui::Pos2::new(center.x + CLOSE_X_SIZE_HOVER, center.y + CLOSE_X_SIZE_HOVER)],
                                egui::Stroke::new(1.5, theme_colors.close_button_hover_text)
                            );
                            ui.painter().line_segment(
                                [egui::Pos2::new(center.x + CLOSE_X_SIZE_HOVER, center.y - CLOSE_X_SIZE_HOVER), egui::Pos2::new(center.x - CLOSE_X_SIZE_HOVER, center.y + CLOSE_X_SIZE_HOVER)],
                                egui::Stroke::new(1.5, theme_colors.close_button_hover_text)
                            );
                        } else {
                            // Draw X normally
                            ui.painter().line_segment(
                                [egui::Pos2::new(center.x - CLOSE_X_SIZE, center.y - CLOSE_X_SIZE), egui::Pos2::new(center.x + CLOSE_X_SIZE, center.y + CLOSE_X_SIZE)],
                                egui::Stroke::new(1.0, theme_colors.title_bar_text)
                            );
                            ui.painter().line_segment(
                                [egui::Pos2::new(center.x + CLOSE_X_SIZE, center.y - CLOSE_X_SIZE), egui::Pos2::new(center.x - CLOSE_X_SIZE, center.y + CLOSE_X_SIZE)],
                                egui::Stroke::new(1.0, theme_colors.title_bar_text)
                            );
                        }
                        // Maximize/Restore button
                        let is_maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                        let max_response = ui.add_sized(
                            [BUTTON_WIDTH, TITLE_BAR_HEIGHT],
                            egui::Button::new("").fill(Color32::TRANSPARENT).frame(false)
                        );
                        if max_response.clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
                        }
                        if max_response.hovered() {
                            ui.painter().rect_filled(
                                max_response.rect,
                                0.0,
                                theme_colors.button_hover
                            );
                        }
                        // Draw maximize/restore icon
                        let center = max_response.rect.center();
                        let icon_color = theme_colors.title_bar_text;
                        if is_maximized {
                            // Restore icon (two overlapping squares)
                            ui.painter().rect_stroke(
                                egui::Rect::from_center_size(center, egui::Vec2::new(MAX_ICON_SIZE * 1.5, MAX_ICON_SIZE * 1.5)),
                                0.0,
                                egui::Stroke::new(1.0, icon_color),
                                egui::StrokeKind::Outside,
                            );
                            ui.painter().rect_stroke(
                                egui::Rect::from_center_size(center + egui::Vec2::new(MAX_ICON_SIZE * 0.5, MAX_ICON_SIZE * 0.5), egui::Vec2::new(MAX_ICON_SIZE * 1.5, MAX_ICON_SIZE * 1.5)),
                                0.0,
                                egui::Stroke::new(1.0, icon_color),
                                egui::StrokeKind::Outside,
                            );
                        } else {
                            // Maximize icon (single square)
                            ui.painter().rect_stroke(
                                egui::Rect::from_center_size(center, egui::Vec2::new(MAX_ICON_SIZE * 1.5, MAX_ICON_SIZE * 1.5)),
                                0.0,
                                egui::Stroke::new(1.0, icon_color),
                                egui::StrokeKind::Outside,
                            );
                        }
                        // Minimize button
                        let min_response = ui.add_sized(
                            [BUTTON_WIDTH, TITLE_BAR_HEIGHT],
                            egui::Button::new("").fill(Color32::TRANSPARENT).frame(false)
                        );
                        if min_response.clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                        if min_response.hovered() {
                            ui.painter().rect_filled(
                                min_response.rect,
                                0.0,
                                theme_colors.button_hover
                            );
                        }
                        // Draw minimize icon (horizontal line)
                        let center = min_response.rect.center();
                        let icon_color = theme_colors.title_bar_text;
                        ui.painter().line_segment(
                            [egui::Pos2::new(center.x - MIN_ICON_SIZE, center.y), egui::Pos2::new(center.x + MIN_ICON_SIZE, center.y)],
                            egui::Stroke::new(1.0, icon_color)
                        );
                    });
                });
            });
    }

    fn show_about_dialog(&mut self, ctx: &Context) {
        if !self.show_about_dialog {
            return;
        }
        // Modal overlay
        Self::draw_modal_overlay(ctx, "about_dialog_overlay");
        egui::Window::new("About Yassh")
            .collapsible(false)
            .resizable(false)
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                // Handle Enter/Escape
                if ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape)) {
                    self.show_about_dialog = false;
                }
                ui.vertical_centered(|ui| {
                    ui.heading("Yassh");
                    ui.add_space(8.0);
                    ui.label("Version 0.1.0");
                    ui.add_space(8.0);
                    ui.label("Yet Another SSH Terminal Emulator");
                    ui.add_space(16.0);
                    if ui.button("OK").clicked() {
                        self.show_about_dialog = false;
                    }
                });
            });
    }

    fn show_delete_confirmation_dialogs(&mut self, ctx: &Context) {
        if let Some(id) = self.confirm_delete_session {
            let session_name = self.persistence.get_session(id)
                .map(|s| s.name.clone())
                .unwrap_or_default();
            // Modal overlay
            Self::draw_modal_overlay(ctx, "delete_session_overlay");
            egui::Window::new("Confirm Delete")
                .collapsible(false)
                .resizable(false)
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    // Handle Enter for confirm, Escape for cancel
                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.persistence.remove_session(id);
                        let _ = self.persistence.save();
                        self.confirm_delete_session = None;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        self.confirm_delete_session = None;
                    }
                    ui.label(format!("Delete session '{}'?", session_name));
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.confirm_delete_session = None;
                        }
                        let delete_btn = ui.button("Delete");
                        delete_btn.request_focus();
                        if delete_btn.clicked() {
                            self.persistence.remove_session(id);
                            let _ = self.persistence.save();
                            self.confirm_delete_session = None;
                        }
                    });
                });
        }
        if let Some(id) = self.confirm_delete_folder {
            let folder_name = self.persistence.get_folder(id)
                .map(|f| f.name.clone())
                .unwrap_or_default();
            // Modal overlay
            Self::draw_modal_overlay(ctx, "delete_folder_overlay");
            egui::Window::new("Confirm Delete")
                .collapsible(false)
                .resizable(false)
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    // Handle Enter for confirm, Escape for cancel
                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.persistence.remove_folder(id);
                        let _ = self.persistence.save();
                        self.confirm_delete_folder = None;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        self.confirm_delete_folder = None;
                    }
                    ui.label(format!("Delete folder '{}' and all its contents?", folder_name));
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.confirm_delete_folder = None;
                        }
                        let delete_btn = ui.button("Delete");
                        delete_btn.request_focus();
                        if delete_btn.clicked() {
                            self.persistence.remove_folder(id);
                            let _ = self.persistence.save();
                            self.confirm_delete_folder = None;
                        }
                    });
                });
        }
    }

    fn show_folder_rename_dialog(&mut self, ctx: &Context) {
        if let Some((id, ref mut name)) = &mut self.folder_rename_dialog {
            let id = *id;
            let mut close = false;
            let mut confirm = false;
            // Modal overlay
            Self::draw_modal_overlay(ctx, "folder_dialog_overlay");
            egui::Window::new("Rename Folder")
                .collapsible(false)
                .resizable(false)
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    // Handle Enter key
                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        confirm = true;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        close = true;
                    }
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(name);
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                        if ui.button("OK").clicked() {
                            confirm = true;
                        }
                    });
                });
            if confirm {
                if let Some(folder) = self.persistence.folders.iter_mut().find(|f| f.id == id) {
                    folder.name = name.clone();
                }
                let _ = self.persistence.save();
                close = true;
            }
            if close {
                self.folder_rename_dialog = None;
            }
        }
    }

    fn any_dialog_visible(&self) -> bool {
        self.config_dialog.is_visible() 
            || self.options_dialog.is_visible() 
            || self.folder_rename_dialog.is_some()
            || self.confirm_delete_session.is_some()
            || self.confirm_delete_folder.is_some()
            || self.show_about_dialog
    }

    fn process_keyboard_input(&mut self, ctx: &Context) {
        // Don't process keyboard when any dialog is visible
        if self.any_dialog_visible() {
            return;
        }
        let has_active_session = self.session_manager.active_session().is_some();
        // Collect text and key events to forward to terminal
        // Use Text events for character input (respects keyboard layout)
        // Use Key events for special keys (arrows, function keys, etc.)
        let mut text_events: Vec<String> = Vec::new();
        let mut key_events: Vec<(egui::Key, egui::Modifiers)> = Vec::new();
        let mut send_ctrl_c = false;
        let mut send_ctrl_x = false;
        ctx.input_mut(|i| {
            i.events.retain(|event| {
                match event {
                    // Intercept Copy/Cut/Paste events - these are Ctrl+C/X/V on Windows
                    egui::Event::Copy => {
                        if has_active_session {
                            send_ctrl_c = true;
                            return false; // Consume the event
                        }
                        true
                    }
                    egui::Event::Cut => {
                        if has_active_session {
                            send_ctrl_x = true;
                            return false; // Consume the event
                        }
                        true
                    }
                    egui::Event::Paste(_) => {
                        // Consume Paste event - Ctrl+V should be forwarded to terminal
                        // User can use Ctrl+Shift+V for app paste
                        if has_active_session {
                            // Send Ctrl+V (0x16) to terminal
                            key_events.push((egui::Key::V, egui::Modifiers::CTRL));
                            return false;
                        }
                        true
                    }
                    egui::Event::Text(text) => {
                        // Text events respect keyboard layout - use these for character input
                        if has_active_session {
                            text_events.push(text.clone());
                            return false; // Consume the event
                        }
                        true
                    }
                    egui::Event::Key { key, pressed: true, modifiers, .. } => {
                        // Collect all keys with Ctrl modifier (Ctrl+character combinations)
                        // Also collect special keys (non-character keys)
                        // Character keys without Ctrl will come through Text events
                        if modifiers.ctrl || !self.is_character_key(*key) {
                            key_events.push((*key, *modifiers));
                        }
                        // Only consume events when there's an active terminal session
                        !has_active_session
                    }
                    _ => true // Keep other events
                }
            });
        });
        // Handle text input (respects keyboard layout)
        for text in text_events {
            if let Some(session) = self.session_manager.active_session() {
                let should_scroll = session.renderer.is_at_bottom(session.emulator.buffer()) 
                    || session.config.reset_scroll_on_input;
                // Convert text to bytes using UTF-8 encoding
                session.send(text.as_bytes());
                let session_id = session.id;
                if let Some(sel_mgr) = self.selection_managers.get_mut(&session_id) {
                    sel_mgr.clear();
                }
                if should_scroll {
                    if let Some(session) = self.session_manager.active_session_mut() {
                        session.renderer.scroll_to_bottom(session.emulator.buffer());
                    }
                }
            }
        }
        // Handle Ctrl+C and Ctrl+X (intercepted from Copy/Cut events)
        if send_ctrl_c {
            if let Some(session) = self.session_manager.active_session() {
                let should_scroll = session.renderer.is_at_bottom(session.emulator.buffer()) 
                    || session.config.reset_scroll_on_input;
                session.send(&[0x03]); // Ctrl+C = ETX (End of Text)
                let session_id = session.id;
                if let Some(sel_mgr) = self.selection_managers.get_mut(&session_id) {
                    sel_mgr.clear();
                }
                if should_scroll {
                    if let Some(session) = self.session_manager.active_session_mut() {
                        session.renderer.scroll_to_bottom(session.emulator.buffer());
                    }
                }
            }
        }
        if send_ctrl_x {
            if let Some(session) = self.session_manager.active_session() {
                let should_scroll = session.renderer.is_at_bottom(session.emulator.buffer()) 
                    || session.config.reset_scroll_on_input;
                session.send(&[0x18]); // Ctrl+X = CAN (Cancel)
                let session_id = session.id;
                if let Some(sel_mgr) = self.selection_managers.get_mut(&session_id) {
                    sel_mgr.clear();
                }
                if should_scroll {
                    if let Some(session) = self.session_manager.active_session_mut() {
                        session.renderer.scroll_to_bottom(session.emulator.buffer());
                    }
                }
            }
        }
        // Process all key events
        for (key, modifiers) in key_events {
            // App shortcuts - checked before forwarding to terminal
            // Ctrl+W: Close current connection
            if modifiers.ctrl && !modifiers.shift && !modifiers.alt && key == egui::Key::W {
                if let Some(session) = self.session_manager.active_session() {
                    let session_id = session.id;
                    self.session_manager.close_session(session_id);
                    self.selection_managers.remove(&session_id);
                }
                continue;
            }
            // Ctrl+Tab: Next tab
            if modifiers.ctrl && !modifiers.shift && !modifiers.alt && key == egui::Key::Tab {
                self.session_manager.next_tab();
                continue;
            }
            // Ctrl+Shift+Tab: Previous tab
            if modifiers.ctrl && modifiers.shift && !modifiers.alt && key == egui::Key::Tab {
                self.session_manager.prev_tab();
                continue;
            }
            // Forward to terminal
            if let Some(session) = self.session_manager.active_session() {
                let backspace_seq = session.backspace_sequence().to_vec();
                if let InputResult::Forward(data) = self.input_handler.handle_key(key, modifiers, &backspace_seq) {
                    // Check scroll settings before sending
                    let should_scroll = session.renderer.is_at_bottom(session.emulator.buffer()) 
                        || session.config.reset_scroll_on_input;
                    session.send(&data);
                    let session_id = session.id;
                    if let Some(sel_mgr) = self.selection_managers.get_mut(&session_id) {
                        sel_mgr.clear();
                    }
                    // Scroll to bottom if needed
                    if should_scroll {
                        if let Some(session) = self.session_manager.active_session_mut() {
                            session.renderer.scroll_to_bottom(session.emulator.buffer());
                        }
                    }
                }
            }
        }
    }

    fn is_character_key(&self, key: egui::Key) -> bool {
        // Check if a key produces character output (should use Text events instead)
        matches!(key,
            egui::Key::A | egui::Key::B | egui::Key::C | egui::Key::D | egui::Key::E | egui::Key::F |
            egui::Key::G | egui::Key::H | egui::Key::I | egui::Key::J | egui::Key::K | egui::Key::L |
            egui::Key::M | egui::Key::N | egui::Key::O | egui::Key::P | egui::Key::Q | egui::Key::R |
            egui::Key::S | egui::Key::T | egui::Key::U | egui::Key::V | egui::Key::W | egui::Key::X |
            egui::Key::Y | egui::Key::Z |
            egui::Key::Num0 | egui::Key::Num1 | egui::Key::Num2 | egui::Key::Num3 | egui::Key::Num4 |
            egui::Key::Num5 | egui::Key::Num6 | egui::Key::Num7 | egui::Key::Num8 | egui::Key::Num9 |
            egui::Key::Space | egui::Key::Minus | egui::Key::Plus | egui::Key::Equals |
            egui::Key::OpenBracket | egui::Key::CloseBracket | egui::Key::Backslash |
            egui::Key::Semicolon | egui::Key::Quote | egui::Key::Comma | egui::Key::Period |
            egui::Key::Slash | egui::Key::Backtick
        )
    }
}

impl eframe::App for YasshApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Draw window border
        egui::Area::new(egui::Id::new("window_border"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(0.0, 0.0))
            .show(ctx, |ui| {
                let screen_rect = ctx.content_rect();
                let border_color = if ctx.style().visuals.dark_mode {
                    Color32::from_rgba_unmultiplied(100, 100, 100, 255)
                } else {
                    Color32::from_rgba_unmultiplied(150, 150, 150, 255)
                };
                ui.painter().rect_stroke(
                    screen_rect,
                    0.0,
                    egui::Stroke::new(WINDOW_BORDER_WIDTH, border_color),
                    egui::StrokeKind::Inside,
                );
            });
        // Process keyboard input FIRST before any UI to prevent egui from consuming events
        self.process_keyboard_input(ctx);
        // Apply theme only once on first frame
        if !self.theme_applied {
            Self::apply_theme(ctx, self.app_config.theme);
            self.theme_applied = true;
            debug_log("[INIT] Theme applied on first frame");
        }
        // Update font if active session's font changed
        if let Some(session) = self.session_manager.active_session() {
            if session.config.font != self.current_font {
                self.current_font = session.config.font.clone();
                setup_terminal_font(ctx, &self.current_font);
            }
        }
        // Update all sessions - request repaint only when data is actually received
        let mut had_activity = self.session_manager.update_all();
        // Process any events that arrived during or after the initial update
        // This ensures we catch events that arrive between frames
        // We limit iterations to avoid blocking the UI thread
        const MAX_EVENT_PROCESSING_ITERATIONS: usize = 10;
        for _ in 0..MAX_EVENT_PROCESSING_ITERATIONS {
            let mut found_events = false;
            for session in self.session_manager.sessions_mut() {
                if let Some(connection) = &session.connection {
                    // Check if there are events waiting
                    if connection.try_recv().is_some() {
                        found_events = true;
                        // Process all available events for this session
                        if session.update() {
                            had_activity = true;
                        }
                        break; // Process one session at a time to be fair
                    }
                }
            }
            if !found_events {
                break; // No more events, we're done
            }
        }
        // Check if there are active connected sessions that might receive data
        let has_active_sessions = self.session_manager.sessions()
            .iter()
            .any(|s| matches!(s.state(), crate::ssh::connection::ConnectionState::Connected));
        
        if had_activity {
            // Request repaint when data is received
            // Use request_repaint_after with minimal delay to ensure event loop wakes up
            // This is necessary because request_repaint() alone may not wake an idle event loop
            ctx.request_repaint_after(std::time::Duration::from_millis(1));
        } else if has_active_sessions {
            // Even if no activity this frame, keep checking for data from active sessions
            // Use a short delay to avoid wasting CPU while still being responsive
            ctx.request_repaint_after(std::time::Duration::from_millis(16)); // ~60 FPS polling
        }
        // Close sessions that had natural disconnects
        let sessions_to_close: Vec<Uuid> = self.session_manager.sessions()
            .iter()
            .filter(|s| s.should_close())
            .map(|s| s.id)
            .collect();
        for session_id in sessions_to_close {
            self.session_manager.close_session(session_id);
            self.selection_managers.remove(&session_id);
        }
        // Handle bells
        let bells = self.session_manager.collect_pending_bells();
        for bell in bells {
            self.handle_bell(bell);
        }
        // Show dialogs
        if let Some(result) = self.config_dialog.show(ctx, &self.persistence) {
            self.handle_dialog_result(result);
        }
        if let Some(result) = self.options_dialog.show(ctx) {
            self.handle_options_result(ctx, result);
        }
        self.show_delete_confirmation_dialogs(ctx);
        self.show_folder_rename_dialog(ctx);
        self.show_about_dialog(ctx);
        // Menu bar
        self.show_menu_bar(ctx);
        // Debug: track frame count
        self.frame_count += 1;
        // Sidebar
        if self.sidebar_visible {
            let panel_response = egui::SidePanel::left("sidebar")
                .resizable(true)
                .default_width(DEFAULT_SIDEBAR_WIDTH)
                .width_range(MIN_SIDEBAR_WIDTH..=MAX_SIDEBAR_WIDTH)
                .show(ctx, |ui| {
                    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                        ui.set_min_width(0.0);
                        ui.label(egui::RichText::new("Sessions").strong());
                        ui.separator();
                        if let Some(action) = self.session_manager_ui.show(ui, &mut self.persistence) {
                            self.handle_session_manager_action(action);
                        }
                    });
                });
            // Debug: log sidebar width changes
            let current_width = panel_response.response.rect.width();
            if (current_width - self.last_sidebar_width).abs() > 0.1 {
                debug_log(&format!(
                    "[RESIZE] Frame {}: sidebar width changed from {:.1} to {:.1}",
                    self.frame_count, self.last_sidebar_width, current_width
                ));
                self.last_sidebar_width = current_width;
            }
        }
        // Main content area
        CentralPanel::default().show(ctx, |ui| {
            // Collect session info for tab bar
            let tab_data: Vec<_> = self.session_manager.sessions().iter()
                .map(|s| (s.id, s.title.clone(), s.state(), s.config.accent()))
                .collect();
            let active_id = self.session_manager.active_session().map(|s| s.id);
            // Tab bar
            if !tab_data.is_empty() {
                TopBottomPanel::top("tabs")
                    .frame(egui::Frame::NONE)
                    .show_inside(ui, |ui| {
                        let action = self.tab_bar.show_with_data(ui, &tab_data, active_id);
                        self.handle_tab_action(action);
                    });
            }
            // Terminal content
            let dialogs_visible = self.any_dialog_visible();
            if let Some(session) = self.session_manager.active_session_mut() {
                let session_id = session.id;
                let bg_color = session.config.background();
                // Update input handler cursor keys mode
                self.input_handler.set_cursor_keys_application(
                    session.emulator.cursor_keys_application()
                );
                // Calculate terminal size based on available space
                let available = ui.available_size();
                let (cols, rows) = session.renderer.calculate_grid_size(available);
                if cols != session.emulator.cols() || rows != session.emulator.rows() {
                    debug_log(&format!(
                        "[TERMINAL] Frame {}: resize from {}x{} to {}x{}, available: {:.1}x{:.1}",
                        self.frame_count,
                        session.emulator.cols(), session.emulator.rows(),
                        cols, rows,
                        available.x, available.y
                    ));
                    session.resize(cols, rows);
                }
                // Get or create selection manager
                let sel_mgr = self.selection_managers
                    .entry(session_id)
                    .or_insert_with(SelectionManager::new);
                // Render terminal
                let response = session.renderer.render(
                    ui,
                    &session.emulator,
                    sel_mgr.selection(),
                    bg_color,
                    !dialogs_visible,
                );
                // Render scrollbar
                session.renderer.render_scrollbar(ui, session.emulator.buffer(), response.rect);
                // Request focus for terminal area when dialog is not visible
                if !dialogs_visible && response.clicked() {
                    ui.memory_mut(|m| m.request_focus(self.terminal_focus_id));
                }
                // Handle mouse input for selection
                if response.drag_started() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        if let Some((line, col)) = session.renderer.cell_at_pos(
                            pos,
                            response.rect.min,
                            session.emulator.buffer(),
                        ) {
                            sel_mgr.start(line, col);
                        }
                    }
                }
                if response.dragged() {
                    if let Some(pos) = ui.ctx().pointer_latest_pos() {
                        if let Some((line, col)) = session.renderer.cell_at_pos(
                            pos,
                            response.rect.min,
                            session.emulator.buffer(),
                        ) {
                            sel_mgr.update(line, col);
                        }
                    }
                }
                if response.drag_stopped() {
                    sel_mgr.finish();
                }
                // Handle scroll
                let scroll_delta = ui.ctx().input(|i| i.smooth_scroll_delta.y);
                if scroll_delta != 0.0 {
                    let lines = (scroll_delta.abs() / 20.0).ceil() as usize;
                    if scroll_delta > 0.0 {
                        session.renderer.scroll_up(lines);
                    } else {
                        session.renderer.scroll_down(lines, session.emulator.buffer());
                    }
                }
                // Show error message if any
                if let Some(error) = &session.error_message {
                    ui.colored_label(Color32::from_rgb(244, 67, 54), format!("Error: {}", error));
                    if ui.button("Reconnect").clicked() {
                        session.connect();
                    }
                }
            } else {
                // No active session - show welcome screen
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(100.0);
                        ui.heading("Welcome to Yassh");
                        ui.add_space(20.0);
                        ui.label("Select a session from the sidebar or create a new one");
                        ui.add_space(20.0);
                        if ui.button("➕ New Session").clicked() {
                            self.config_dialog.open_new();
                        }
                        if ui.button("⚡ Quick Connect").clicked() {
                            self.config_dialog.open_quick_connect();
                        }
                    });
                });
            }
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Save all open sessions - include duplicates for multiple connections to the same server
        let open_ids: Vec<Uuid> = self.session_manager.sessions()
            .iter()
            .map(|s| s.config.id)
            .collect();
        let _ = save_open_sessions(&open_ids);
        let _ = save_app_config(&self.app_config);
    }
}
