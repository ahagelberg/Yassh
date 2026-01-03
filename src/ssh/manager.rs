use super::connection::{ConnectionState, SshConnection, SshEvent};
use crate::config::{AutoReconnect, SessionConfig};
use crate::terminal::emulator::TerminalEmulator;
use crate::terminal::renderer::TerminalRenderer;
use uuid::Uuid;

pub struct ManagedSession {
    pub id: Uuid,
    pub config: SessionConfig,
    pub connection: Option<SshConnection>,
    pub emulator: TerminalEmulator,
    pub renderer: TerminalRenderer,
    pub title: String,
    pub error_message: Option<String>,
    reconnect_pending: ReconnectState,
    reconnect_attempts: u32,
    is_focused: bool,
    should_close: bool,
    scroll_offset: usize,
    was_at_bottom: bool,
    last_viewport_size: Option<(usize, usize)>,
}

#[derive(Clone, Copy, PartialEq)]
enum ReconnectState {
    None,
    PendingImmediate,
    PendingOnFocus,
}

impl ManagedSession {
    pub fn new(config: SessionConfig) -> Self {
        let title = config.name.clone();
        let emulator = TerminalEmulator::new(&config);
        let renderer = TerminalRenderer::new(config.font_size, config.font.clone(), config.cursor_type.clone());
        // Generate a new unique ID for this connection instance
        // (different from the stored session's config.id)
        let connection_id = Uuid::new_v4();
        Self {
            id: connection_id,
            config,
            connection: None,
            emulator,
            renderer,
            title,
            error_message: None,
            reconnect_pending: ReconnectState::None,
            reconnect_attempts: 0,
            is_focused: false,
            should_close: false,
            scroll_offset: 0,
            was_at_bottom: true,
            last_viewport_size: None,
        }
    }

    pub fn connect(&mut self) {
        // Disconnect any existing connection first
        if self.connection.is_some() {
            self.disconnect();
        }
        self.error_message = None;
        self.reconnect_pending = ReconnectState::None;
        self.reconnect_attempts = 0;
        self.connection = Some(SshConnection::new(self.config.clone()));
    }

    pub fn disconnect(&mut self) {
        if let Some(conn) = &self.connection {
            conn.disconnect();
        }
        self.connection = None;
        self.reconnect_pending = ReconnectState::None;
        self.reconnect_attempts = 0;
    }

    pub fn update_config(&mut self, config: SessionConfig) {
        self.renderer.update_font(config.font_size);
        self.renderer.update_cursor_type(config.cursor_type.clone());
        self.emulator.update_config(&config);
        self.config = config;
    }

    pub fn set_focused(&mut self, focused: bool) {
        let was_focused = self.is_focused;
        self.is_focused = focused;
        // If just gained focus and pending reconnect on focus, trigger it
        if focused && !was_focused && self.reconnect_pending == ReconnectState::PendingOnFocus {
            self.attempt_reconnect();
        }
    }

    fn should_auto_reconnect(&self) -> bool {
        match self.config.auto_reconnect {
            AutoReconnect::Manual => false,
            AutoReconnect::OnTabFocus | AutoReconnect::Immediate => {
                self.reconnect_attempts < self.config.reconnect_max_attempts
            }
        }
    }

    fn schedule_reconnect(&mut self) {
        if !self.should_auto_reconnect() {
            return;
        }
        match self.config.auto_reconnect {
            AutoReconnect::Manual => {}
            AutoReconnect::OnTabFocus => {
                self.reconnect_pending = ReconnectState::PendingOnFocus;
            }
            AutoReconnect::Immediate => {
                self.reconnect_pending = ReconnectState::PendingImmediate;
            }
        }
    }

    fn attempt_reconnect(&mut self) {
        if self.reconnect_attempts >= self.config.reconnect_max_attempts {
            self.error_message = Some(format!(
                "Reconnection failed after {} attempts",
                self.config.reconnect_max_attempts
            ));
            self.reconnect_pending = ReconnectState::None;
            return;
        }
        self.reconnect_attempts += 1;
        self.reconnect_pending = ReconnectState::None;
        self.connection = Some(SshConnection::new(self.config.clone()));
    }

    pub fn update(&mut self) -> bool {
        // Handle pending immediate reconnect
        if self.reconnect_pending == ReconnectState::PendingImmediate {
            self.attempt_reconnect();
            return true;
        }
        // Collect events from connection
        let mut events = Vec::new();
        if let Some(connection) = &self.connection {
            while let Some(event) = connection.try_recv() {
                events.push(event);
            }
        }
        let had_events = !events.is_empty();
        // Process collected events
        for event in events {
            match event {
                SshEvent::Connected => {
                    self.error_message = None;
                    self.reconnect_attempts = 0;
                    self.last_viewport_size = None;
                }
                SshEvent::Data(data) => {
                    self.emulator.process(&data);
                    if self.was_at_bottom || self.config.reset_scroll_on_output {
                        self.scroll_offset = usize::MAX;
                        self.was_at_bottom = true;
                    }
                    if let Some(new_title) = self.emulator.take_title() {
                        self.title = new_title;
                    }
                }
                SshEvent::Disconnected { natural } => {
                    // Clear the connection since the thread has exited
                    self.connection = None;
                    if natural {
                        // Natural close (user exited shell) - mark for removal
                        self.should_close = true;
                    } else {
                        // Irregular close (network error, etc.) - keep tab open, mark as disconnected
                        if self.error_message.is_none() {
                            self.schedule_reconnect();
                        }
                    }
                }
                SshEvent::Error(msg) => {
                    self.error_message = Some(msg);
                    self.schedule_reconnect();
                }
            }
        }
        had_events
    }

    pub fn send(&self, data: &[u8]) {
        if let Some(connection) = &self.connection {
            connection.send(data);
        }
    }

    #[allow(dead_code)]
    pub fn send_key(&self, key: &str) {
        if let Some(connection) = &self.connection {
            connection.send_key(key);
        }
    }


    pub fn state(&self) -> ConnectionState {
        if self.should_close {
            // Natural close - still show as disconnected until tab is closed
            ConnectionState::Disconnected
        } else {
            self.connection
                .as_ref()
                .map(|c| c.state())
                .unwrap_or(ConnectionState::Disconnected)
        }
    }

    pub fn should_close(&self) -> bool {
        self.should_close
    }

    #[allow(dead_code)]
    pub fn is_connected(&self) -> bool {
        matches!(self.state(), ConnectionState::Connected)
    }

    pub fn backspace_sequence(&self) -> &[u8] {
        self.connection
            .as_ref()
            .map(|c| c.backspace_sequence())
            .unwrap_or(&[0x7F])
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn set_scroll_offset_with_bottom(&mut self, offset: usize, is_at_bottom: bool) {
        self.scroll_offset = offset;
        self.was_at_bottom = is_at_bottom;
    }

    pub fn reset_scroll_to_bottom(&mut self) {
        self.scroll_offset = usize::MAX;
        self.was_at_bottom = true;
    }

    #[allow(dead_code)]
    pub fn is_reconnecting(&self) -> bool {
        self.reconnect_pending != ReconnectState::None
    }

    #[allow(dead_code)]
    pub fn reconnect_status(&self) -> Option<String> {
        match self.reconnect_pending {
            ReconnectState::None => None,
            ReconnectState::PendingImmediate => Some(format!(
                "Reconnecting... (attempt {}/{})",
                self.reconnect_attempts + 1,
                self.config.reconnect_max_attempts
            )),
            ReconnectState::PendingOnFocus => Some(format!(
                "Will reconnect on focus (attempt {}/{})",
                self.reconnect_attempts + 1,
                self.config.reconnect_max_attempts
            )),
        }
    }

    pub fn check_and_handle_resize(&mut self, cols: usize, rows: usize, send_to_server: bool) -> bool {
        let new_size = (cols, rows);
        if self.last_viewport_size == Some(new_size) {
            return false;
        }
        if send_to_server {
            if let Some(connection) = &self.connection {
                connection.resize_terminal(cols as u32, rows as u32);
                // Only update last_viewport_size when we actually send to server
                self.last_viewport_size = Some(new_size);
            }
        }
        // Don't update last_viewport_size when not sending to server
        // This ensures that when drag stops, we can detect the size change and send it
        let buffer = self.emulator.buffer_mut();
        let current_scroll_offset = self.scroll_offset;
        let was_at_bottom = self.was_at_bottom;
        let total_lines = buffer.total_lines();
        buffer.resize(cols, rows);
        let new_max_scroll = total_lines.saturating_sub(rows);
        if was_at_bottom {
            self.scroll_offset = usize::MAX;
        } else {
            self.scroll_offset = current_scroll_offset.min(new_max_scroll);
        }
        true
    }
}

pub struct SessionManager {
    sessions: Vec<ManagedSession>,
    active_index: Option<usize>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            active_index: None,
        }
    }

    pub fn add_session(&mut self, config: SessionConfig) -> Uuid {
        // Each connection gets its own unique ID (generated in ManagedSession::new)
        // so multiple connections to the same stored session are allowed
        let session = ManagedSession::new(config);
        let id = session.id;
        self.sessions.push(session);
        if self.active_index.is_none() {
            self.active_index = Some(self.sessions.len() - 1);
        }
        id
    }

    pub fn connect_session(&mut self, id: Uuid) {
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == id) {
            session.connect();
        }
    }

    #[allow(dead_code)]
    pub fn disconnect_session(&mut self, id: Uuid) {
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == id) {
            session.disconnect();
        }
    }

    pub fn close_session(&mut self, id: Uuid) {
        if let Some(index) = self.sessions.iter().position(|s| s.id == id) {
            self.sessions[index].disconnect();
            self.sessions.remove(index);
            if let Some(active) = self.active_index {
                if active >= self.sessions.len() {
                    self.active_index = if self.sessions.is_empty() {
                        None
                    } else {
                        Some(self.sessions.len() - 1)
                    };
                } else if active > index {
                    self.active_index = Some(active - 1);
                }
            }
        }
    }

    pub fn set_active(&mut self, id: Uuid) {
        // Update focus state for all sessions
        for (i, session) in self.sessions.iter_mut().enumerate() {
            let is_active = session.id == id;
            session.set_focused(is_active);
            if is_active {
                self.active_index = Some(i);
            }
        }
    }

    pub fn set_active_index(&mut self, index: usize) {
        if index < self.sessions.len() {
            self.active_index = Some(index);
            // Update focus state
            for (i, session) in self.sessions.iter_mut().enumerate() {
                session.set_focused(i == index);
            }
        }
    }

    pub fn active_session(&self) -> Option<&ManagedSession> {
        self.active_index.and_then(|i| self.sessions.get(i))
    }

    pub fn active_session_mut(&mut self) -> Option<&mut ManagedSession> {
        self.active_index.and_then(|i| self.sessions.get_mut(i))
    }

    #[allow(dead_code)]
    pub fn active_index(&self) -> Option<usize> {
        self.active_index
    }

    pub fn sessions(&self) -> &[ManagedSession] {
        &self.sessions
    }

    #[allow(dead_code)]
    pub fn sessions_mut(&mut self) -> &mut [ManagedSession] {
        &mut self.sessions
    }

    pub fn get_session(&self, id: Uuid) -> Option<&ManagedSession> {
        self.sessions.iter().find(|s| s.id == id)
    }

    pub fn get_session_mut(&mut self, id: Uuid) -> Option<&mut ManagedSession> {
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    pub fn update_all(&mut self) -> bool {
        let mut had_activity = false;
        for session in &mut self.sessions {
            if session.update() {
                had_activity = true;
            }
        }
        had_activity
    }

    pub fn next_tab(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        let current = self.active_index.unwrap_or(0);
        let new_index = (current + 1) % self.sessions.len();
        self.set_active_index(new_index);
    }

    pub fn prev_tab(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        let current = self.active_index.unwrap_or(0);
        let new_index = if current == 0 {
            self.sessions.len() - 1
        } else {
            current - 1
        };
        self.set_active_index(new_index);
    }

    #[allow(dead_code)]
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }


    pub fn collect_pending_bells(&mut self) -> Vec<crate::config::BellNotification> {
        let mut bells = Vec::new();
        for session in &mut self.sessions {
            if let Some(bell) = session.emulator.take_bell() {
                bells.push(bell);
            }
        }
        bells
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
