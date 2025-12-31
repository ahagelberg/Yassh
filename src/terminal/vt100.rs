use super::ansi::{parse_sgr, AnsiAction, AnsiParser};
use super::buffer::TerminalBuffer;

// VT100 mode flags
const MODE_CURSOR_KEYS: u16 = 1;
const MODE_ANSI: u16 = 2;
const MODE_COLUMN_132: u16 = 3;
const MODE_SMOOTH_SCROLL: u16 = 4;
const MODE_REVERSE_VIDEO: u16 = 5;
const MODE_ORIGIN: u16 = 6;
const MODE_AUTO_WRAP: u16 = 7;
const MODE_AUTO_REPEAT: u16 = 8;
const MODE_CURSOR_VISIBLE: u16 = 25;
const MODE_BRACKETED_PASTE: u16 = 2004;

pub struct Vt100Mode {
    parser: AnsiParser,
    cursor_keys_application: bool,
    auto_wrap: bool,
    cursor_visible: bool,
    reverse_video: bool,
    bracketed_paste: bool,
    utf8_buffer: Vec<u8>,
    bell_pending: bool,
    title: Option<String>,
}

impl Default for Vt100Mode {
    fn default() -> Self {
        Self::new()
    }
}

impl Vt100Mode {
    pub fn new() -> Self {
        Self {
            parser: AnsiParser::new(),
            cursor_keys_application: false,
            auto_wrap: true,
            cursor_visible: true,
            reverse_video: false,
            bracketed_paste: false,
            utf8_buffer: Vec::new(),
            bell_pending: false,
            title: None,
        }
    }

    pub fn process(&mut self, buffer: &mut TerminalBuffer, data: &[u8]) {
        for &byte in data {
            if !self.utf8_buffer.is_empty() {
                self.utf8_buffer.push(byte);
                if let Some(ch) = self.try_decode_utf8() {
                    buffer.put_char(ch);
                }
                continue;
            }
            if byte >= 0x80 && byte < 0xC0 {
                continue;
            }
            if byte >= 0xC0 {
                self.utf8_buffer.push(byte);
                continue;
            }
            if let Some(action) = self.parser.parse(byte) {
                self.handle_action(buffer, action);
            }
        }
    }

    fn try_decode_utf8(&mut self) -> Option<char> {
        let bytes = &self.utf8_buffer;
        let expected_len = if bytes[0] < 0xE0 {
            2
        } else if bytes[0] < 0xF0 {
            3
        } else {
            4
        };
        if bytes.len() < expected_len {
            return None;
        }
        let result = std::str::from_utf8(bytes).ok().and_then(|s| s.chars().next());
        self.utf8_buffer.clear();
        result
    }

    fn handle_action(&mut self, buffer: &mut TerminalBuffer, action: AnsiAction) {
        match action {
            AnsiAction::Print(ch) => {
                buffer.put_char(ch);
            }
            AnsiAction::Execute(byte) => {
                self.handle_execute(buffer, byte);
            }
            AnsiAction::CsiDispatch { params, intermediates, final_byte } => {
                self.handle_csi(buffer, &params, &intermediates, final_byte);
            }
            AnsiAction::EscDispatch { intermediates, final_byte } => {
                self.handle_esc(buffer, &intermediates, final_byte);
            }
            AnsiAction::OscDispatch { params } => {
                self.handle_osc(&params);
            }
            AnsiAction::DcsHook { .. } | AnsiAction::DcsPut(_) | AnsiAction::DcsUnhook => {}
        }
    }

    fn handle_execute(&mut self, buffer: &mut TerminalBuffer, byte: u8) {
        match byte {
            0x07 => self.bell_pending = true,
            0x08 => buffer.backspace(),
            0x09 => buffer.tab(),
            0x0A | 0x0B | 0x0C => buffer.new_line(),
            0x0D => buffer.carriage_return(),
            0x0E => {} // Shift Out
            0x0F => {} // Shift In
            _ => {}
        }
    }

    fn handle_csi(&mut self, buffer: &mut TerminalBuffer, params: &[u16], intermediates: &[u8], final_byte: char) {
        let param = |i: usize, default: u16| -> u16 {
            params.get(i).copied().filter(|&p| p != 0).unwrap_or(default)
        };
        if intermediates.first() == Some(&b'?') {
            self.handle_dec_private_mode(buffer, params, final_byte);
            return;
        }
        match final_byte {
            '@' => buffer.insert_chars(param(0, 1) as usize),
            'A' => buffer.move_cursor_up(param(0, 1) as usize),
            'B' | 'e' => buffer.move_cursor_down(param(0, 1) as usize),
            'C' | 'a' => buffer.move_cursor_right(param(0, 1) as usize),
            'D' => buffer.move_cursor_left(param(0, 1) as usize),
            'E' => {
                buffer.move_cursor_down(param(0, 1) as usize);
                buffer.carriage_return();
            }
            'F' => {
                buffer.move_cursor_up(param(0, 1) as usize);
                buffer.carriage_return();
            }
            'G' | '`' => {
                let cursor = buffer.cursor();
                buffer.set_cursor_position(cursor.row, (param(0, 1) as usize).saturating_sub(1));
            }
            'H' | 'f' => {
                let row = (param(0, 1) as usize).saturating_sub(1);
                let col = (param(1, 1) as usize).saturating_sub(1);
                buffer.set_cursor_position(row, col);
            }
            'I' => {
                for _ in 0..param(0, 1) {
                    buffer.tab();
                }
            }
            'J' => buffer.erase_in_display(param(0, 0) as u8),
            'K' => buffer.erase_in_line(param(0, 0) as u8),
            'L' => buffer.insert_lines(param(0, 1) as usize),
            'M' => buffer.delete_lines(param(0, 1) as usize),
            'P' => buffer.delete_chars(param(0, 1) as usize),
            'S' => buffer.scroll_up(param(0, 1) as usize),
            'T' => buffer.scroll_down(param(0, 1) as usize),
            'X' => {
                let count = param(0, 1) as usize;
                let cursor = buffer.cursor();
                let _style = buffer.current_style();
                for i in 0..count {
                    let col = cursor.col + i;
                    if col < buffer.cols() {
                        if let Some(line) = buffer.screen().get(cursor.row) {
                            let _ = line; // We need mutable access
                        }
                    }
                }
                // Erase characters by clearing the range
                buffer.erase_in_line(0);
            }
            'd' => {
                let cursor = buffer.cursor();
                buffer.set_cursor_position((param(0, 1) as usize).saturating_sub(1), cursor.col);
            }
            'h' => self.handle_set_mode(buffer, params, true),
            'l' => self.handle_set_mode(buffer, params, false),
            'm' => {
                let params_vec: Vec<u16> = if params.is_empty() { vec![0] } else { params.to_vec() };
                let mut style = buffer.current_style();
                parse_sgr(&params_vec, &mut style, buffer.default_fg());
                buffer.set_style(style);
            }
            'n' => {} // Device status report - handled at SSH level
            'r' => {
                let top = (param(0, 1) as usize).saturating_sub(1);
                let bottom = (param(1, buffer.rows() as u16) as usize).saturating_sub(1);
                buffer.set_scroll_region(top, bottom);
            }
            's' => buffer.save_cursor(),
            'u' => buffer.restore_cursor(),
            _ => {}
        }
    }

    fn handle_dec_private_mode(&mut self, buffer: &mut TerminalBuffer, params: &[u16], final_byte: char) {
        let set = final_byte == 'h';
        for &param in params {
            match param {
                MODE_CURSOR_KEYS => self.cursor_keys_application = set,
                MODE_ANSI => {}
                MODE_COLUMN_132 => {}
                MODE_SMOOTH_SCROLL => {}
                MODE_REVERSE_VIDEO => self.reverse_video = set,
                MODE_ORIGIN => buffer.set_origin_mode(set),
                MODE_AUTO_WRAP => self.auto_wrap = set,
                MODE_AUTO_REPEAT => {}
                MODE_CURSOR_VISIBLE => self.cursor_visible = set,
                MODE_BRACKETED_PASTE => self.bracketed_paste = set,
                47 | 1047 => {
                    // Alternate screen buffer - reset
                    if !set {
                        buffer.erase_in_display(2);
                    }
                }
                1049 => {
                    // Alternate screen buffer with cursor save
                    if set {
                        buffer.save_cursor();
                        buffer.erase_in_display(2);
                    } else {
                        buffer.erase_in_display(2);
                        buffer.restore_cursor();
                    }
                }
                _ => {}
            }
        }
    }

    fn handle_set_mode(&mut self, _buffer: &mut TerminalBuffer, params: &[u16], set: bool) {
        for &param in params {
            match param {
                4 => {} // Insert mode
                20 => {} // Automatic newline
                _ => {}
            }
            let _ = set;
        }
    }

    fn handle_esc(&mut self, buffer: &mut TerminalBuffer, intermediates: &[u8], final_byte: char) {
        match (intermediates.first(), final_byte) {
            (None, '7') => buffer.save_cursor(),
            (None, '8') => buffer.restore_cursor(),
            (None, 'D') => buffer.new_line(),
            (None, 'E') => {
                buffer.carriage_return();
                buffer.new_line();
            }
            (None, 'M') => buffer.move_cursor_up(1),
            (None, 'c') => {
                buffer.erase_in_display(2);
                buffer.set_cursor_position(0, 0);
                buffer.reset_style();
                buffer.reset_scroll_region();
                self.cursor_keys_application = false;
                self.auto_wrap = true;
                self.cursor_visible = true;
                self.reverse_video = false;
            }
            (Some(&b'#'), '8') => {
                // DEC Screen Alignment Test - fill screen with 'E'
                let rows = buffer.rows();
                let cols = buffer.cols();
                for row in 0..rows {
                    buffer.set_cursor_position(row, 0);
                    for _ in 0..cols {
                        buffer.put_char('E');
                    }
                }
                buffer.set_cursor_position(0, 0);
            }
            _ => {}
        }
    }

    fn handle_osc(&mut self, params: &[String]) {
        if params.is_empty() {
            return;
        }
        match params[0].as_str() {
            "0" | "2" => {
                if params.len() > 1 {
                    self.title = Some(params[1..].join(";"));
                }
            }
            "1" => {} // Icon name - ignored
            _ => {}
        }
    }

    pub fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    pub fn take_bell(&mut self) -> bool {
        std::mem::take(&mut self.bell_pending)
    }

    pub fn take_title(&mut self) -> Option<String> {
        self.title.take()
    }

    pub fn cursor_keys_application(&self) -> bool {
        self.cursor_keys_application
    }

    pub fn bracketed_paste(&self) -> bool {
        self.bracketed_paste
    }

    pub fn reverse_video(&self) -> bool {
        self.reverse_video
    }
}

