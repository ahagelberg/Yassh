use egui::Color32;
use super::buffer::CellStyle;

const ANSI_COLORS: [Color32; 16] = [
    Color32::from_rgb(0, 0, 0),       // Black
    Color32::from_rgb(170, 0, 0),     // Red
    Color32::from_rgb(0, 170, 0),     // Green
    Color32::from_rgb(170, 85, 0),    // Yellow (brown)
    Color32::from_rgb(50, 100, 255),   // Blue
    Color32::from_rgb(170, 0, 170),   // Magenta
    Color32::from_rgb(0, 170, 170),   // Cyan
    Color32::from_rgb(170, 170, 170), // White
    Color32::from_rgb(85, 85, 85),    // Bright Black (Gray)
    Color32::from_rgb(255, 85, 85),   // Bright Red
    Color32::from_rgb(85, 255, 85),   // Bright Green
    Color32::from_rgb(255, 255, 85),  // Bright Yellow
    Color32::from_rgb(85, 85, 255),   // Bright Blue
    Color32::from_rgb(255, 85, 255),  // Bright Magenta
    Color32::from_rgb(85, 255, 255),  // Bright Cyan
    Color32::from_rgb(255, 255, 255), // Bright White
];

#[derive(Debug, Clone, PartialEq)]
pub enum AnsiAction {
    Print(char),
    Execute(u8),
    CsiDispatch { params: Vec<u16>, intermediates: Vec<u8>, final_byte: char },
    EscDispatch { intermediates: Vec<u8>, final_byte: char },
    OscDispatch { params: Vec<String> },
    DcsHook { params: Vec<u16>, intermediates: Vec<u8>, final_byte: char },
    DcsPut(u8),
    DcsUnhook,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Ground,
    Escape,
    EscapeIntermediate,
    CsiEntry,
    CsiParam,
    CsiIntermediate,
    CsiIgnore,
    DcsEntry,
    DcsParam,
    DcsIntermediate,
    DcsPassthrough,
    DcsIgnore,
    OscString,
    SosPmApcString,
}

pub struct AnsiParser {
    state: State,
    params: Vec<u16>,
    current_param: u16,
    intermediates: Vec<u8>,
    osc_string: String,
}

impl AnsiParser {
    pub fn new() -> Self {
        Self {
            state: State::Ground,
            params: Vec::new(),
            current_param: 0,
            intermediates: Vec::new(),
            osc_string: String::new(),
        }
    }

    pub fn parse(&mut self, byte: u8) -> Option<AnsiAction> {
        match self.state {
            State::Ground => self.ground(byte),
            State::Escape => self.escape(byte),
            State::EscapeIntermediate => self.escape_intermediate(byte),
            State::CsiEntry => self.csi_entry(byte),
            State::CsiParam => self.csi_param(byte),
            State::CsiIntermediate => self.csi_intermediate(byte),
            State::CsiIgnore => self.csi_ignore(byte),
            State::DcsEntry => self.dcs_entry(byte),
            State::DcsParam => self.dcs_param(byte),
            State::DcsIntermediate => self.dcs_intermediate(byte),
            State::DcsPassthrough => self.dcs_passthrough(byte),
            State::DcsIgnore => self.dcs_ignore(byte),
            State::OscString => self.osc_string_state(byte),
            State::SosPmApcString => self.sos_pm_apc_string(byte),
        }
    }

    fn clear(&mut self) {
        self.params.clear();
        self.current_param = 0;
        self.intermediates.clear();
        self.osc_string.clear();
    }

    fn collect_param(&mut self) {
        self.params.push(self.current_param);
        self.current_param = 0;
    }

    fn ground(&mut self, byte: u8) -> Option<AnsiAction> {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => Some(AnsiAction::Execute(byte)),
            0x1B => {
                self.state = State::Escape;
                self.clear();
                None
            }
            0x20..=0x7F => Some(AnsiAction::Print(byte as char)),
            0x80..=0x8F | 0x91..=0x97 | 0x99 | 0x9A => Some(AnsiAction::Execute(byte)),
            0x90 => {
                self.state = State::DcsEntry;
                self.clear();
                None
            }
            0x9B => {
                self.state = State::CsiEntry;
                self.clear();
                None
            }
            0x9C => None,
            0x9D => {
                self.state = State::OscString;
                self.clear();
                None
            }
            0x98 | 0x9E | 0x9F => {
                self.state = State::SosPmApcString;
                None
            }
            0xA0..=0xFF => Some(AnsiAction::Print(byte as char)),
            _ => None,
        }
    }

    fn escape(&mut self, byte: u8) -> Option<AnsiAction> {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => Some(AnsiAction::Execute(byte)),
            0x20..=0x2F => {
                self.intermediates.push(byte);
                self.state = State::EscapeIntermediate;
                None
            }
            0x30..=0x4F | 0x51..=0x57 | 0x59 | 0x5A | 0x5C | 0x60..=0x7E => {
                self.state = State::Ground;
                Some(AnsiAction::EscDispatch {
                    intermediates: self.intermediates.clone(),
                    final_byte: byte as char,
                })
            }
            0x50 => {
                self.state = State::DcsEntry;
                self.clear();
                None
            }
            0x58 | 0x5E | 0x5F => {
                self.state = State::SosPmApcString;
                None
            }
            0x5B => {
                self.state = State::CsiEntry;
                self.clear();
                None
            }
            0x5D => {
                self.state = State::OscString;
                self.clear();
                None
            }
            0x7F => None,
            0x1B => {
                self.clear();
                None
            }
            _ => {
                self.state = State::Ground;
                None
            }
        }
    }

    fn escape_intermediate(&mut self, byte: u8) -> Option<AnsiAction> {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => Some(AnsiAction::Execute(byte)),
            0x20..=0x2F => {
                self.intermediates.push(byte);
                None
            }
            0x30..=0x7E => {
                self.state = State::Ground;
                Some(AnsiAction::EscDispatch {
                    intermediates: self.intermediates.clone(),
                    final_byte: byte as char,
                })
            }
            0x7F => None,
            0x1B => {
                self.state = State::Escape;
                self.clear();
                None
            }
            _ => {
                self.state = State::Ground;
                None
            }
        }
    }

    fn csi_entry(&mut self, byte: u8) -> Option<AnsiAction> {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => Some(AnsiAction::Execute(byte)),
            0x20..=0x2F => {
                self.intermediates.push(byte);
                self.state = State::CsiIntermediate;
                None
            }
            0x30..=0x39 => {
                self.current_param = (byte - 0x30) as u16;
                self.state = State::CsiParam;
                None
            }
            0x3A => {
                self.state = State::CsiIgnore;
                None
            }
            0x3B => {
                self.collect_param();
                self.state = State::CsiParam;
                None
            }
            0x3C..=0x3F => {
                self.intermediates.push(byte);
                self.state = State::CsiParam;
                None
            }
            0x40..=0x7E => {
                self.collect_param();
                self.state = State::Ground;
                Some(AnsiAction::CsiDispatch {
                    params: self.params.clone(),
                    intermediates: self.intermediates.clone(),
                    final_byte: byte as char,
                })
            }
            0x7F => None,
            0x1B => {
                self.state = State::Escape;
                self.clear();
                None
            }
            _ => {
                self.state = State::Ground;
                None
            }
        }
    }

    fn csi_param(&mut self, byte: u8) -> Option<AnsiAction> {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => Some(AnsiAction::Execute(byte)),
            0x20..=0x2F => {
                self.intermediates.push(byte);
                self.state = State::CsiIntermediate;
                None
            }
            0x30..=0x39 => {
                self.current_param = self.current_param.saturating_mul(10).saturating_add((byte - 0x30) as u16);
                None
            }
            0x3A => {
                self.state = State::CsiIgnore;
                None
            }
            0x3B => {
                self.collect_param();
                None
            }
            0x3C..=0x3F => {
                self.state = State::CsiIgnore;
                None
            }
            0x40..=0x7E => {
                self.collect_param();
                self.state = State::Ground;
                Some(AnsiAction::CsiDispatch {
                    params: self.params.clone(),
                    intermediates: self.intermediates.clone(),
                    final_byte: byte as char,
                })
            }
            0x7F => None,
            0x1B => {
                self.state = State::Escape;
                self.clear();
                None
            }
            _ => {
                self.state = State::Ground;
                None
            }
        }
    }

    fn csi_intermediate(&mut self, byte: u8) -> Option<AnsiAction> {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => Some(AnsiAction::Execute(byte)),
            0x20..=0x2F => {
                self.intermediates.push(byte);
                None
            }
            0x30..=0x3F => {
                self.state = State::CsiIgnore;
                None
            }
            0x40..=0x7E => {
                self.collect_param();
                self.state = State::Ground;
                Some(AnsiAction::CsiDispatch {
                    params: self.params.clone(),
                    intermediates: self.intermediates.clone(),
                    final_byte: byte as char,
                })
            }
            0x7F => None,
            0x1B => {
                self.state = State::Escape;
                self.clear();
                None
            }
            _ => {
                self.state = State::Ground;
                None
            }
        }
    }

    fn csi_ignore(&mut self, byte: u8) -> Option<AnsiAction> {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => Some(AnsiAction::Execute(byte)),
            0x20..=0x3F | 0x7F => None,
            0x40..=0x7E => {
                self.state = State::Ground;
                None
            }
            0x1B => {
                self.state = State::Escape;
                self.clear();
                None
            }
            _ => {
                self.state = State::Ground;
                None
            }
        }
    }

    fn dcs_entry(&mut self, byte: u8) -> Option<AnsiAction> {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F | 0x7F => None,
            0x20..=0x2F => {
                self.intermediates.push(byte);
                self.state = State::DcsIntermediate;
                None
            }
            0x30..=0x39 => {
                self.current_param = (byte - 0x30) as u16;
                self.state = State::DcsParam;
                None
            }
            0x3A => {
                self.state = State::DcsIgnore;
                None
            }
            0x3B => {
                self.collect_param();
                self.state = State::DcsParam;
                None
            }
            0x3C..=0x3F => {
                self.intermediates.push(byte);
                self.state = State::DcsParam;
                None
            }
            0x40..=0x7E => {
                self.collect_param();
                self.state = State::DcsPassthrough;
                Some(AnsiAction::DcsHook {
                    params: self.params.clone(),
                    intermediates: self.intermediates.clone(),
                    final_byte: byte as char,
                })
            }
            0x1B => {
                self.state = State::Escape;
                self.clear();
                None
            }
            _ => {
                self.state = State::Ground;
                None
            }
        }
    }

    fn dcs_param(&mut self, byte: u8) -> Option<AnsiAction> {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F | 0x7F => None,
            0x20..=0x2F => {
                self.intermediates.push(byte);
                self.state = State::DcsIntermediate;
                None
            }
            0x30..=0x39 => {
                self.current_param = self.current_param.saturating_mul(10).saturating_add((byte - 0x30) as u16);
                None
            }
            0x3A | 0x3C..=0x3F => {
                self.state = State::DcsIgnore;
                None
            }
            0x3B => {
                self.collect_param();
                None
            }
            0x40..=0x7E => {
                self.collect_param();
                self.state = State::DcsPassthrough;
                Some(AnsiAction::DcsHook {
                    params: self.params.clone(),
                    intermediates: self.intermediates.clone(),
                    final_byte: byte as char,
                })
            }
            0x1B => {
                self.state = State::Escape;
                self.clear();
                None
            }
            _ => {
                self.state = State::Ground;
                None
            }
        }
    }

    fn dcs_intermediate(&mut self, byte: u8) -> Option<AnsiAction> {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F | 0x7F => None,
            0x20..=0x2F => {
                self.intermediates.push(byte);
                None
            }
            0x30..=0x3F => {
                self.state = State::DcsIgnore;
                None
            }
            0x40..=0x7E => {
                self.collect_param();
                self.state = State::DcsPassthrough;
                Some(AnsiAction::DcsHook {
                    params: self.params.clone(),
                    intermediates: self.intermediates.clone(),
                    final_byte: byte as char,
                })
            }
            0x1B => {
                self.state = State::Escape;
                self.clear();
                None
            }
            _ => {
                self.state = State::Ground;
                None
            }
        }
    }

    fn dcs_passthrough(&mut self, byte: u8) -> Option<AnsiAction> {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F | 0x20..=0x7E => Some(AnsiAction::DcsPut(byte)),
            0x7F => None,
            0x9C => {
                self.state = State::Ground;
                Some(AnsiAction::DcsUnhook)
            }
            0x1B => {
                self.state = State::Escape;
                self.clear();
                Some(AnsiAction::DcsUnhook)
            }
            _ => {
                self.state = State::Ground;
                Some(AnsiAction::DcsUnhook)
            }
        }
    }

    fn dcs_ignore(&mut self, byte: u8) -> Option<AnsiAction> {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F | 0x20..=0x7F => None,
            0x9C => {
                self.state = State::Ground;
                None
            }
            0x1B => {
                self.state = State::Escape;
                self.clear();
                None
            }
            _ => {
                self.state = State::Ground;
                None
            }
        }
    }

    fn osc_string_state(&mut self, byte: u8) -> Option<AnsiAction> {
        match byte {
            0x00..=0x06 | 0x08..=0x17 | 0x19 | 0x1C..=0x1F => None,
            0x07 | 0x9C => {
                self.state = State::Ground;
                let params: Vec<String> = self.osc_string.split(';').map(String::from).collect();
                Some(AnsiAction::OscDispatch { params })
            }
            0x20..=0x7F => {
                self.osc_string.push(byte as char);
                None
            }
            0x1B => {
                self.state = State::Escape;
                let params: Vec<String> = self.osc_string.split(';').map(String::from).collect();
                self.clear();
                Some(AnsiAction::OscDispatch { params })
            }
            _ => {
                self.osc_string.push(byte as char);
                None
            }
        }
    }

    fn sos_pm_apc_string(&mut self, byte: u8) -> Option<AnsiAction> {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F | 0x20..=0x7F => None,
            0x9C => {
                self.state = State::Ground;
                None
            }
            0x1B => {
                self.state = State::Escape;
                self.clear();
                None
            }
            _ => None,
        }
    }
}

pub fn parse_sgr(params: &[u16], style: &mut CellStyle, default_fg: Color32) {
    let mut i = 0;
    while i < params.len() {
        match params[i] {
            0 => {
                *style = CellStyle {
                    fg: default_fg,
                    ..CellStyle::default()
                };
            }
            1 => style.bold = true,
            2 => style.dim = true,
            3 => style.italic = true,
            4 => style.underline = true,
            5 | 6 => style.blink = true,
            7 => style.inverse = true,
            8 => {} // Hidden - not implemented
            9 => style.strikethrough = true,
            21 => style.bold = false,
            22 => {
                style.bold = false;
                style.dim = false;
            }
            23 => style.italic = false,
            24 => style.underline = false,
            25 => style.blink = false,
            27 => style.inverse = false,
            28 => {} // Not hidden
            29 => style.strikethrough = false,
            30..=37 => {
                style.fg = ANSI_COLORS[(params[i] - 30) as usize];
            }
            38 => {
                if i + 2 < params.len() && params[i + 1] == 5 {
                    style.fg = color_from_256(params[i + 2]);
                    i += 2;
                } else if i + 4 < params.len() && params[i + 1] == 2 {
                    style.fg = Color32::from_rgb(
                        params[i + 2] as u8,
                        params[i + 3] as u8,
                        params[i + 4] as u8,
                    );
                    i += 4;
                }
            }
            39 => style.fg = default_fg,
            40..=47 => {
                style.bg = ANSI_COLORS[(params[i] - 40) as usize];
            }
            48 => {
                if i + 2 < params.len() && params[i + 1] == 5 {
                    style.bg = color_from_256(params[i + 2]);
                    i += 2;
                } else if i + 4 < params.len() && params[i + 1] == 2 {
                    style.bg = Color32::from_rgb(
                        params[i + 2] as u8,
                        params[i + 3] as u8,
                        params[i + 4] as u8,
                    );
                    i += 4;
                }
            }
            49 => style.bg = Color32::TRANSPARENT,
            90..=97 => {
                style.fg = ANSI_COLORS[(params[i] - 90 + 8) as usize];
            }
            100..=107 => {
                style.bg = ANSI_COLORS[(params[i] - 100 + 8) as usize];
            }
            _ => {}
        }
        i += 1;
    }
}

fn color_from_256(index: u16) -> Color32 {
    let index = index as usize;
    if index < 16 {
        return ANSI_COLORS[index];
    }
    if index < 232 {
        let index = index - 16;
        let r = (index / 36) % 6;
        let g = (index / 6) % 6;
        let b = index % 6;
        let r = if r > 0 { (r * 40 + 55) as u8 } else { 0 };
        let g = if g > 0 { (g * 40 + 55) as u8 } else { 0 };
        let b = if b > 0 { (b * 40 + 55) as u8 } else { 0 };
        return Color32::from_rgb(r, g, b);
    }
    let gray = ((index - 232) * 10 + 8) as u8;
    Color32::from_rgb(gray, gray, gray)
}

impl Default for AnsiParser {
    fn default() -> Self {
        Self::new()
    }
}

