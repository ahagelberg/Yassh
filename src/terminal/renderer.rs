use super::buffer::TerminalBuffer;
use super::emulator::TerminalEmulator;
use crate::config::CursorType;
use crate::selection::Selection;
use egui::{Color32, FontFamily, FontId, Pos2, Rect, Response, Sense, Ui, Vec2};

// Rendering constants
const CURSOR_BLINK_INTERVAL_SECS: f64 = 0.5;
const CURSOR_BLINK_INTERVAL_MS: u64 = 250;
const CELL_WIDTH_MULTIPLIER: f32 = 0.6;
const CELL_HEIGHT_MULTIPLIER: f32 = 1.2;
const UNDERLINE_OFFSET_PIXELS: f32 = 2.0;
const UNDERLINE_STROKE_WIDTH: f32 = 1.0;
const STRIKETHROUGH_STROKE_WIDTH: f32 = 1.0;
const CURSOR_UNDERLINE_THICKNESS_MULTIPLIER: f32 = 0.15;
const CURSOR_UNDERLINE_MIN_THICKNESS: f32 = 2.0;
const CURSOR_VERTICAL_WIDTH_MULTIPLIER: f32 = 0.1;
const CURSOR_VERTICAL_MIN_WIDTH: f32 = 1.0;
const SCROLLBAR_WIDTH: f32 = 12.0;
const SCROLLBAR_MIN_THUMB_HEIGHT: f32 = 20.0;

pub struct TerminalRenderer {
    font_size: f32,
    cell_width: f32,
    cell_height: f32,
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

    #[allow(dead_code)]
    pub fn calculate_size(&self, cols: usize, rows: usize) -> Vec2 {
        Vec2::new(
            cols as f32 * self.cell_width,
            rows as f32 * self.cell_height,
        )
    }

    #[allow(dead_code)]
    pub fn calculate_grid_size(&self, available_size: Vec2) -> (usize, usize) {
        let cols = (available_size.x / self.cell_width.max(1.0)).floor() as usize;
        let rows = (available_size.y / self.cell_height.max(1.0)).floor() as usize;
        (cols.max(1), rows.max(1))
    }

    pub fn render(
        &mut self,
        ui: &mut Ui,
        emulator: &TerminalEmulator,
        selection: Option<&Selection>,
        background: Color32,
        focused: bool,
        invert_colors: bool,
        scroll_offset: usize,
    ) -> (Response, usize, bool, usize, usize) {
        let buffer = emulator.buffer();
        let available = ui.available_size();
        let viewport_cols = (available.x / self.cell_width).floor() as usize;
        let viewport_rows = (available.y / self.cell_height).floor() as usize;
        let terminal_width = viewport_cols as f32 * self.cell_width;
        let terminal_height = viewport_rows as f32 * self.cell_height;
        let total_lines = buffer.total_lines();
        let max_scroll = if total_lines <= viewport_rows {
            0
        } else {
            total_lines - viewport_rows
        };
        let mut new_scroll_offset = scroll_offset;
        if new_scroll_offset == usize::MAX {
            new_scroll_offset = max_scroll;
        }
        let show_scrollbar = max_scroll > 0;
        let content_width = terminal_width;
        let total_width = content_width + if show_scrollbar { SCROLLBAR_WIDTH } else { 0.0 };
        let desired_size = Vec2::new(total_width, terminal_height);
        let (outer_rect, outer_response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
        let terminal_rect = Rect::from_min_size(outer_rect.min, Vec2::new(content_width, terminal_height));
        if !ui.is_rect_visible(terminal_rect) {
            let is_at_bottom = new_scroll_offset >= max_scroll;
            return (outer_response, new_scroll_offset, is_at_bottom, viewport_cols, viewport_rows);
        }
        let pointer_pos = ui.ctx().pointer_latest_pos();
        let is_over_terminal = pointer_pos.map_or(false, |p| p.x >= outer_rect.min.x && p.x < outer_rect.min.x + content_width && p.y >= outer_rect.min.y && p.y < outer_rect.max.y);
        if is_over_terminal {
            let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
            if scroll_delta != 0.0 {
                let lines_to_scroll = (scroll_delta / self.cell_height).round() as i32;
                let new_offset_i32 = new_scroll_offset as i32 - lines_to_scroll;
                let new_offset = new_offset_i32.max(0) as usize;
                new_scroll_offset = new_offset.min(max_scroll);
            }
        }
        new_scroll_offset = new_scroll_offset.min(max_scroll);
        let visible_start = new_scroll_offset;
        let visible_end = (visible_start + viewport_rows).min(total_lines);
        let painter = ui.painter_at(terminal_rect);
        let bg_color = if emulator.reverse_video() {
            buffer.default_fg()
        } else {
            background
        };
        painter.rect_filled(terminal_rect, 0.0, bg_color);
        for line_idx in visible_start..visible_end {
            let screen_row = line_idx - visible_start;
            self.render_line(
                &painter,
                buffer,
                line_idx,
                screen_row,
                terminal_rect.min,
                selection,
                emulator.reverse_video(),
                invert_colors,
            );
        }
        if focused && emulator.cursor_visible() {
            self.update_cursor_blink(ui.ctx().input(|i| i.time));
            if self.cursor_visible {
                self.render_cursor(&painter, buffer, terminal_rect.min, viewport_rows, visible_start, emulator.reverse_video(), invert_colors);
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(CURSOR_BLINK_INTERVAL_MS));
        }
        if show_scrollbar {
            let scrollbar_rect = Rect::from_min_size(
                Pos2::new(outer_rect.min.x + content_width, outer_rect.min.y),
                Vec2::new(SCROLLBAR_WIDTH, terminal_height),
            );
            let scrollbar_response = ui.allocate_response(scrollbar_rect.size(), Sense::click_and_drag());
            let new_offset = self.render_scrollbar(
                ui,
                scrollbar_rect,
                scrollbar_response,
                new_scroll_offset,
                max_scroll,
                total_lines,
                viewport_rows,
                background,
            );
            if let Some(new_offset_val) = new_offset {
                new_scroll_offset = new_offset_val;
            }
        }
        let is_at_bottom = new_scroll_offset >= max_scroll;
        (outer_response, new_scroll_offset, is_at_bottom, viewport_cols, viewport_rows)
    }

    fn render_scrollbar(
        &self,
        ui: &mut Ui,
        rect: Rect,
        response: Response,
        scroll_offset: usize,
        max_scroll: usize,
        total_lines: usize,
        viewport_rows: usize,
        _background: Color32,
    ) -> Option<usize> {
        let painter = ui.painter_at(rect);
        let scrollbar_bg = Color32::from_rgba_unmultiplied(40, 40, 40, 255);
        painter.rect_filled(rect, 0.0, scrollbar_bg);
        if max_scroll == 0 {
            return None;
        }
        let scrollbar_height = rect.height();
        let thumb_height = (scrollbar_height * (viewport_rows as f32 / total_lines as f32)).max(SCROLLBAR_MIN_THUMB_HEIGHT);
        let scrollable_height = scrollbar_height - thumb_height;
        let thumb_position = if max_scroll > 0 {
            (scroll_offset as f32 / max_scroll as f32) * scrollable_height
        } else {
            0.0
        };
        let thumb_rect = Rect::from_min_size(
            Pos2::new(rect.min.x + 2.0, rect.min.y + thumb_position),
            Vec2::new(SCROLLBAR_WIDTH - 4.0, thumb_height),
        );
        let thumb_color = if response.hovered() || response.dragged() {
            Color32::from_rgba_unmultiplied(120, 120, 120, 255)
        } else {
            Color32::from_rgba_unmultiplied(80, 80, 80, 255)
        };
        painter.rect_filled(thumb_rect, 2.0, thumb_color);
        let mut new_scroll_offset = None;
        if response.dragged() {
            if let Some(pos) = ui.ctx().pointer_latest_pos() {
                let relative_y = (pos.y - rect.min.y - thumb_height / 2.0).max(0.0).min(scrollable_height);
                let new_offset = if scrollable_height > 0.0 {
                    ((relative_y / scrollable_height) * max_scroll as f32).round() as usize
                } else {
                    0
                };
                new_scroll_offset = Some(new_offset.min(max_scroll));
            }
        } else if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let relative_y = pos.y - rect.min.y;
                let new_offset = if scrollable_height > 0.0 {
                    ((relative_y / scrollable_height) * max_scroll as f32).round() as usize
                } else {
                    0
                };
                new_scroll_offset = Some(new_offset.min(max_scroll));
            }
        }
        new_scroll_offset
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
        invert_colors: bool,
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
            let selection_bg = if is_selected {
                std::mem::swap(&mut fg, &mut bg);
                Some(bg)
            } else {
                None
            };
            if invert_colors {
                std::mem::swap(&mut fg, &mut bg);
            }
            if bg != Color32::TRANSPARENT && bg != buffer.default_bg() && !is_selected {
                painter.rect_filled(cell_rect, 0.0, bg);
            }
            if let Some(sel_bg) = selection_bg {
                // Draw selection highlight 2 pixels above text, 2 pixels less from bottom
                const SELECTION_TOP_OFFSET: f32 = 2.0;
                const SELECTION_BOTTOM_OFFSET: f32 = 2.0;
                let selection_rect = Rect::from_min_max(
                    Pos2::new(cell_rect.min.x, cell_rect.min.y - SELECTION_TOP_OFFSET),
                    Pos2::new(cell_rect.max.x, cell_rect.max.y - SELECTION_BOTTOM_OFFSET),
                );
                painter.rect_filled(selection_rect, 0.0, sel_bg);
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
                let strike_y = ((y + next_y) / 2.0).floor();
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
        actual_rows: usize,
        visible_start: usize,
        reverse_video: bool,
        invert_colors: bool,
    ) {
        if self.cursor_type == CursorType::None {
            return;
        }
        let cursor = buffer.cursor();
        let scrollback_len = buffer.scrollback_len();
        let cursor_line = scrollback_len + cursor.row;
        let visible_end = visible_start + actual_rows;
        if cursor_line < visible_start || cursor_line >= visible_end {
            return;
        }
        let screen_row = cursor_line - visible_start;
        let x = (origin.x + cursor.col as f32 * self.cell_width).floor();
        let y = (origin.y + screen_row as f32 * self.cell_height).floor();
        let cursor_color = if reverse_video {
            buffer.default_bg()
        } else {
            buffer.default_fg()
        };
        let cursor_color = if invert_colors {
            // Invert cursor color
            if cursor_color == buffer.default_fg() {
                buffer.default_bg()
            } else {
                buffer.default_fg()
            }
        } else {
            cursor_color
        };
        match self.cursor_type {
            CursorType::Block => {
                let cursor_rect = Rect::from_min_size(
                    Pos2::new(x, y),
                    Vec2::new(self.cell_width, self.cell_height),
                );
                painter.rect_filled(cursor_rect, 0.0, cursor_color);
                // Get the line at cursor position (scrollback_len + cursor.row)
                if let Some(line) = buffer.get_line(scrollback_len + cursor.row) {
                    if let Some(cell) = line.get(cursor.col) {
                        if cell.ch != ' ' {
                            let mut text_color = if reverse_video {
                                buffer.default_fg()
                            } else {
                                buffer.default_bg()
                            };
                            if invert_colors {
                                // Invert text color
                                text_color = if text_color == buffer.default_fg() {
                                    buffer.default_bg()
                                } else {
                                    buffer.default_fg()
                                };
                            }
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


    pub fn cell_at_pos(&self, pos: Pos2, origin: Pos2, buffer: &TerminalBuffer, rect_height: f32, scroll_offset: usize) -> Option<(usize, usize)> {
        let relative = pos - origin;
        if relative.x < 0.0 || relative.y < 0.0 {
            return None;
        }
        let col = (relative.x / self.cell_width) as usize;
        let row = (relative.y / self.cell_height) as usize;
        let actual_rows = (rect_height / self.cell_height).floor() as usize;
        if col >= buffer.cols() || row >= actual_rows {
            return None;
        }
        let line_idx = scroll_offset + row;
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

    pub fn render_line_inverted(
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
            // Invert colors for bell blink
            std::mem::swap(&mut fg, &mut bg);

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
                let strikethrough_y = (y + next_y) / 2.0;
                painter.line_segment(
                    [Pos2::new(x, strikethrough_y), Pos2::new(next_x, strikethrough_y)],
                    egui::Stroke::new(STRIKETHROUGH_STROKE_WIDTH, fg),
                );
            }
        }
    }

}

