use egui::{Key, Modifiers};

// Input handling result
pub enum InputResult {
    Forward(Vec<u8>),
    Ignored,
}

pub struct InputHandler {
    cursor_keys_application: bool,
}

impl InputHandler {
    pub fn new() -> Self {
        Self {
            cursor_keys_application: false,
        }
    }

    pub fn set_cursor_keys_application(&mut self, enabled: bool) {
        self.cursor_keys_application = enabled;
    }

    pub fn handle_key(
        &self,
        key: Key,
        modifiers: Modifiers,
        backspace_seq: &[u8],
    ) -> InputResult {
        // Forward ALL keys to terminal - no app shortcuts
        let bytes = self.key_to_bytes(key, modifiers, backspace_seq);
        if !bytes.is_empty() {
            InputResult::Forward(bytes)
        } else {
            InputResult::Ignored
        }
    }

    fn key_to_bytes(&self, key: Key, modifiers: Modifiers, backspace_seq: &[u8]) -> Vec<u8> {
        let app_mode = self.cursor_keys_application;
        // Handle special keys first
        match key {
            Key::Enter => return vec![b'\r'],
            Key::Tab => {
                if modifiers.shift {
                    return vec![0x1b, b'[', b'Z'];
                } else {
                    return vec![b'\t'];
                }
            }
            Key::Backspace => return backspace_seq.to_vec(),
            Key::Escape => return vec![0x1b],
            Key::ArrowUp => {
                return if app_mode {
                    vec![0x1b, b'O', b'A']
                } else {
                    vec![0x1b, b'[', b'A']
                };
            }
            Key::ArrowDown => {
                return if app_mode {
                    vec![0x1b, b'O', b'B']
                } else {
                    vec![0x1b, b'[', b'B']
                };
            }
            Key::ArrowRight => {
                return if app_mode {
                    vec![0x1b, b'O', b'C']
                } else {
                    vec![0x1b, b'[', b'C']
                };
            }
            Key::ArrowLeft => {
                return if app_mode {
                    vec![0x1b, b'O', b'D']
                } else {
                    vec![0x1b, b'[', b'D']
                };
            }
            Key::Home => {
                return if modifiers.ctrl {
                    vec![0x1b, b'[', b'1', b';', b'5', b'H']
                } else if app_mode {
                    vec![0x1b, b'O', b'H']
                } else {
                    vec![0x1b, b'[', b'H']
                };
            }
            Key::End => {
                return if modifiers.ctrl {
                    vec![0x1b, b'[', b'1', b';', b'5', b'F']
                } else if app_mode {
                    vec![0x1b, b'O', b'F']
                } else {
                    vec![0x1b, b'[', b'F']
                };
            }
            Key::Insert => return vec![0x1b, b'[', b'2', b'~'],
            Key::Delete => return vec![0x1b, b'[', b'3', b'~'],
            Key::PageUp => return vec![0x1b, b'[', b'5', b'~'],
            Key::PageDown => return vec![0x1b, b'[', b'6', b'~'],
            Key::F1 => return vec![0x1b, b'O', b'P'],
            Key::F2 => return vec![0x1b, b'O', b'Q'],
            Key::F3 => return vec![0x1b, b'O', b'R'],
            Key::F4 => return vec![0x1b, b'O', b'S'],
            Key::F5 => return vec![0x1b, b'[', b'1', b'5', b'~'],
            Key::F6 => return vec![0x1b, b'[', b'1', b'7', b'~'],
            Key::F7 => return vec![0x1b, b'[', b'1', b'8', b'~'],
            Key::F8 => return vec![0x1b, b'[', b'1', b'9', b'~'],
            Key::F9 => return vec![0x1b, b'[', b'2', b'0', b'~'],
            Key::F10 => return vec![0x1b, b'[', b'2', b'1', b'~'],
            Key::F11 => return vec![0x1b, b'[', b'2', b'3', b'~'],
            Key::F12 => return vec![0x1b, b'[', b'2', b'4', b'~'],
            _ => {}
        }
        // Handle character keys
        self.handle_char_key(key, modifiers)
    }

    fn handle_char_key(&self, key: Key, modifiers: Modifiers) -> Vec<u8> {
        let ch = match key {
            Key::A => 'a',
            Key::B => 'b',
            Key::C => 'c',
            Key::D => 'd',
            Key::E => 'e',
            Key::F => 'f',
            Key::G => 'g',
            Key::H => 'h',
            Key::I => 'i',
            Key::J => 'j',
            Key::K => 'k',
            Key::L => 'l',
            Key::M => 'm',
            Key::N => 'n',
            Key::O => 'o',
            Key::P => 'p',
            Key::Q => 'q',
            Key::R => 'r',
            Key::S => 's',
            Key::T => 't',
            Key::U => 'u',
            Key::V => 'v',
            Key::W => 'w',
            Key::X => 'x',
            Key::Y => 'y',
            Key::Z => 'z',
            Key::Num0 => '0',
            Key::Num1 => '1',
            Key::Num2 => '2',
            Key::Num3 => '3',
            Key::Num4 => '4',
            Key::Num5 => '5',
            Key::Num6 => '6',
            Key::Num7 => '7',
            Key::Num8 => '8',
            Key::Num9 => '9',
            Key::Space => ' ',
            Key::Minus => '-',
            Key::Plus => '+',
            Key::Equals => '=',
            Key::OpenBracket => '[',
            Key::CloseBracket => ']',
            Key::Backslash => '\\',
            Key::Semicolon => ';',
            Key::Quote => '\'',
            Key::Comma => ',',
            Key::Period => '.',
            Key::Slash => '/',
            Key::Backtick => '`',
            _ => return Vec::new(),
        };
        if modifiers.ctrl && !modifiers.alt {
            // Ctrl+letter sends control character
            let upper = ch.to_ascii_uppercase();
            if upper >= 'A' && upper <= 'Z' {
                return vec![(upper as u8) - b'A' + 1];
            }
            match ch {
                '[' | '3' => return vec![0x1b],
                '\\' | '4' => return vec![0x1c],
                ']' | '5' => return vec![0x1d],
                '^' | '6' => return vec![0x1e],
                '_' | '7' => return vec![0x1f],
                ' ' | '2' => return vec![0x00],
                _ => {}
            }
        }
        if modifiers.alt {
            let byte = if modifiers.shift {
                ch.to_ascii_uppercase() as u8
            } else {
                ch as u8
            };
            return vec![0x1b, byte];
        }
        if modifiers.shift {
            let shifted = match ch {
                '1' => '!',
                '2' => '@',
                '3' => '#',
                '4' => '$',
                '5' => '%',
                '6' => '^',
                '7' => '&',
                '8' => '*',
                '9' => '(',
                '0' => ')',
                '-' => '_',
                '=' => '+',
                '[' => '{',
                ']' => '}',
                '\\' => '|',
                ';' => ':',
                '\'' => '"',
                ',' => '<',
                '.' => '>',
                '/' => '?',
                '`' => '~',
                _ => ch.to_ascii_uppercase(),
            };
            return vec![shifted as u8];
        }
        vec![ch as u8]
    }
}

impl Default for InputHandler {
    fn default() -> Self {
        Self::new()
    }
}
