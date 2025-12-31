use super::buffer::TerminalBuffer;
use super::emulator::TerminalEmulator;
use crate::config::CursorType;
use crate::selection::Selection;
use egui::{Color32, FontFamily, FontId, Pos2, Rect, Response, Sense, Ui, Vec2};

// Rendering constants
const CURSOR_BLINK_INTERVAL_SECS: f64 = 0.5;
const CURSOR_BLINK_INTERVAL_MS: u64 = 250;
const MIN_CELL_WIDTH: f32 = 1.0;
const MIN_CELL_HEIGHT: f32 = 1.0;
const SCROLLBAR_WIDTH: f32 = 12.0;
const SCROLLBAR_MIN_THUMB_HEIGHT: f32 = 20.0;
const CELL_WIDTH_MULTIPLIER: f32 = 0.6;
const CELL_HEIGHT_MULTIPLIER: f32 = 1.2;
const UNDERLINE_OFFSET_PIXELS: f32 = 2.0;
const UNDERLINE_STROKE_WIDTH: f32 = 1.0;
const STRIKETHROUGH_STROKE_WIDTH: f32 = 1.0;
const CURSOR_UNDERLINE_THICKNESS_MULTIPLIER: f32 = 0.15;
const CURSOR_UNDERLINE_MIN_THICKNESS: f32 = 2.0;
const CURSOR_VERTICAL_WIDTH_MULTIPLIER: f32 = 0.1;
const CURSOR_VERTICAL_MIN_WIDTH: f32 = 1.0;
const SCROLLBAR_CORNER_RADIUS: f32 = 4.0;
const SCROLLBAR_PADDING: f32 = 2.0;
const MIDPOINT_DIVISOR: f32 = 2.0;

fn scrollbar_track_color() -> Color32 {
    Color32::from_rgba_unmultiplied(128, 128, 128, 40)
}

fn scrollbar_thumb_hover_color() -> Color32 {
    Color32::from_rgba_unmultiplied(180, 180, 180, 200)
}

fn scrollbar_thumb_normal_color() -> Color32 {
    Color32::from_rgba_unmultiplied(140, 140, 140, 160)
}

pub struct TerminalRenderer {
    font_size: f32,
    cell_width: f32,
    cell_height: f32,
    scroll_offset: usize,
    cursor_blink_time: f64,
    cursor_visible: bool,
    cursor_type: CursorType,
}

impl TerminalRenderer {
    pub fn new(font_size: u32, _font_family: String, cursor_type: CursorType) -> Self {
        let font_size = font_size as f32;
        Self {
            font_size,
            cell_width: font_size * CELL_WIDTH_MULTIPLIER,
            cell_height: font_size * CELL_HEIGHT_MULTIPLIER,
            scroll_offset: 0,
            cursor_blink_time: 0.0,
            cursor_visible: true,
            cursor_type,
        }
    }

    pub fn update_font(&mut self, font_size: u32) {
        let font_size = font_size as f32;
        if self.font_size != font_size {
            self.font_size = font_size;
            self.cell_width = font_size * CELL_WIDTH_MULTIPLIER;
            self.cell_height = font_size * CELL_HEIGHT_MULTIPLIER;
        }
    }

    pub fn update_cursor_type(&mut self, cursor_type: CursorType) {
        self.cursor_type = cursor_type;
    }

    pub fn calculate_size(&self, cols: usize, rows: usize) -> Vec2 {
        Vec2::new(
            cols as f32 * self.cell_width,
            rows as f32 * self.cell_height,
        )
    }

    pub fn calculate_grid_size(&self, available_size: Vec2) -> (usize, usize) {
        let cols = (available_size.x / self.cell_width.max(MIN_CELL_WIDTH)).floor() as usize;
        let rows = (available_size.y / self.cell_height.max(MIN_CELL_HEIGHT)).floor() as usize;
        (cols.max(1), rows.max(1))
    }

    pub fn render(
        &mut self,
        ui: &mut Ui,
        emulator: &TerminalEmulator,
        selection: Option<&Selection>,
        background: Color32,
        focused: bool,
    ) -> Response {
        let buffer = emulator.buffer();
        let desired_size = self.calculate_size(buffer.cols(), buffer.rows());
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let painter = ui.painter_at(rect);
        let bg_color = if emulator.reverse_video() {
            buffer.default_fg()
        } else {
            background
        };
        painter.rect_filled(rect, 0.0, bg_color);
        let visible_start = self.scroll_offset;
        let visible_end = (visible_start + buffer.rows()).min(buffer.total_lines());
        for line_idx in visible_start..visible_end {
            let screen_row = line_idx - visible_start;
            self.render_line(
                &painter,
                buffer,
                line_idx,
                screen_row,
                rect.min,
                selection,
                emulator.reverse_video(),
            );
        }
        if focused && emulator.cursor_visible() {
            self.update_cursor_blink(ui.ctx().input(|i| i.time));
            if self.cursor_visible {
                self.render_cursor(&painter, buffer, rect.min, emulator.reverse_video());
            }
            // Request repaint for cursor blink - use after() to avoid continuous repainting
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(CURSOR_BLINK_INTERVAL_MS));
        }
        response
    }

    fn render_line(
        &self,
        painter: &egui::Painter,
        buffer: &TerminalBuffer,
        line_idx: usize,
        screen_row: usize,
        origin: Pos2,
        selection: Option<&Selection>,
        reverse_video: bool,
    ) {
        let Some(line) = buffer.get_line(line_idx) else {
            return;
        };
        // Use floor to snap to pixel boundaries and avoid sub-pixel gaps
        let y = (origin.y + screen_row as f32 * self.cell_height).floor();
        let _scrollback_offset = buffer.scrollback_len();
        for (col, cell) in line.cells().iter().enumerate() {
            let x = (origin.x + col as f32 * self.cell_width).floor();
            // Calculate next cell position to ensure no gaps
            let next_x = (origin.x + (col + 1) as f32 * self.cell_width).floor();
            let next_y = (origin.y + (screen_row + 1) as f32 * self.cell_height).floor();
            let cell_rect = Rect::from_min_max(
                Pos2::new(x, y),
                Pos2::new(next_x, next_y),
            );
            let is_selected = selection.map_or(false, |sel| {
                sel.contains(line_idx, col)
            });
            let (mut fg, mut bg) = cell.style.effective_colors(buffer.default_bg());
            if reverse_video {
                std::mem::swap(&mut fg, &mut bg);
            }
            if is_selected {
                std::mem::swap(&mut fg, &mut bg);
            }
            if bg != Color32::TRANSPARENT && bg != buffer.default_bg() {
                painter.rect_filled(cell_rect, 0.0, bg);
            }
            if cell.ch != ' ' {
                let font_id = FontId::new(self.font_size, FontFamily::Monospace);
                painter.text(
                    cell_rect.min,
                    egui::Align2::LEFT_TOP,
                    cell.ch,
                    font_id,
                    fg,
                );
            }
            if cell.style.underline {
                let underline_y = (next_y - UNDERLINE_OFFSET_PIXELS).floor();
                painter.line_segment(
                    [Pos2::new(x, underline_y), Pos2::new(next_x, underline_y)],
                    egui::Stroke::new(UNDERLINE_STROKE_WIDTH, fg),
                );
            }
            if cell.style.strikethrough {
                let strike_y = ((y + next_y) / MIDPOINT_DIVISOR).floor();
                painter.line_segment(
                    [Pos2::new(x, strike_y), Pos2::new(next_x, strike_y)],
                    egui::Stroke::new(STRIKETHROUGH_STROKE_WIDTH, fg),
                );
            }
        }
    }

    fn render_cursor(
        &self,
        painter: &egui::Painter,
        buffer: &TerminalBuffer,
        origin: Pos2,
        reverse_video: bool,
    ) {
        if self.cursor_type == CursorType::None {
            return;
        }
        let cursor = buffer.cursor();
        let scrollback_len = buffer.scrollback_len();
        let cursor_line = scrollback_len + cursor.row;
        if cursor_line < self.scroll_offset || cursor_line >= self.scroll_offset + buffer.rows() {
            return;
        }
        let screen_row = cursor_line - self.scroll_offset;
        let x = (origin.x + cursor.col as f32 * self.cell_width).floor();
        let y = (origin.y + screen_row as f32 * self.cell_height).floor();
        let cursor_color = if reverse_video {
            buffer.default_bg()
        } else {
            buffer.default_fg()
        };
        match self.cursor_type {
            CursorType::Block => {
                let cursor_rect = Rect::from_min_size(
                    Pos2::new(x, y),
                    Vec2::new(self.cell_width, self.cell_height),
                );
                painter.rect_filled(cursor_rect, 0.0, cursor_color);
                if let Some(line) = buffer.screen().get(cursor.row) {
                    if let Some(cell) = line.get(cursor.col) {
                        if cell.ch != ' ' {
                            let text_color = if reverse_video {
                                buffer.default_fg()
                            } else {
                                buffer.default_bg()
                            };
                            let font_id = FontId::new(self.font_size, FontFamily::Monospace);
                            painter.text(
                                Pos2::new(x, y),
                                egui::Align2::LEFT_TOP,
                                cell.ch,
                                font_id,
                                text_color,
                            );
                        }
                    }
                }
            }
            CursorType::Underline => {
                let underline_thickness = (self.font_size * CURSOR_UNDERLINE_THICKNESS_MULTIPLIER).max(CURSOR_UNDERLINE_MIN_THICKNESS);
                let underline_y = (y + self.cell_height - underline_thickness).floor();
                painter.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(x, underline_y),
                        Vec2::new(self.cell_width, underline_thickness),
                    ),
                    0.0,
                    cursor_color,
                );
            }
            CursorType::Vertical => {
                let vertical_width = (self.font_size * CURSOR_VERTICAL_WIDTH_MULTIPLIER).max(CURSOR_VERTICAL_MIN_WIDTH);
                painter.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(x, y),
                        Vec2::new(vertical_width, self.cell_height),
                    ),
                    0.0,
                    cursor_color,
                );
            }
            CursorType::None => {}
        }
    }

    fn update_cursor_blink(&mut self, time: f64) {
        let elapsed = time - self.cursor_blink_time;
        if elapsed >= CURSOR_BLINK_INTERVAL_SECS {
            self.cursor_visible = !self.cursor_visible;
            self.cursor_blink_time = time;
        }
    }

    #[allow(dead_code)]
    pub fn reset_cursor_blink(&mut self) {
        self.cursor_visible = true;
        self.cursor_blink_time = 0.0;
    }

    pub fn scroll_to_bottom(&mut self, buffer: &TerminalBuffer) {
        let total = buffer.total_lines();
        let rows = buffer.rows();
        self.scroll_offset = total.saturating_sub(rows);
    }

    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    pub fn scroll_down(&mut self, lines: usize, buffer: &TerminalBuffer) {
        let max_offset = buffer.total_lines().saturating_sub(buffer.rows());
        self.scroll_offset = (self.scroll_offset + lines).min(max_offset);
    }

    pub fn is_at_bottom(&self, buffer: &TerminalBuffer) -> bool {
        let max_offset = buffer.total_lines().saturating_sub(buffer.rows());
        self.scroll_offset >= max_offset
    }

    #[allow(dead_code)]
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    #[allow(dead_code)]
    pub fn set_scroll_offset(&mut self, offset: usize) {
        self.scroll_offset = offset;
    }

    pub fn cell_at_pos(&self, pos: Pos2, origin: Pos2, buffer: &TerminalBuffer) -> Option<(usize, usize)> {
        let relative = pos - origin;
        if relative.x < 0.0 || relative.y < 0.0 {
            return None;
        }
        let col = (relative.x / self.cell_width) as usize;
        let row = (relative.y / self.cell_height) as usize;
        if col >= buffer.cols() || row >= buffer.rows() {
            return None;
        }
        let line_idx = self.scroll_offset + row;
        if line_idx >= buffer.total_lines() {
            return None;
        }
        Some((line_idx, col))
    }

    #[allow(dead_code)]
    pub fn cell_width(&self) -> f32 {
        self.cell_width
    }

    #[allow(dead_code)]
    pub fn cell_height(&self) -> f32 {
        self.cell_height
    }

    /// Renders a scrollbar and returns true if scroll position was changed via drag
    pub fn render_scrollbar(
        &mut self,
        ui: &mut Ui,
        buffer: &TerminalBuffer,
        terminal_rect: Rect,
    ) -> bool {
        let total_lines = buffer.total_lines();
        let visible_lines = buffer.rows();
        
        // Only show scrollbar if there's content to scroll
        if total_lines <= visible_lines {
            return false;
        }
        
        let scrollbar_rect = Rect::from_min_size(
            Pos2::new(terminal_rect.max.x - SCROLLBAR_WIDTH, terminal_rect.min.y),
            Vec2::new(SCROLLBAR_WIDTH, terminal_rect.height()),
        );
        
        let painter = ui.painter_at(scrollbar_rect);
        
        // Draw scrollbar track
        painter.rect_filled(scrollbar_rect, SCROLLBAR_CORNER_RADIUS, scrollbar_track_color());
        
        // Calculate thumb position and size
        let content_height = total_lines as f32;
        let viewport_height = visible_lines as f32;
        let thumb_height_ratio = viewport_height / content_height;
        let thumb_height = (scrollbar_rect.height() * thumb_height_ratio)
            .max(SCROLLBAR_MIN_THUMB_HEIGHT)
            .min(scrollbar_rect.height());
        
        let max_scroll = total_lines.saturating_sub(visible_lines);
        let scroll_ratio = if max_scroll > 0 {
            self.scroll_offset as f32 / max_scroll as f32
        } else {
            0.0
        };
        
        let available_track = scrollbar_rect.height() - thumb_height;
        let thumb_top = scrollbar_rect.min.y + (scroll_ratio * available_track);
        
        let thumb_rect = Rect::from_min_size(
            Pos2::new(scrollbar_rect.min.x + SCROLLBAR_PADDING, thumb_top),
            Vec2::new(SCROLLBAR_WIDTH - (SCROLLBAR_PADDING * MIDPOINT_DIVISOR), thumb_height),
        );
        
        // Handle scrollbar interaction
        let response = ui.interact(scrollbar_rect, ui.id().with("scrollbar"), Sense::click_and_drag());
        
        let thumb_color = if response.dragged() || response.hovered() {
            scrollbar_thumb_hover_color()
        } else {
            scrollbar_thumb_normal_color()
        };
        
        painter.rect_filled(thumb_rect, SCROLLBAR_CORNER_RADIUS, thumb_color);
        
        // Handle drag to scroll
        if response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                let relative_y = pos.y - scrollbar_rect.min.y - (thumb_height / MIDPOINT_DIVISOR);
                let ratio = (relative_y / available_track).clamp(0.0, 1.0);
                let new_offset = (ratio * max_scroll as f32).round() as usize;
                if new_offset != self.scroll_offset {
                    self.scroll_offset = new_offset;
                    return true;
                }
            }
        }
        
        // Handle click to jump
        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let relative_y = pos.y - scrollbar_rect.min.y - (thumb_height / MIDPOINT_DIVISOR);
                let ratio = (relative_y / available_track).clamp(0.0, 1.0);
                let new_offset = (ratio * max_scroll as f32).round() as usize;
                self.scroll_offset = new_offset;
                return true;
            }
        }
        
        false
    }
}

