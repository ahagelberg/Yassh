use egui::Color32;
use font_kit::source::SystemSource;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;
use uuid::Uuid;

// Default configuration values
const DEFAULT_PORT: u16 = 22;
const DEFAULT_FONT_SIZE: u32 = 14;
const DEFAULT_SCROLLBACK_LINES: usize = 20000;
const DEFAULT_TIMEOUT_SECS: u64 = 30;
const DEFAULT_KEEPALIVE_INTERVAL_SECS: u64 = 60;
const DEFAULT_RECONNECT_MAX_ATTEMPTS: u32 = 5;

static AVAILABLE_MONOSPACE_FONTS: OnceLock<Vec<String>> = OnceLock::new();

pub fn get_available_monospace_fonts() -> &'static Vec<String> {
    AVAILABLE_MONOSPACE_FONTS.get_or_init(|| {
        let mut fonts = Vec::new();
        let source = SystemSource::new();
        if let Ok(all_fonts) = source.all_families() {
            for family in all_fonts {
                let family_lower = family.to_lowercase();
                // Only include fonts that are clearly monospace based on name
                let is_monospace = family_lower.contains("mono")
                    || family_lower.contains("courier")
                    || family_lower.contains("consolas")
                    || family_lower == "hack"
                    || family_lower == "inconsolata"
                    || family_lower == "iosevka"
                    || family_lower.starts_with("cascadia")
                    || family_lower.starts_with("jetbrains")
                    || family_lower.starts_with("fira code")
                    || family_lower.starts_with("source code")
                    || family_lower == "menlo"
                    || family_lower == "monaco"
                    || family_lower == "lucida console"
                    || family_lower.contains("terminal");
                if is_monospace {
                    fonts.push(family);
                }
            }
        }
        // Fallback if no fonts found
        if fonts.is_empty() {
            fonts.push("Consolas".to_string());
        }
        fonts.sort();
        fonts.dedup();
        fonts
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AuthMethod {
    Password,
    PrivateKey,
}

impl Default for AuthMethod {
    fn default() -> Self {
        Self::Password
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BellNotification {
    Sound,
    BlinkScreen,
    BlinkLine,
    None,
}

impl Default for BellNotification {
    fn default() -> Self {
        Self::BlinkLine
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TerminalMode {
    VT100,
}

impl Default for TerminalMode {
    fn default() -> Self {
        Self::VT100
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BackspaceKey {
    Del,
    CtrlH,
}

impl Default for BackspaceKey {
    fn default() -> Self {
        Self::Del
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ResizeMethod {
    Ssh,
    Ansi,
    Stty,
    XTerm,
    None,
}

impl Default for ResizeMethod {
    fn default() -> Self {
        Self::Ssh
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LineEnding {
    Lf,
    CrLf,
    Cr,
}

impl Default for LineEnding {
    fn default() -> Self {
        Self::Lf
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CursorType {
    Underline,
    Block,
    Vertical,
    None,
}

impl Default for CursorType {
    fn default() -> Self {
        Self::Underline
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AutoReconnect {
    Manual,
    OnTabFocus,
    Immediate,
}

impl Default for AutoReconnect {
    fn default() -> Self {
        Self::OnTabFocus
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortForward {
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl From<Color32> for SerializableColor {
    fn from(c: Color32) -> Self {
        Self {
            r: c.r(),
            g: c.g(),
            b: c.b(),
            a: c.a(),
        }
    }
}

impl From<SerializableColor> for Color32 {
    fn from(c: SerializableColor) -> Self {
        Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub id: Uuid,
    #[serde(default = "default_session_name")]
    pub name: String,
    #[serde(default)]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub auth_method: AuthMethod,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_key_path: Option<PathBuf>,
    #[serde(default = "default_font")]
    pub font: String,
    #[serde(default = "default_font_size")]
    pub font_size: u32,
    #[serde(default = "default_foreground_color")]
    pub foreground_color: SerializableColor,
    #[serde(default = "default_background_color")]
    pub background_color: SerializableColor,
    #[serde(default = "default_accent_color")]
    pub accent_color: SerializableColor,
    #[serde(default)]
    pub cursor_type: CursorType,
    #[serde(default = "default_scrollback_lines")]
    pub scrollback_lines: usize,
    #[serde(default = "default_true")]
    pub reset_scroll_on_input: bool,
    #[serde(default)]
    pub reset_scroll_on_output: bool,
    #[serde(default)]
    pub bell_notification: BellNotification,
    #[serde(default)]
    pub auto_reconnect: AutoReconnect,
    #[serde(default = "default_reconnect_max_attempts")]
    pub reconnect_max_attempts: u32,
    #[serde(default)]
    pub terminal_mode: TerminalMode,
    #[serde(default)]
    pub backspace_key: BackspaceKey,
    #[serde(default)]
    pub resize_method: ResizeMethod,
    #[serde(default)]
    pub line_ending: LineEnding,
    #[serde(default = "default_true")]
    pub keep_alive: bool,
    #[serde(default = "default_timeout", with = "duration_secs")]
    pub timeout: Duration,
    #[serde(default = "default_keepalive_interval", with = "duration_secs")]
    pub keepalive_interval: Duration,
    #[serde(default)]
    pub compression: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway_session: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen_session: Option<String>,
    #[serde(default)]
    pub x11_forwarding: bool,
    #[serde(default)]
    pub local_forwards: Vec<PortForward>,
    #[serde(default)]
    pub remote_forwards: Vec<PortForward>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_id: Option<Uuid>,
    #[serde(default)]
    pub order: u32,
}

fn default_session_name() -> String { String::from("New Session") }
fn default_port() -> u16 { DEFAULT_PORT }
fn default_font() -> String { String::from("Consolas") }
fn default_font_size() -> u32 { DEFAULT_FONT_SIZE }
fn default_foreground_color() -> SerializableColor { Color32::from_rgb(204, 204, 204).into() }
fn default_background_color() -> SerializableColor { Color32::from_rgb(30, 30, 30).into() }
fn default_accent_color() -> SerializableColor { Color32::from_rgb(128, 128, 128).into() }
fn default_scrollback_lines() -> usize { DEFAULT_SCROLLBACK_LINES }
fn default_reconnect_max_attempts() -> u32 { DEFAULT_RECONNECT_MAX_ATTEMPTS }
fn default_timeout() -> Duration { Duration::from_secs(DEFAULT_TIMEOUT_SECS) }
fn default_keepalive_interval() -> Duration { Duration::from_secs(DEFAULT_KEEPALIVE_INTERVAL_SECS) }
fn default_true() -> bool { true }

mod duration_secs {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_secs().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: String::from("New Session"),
            host: String::new(),
            port: DEFAULT_PORT,
            username: String::new(),
            auth_method: AuthMethod::default(),
            password: None,
            private_key_path: None,
            font: String::from("Consolas"),
            font_size: DEFAULT_FONT_SIZE,
            foreground_color: Color32::from_rgb(204, 204, 204).into(),
            background_color: Color32::from_rgb(30, 30, 30).into(),
            accent_color: Color32::from_rgb(128, 128, 128).into(),
            cursor_type: CursorType::default(),
            scrollback_lines: DEFAULT_SCROLLBACK_LINES,
            reset_scroll_on_input: true,
            reset_scroll_on_output: false,
            bell_notification: BellNotification::default(),
            auto_reconnect: AutoReconnect::default(),
            reconnect_max_attempts: DEFAULT_RECONNECT_MAX_ATTEMPTS,
            terminal_mode: TerminalMode::default(),
            backspace_key: BackspaceKey::default(),
            resize_method: ResizeMethod::default(),
            line_ending: LineEnding::default(),
            keep_alive: true,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            keepalive_interval: Duration::from_secs(DEFAULT_KEEPALIVE_INTERVAL_SECS),
            compression: false,
            gateway_session: None,
            screen_session: None,
            x11_forwarding: false,
            local_forwards: Vec::new(),
            remote_forwards: Vec::new(),
            folder_id: None,
            order: 0,
        }
    }
}

impl SessionConfig {
    pub fn foreground(&self) -> Color32 {
        self.foreground_color.clone().into()
    }

    pub fn background(&self) -> Color32 {
        self.background_color.clone().into()
    }

    pub fn accent(&self) -> Color32 {
        self.accent_color.clone().into()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionFolder {
    pub id: Uuid,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub parent_id: Option<Uuid>,
    #[serde(default = "default_true")]
    pub expanded: bool,
}

impl SessionFolder {
    pub fn new(name: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            parent_id: None,
            expanded: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Theme {
    Dark,
    Light,
    #[serde(rename = "dark_blue")]
    DarkBlue,
    #[serde(rename = "light_blue")]
    LightBlue,
    #[serde(rename = "dark_green")]
    DarkGreen,
}

impl Default for Theme {
    fn default() -> Self {
        Self::Dark
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ThemeColors {
    pub title_bar_bg: Color32,
    pub title_bar_text: Color32,
    pub title_bar_icon: Color32,
    pub button_hover: Color32,
    #[allow(dead_code)]
    pub button_hover_text: Color32,
    pub close_button_hover: Color32,
    pub close_button_hover_text: Color32,
}

impl ThemeColors {
    pub fn for_theme(theme: Theme) -> Self {
        match theme {
            Theme::Dark => Self {
                title_bar_bg: Color32::from_rgb(30, 30, 30),
                title_bar_text: Color32::from_rgb(204, 204, 204),
                title_bar_icon: Color32::from_rgb(204, 204, 204),
                button_hover: Color32::from_rgba_unmultiplied(255, 255, 255, 20),
                button_hover_text: Color32::from_rgb(204, 204, 204),
                close_button_hover: Color32::from_rgb(232, 17, 35),
                close_button_hover_text: Color32::WHITE,
            },
            Theme::Light => Self {
                title_bar_bg: Color32::from_rgb(251, 251, 251),
                title_bar_text: Color32::from_rgb(51, 51, 51),
                title_bar_icon: Color32::from_rgb(51, 51, 51),
                button_hover: Color32::from_rgba_unmultiplied(0, 0, 0, 20),
                button_hover_text: Color32::from_rgb(51, 51, 51),
                close_button_hover: Color32::from_rgb(232, 17, 35),
                close_button_hover_text: Color32::WHITE,
            },
            Theme::DarkBlue => Self {
                title_bar_bg: Color32::from_rgb(25, 30, 40),
                title_bar_text: Color32::from_rgb(200, 210, 230),
                title_bar_icon: Color32::from_rgb(200, 210, 230),
                button_hover: Color32::from_rgba_unmultiplied(100, 150, 255, 30),
                button_hover_text: Color32::from_rgb(200, 210, 230),
                close_button_hover: Color32::from_rgb(232, 17, 35),
                close_button_hover_text: Color32::WHITE,
            },
            Theme::LightBlue => Self {
                title_bar_bg: Color32::from_rgb(240, 245, 255),
                title_bar_text: Color32::from_rgb(30, 50, 80),
                title_bar_icon: Color32::from_rgb(30, 50, 80),
                button_hover: Color32::from_rgba_unmultiplied(100, 150, 255, 30),
                button_hover_text: Color32::from_rgb(30, 50, 80),
                close_button_hover: Color32::from_rgb(232, 17, 35),
                close_button_hover_text: Color32::WHITE,
            },
            Theme::DarkGreen => Self {
                title_bar_bg: Color32::from_rgb(20, 35, 25),
                title_bar_text: Color32::from_rgb(180, 220, 180),
                title_bar_icon: Color32::from_rgb(180, 220, 180),
                button_hover: Color32::from_rgba_unmultiplied(100, 200, 100, 30),
                button_hover_text: Color32::from_rgb(180, 220, 180),
                close_button_hover: Color32::from_rgb(232, 17, 35),
                close_button_hover_text: Color32::WHITE,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub theme: Theme,
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: f32,
    #[serde(default = "default_window_width")]
    pub window_width: f32,
    #[serde(default = "default_window_height")]
    pub window_height: f32,
    #[serde(default)]
    pub window_maximized: bool,
}

fn default_sidebar_width() -> f32 { DEFAULT_SIDEBAR_WIDTH }
fn default_window_width() -> f32 { DEFAULT_WINDOW_WIDTH }
fn default_window_height() -> f32 { DEFAULT_WINDOW_HEIGHT }

const DEFAULT_SIDEBAR_WIDTH: f32 = 130.0;
const DEFAULT_WINDOW_WIDTH: f32 = 1200.0;
const DEFAULT_WINDOW_HEIGHT: f32 = 800.0;

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            sidebar_width: DEFAULT_SIDEBAR_WIDTH,
            window_width: DEFAULT_WINDOW_WIDTH,
            window_height: DEFAULT_WINDOW_HEIGHT,
            window_maximized: false,
        }
    }
}
