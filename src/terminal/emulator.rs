use super::buffer::TerminalBuffer;
use super::vt100::Vt100Mode;
use crate::config::{BellNotification, SessionConfig, TerminalMode};

pub struct TerminalEmulator {
    buffer: TerminalBuffer,
    vt100: Vt100Mode,
    mode: TerminalMode,
    bell_notification: BellNotification,
    bell_pending: bool,
    title: Option<String>,
}

impl TerminalEmulator {
    pub fn new(config: &SessionConfig) -> Self {
        let buffer = TerminalBuffer::new(
            config.scrollback_lines,
            config.foreground(),
            config.background(),
        );
        Self {
            buffer,
            vt100: Vt100Mode::new(),
            mode: config.terminal_mode.clone(),
            bell_notification: config.bell_notification.clone(),
            bell_pending: false,
            title: None,
        }
    }

    pub fn process(&mut self, data: &[u8]) {
        match self.mode {
            TerminalMode::VT100 => {
                self.vt100.process(&mut self.buffer, data);
                if self.vt100.take_bell() {
                    self.bell_pending = true;
                }
                if let Some(title) = self.vt100.take_title() {
                    self.title = Some(title);
                }
            }
        }
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.buffer.resize(cols, rows);
    }

    pub fn buffer(&self) -> &TerminalBuffer {
        &self.buffer
    }

    pub fn cursor_visible(&self) -> bool {
        match self.mode {
            TerminalMode::VT100 => self.vt100.cursor_visible(),
        }
    }

    pub fn take_bell(&mut self) -> Option<BellNotification> {
        if self.bell_pending {
            self.bell_pending = false;
            Some(self.bell_notification.clone())
        } else {
            None
        }
    }

    pub fn take_title(&mut self) -> Option<String> {
        self.title.take()
    }

    pub fn cursor_keys_application(&self) -> bool {
        match self.mode {
            TerminalMode::VT100 => self.vt100.cursor_keys_application(),
        }
    }

    pub fn bracketed_paste(&self) -> bool {
        match self.mode {
            TerminalMode::VT100 => self.vt100.bracketed_paste(),
        }
    }

    pub fn reverse_video(&self) -> bool {
        match self.mode {
            TerminalMode::VT100 => self.vt100.reverse_video(),
        }
    }

    pub fn cols(&self) -> usize {
        self.buffer.cols()
    }

    pub fn rows(&self) -> usize {
        self.buffer.rows()
    }

    pub fn update_config(&mut self, config: &SessionConfig) {
        self.buffer.set_default_colors(config.foreground(), config.background());
        self.bell_notification = config.bell_notification.clone();
    }
}

