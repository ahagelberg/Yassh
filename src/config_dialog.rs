use crate::config::{
    get_available_monospace_fonts, AuthMethod, AutoReconnect, BackspaceKey, BellNotification,
    LineEnding, PortForward, ResizeMethod, SessionConfig, TerminalMode,
};
use crate::persistence::PersistenceManager;
use egui::{Align2, Area, Color32, Order, RichText, Ui, Window};
use uuid::Uuid;

// Dialog constants
const INPUT_WIDTH: f32 = 300.0;
const MIN_FONT_SIZE: u32 = 6;
const MAX_FONT_SIZE: u32 = 72;
const OVERLAY_COLOR: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 180);

#[derive(Debug, Clone, PartialEq)]
pub enum DialogMode {
    New,
    Edit(Uuid),               // Edit stored session
    EditConnection(Uuid),     // Edit open connection (runtime settings only)
    QuickConnect,
}

pub struct ConfigDialog {
    pub visible: bool,
    pub mode: DialogMode,
    pub config: SessionConfig,
    pub password_visible: bool,
    new_local_forward: PortForwardEdit,
    validation_error: Option<String>,
}

#[derive(Default)]
struct PortForwardEdit {
    local_port: String,
    remote_host: String,
    remote_port: String,
}

impl Default for ConfigDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigDialog {
    pub fn new() -> Self {
        Self {
            visible: false,
            mode: DialogMode::New,
            config: SessionConfig::default(),
            password_visible: false,
            new_local_forward: PortForwardEdit::default(),
            validation_error: None,
        }
    }

    pub fn open_new(&mut self) {
        self.mode = DialogMode::New;
        self.config = SessionConfig::default();
        self.password_visible = false;
        self.validation_error = None;
        self.visible = true;
    }

    pub fn open_new_in_folder(&mut self, folder_id: Uuid) {
        self.mode = DialogMode::New;
        self.config = SessionConfig::default();
        self.config.folder_id = Some(folder_id);
        self.password_visible = false;
        self.validation_error = None;
        self.visible = true;
    }

    pub fn open_edit(&mut self, config: SessionConfig) {
        self.mode = DialogMode::Edit(config.id);
        self.config = config;
        self.password_visible = false;
        self.validation_error = None;
        self.visible = true;
    }

    pub fn open_edit_connection(&mut self, connection_id: Uuid, config: SessionConfig) {
        self.mode = DialogMode::EditConnection(connection_id);
        self.config = config;
        self.password_visible = false;
        self.validation_error = None;
        self.visible = true;
    }

    pub fn open_quick_connect(&mut self) {
        self.mode = DialogMode::QuickConnect;
        self.config = SessionConfig::default();
        self.config.name = String::from("Quick Connect");
        self.password_visible = false;
        self.validation_error = None;
        self.visible = true;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn show(&mut self, ctx: &egui::Context, persistence: &PersistenceManager) -> Option<DialogResult> {
        if !self.visible {
            return None;
        }
        let mut result = None;
        // Draw modal overlay to block interaction with background
        Area::new(egui::Id::new("modal_overlay"))
            .order(Order::Middle)
            .anchor(Align2::LEFT_TOP, [0.0, 0.0])
            .show(ctx, |ui| {
                let screen_rect = ctx.content_rect();
                ui.allocate_response(screen_rect.size(), egui::Sense::click());
                ui.painter().rect_filled(screen_rect, 0.0, OVERLAY_COLOR);
            });
        let title = match &self.mode {
            DialogMode::New => "New Session",
            DialogMode::Edit(_) => "Edit Session",
            DialogMode::EditConnection(_) => "Connection Settings",
            DialogMode::QuickConnect => "Quick Connect",
        };
        Window::new(title)
            .collapsible(false)
            .resizable(false)
            .order(Order::Foreground)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .min_width(500.0)
            .show(ctx, |ui| {
                result = self.show_content(ui, persistence);
            });
        result
    }

    fn show_content(&mut self, ui: &mut Ui, persistence: &PersistenceManager) -> Option<DialogResult> {
        let mut result = None;
        let is_connection_edit = matches!(self.mode, DialogMode::EditConnection(_));
        egui::ScrollArea::vertical()
            .max_height(500.0)
            .show(ui, |ui| {
            // Connection section - hidden when editing an open connection
            if !is_connection_edit {
                ui.heading("Connection");
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.add(egui::TextEdit::singleline(&mut self.config.name).desired_width(INPUT_WIDTH));
                });
                ui.horizontal(|ui| {
                    ui.label("Host:");
                    ui.add(egui::TextEdit::singleline(&mut self.config.host).desired_width(INPUT_WIDTH));
                });
                ui.horizontal(|ui| {
                    ui.label("Port:");
                    ui.add(egui::DragValue::new(&mut self.config.port).range(1..=65535));
                });
                ui.add_space(8.0);
                // Authentication section
                ui.heading("Authentication");
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Username:");
                    ui.add(egui::TextEdit::singleline(&mut self.config.username).desired_width(INPUT_WIDTH));
                });
            }
            if !is_connection_edit {
                ui.horizontal(|ui| {
                    ui.label("Auth Method:");
                    egui::ComboBox::from_id_salt("auth_method")
                        .selected_text(match self.config.auth_method {
                            AuthMethod::Password => "Password",
                            AuthMethod::PrivateKey => "Private Key",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.config.auth_method, AuthMethod::Password, "Password");
                            ui.selectable_value(&mut self.config.auth_method, AuthMethod::PrivateKey, "Private Key");
                        });
                });
                match self.config.auth_method {
                    AuthMethod::Password => {
                        ui.horizontal(|ui| {
                            ui.label("Password:");
                            let password = self.config.password.get_or_insert_with(String::new);
                            if self.password_visible {
                                ui.add(egui::TextEdit::singleline(password).desired_width(INPUT_WIDTH - 60.0));
                            } else {
                                ui.add(egui::TextEdit::singleline(password).password(true).desired_width(INPUT_WIDTH - 60.0));
                            }
                            if ui.button(if self.password_visible { "🙈" } else { "👁" }).clicked() {
                                self.password_visible = !self.password_visible;
                            }
                        });
                    }
                    AuthMethod::PrivateKey => {
                        ui.horizontal(|ui| {
                            ui.label("Key File:");
                            let path_str = self.config.private_key_path
                                .as_ref()
                                .map(|p| p.display().to_string())
                                .unwrap_or_default();
                            ui.label(&path_str);
                            if ui.button("Browse...").clicked() {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("Private Key", &["pem", "ppk", "key", ""])
                                    .pick_file()
                                {
                                    self.config.private_key_path = Some(path);
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Passphrase:");
                            let password = self.config.password.get_or_insert_with(String::new);
                            ui.add(egui::TextEdit::singleline(password).password(true).desired_width(INPUT_WIDTH - 60.0));
                        });
                    }
                }
                ui.add_space(8.0);
            }
            // Appearance section
            let header = egui::CollapsingHeader::new("Appearance");
            header.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Font:");
                    egui::ComboBox::from_id_salt("font_select")
                        .selected_text(&self.config.font)
                        .width(INPUT_WIDTH)
                        .show_ui(ui, |ui| {
                            for font in get_available_monospace_fonts() {
                                ui.selectable_value(&mut self.config.font, font.clone(), font);
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Font Size:");
                    if ui.button("−").clicked() && self.config.font_size > MIN_FONT_SIZE {
                        self.config.font_size -= 1;
                    }
                    ui.add(egui::DragValue::new(&mut self.config.font_size)
                        .range(MIN_FONT_SIZE..=MAX_FONT_SIZE)
                        .speed(0.0));
                    if ui.button("+").clicked() && self.config.font_size < MAX_FONT_SIZE {
                        self.config.font_size += 1;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Foreground:");
                    let mut color: [u8; 3] = [
                        self.config.foreground_color.r,
                        self.config.foreground_color.g,
                        self.config.foreground_color.b,
                    ];
                    if ui.color_edit_button_srgb(&mut color).changed() {
                        self.config.foreground_color.r = color[0];
                        self.config.foreground_color.g = color[1];
                        self.config.foreground_color.b = color[2];
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Background:");
                    let mut color: [u8; 3] = [
                        self.config.background_color.r,
                        self.config.background_color.g,
                        self.config.background_color.b,
                    ];
                    if ui.color_edit_button_srgb(&mut color).changed() {
                        self.config.background_color.r = color[0];
                        self.config.background_color.g = color[1];
                        self.config.background_color.b = color[2];
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Accent:");
                    let mut color: [u8; 3] = [
                        self.config.accent_color.r,
                        self.config.accent_color.g,
                        self.config.accent_color.b,
                    ];
                    if ui.color_edit_button_srgb(&mut color).changed() {
                        self.config.accent_color.r = color[0];
                        self.config.accent_color.g = color[1];
                        self.config.accent_color.b = color[2];
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Cursor Type:");
                    egui::ComboBox::from_id_salt("cursor_type")
                        .selected_text(match self.config.cursor_type {
                            crate::config::CursorType::Underline => "Underline",
                            crate::config::CursorType::Block => "Block",
                            crate::config::CursorType::Vertical => "Vertical",
                            crate::config::CursorType::None => "None",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.config.cursor_type, crate::config::CursorType::Underline, "Underline");
                            ui.selectable_value(&mut self.config.cursor_type, crate::config::CursorType::Block, "Block");
                            ui.selectable_value(&mut self.config.cursor_type, crate::config::CursorType::Vertical, "Vertical");
                            ui.selectable_value(&mut self.config.cursor_type, crate::config::CursorType::None, "None");
                        });
                });
            });
            // Behavior section
            let header = egui::CollapsingHeader::new("Behavior");
            header.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Scrollback Lines:");
                    ui.add(egui::DragValue::new(&mut self.config.scrollback_lines).range(1000..=100000).speed(100));
                });
                ui.checkbox(&mut self.config.reset_scroll_on_input, "Reset scroll position on user input");
                ui.checkbox(&mut self.config.reset_scroll_on_output, "Reset scroll position on server output");
                ui.horizontal(|ui| {
                    ui.label("Auto-reconnect:");
                    egui::ComboBox::from_id_salt("auto_reconnect")
                        .selected_text(match self.config.auto_reconnect {
                            AutoReconnect::Manual => "Manual",
                            AutoReconnect::OnTabFocus => "On tab focus",
                            AutoReconnect::Immediate => "Immediate",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.config.auto_reconnect, AutoReconnect::Manual, "Manual");
                            ui.selectable_value(&mut self.config.auto_reconnect, AutoReconnect::OnTabFocus, "On tab focus");
                            ui.selectable_value(&mut self.config.auto_reconnect, AutoReconnect::Immediate, "Immediate");
                        });
                });
                if self.config.auto_reconnect != AutoReconnect::Manual {
                    ui.horizontal(|ui| {
                        ui.label("Max reconnect attempts:");
                        ui.add(egui::DragValue::new(&mut self.config.reconnect_max_attempts).range(1..=100));
                    });
                }
                ui.horizontal(|ui| {
                    ui.label("Bell Notification:");
                    egui::ComboBox::from_id_salt("bell")
                        .selected_text(match self.config.bell_notification {
                            BellNotification::Sound => "Sound",
                            BellNotification::BlinkScreen => "Blink Screen",
                            BellNotification::BlinkLine => "Blink Line",
                            BellNotification::None => "None",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.config.bell_notification, BellNotification::Sound, "Sound");
                            ui.selectable_value(&mut self.config.bell_notification, BellNotification::BlinkScreen, "Blink Screen");
                            ui.selectable_value(&mut self.config.bell_notification, BellNotification::BlinkLine, "Blink Line");
                            ui.selectable_value(&mut self.config.bell_notification, BellNotification::None, "None");
                        });
                });
            });
            // Compatibility section
            let header = egui::CollapsingHeader::new("Compatibility");
            header.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Terminal Mode:");
                    egui::ComboBox::from_id_salt("term_mode")
                        .selected_text(match self.config.terminal_mode {
                            TerminalMode::VT100 => "VT100",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.config.terminal_mode, TerminalMode::VT100, "VT100");
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Backspace Key:");
                    egui::ComboBox::from_id_salt("backspace")
                        .selected_text(match self.config.backspace_key {
                            BackspaceKey::Del => "DEL (0x7F)",
                            BackspaceKey::CtrlH => "Ctrl+H (0x08)",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.config.backspace_key, BackspaceKey::Del, "DEL (0x7F)");
                            ui.selectable_value(&mut self.config.backspace_key, BackspaceKey::CtrlH, "Ctrl+H (0x08)");
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Resize Method:");
                    egui::ComboBox::from_id_salt("resize")
                        .selected_text(match self.config.resize_method {
                            ResizeMethod::Ssh => "SSH",
                            ResizeMethod::Ansi => "ANSI",
                            ResizeMethod::Stty => "STTY",
                            ResizeMethod::XTerm => "XTerm",
                            ResizeMethod::None => "None",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.config.resize_method, ResizeMethod::Ssh, "SSH");
                            ui.selectable_value(&mut self.config.resize_method, ResizeMethod::Ansi, "ANSI");
                            ui.selectable_value(&mut self.config.resize_method, ResizeMethod::Stty, "STTY");
                            ui.selectable_value(&mut self.config.resize_method, ResizeMethod::XTerm, "XTerm");
                            ui.selectable_value(&mut self.config.resize_method, ResizeMethod::None, "None");
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Line Ending:");
                    egui::ComboBox::from_id_salt("line_ending")
                        .selected_text(match self.config.line_ending {
                            LineEnding::Lf => "LF",
                            LineEnding::CrLf => "CR+LF",
                            LineEnding::Cr => "CR",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.config.line_ending, LineEnding::Lf, "LF");
                            ui.selectable_value(&mut self.config.line_ending, LineEnding::CrLf, "CR+LF");
                            ui.selectable_value(&mut self.config.line_ending, LineEnding::Cr, "CR");
                        });
                });
            });
            // Connection Options section
            let header = egui::CollapsingHeader::new("Connection Options");
            header.show(ui, |ui| {
                ui.checkbox(&mut self.config.keep_alive, "Enable keep-alive");
                ui.checkbox(&mut self.config.compression, "Enable compression");
                ui.horizontal(|ui| {
                    ui.label("Timeout (seconds):");
                    let mut secs = self.config.timeout.as_secs() as u32;
                    if ui.add(egui::DragValue::new(&mut secs).range(1..=300)).changed() {
                        self.config.timeout = std::time::Duration::from_secs(secs as u64);
                    }
                });
                // Gateway session selection
                ui.horizontal(|ui| {
                    ui.label("Gateway Session:");
                    let current_name = self.config.gateway_session
                        .and_then(|id| persistence.get_session(id))
                        .map(|s| s.name.clone())
                        .unwrap_or_else(|| "None".to_string());
                    egui::ComboBox::from_id_salt("gateway_session")
                        .selected_text(&current_name)
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(self.config.gateway_session.is_none(), "None").clicked() {
                                self.config.gateway_session = None;
                            }
                            for session in persistence.sessions.iter() {
                                // Don't allow selecting self as gateway
                                if session.id != self.config.id {
                                    let selected = self.config.gateway_session == Some(session.id);
                                    if ui.selectable_label(selected, &session.name).clicked() {
                                        self.config.gateway_session = Some(session.id);
                                    }
                                }
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Screen Session:");
                    let mut screen = self.config.screen_session.clone().unwrap_or_default();
                    if ui.add(egui::TextEdit::singleline(&mut screen).hint_text("Session name").desired_width(INPUT_WIDTH)).changed() {
                        self.config.screen_session = if screen.is_empty() { None } else { Some(screen) };
                    }
                });
            });
            // Port Forwarding section
            let header = egui::CollapsingHeader::new("Port Forwarding");
            header.show(ui, |ui| {
                ui.checkbox(&mut self.config.x11_forwarding, "Enable X11 forwarding");
                ui.add_space(4.0);
                ui.label("Local Port Forwards:");
                let mut to_remove = None;
                for (i, fwd) in self.config.local_forwards.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("localhost:{} → {}:{}", fwd.local_port, fwd.remote_host, fwd.remote_port));
                        if ui.small_button("❌").clicked() {
                            to_remove = Some(i);
                        }
                    });
                }
                if let Some(i) = to_remove {
                    self.config.local_forwards.remove(i);
                }
                ui.horizontal(|ui| {
                    ui.label("Local:");
                    ui.add(egui::TextEdit::singleline(&mut self.new_local_forward.local_port).desired_width(50.0).hint_text("Port"));
                    ui.label("→");
                    ui.add(egui::TextEdit::singleline(&mut self.new_local_forward.remote_host).desired_width(100.0).hint_text("Host"));
                    ui.label(":");
                    ui.add(egui::TextEdit::singleline(&mut self.new_local_forward.remote_port).desired_width(50.0).hint_text("Port"));
                    if ui.button("Add").clicked() {
                        if let (Ok(lp), Ok(rp)) = (
                            self.new_local_forward.local_port.parse::<u16>(),
                            self.new_local_forward.remote_port.parse::<u16>(),
                        ) {
                            if !self.new_local_forward.remote_host.is_empty() {
                                self.config.local_forwards.push(PortForward {
                                    local_port: lp,
                                    remote_host: self.new_local_forward.remote_host.clone(),
                                    remote_port: rp,
                                });
                                self.new_local_forward = PortForwardEdit::default();
                            }
                        }
                    }
                });
                ui.add_space(4.0);
                ui.label("Remote Port Forwards:");
                let mut to_remove = None;
                for (i, fwd) in self.config.remote_forwards.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{}:{} ← localhost:{}", fwd.remote_host, fwd.remote_port, fwd.local_port));
                        if ui.small_button("❌").clicked() {
                            to_remove = Some(i);
                        }
                    });
                }
                if let Some(i) = to_remove {
                    self.config.remote_forwards.remove(i);
                }
            });
        });
        ui.add_space(8.0);
        // Show validation error if any
        if let Some(error) = &self.validation_error {
            ui.label(RichText::new(error).color(Color32::from_rgb(244, 67, 54)));
        }
        ui.separator();
        // Check for Enter key to submit
        let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                result = Some(DialogResult::Cancelled);
                self.visible = false;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let button_text = match &self.mode {
                    DialogMode::New => "Create",
                    DialogMode::Edit(_) => "Save",
                    DialogMode::EditConnection(_) => "Apply",
                    DialogMode::QuickConnect => "Connect",
                };
                if ui.button(button_text).clicked() || enter_pressed {
                    match self.validate(persistence) {
                        Ok(()) => {
                            result = Some(DialogResult::Confirmed(self.config.clone()));
                            self.visible = false;
                        }
                        Err(error) => {
                            self.validation_error = Some(error);
                        }
                    }
                }
            });
        });
        result
    }

    fn validate(&self, persistence: &PersistenceManager) -> Result<(), String> {
        // Skip connection field validation for EditConnection mode (runtime settings only)
        if !matches!(self.mode, DialogMode::EditConnection(_)) {
            if self.config.name.trim().is_empty() {
                return Err("Session name is required".to_string());
            }
            if self.config.host.trim().is_empty() {
                return Err("Host is required".to_string());
            }
            if self.config.username.trim().is_empty() {
                return Err("Username is required".to_string());
            }
            // Check for duplicate names (only for New mode, not Edit or QuickConnect)
            if let DialogMode::New = &self.mode {
                let name_lower = self.config.name.trim().to_lowercase();
                for session in &persistence.sessions {
                    if session.name.trim().to_lowercase() == name_lower {
                        return Err(format!("A session named '{}' already exists", self.config.name.trim()));
                    }
                }
            }
            // For Edit mode, check that we're not renaming to an existing name (excluding self)
            if let DialogMode::Edit(id) = &self.mode {
                let name_lower = self.config.name.trim().to_lowercase();
                for session in &persistence.sessions {
                    if session.id != *id && session.name.trim().to_lowercase() == name_lower {
                        return Err(format!("A session named '{}' already exists", self.config.name.trim()));
                    }
                }
            }
        }
        Ok(())
    }
}

pub enum DialogResult {
    Confirmed(SessionConfig),
    Cancelled,
}
