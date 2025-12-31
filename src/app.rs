use crate::config::{AppConfig, BellNotification, SessionFolder, Theme};
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

// App layout constants
const MIN_SIDEBAR_WIDTH: f32 = 170.0;
const MAX_SIDEBAR_WIDTH: f32 = 400.0;
const DEFAULT_SIDEBAR_WIDTH: f32 = 130.0;

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
        TopBottomPanel::top("menu_bar").show(ctx, |ui| {
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
        // Collect and consume key events to prevent egui from handling them
        // This ensures Ctrl+C, Alt+key, etc. are forwarded to the terminal
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
                    egui::Event::Key { key, pressed: true, modifiers, .. } => {
                        key_events.push((*key, *modifiers));
                        // Only consume events when there's an active terminal session
                        !has_active_session
                    }
                    _ => true // Keep other events
                }
            });
        });
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
}

impl eframe::App for YasshApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
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
        // Update all sessions - request repaint if there was SSH activity
        let had_activity = self.session_manager.update_all();
        if had_activity {
            ctx.request_repaint();
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
                    .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(4, 2)))
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
