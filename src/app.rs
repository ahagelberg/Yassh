use crate::config::{AppConfig, BellNotification, SessionFolder, Theme};
use crate::config_dialog::{ConfigDialog, DialogMode, DialogResult};
use crate::debug;
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

// Global flag to indicate when Tab interception should be active
static TAB_INTERCEPTION_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// Global queue for intercepted Tab events
static INTERCEPTED_TAB_EVENTS: std::sync::Mutex<Vec<(egui::Modifiers, egui::Key)>> = std::sync::Mutex::new(Vec::new());
use std::sync::Arc;
use font_kit::source::SystemSource;
use font_kit::properties::{Properties, Style, Weight};
use font_kit::family_name::FamilyName;

// App layout constants
const MIN_SIDEBAR_WIDTH: f32 = 170.0;
const MAX_SIDEBAR_WIDTH: f32 = 400.0;
const DEFAULT_SIDEBAR_WIDTH: f32 = 130.0;

// Windows API constants for bell sound
#[cfg(windows)]
const MB_ICONASTERISK: u32 = 0x00000040;

// Bell notification constants
const BELL_BLINK_DURATION_MS: u64 = 100;


// Welcome screen spacing
const WELCOME_SCREEN_TOP_MARGIN: f32 = 100.0;
const WELCOME_SCREEN_ELEMENT_SPACING: f32 = 20.0;

// Plugin to intercept Tab key events before egui processes them
pub struct TabInterceptionPlugin;

impl egui::Plugin for TabInterceptionPlugin {
    fn debug_name(&self) -> &'static str {
        "TabInterceptionPlugin"
    }

    fn input_hook(&mut self, input: &mut egui::RawInput) {
        // Only intercept Tab events if the flag is set (when there's an active session)
        if TAB_INTERCEPTION_ACTIVE.load(std::sync::atomic::Ordering::Relaxed) {
            // Filter out ALL Tab key events (including Ctrl+Tab) from the raw input
            // This prevents egui's default focus navigation behavior for all Tab variants
            input.events.retain(|event| {
                if let egui::Event::Key { key, modifiers, pressed: true, .. } = event {
                    if *key == egui::Key::Tab {
                        // Store ALL Tab events (plain, Shift+Tab, Ctrl+Tab, Ctrl+Shift+Tab) for app processing
                        // Remove from egui's event stream to prevent focus navigation
                        if let Ok(mut queue) = INTERCEPTED_TAB_EVENTS.lock() {
                            queue.push((*modifiers, *key));
                        }
                        return false; // Remove from egui's event stream to prevent focus navigation
                    }
                }
                true
            });
        }
    }
}

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
    bell_blink_timer: Option<(std::time::Instant, BellNotification)>,
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

const SYMBOL_FONT_NAME: &str = "symbol_fallback";

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
    // Add Segoe UI Symbol as fallback for geometric shapes (arrows, etc.)
    if let Some(symbol_data) = load_system_font("Segoe UI Symbol") {
        fonts.font_data.insert(
            SYMBOL_FONT_NAME.to_owned(),
            Arc::new(FontData::from_owned(symbol_data)),
        );
        // Add as fallback for proportional (UI) font
        fonts.families
            .entry(FontFamily::Proportional)
            .or_default()
            .push(SYMBOL_FONT_NAME.to_owned());
        log::info!("Added Segoe UI Symbol as fallback for UI symbols");
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

        // Register Tab interception plugin
        cc.egui_ctx.add_plugin(TabInterceptionPlugin);

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
            bell_blink_timer: None,
        };
        // Restore open sessions
        if let Ok(open_ids) = load_open_sessions() {
            debug::log(&format!("[DEBUG APP] Restoring {} open sessions: {:?}", open_ids.len(), open_ids));
            for id in open_ids {
                debug::log(&format!("[DEBUG APP] Restoring session {}", id));
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

    fn close_active_session(&mut self) {
        if let Some(session) = self.session_manager.active_session() {
            let session_id = session.id;
            self.session_manager.close_session(session_id);
            self.selection_managers.remove(&session_id);
        }
    }

    fn close_session_by_id(&mut self, id: Uuid) {
        self.session_manager.close_session(id);
        self.selection_managers.remove(&id);
    }

    fn handle_app_shortcut(&mut self, key: egui::Key, modifiers: egui::Modifiers) -> bool {
        // Returns true if the shortcut was handled, false otherwise
        if modifiers.ctrl && !modifiers.shift && !modifiers.alt && key == egui::Key::W {
            self.close_active_session();
            return true;
        }
        if modifiers.ctrl && modifiers.alt && !modifiers.shift && key == egui::Key::C {
            self.copy_selection();
            if let Some(session) = self.session_manager.active_session() {
                if let Some(sel_mgr) = self.selection_managers.get_mut(&session.id) {
                    sel_mgr.clear();
                }
            }
            return true;
        }
        if modifiers.ctrl && modifiers.alt && !modifiers.shift && key == egui::Key::V {
            self.paste();
            return true;
        }
        if modifiers.ctrl && !modifiers.alt && !modifiers.shift && key == egui::Key::Insert {
            self.copy_selection();
            if let Some(session) = self.session_manager.active_session() {
                if let Some(sel_mgr) = self.selection_managers.get_mut(&session.id) {
                    sel_mgr.clear();
                }
            }
            return true;
        }
        if !modifiers.ctrl && !modifiers.alt && modifiers.shift && key == egui::Key::Insert {
            self.paste();
            return true;
        }
        false
    }

    fn handle_tab_action(&mut self, action: TabAction) {
        match action {
            TabAction::Select(id) => {
                self.session_manager.set_active(id);
            }
            TabAction::Close(id) => {
                self.close_session_by_id(id);
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
        let Ok(mut text) = clipboard.get_text() else {
            return;
        };
        let Some(session) = self.session_manager.active_session() else {
            return;
        };
        let session_id = session.id;
        // Clear selection BEFORE pasting to prevent pasted text from being highlighted
        // Also finish any active selection to cancel ongoing drags
        if let Some(sel_mgr) = self.selection_managers.get_mut(&session_id) {
            // Finish any active selection first to cancel drag
            if sel_mgr.is_selecting() {
                sel_mgr.finish();
            }
            // Then clear it
            sel_mgr.clear();
        }
        // Normalize clipboard text to \n first, then session.send() will convert to configured format
        // This prevents double conversion: clipboard \r\n -> normalize to \n -> convert to configured format
        text = text.replace("\r\n", "\n").replace('\r', "\n");
        let bracketed = session.emulator.bracketed_paste();
        let data = if bracketed {
            format!("\x1b[200~{}\x1b[201~", text)
        } else {
            text
        };
        session.send(data.as_bytes());
        // Clear selection again after sending - ensure it stays cleared
        if let Some(sel_mgr) = self.selection_managers.get_mut(&session_id) {
            sel_mgr.clear();
        }
    }

    fn handle_bell(&mut self, notification: BellNotification) {
        match notification {
            BellNotification::Sound => {
                #[cfg(windows)]
                {
                    #[link(name = "user32")]
                    extern "system" {
                        fn MessageBeep(uType: u32) -> i32;
                    }
                    // Use Windows standard notification sound
                    unsafe { MessageBeep(MB_ICONASTERISK) };
                }
            }
            BellNotification::BlinkScreen | BellNotification::BlinkLine => {
                // Start a blink timer for visual feedback
                self.bell_blink_timer = Some((std::time::Instant::now(), notification));
            }
            BellNotification::None => {}
        }
    }


    fn show_menu_bar(&mut self, ctx: &Context) {
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
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
                    if ui.button("Preferences...").clicked() {
                        self.options_dialog.open(self.app_config.clone());
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
                        self.close_active_session();
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
        // Collect text and key events to forward to terminal
        // Use Text events for character input (respects keyboard layout)
        // Use Key events for special keys (arrows, function keys, etc.)
        let mut text_events: Vec<String> = Vec::new();
        let mut key_events: Vec<(egui::Key, egui::Modifiers)> = Vec::new();
        let mut send_ctrl_c = false;
        let mut send_ctrl_x = false;
        let mut app_shortcuts: Vec<(egui::Key, egui::Modifiers)> = Vec::new();
        ctx.input_mut(|i| {
            // Note: Tab events are now intercepted early in update() before UI processing
            // No need to filter them here again
            // THEN: Process remaining events
            i.events.retain(|event| {
                match event {
                    // Handle Key events FIRST to catch app shortcuts before they become Copy/Paste
                    egui::Event::Key { key, pressed: true, modifiers, .. } => {
                        // Tab is already intercepted by the plugin
                        if *key == egui::Key::Tab {
                            return false; // Should already be intercepted, but be safe
                        }
                        if has_active_session {
                            // Check for app shortcuts - these should NOT be forwarded to server
                            // Handle them exactly like Ctrl+W
                            if (modifiers.ctrl && !modifiers.shift && !modifiers.alt && *key == egui::Key::W) ||
                               (modifiers.ctrl && modifiers.alt && !modifiers.shift && (*key == egui::Key::C || *key == egui::Key::V)) ||
                               (modifiers.ctrl && !modifiers.alt && !modifiers.shift && *key == egui::Key::Insert) ||
                               (!modifiers.ctrl && !modifiers.alt && modifiers.shift && *key == egui::Key::Insert) {
                                // Collect app shortcuts to handle after closure
                                app_shortcuts.push((*key, *modifiers));
                                return false; // Consume the event, don't forward - this prevents Copy/Paste events from being generated
                            }
                            // Check if Alt is currently held - if so, ignore character keys without Alt modifier
                            // This prevents Alt+W from also sending plain W
                            let alt_held = i.modifiers.alt;
                            if alt_held && !modifiers.alt && self.is_character_key(*key) {
                                // Alt is held but this key event doesn't have Alt - ignore it to prevent duplication
                                return false; // Consume the event
                            }
                            // Don't forward character keys without modifiers - they come through Text events
                            // Only forward special keys (arrows, function keys, etc.) or keys with modifiers
                            if modifiers.ctrl || modifiers.alt || !self.is_character_key(*key) {
                                // Special key or key with modifier - forward to terminal
                                key_events.push((*key, *modifiers));
                            }
                            // Character keys without modifiers will come through Text events, so we ignore them here
                            return false; // Consume the event so egui doesn't process it
                        }
                        // Let egui handle keys when there's no active session
                        true
                    }
                    // Handle Copy/Paste events - egui converts some key combinations to these
                    egui::Event::Copy => {
                        if has_active_session {
                            let current_modifiers = i.modifiers;
                            // Ctrl+Alt+C (app shortcut) - Alt modifier indicates app shortcut
                            if current_modifiers.alt {
                                app_shortcuts.push((egui::Key::C, current_modifiers));
                                return false; // Consume, don't forward
                            }
                            // Regular Ctrl+C - forward to terminal
                            // Note: Ctrl+Insert is handled in Key events, not here
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
                        if has_active_session {
                            let current_modifiers = i.modifiers;
                            // Ctrl+Alt+V (app shortcut) - Alt modifier indicates app shortcut
                            if current_modifiers.alt {
                                app_shortcuts.push((egui::Key::V, current_modifiers));
                                return false; // Consume, don't forward
                            }
                            // Shift+Insert (app shortcut) - Shift without Ctrl indicates Shift+Insert
                            if current_modifiers.shift && !current_modifiers.ctrl {
                                app_shortcuts.push((egui::Key::Insert, current_modifiers));
                                return false; // Consume, don't forward
                            }
                            // Regular Ctrl+V - forward to terminal
                            key_events.push((egui::Key::V, egui::Modifiers::CTRL));
                            return false;
                        }
                        true
                    }
                    egui::Event::Text(text) => {
                        // Text events respect keyboard layout - use these for character input
                        // But don't process Text events when Alt is held - those come through Key events
                        if has_active_session {
                            let current_modifiers = i.modifiers;
                            // Only process Text events if Alt is not held
                            // Alt+key combinations should only come through Key events to avoid duplication
                            if !current_modifiers.alt {
                                text_events.push(text.clone());
                            }
                            return false; // Consume the event
                        }
                        true
                    }
                    _ => true // Keep other events
                }
            });
        });
        
        // Handle app shortcuts (these are NOT forwarded to server)
        for (key, modifiers) in app_shortcuts {
            self.handle_app_shortcut(key, modifiers);
        }
        
        // Handle text input (respects keyboard layout)
        for text in text_events {
            if let Some(session) = self.session_manager.active_session_mut() {
                // Convert text to bytes using UTF-8 encoding
                session.send(text.as_bytes());
                if session.config.reset_scroll_on_input {
                    session.reset_scroll_to_bottom();
                }
                let session_id = session.id;
                if let Some(sel_mgr) = self.selection_managers.get_mut(&session_id) {
                    sel_mgr.clear();
                }
            }
        }
        // Handle Ctrl+C and Ctrl+X (intercepted from Copy/Cut events)
        if send_ctrl_c {
            if let Some(session) = self.session_manager.active_session_mut() {
                session.send(&[0x03]); // Ctrl+C = ETX (End of Text)
                if session.config.reset_scroll_on_input {
                    session.reset_scroll_to_bottom();
                }
                let session_id = session.id;
                if let Some(sel_mgr) = self.selection_managers.get_mut(&session_id) {
                    sel_mgr.clear();
                }
            }
        }
        if send_ctrl_x {
            if let Some(session) = self.session_manager.active_session_mut() {
                session.send(&[0x18]); // Ctrl+X = CAN (Cancel)
                if session.config.reset_scroll_on_input {
                    session.reset_scroll_to_bottom();
                }
                let session_id = session.id;
                if let Some(sel_mgr) = self.selection_managers.get_mut(&session_id) {
                    sel_mgr.clear();
                }
            }
        }
        // Process all key events - forward to terminal
        for (key, modifiers) in key_events {
            // Forward to terminal
            if let Some(session) = self.session_manager.active_session_mut() {
                let backspace_seq = session.backspace_sequence().to_vec();
                if let InputResult::Forward(data) = self.input_handler.handle_key(key, modifiers, &backspace_seq) {
                    session.send(&data);
                    if session.config.reset_scroll_on_input {
                        session.reset_scroll_to_bottom();
                    }
                    let session_id = session.id;
                    if let Some(sel_mgr) = self.selection_managers.get_mut(&session_id) {
                        sel_mgr.clear();
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
        // Update Tab interception flag based on active session state
        let has_active_session = self.session_manager.active_session().is_some();
        TAB_INTERCEPTION_ACTIVE.store(has_active_session, std::sync::atomic::Ordering::Relaxed);

        // Process any Tab events that were intercepted by the plugin
        if let Ok(mut queue) = INTERCEPTED_TAB_EVENTS.lock() {
            for (modifiers, key) in queue.drain(..) {
                // Handle Ctrl+Tab combinations for terminal switching (don't forward to server)
                if modifiers.ctrl && !modifiers.alt {
                    if modifiers.shift {
                        // Ctrl+Shift+Tab: Previous tab
                        self.session_manager.prev_tab();
                    } else {
                        // Ctrl+Tab: Next tab
                        self.session_manager.next_tab();
                    }
                    // Don't forward Ctrl+Tab combinations to terminal
                    continue;
                }
                // Forward plain Tab and Shift+Tab to terminal
                if let Some(session) = self.session_manager.active_session_mut() {
                    let backspace_seq = session.backspace_sequence().to_vec();
                    if let InputResult::Forward(data) = self.input_handler.handle_key(key, modifiers, &backspace_seq) {
                        session.send(&data);
                        if session.config.reset_scroll_on_input {
                            session.reset_scroll_to_bottom();
                        }
                        let session_id = session.id;
                        if let Some(sel_mgr) = self.selection_managers.get_mut(&session_id) {
                            sel_mgr.clear();
                        }
                    }
                }
            }
        }

        // Process keyboard input FIRST before any UI to prevent egui from consuming events
        // This MUST be called before ANY UI widgets are shown
        self.process_keyboard_input(ctx);
        // Apply theme only once on first frame
        if !self.theme_applied {
            Self::apply_theme(ctx, self.app_config.theme);
            self.theme_applied = true;
            debug::log("[INIT] Theme applied on first frame");
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
            self.close_session_by_id(session_id);
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
                debug::log(&format!(
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
            let mut should_copy_on_click = false;
            let mut should_paste_on_click = false;
            let mut copy_session_id = None;
            
            if let Some(session) = self.session_manager.active_session_mut() {
                let session_id = session.id;
                let bg_color = session.config.background();
                // Update input handler cursor keys mode
                self.input_handler.set_cursor_keys_application(
                    session.emulator.cursor_keys_application()
                );
                // Get or create selection manager
                let sel_mgr = self.selection_managers
                    .entry(session_id)
                    .or_insert_with(SelectionManager::new);
                // Determine if we should invert colors for bell feedback
                let invert_colors = if let Some((start_time, bell_type)) = &self.bell_blink_timer {
                    let elapsed = start_time.elapsed();
                    let blink_duration = std::time::Duration::from_millis(BELL_BLINK_DURATION_MS);
                    elapsed < blink_duration && matches!(bell_type, BellNotification::BlinkScreen)
                } else {
                    false
                };

                // Calculate viewport size and handle resize
                let available = ui.available_size();
                let cell_width = session.renderer.cell_width();
                let cell_height = session.renderer.cell_height();
                let viewport_cols = (available.x / cell_width).floor() as usize;
                let viewport_rows = (available.y / cell_height).floor() as usize;
                session.check_and_handle_resize(viewport_cols, viewport_rows, true);
                // Render terminal
                let current_scroll_offset = session.scroll_offset();
                let (response, new_scroll_offset, is_at_bottom, _viewport_cols, _viewport_rows) = session.renderer.render(
                    ui,
                    &session.emulator,
                    sel_mgr.selection(),
                    bg_color,
                    !dialogs_visible,
                    invert_colors,
                    current_scroll_offset,
                );
                session.set_scroll_offset_with_bottom(new_scroll_offset, is_at_bottom);
                // Handle bell visual feedback
                if let Some((start_time, bell_type)) = &self.bell_blink_timer {
                    let elapsed = start_time.elapsed();
                    let blink_duration = std::time::Duration::from_millis(BELL_BLINK_DURATION_MS);

                    if elapsed < blink_duration {
                        match *bell_type {
                            BellNotification::BlinkScreen => {
                                // Entire screen is already inverted by the render() call above
                            }
                            BellNotification::BlinkLine => {
                                // Render inverted cursor line on top of normal rendering
                                let cursor_pos = session.emulator.buffer().cursor();
                                let line_idx = session.emulator.buffer().scrollback_len() + cursor_pos.row;

                                // Create a temporary painter for just this line
                                let line_y = response.rect.min.y + cursor_pos.row as f32 * session.renderer.cell_height();
                                let line_rect = egui::Rect::from_min_size(
                                    egui::pos2(response.rect.min.x, line_y),
                                    egui::vec2(response.rect.width(), session.renderer.cell_height()),
                                );

                                // Render the inverted line on top
                                let painter = ui.painter_at(line_rect);
                                session.renderer.render_line_inverted(
                                    &painter,
                                    session.emulator.buffer(),
                                    line_idx,
                                    0, // screen_row is 0 since we're rendering just this line
                                    response.rect.min + egui::vec2(0.0, cursor_pos.row as f32 * session.renderer.cell_height()),
                                    sel_mgr.selection(),
                                    session.emulator.reverse_video(),
                                );
                            }
                            BellNotification::Sound | BellNotification::None => {
                                // No visual feedback needed
                            }
                        }
                    } else {
                        // Blink duration expired, clear the timer
                        self.bell_blink_timer = None;
                    }
                }

                // Handle left-click: copy selection if present, otherwise request focus
                if !dialogs_visible && response.clicked() && sel_mgr.selection().is_some() {
                    should_copy_on_click = true;
                    copy_session_id = Some(session_id);
                } else if !dialogs_visible && response.clicked() {
                    // No selection, just focus the terminal
                    ui.memory_mut(|m| m.request_focus(self.terminal_focus_id));
                }
                
                // Handle right-click: paste from clipboard
                if !dialogs_visible && response.secondary_clicked() {
                    should_paste_on_click = true;
                }
                // Handle mouse input for selection
                // Only process drag events if they're from the primary button (left click)
                // Right-click drags should not create selections
                let is_primary_drag = ui.input(|i| {
                    i.pointer.primary_down() && !i.pointer.secondary_down()
                });
                
                if response.drag_started() && sel_mgr.selection().is_none() && is_primary_drag {
                    if let Some(pos) = response.interact_pointer_pos() {
                        if let Some((line, col)) = session.renderer.cell_at_pos(
                            pos,
                            response.rect.min,
                            session.emulator.buffer(),
                            response.rect.height(),
                            session.scroll_offset(),
                        ) {
                            sel_mgr.start(line, col);
                        }
                    }
                }
                if response.dragged() && is_primary_drag {
                    if let Some(pos) = ui.ctx().pointer_latest_pos() {
                        if let Some((line, col)) = session.renderer.cell_at_pos(
                            pos,
                            response.rect.min,
                            session.emulator.buffer(),
                            response.rect.height(),
                            session.scroll_offset(),
                        ) {
                            sel_mgr.update(line, col);
                        }
                    }
                }
                if response.drag_stopped() {
                    sel_mgr.finish();
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
                        ui.add_space(WELCOME_SCREEN_TOP_MARGIN);
                        ui.heading("Welcome to Yassh");
                        ui.add_space(WELCOME_SCREEN_ELEMENT_SPACING);
                        ui.label("Select a session from the sidebar or create a new one");
                        ui.add_space(WELCOME_SCREEN_ELEMENT_SPACING);
                        if ui.button("➕ New Session").clicked() {
                            self.config_dialog.open_new();
                        }
                        if ui.button("⚡ Quick Connect").clicked() {
                            self.config_dialog.open_quick_connect();
                        }
                    });
                });
            }
            
            // Handle deferred copy/paste operations after session borrow is released
            if should_copy_on_click {
                if let Some(session_id) = copy_session_id {
                    self.copy_selection();
                    if let Some(sel_mgr) = self.selection_managers.get_mut(&session_id) {
                        sel_mgr.clear();
                    }
                }
            }
            if should_paste_on_click {
                self.paste();
                // Clear selection after paste to prevent pasted text from being highlighted
                if let Some(session) = self.session_manager.active_session() {
                    if let Some(sel_mgr) = self.selection_managers.get_mut(&session.id) {
                        sel_mgr.clear();
                    }
                }
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

