use crate::debug;
use egui::Color32;
use std::collections::VecDeque;

const DEFAULT_COLS: usize = 80;
const DEFAULT_ROWS: usize = 24;
const MIN_BUFFER_SIZE: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellStyle {
    pub fg: Color32,
    pub bg: Color32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub inverse: bool,
    pub dim: bool,
    pub blink: bool,
}

impl Default for CellStyle {
    fn default() -> Self {
        Self {
            fg: Color32::from_rgb(204, 204, 204),
            bg: Color32::TRANSPARENT,
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
            inverse: false,
            dim: false,
            blink: false,
        }
    }
}

impl CellStyle {
    pub fn effective_colors(&self, default_bg: Color32) -> (Color32, Color32) {
        let bg = if self.bg == Color32::TRANSPARENT {
            default_bg
        } else {
            self.bg
        };
        if self.inverse {
            (bg, self.fg)
        } else {
            (self.fg, bg)
        }
    }
}

#[derive(Debug, Clone)]
pub struct Cell {
    pub ch: char,
    pub style: CellStyle,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            style: CellStyle::default(),
        }
    }
}

impl Cell {
    pub fn new(ch: char, style: CellStyle) -> Self {
        Self { ch, style }
    }
}

#[derive(Debug, Clone)]
pub struct Line {
    cells: Vec<Cell>,
    wrapped: bool,
}

impl Line {
    #[allow(dead_code)]
    pub fn new(cols: usize) -> Self {
        Self {
            cells: vec![Cell::default(); cols],
            wrapped: false,
        }
    }

    pub fn with_style(cols: usize, style: CellStyle) -> Self {
        Self {
            cells: vec![Cell { ch: ' ', style }; cols],
            wrapped: false,
        }
    }

    pub fn get(&self, col: usize) -> Option<&Cell> {
        self.cells.get(col)
    }

    #[allow(dead_code)]
    pub fn get_mut(&mut self, col: usize) -> Option<&mut Cell> {
        self.cells.get_mut(col)
    }

    pub fn set(&mut self, col: usize, cell: Cell) {
        if col < self.cells.len() {
            self.cells[col] = cell;
        }
    }

    pub fn len(&self) -> usize {
        self.cells.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    pub fn clear(&mut self, style: CellStyle) {
        for cell in &mut self.cells {
            cell.ch = ' ';
            cell.style = style;
        }
    }

    pub fn clear_range(&mut self, start: usize, end: usize, style: CellStyle) {
        let end = end.min(self.cells.len());
        for cell in &mut self.cells[start..end] {
            cell.ch = ' ';
            cell.style = style;
        }
    }

    pub fn is_wrapped(&self) -> bool {
        self.wrapped
    }

    #[allow(dead_code)]
    pub fn set_wrapped(&mut self, wrapped: bool) {
        self.wrapped = wrapped;
    }

    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    #[allow(dead_code)]
    pub fn to_string(&self) -> String {
        self.cells.iter().map(|c| c.ch).collect::<String>().trim_end().to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CursorPosition {
    pub row: usize,
    pub col: usize,
}

impl Default for CursorPosition {
    fn default() -> Self {
        Self { row: 0, col: 0 }
    }
}

/// Unified terminal buffer - one continuous buffer where the "screen" is a
/// fixed-size viewport into the buffer. The server_screen_start tracks where the
/// server's view begins in the buffer.
pub struct TerminalBuffer {
    // All lines in one buffer - history + current screen
    lines: VecDeque<Line>,
    cols: usize,
    rows: usize,
    max_lines: usize,
    // Buffer index where the server's screen starts
    // The screen occupies lines[server_screen_start..server_screen_start+rows]
    server_screen_start: usize,
    // Cursor position relative to the screen (0 = first screen line, rows-1 = last)
    cursor: CursorPosition,
    saved_cursor: CursorPosition,
    current_style: CellStyle,
    // Scroll region (relative to screen, 0-based)
    scroll_top: usize,
    scroll_bottom: usize,
    origin_mode: bool,
    default_fg: Color32,
    default_bg: Color32,
}

impl TerminalBuffer {

    pub fn new(max_scrollback: usize, default_fg: Color32, default_bg: Color32) -> Self {
        let rows = DEFAULT_ROWS;
        let cols = DEFAULT_COLS;
        let max_lines = max_scrollback.max(MIN_BUFFER_SIZE) + rows;
        let mut style = CellStyle::default();
        style.fg = default_fg;
        // Start with empty buffer - lines will be added as content is written
        let lines: VecDeque<Line> = VecDeque::new();
        Self {
            lines,
            cols,
            rows,
            max_lines,
            server_screen_start: 0, // Server's screen starts at the beginning of the buffer
            cursor: CursorPosition::default(),
            saved_cursor: CursorPosition::default(),
            current_style: style,
            scroll_top: 0,
            scroll_bottom: rows - 1,
            origin_mode: false,
            default_fg,
            default_bg,
        }
    }

    /// Convert server screen-relative row to absolute buffer index
    fn server_screen_to_buffer(&self, screen_row: usize) -> usize {
        self.server_screen_start + screen_row
    }


    /// Get the buffer index of the last line in the server's screen
    /// This is the line that should be at the bottom when "scrolled to bottom"
    #[allow(dead_code)]
    pub fn server_screen_end(&self) -> usize {
        // The server's screen ends at server_screen_start + rows - 1
        // But if the buffer doesn't have enough lines, use total_lines - 1
        let theoretical_end = self.server_screen_start + self.rows.saturating_sub(1);
        theoretical_end.min(self.lines.len().saturating_sub(1))
    }

    fn trim_buffer(&mut self) {
        while self.lines.len() > self.max_lines {
            self.lines.pop_front();
            // Adjust server_screen_start since we removed a line from the front
            self.server_screen_start = self.server_screen_start.saturating_sub(1);
        }
    }

    /// Ensure buffer has enough lines to access the given index
    /// Called when server sends data that requires more lines
    fn ensure_line_exists(&mut self, buffer_idx: usize) {
        while self.lines.len() <= buffer_idx {
            self.lines.push_back(Line::with_style(self.cols, self.current_style));
        }
    }

    pub fn put_char(&mut self, ch: char) {
        // Handle line wrap
        if self.cursor.col >= self.cols {
            self.cursor.col = 0;
            self.new_line();
        }
        // Ensure cursor row is valid
        self.cursor.row = self.cursor.row.min(self.rows.saturating_sub(1));
        // Ensure the line exists (server is writing data, so create line if needed)
        let idx = self.server_screen_to_buffer(self.cursor.row);
        self.ensure_line_exists(idx);
        // Now write the character
        let col = self.cursor.col;
        let style = self.current_style;
        if let Some(line) = self.lines.get_mut(idx) {
            line.set(col, Cell::new(ch, style));
            self.cursor.col += 1;
        }
    }

    pub fn new_line(&mut self) {
        if self.cursor.row >= self.scroll_bottom {
            self.scroll_up(1);
        } else {
            self.cursor.row += 1;
        }
    }

    pub fn carriage_return(&mut self) {
        self.cursor.col = 0;
    }

    pub fn backspace(&mut self) {
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        }
    }

    pub fn tab(&mut self) {
        const TAB_WIDTH: usize = 8;
        let next_tab = ((self.cursor.col / TAB_WIDTH) + 1) * TAB_WIDTH;
        self.cursor.col = next_tab.min(self.cols.saturating_sub(1));
    }

    pub fn scroll_up(&mut self, count: usize) {
        for _ in 0..count {
            if self.scroll_top == 0 {
                // Scrolling full screen - add new line at bottom
                // The old top line becomes history, server_screen_start advances
                self.lines.push_back(Line::with_style(self.cols, self.current_style));
                self.server_screen_start += 1;
                self.trim_buffer();
            } else {
                // Scroll region - remove line at scroll_top, insert at scroll_bottom
                let top_idx = self.server_screen_to_buffer(self.scroll_top);
                let bottom_idx = self.server_screen_to_buffer(self.scroll_bottom);
                if top_idx < self.lines.len() {
                    self.lines.remove(top_idx);
                    let insert_idx = bottom_idx.min(self.lines.len());
                    self.lines.insert(insert_idx, Line::with_style(self.cols, self.current_style));
                }
            }
        }
    }

    pub fn scroll_down(&mut self, count: usize) {
        for _ in 0..count {
            let top_idx = self.server_screen_to_buffer(self.scroll_top);
            let bottom_idx = self.server_screen_to_buffer(self.scroll_bottom);
            if bottom_idx < self.lines.len() {
                self.lines.remove(bottom_idx);
                self.lines.insert(top_idx, Line::with_style(self.cols, self.current_style));
            }
        }
    }

    pub fn set_cursor_position(&mut self, row: usize, col: usize) {
        let row = if self.origin_mode {
            (self.scroll_top + row).min(self.scroll_bottom)
        } else {
            row.min(self.rows.saturating_sub(1))
        };
        self.cursor.row = row;
        self.cursor.col = col.min(self.cols.saturating_sub(1));
    }

    pub fn move_cursor_up(&mut self, count: usize) {
        let min_row = if self.origin_mode { self.scroll_top } else { 0 };
        self.cursor.row = self.cursor.row.saturating_sub(count).max(min_row);
    }

    pub fn move_cursor_down(&mut self, count: usize) {
        let max_row = if self.origin_mode { self.scroll_bottom } else { self.rows - 1 };
        self.cursor.row = (self.cursor.row + count).min(max_row);
    }

    pub fn move_cursor_left(&mut self, count: usize) {
        self.cursor.col = self.cursor.col.saturating_sub(count);
    }

    pub fn move_cursor_right(&mut self, count: usize) {
        self.cursor.col = (self.cursor.col + count).min(self.cols.saturating_sub(1));
    }

    pub fn save_cursor(&mut self) {
        self.saved_cursor = self.cursor;
    }

    pub fn restore_cursor(&mut self) {
        self.cursor = self.saved_cursor;
    }

    pub fn erase_in_display(&mut self, mode: u8) {
        let cursor_row = self.cursor.row;
        let cursor_col = self.cursor.col;
        let cols = self.cols;
        let rows = self.rows;
        let style = self.current_style;
        // Only erase lines that exist - do NOT create new lines
        let screen_start = self.server_screen_start;
        debug::log(&format!(
            "[ERASE] mode={}, cursor=({},{}), rows={}, screen_start={}, total_lines={}",
            mode, cursor_row, cursor_col, rows, screen_start, self.lines.len()
        ));
        match mode {
            0 => {
                // Erase from cursor to end of display
                debug::log(&format!(
                    "[ERASE] Clearing from line {} (cursor_row={}) to line {} (rows-1={})",
                    screen_start + cursor_row, cursor_row, 
                    screen_start + rows.saturating_sub(1), rows.saturating_sub(1)
                ));
                if let Some(line) = self.lines.get_mut(screen_start + cursor_row) {
                    line.clear_range(cursor_col, cols, style);
                }
                for row in (cursor_row + 1)..rows {
                    if let Some(line) = self.lines.get_mut(screen_start + row) {
                        line.clear(style);
                    }
                }
            }
            1 => {
                // Erase from start of display to cursor
                for row in 0..cursor_row {
                    if let Some(line) = self.lines.get_mut(screen_start + row) {
                        line.clear(style);
                    }
                }
                if let Some(line) = self.lines.get_mut(screen_start + cursor_row) {
                    line.clear_range(0, cursor_col + 1, style);
                }
            }
            2 | 3 => {
                // Erase entire display
                for row in 0..rows {
                    if let Some(line) = self.lines.get_mut(screen_start + row) {
                        line.clear(style);
                    }
                }
            }
            _ => {}
        }
    }

    pub fn erase_in_line(&mut self, mode: u8) {
        let cursor_col = self.cursor.col;
        let cols = self.cols;
        let style = self.current_style;
        let idx = self.server_screen_to_buffer(self.cursor.row);
        // Only erase if line exists - do NOT create new lines
        if let Some(line) = self.lines.get_mut(idx) {
            match mode {
                0 => line.clear_range(cursor_col, cols, style),
                1 => line.clear_range(0, cursor_col + 1, style),
                2 => line.clear(style),
                _ => {}
            }
        }
    }

    pub fn insert_lines(&mut self, count: usize) {
        let count = count.min(self.scroll_bottom - self.cursor.row + 1);
        for _ in 0..count {
            let bottom_idx = self.server_screen_to_buffer(self.scroll_bottom);
            let cursor_idx = self.server_screen_to_buffer(self.cursor.row);
            if bottom_idx < self.lines.len() {
                self.lines.remove(bottom_idx);
            }
            self.lines.insert(cursor_idx, Line::with_style(self.cols, self.current_style));
        }
    }

    pub fn delete_lines(&mut self, count: usize) {
        let count = count.min(self.scroll_bottom - self.cursor.row + 1);
        for _ in 0..count {
            let cursor_idx = self.server_screen_to_buffer(self.cursor.row);
            let bottom_idx = self.server_screen_to_buffer(self.scroll_bottom);
            if cursor_idx < self.lines.len() {
                self.lines.remove(cursor_idx);
            }
            let insert_idx = bottom_idx.min(self.lines.len());
            self.lines.insert(insert_idx, Line::with_style(self.cols, self.current_style));
        }
    }

    pub fn insert_chars(&mut self, count: usize) {
        let cursor_col = self.cursor.col;
        let style = self.current_style;
        let idx = self.server_screen_to_buffer(self.cursor.row);
        // Only operate on lines that exist - do NOT create new lines
        if let Some(line) = self.lines.get_mut(idx) {
            for _ in 0..count {
                if cursor_col < line.len() {
                    line.cells.pop();
                    line.cells.insert(cursor_col, Cell { ch: ' ', style });
                }
            }
        }
    }

    pub fn delete_chars(&mut self, count: usize) {
        let cursor_col = self.cursor.col;
        let style = self.current_style;
        let idx = self.server_screen_to_buffer(self.cursor.row);
        // Only operate on lines that exist - do NOT create new lines
        if let Some(line) = self.lines.get_mut(idx) {
            for _ in 0..count {
                if cursor_col < line.len() {
                    line.cells.remove(cursor_col);
                    line.cells.push(Cell { ch: ' ', style });
                }
            }
        }
    }

    pub fn erase_chars(&mut self, count: usize) {
        let cursor_col = self.cursor.col;
        let style = self.current_style;
        let idx = self.server_screen_to_buffer(self.cursor.row);
        // Only operate on lines that exist - do NOT create new lines
        if let Some(line) = self.lines.get_mut(idx) {
            let end_col = (cursor_col + count).min(line.len());
            for col in cursor_col..end_col {
                if let Some(cell) = line.cells.get_mut(col) {
                    cell.ch = ' ';
                    cell.style = style;
                }
            }
        }
    }

    pub fn set_scroll_region(&mut self, top: usize, bottom: usize) {
        let top = top.min(self.rows.saturating_sub(1));
        let bottom = bottom.min(self.rows.saturating_sub(1));
        if top < bottom {
            self.scroll_top = top;
            self.scroll_bottom = bottom;
        }
    }

    pub fn reset_scroll_region(&mut self) {
        self.scroll_top = 0;
        self.scroll_bottom = self.rows.saturating_sub(1);
    }

    pub fn set_origin_mode(&mut self, enabled: bool) {
        self.origin_mode = enabled;
        if enabled {
            self.cursor.row = self.scroll_top;
        } else {
            self.cursor.row = 0;
        }
        self.cursor.col = 0;
    }

    pub fn set_style(&mut self, style: CellStyle) {
        self.current_style = style;
    }

    pub fn current_style(&self) -> CellStyle {
        self.current_style
    }

    pub fn reset_style(&mut self) {
        self.current_style = CellStyle {
            fg: self.default_fg,
            ..CellStyle::default()
        };
    }

    pub fn cursor(&self) -> CursorPosition {
        self.cursor
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    #[allow(dead_code)]
    /// Get the screen portion of the buffer - deprecated, use get_line() instead
    pub fn screen(&self) -> &[Line] {
        // VecDeque doesn't have contiguous slices
        // Callers should use get_line() instead
        &[]
    }

    #[allow(dead_code)]
    pub fn scrollback(&self) -> &VecDeque<Line> {
        // For compatibility - return the whole buffer
        // Actual scrollback is lines before server_screen_start
        &self.lines
    }

    pub fn scrollback_len(&self) -> usize {
        self.server_screen_start
    }

    pub fn total_lines(&self) -> usize {
        // Return the actual number of lines in the buffer
        // Buffer starts empty and only contains lines that have been written to
        self.lines.len()
    }


    /// Get a line by absolute index (0 = oldest line in buffer)
    pub fn get_line(&self, index: usize) -> Option<&Line> {
        self.lines.get(index)
    }

    #[allow(dead_code)]
    pub fn clear_scrollback(&mut self) {
        // Keep only the screen lines
        while self.lines.len() > self.rows {
            self.lines.pop_front();
        }
    }

    pub fn get_text_range(&self, start_row: usize, start_col: usize, end_row: usize, end_col: usize) -> String {
        let mut result = String::new();
        for row in start_row..=end_row {
            if let Some(line) = self.get_line(row) {
                let col_start = if row == start_row { start_col } else { 0 };
                let col_end = if row == end_row { end_col + 1 } else { line.len() };
                let mut line_text = String::new();
                for col in col_start..col_end.min(line.len()) {
                    if let Some(cell) = line.get(col) {
                        line_text.push(cell.ch);
                    }
                }
                // Trim trailing whitespace from each line before adding to result
                let trimmed = line_text.trim_end();
                result.push_str(trimmed);
                if row < end_row && !line.is_wrapped() {
                    result.push('\n');
                }
            }
        }
        result
    }

    pub fn default_fg(&self) -> Color32 {
        self.default_fg
    }

    pub fn default_bg(&self) -> Color32 {
        self.default_bg
    }

    pub fn set_default_colors(&mut self, fg: Color32, bg: Color32) {
        self.default_fg = fg;
        self.default_bg = bg;
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.cols = cols;
        self.rows = rows;
        self.scroll_bottom = rows.saturating_sub(1);
    }
}
