use egui::Color32;
use std::collections::VecDeque;

const DEFAULT_COLS: usize = 80;
const DEFAULT_ROWS: usize = 24;
const MIN_SCROLLBACK: usize = 1000;

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

    pub fn resize(&mut self, cols: usize, style: CellStyle) {
        self.cells.resize(cols, Cell { ch: ' ', style });
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

pub struct TerminalBuffer {
    scrollback: VecDeque<Line>,
    screen: Vec<Line>,
    cols: usize,
    rows: usize,
    max_scrollback: usize,
    cursor: CursorPosition,
    saved_cursor: CursorPosition,
    current_style: CellStyle,
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
        let max_scrollback = max_scrollback.max(MIN_SCROLLBACK);
        let mut style = CellStyle::default();
        style.fg = default_fg;
        let screen = (0..rows).map(|_| Line::with_style(cols, style)).collect();
        Self {
            scrollback: VecDeque::new(),
            screen,
            cols,
            rows,
            max_scrollback,
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

    pub fn resize(&mut self, new_cols: usize, new_rows: usize) {
        if new_cols == self.cols && new_rows == self.rows {
            return;
        }
        for line in &mut self.screen {
            line.resize(new_cols, self.current_style);
        }
        for line in &mut self.scrollback {
            line.resize(new_cols, self.current_style);
        }
        while self.screen.len() < new_rows {
            self.screen.push(Line::with_style(new_cols, self.current_style));
        }
        while self.screen.len() > new_rows {
            let line = self.screen.remove(0);
            self.add_to_scrollback(line);
        }
        self.cols = new_cols;
        self.rows = new_rows;
        self.scroll_bottom = new_rows - 1;
        self.cursor.row = self.cursor.row.min(new_rows.saturating_sub(1));
        self.cursor.col = self.cursor.col.min(new_cols.saturating_sub(1));
    }

    fn add_to_scrollback(&mut self, line: Line) {
        self.scrollback.push_back(line);
        while self.scrollback.len() > self.max_scrollback {
            self.scrollback.pop_front();
        }
    }

    pub fn put_char(&mut self, ch: char) {
        if self.cursor.col >= self.cols {
            self.cursor.col = 0;
            self.new_line();
        }
        if let Some(line) = self.screen.get_mut(self.cursor.row) {
            line.set(self.cursor.col, Cell::new(ch, self.current_style));
        }
        self.cursor.col += 1;
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
                let line = self.screen.remove(self.scroll_top);
                self.add_to_scrollback(line);
            } else {
                self.screen.remove(self.scroll_top);
            }
            self.screen.insert(self.scroll_bottom, Line::with_style(self.cols, self.current_style));
        }
    }

    pub fn scroll_down(&mut self, count: usize) {
        for _ in 0..count {
            self.screen.remove(self.scroll_bottom);
            self.screen.insert(self.scroll_top, Line::with_style(self.cols, self.current_style));
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
        match mode {
            0 => {
                // Erase from cursor to end of display
                if let Some(line) = self.screen.get_mut(self.cursor.row) {
                    line.clear_range(self.cursor.col, self.cols, self.current_style);
                }
                for row in (self.cursor.row + 1)..self.rows {
                    if let Some(line) = self.screen.get_mut(row) {
                        line.clear(self.current_style);
                    }
                }
            }
            1 => {
                // Erase from start of display to cursor
                for row in 0..self.cursor.row {
                    if let Some(line) = self.screen.get_mut(row) {
                        line.clear(self.current_style);
                    }
                }
                if let Some(line) = self.screen.get_mut(self.cursor.row) {
                    line.clear_range(0, self.cursor.col + 1, self.current_style);
                }
            }
            2 | 3 => {
                // Erase entire display
                for line in &mut self.screen {
                    line.clear(self.current_style);
                }
            }
            _ => {}
        }
    }

    pub fn erase_in_line(&mut self, mode: u8) {
        if let Some(line) = self.screen.get_mut(self.cursor.row) {
            match mode {
                0 => line.clear_range(self.cursor.col, self.cols, self.current_style),
                1 => line.clear_range(0, self.cursor.col + 1, self.current_style),
                2 => line.clear(self.current_style),
                _ => {}
            }
        }
    }

    pub fn insert_lines(&mut self, count: usize) {
        let count = count.min(self.scroll_bottom - self.cursor.row + 1);
        for _ in 0..count {
            if self.scroll_bottom < self.screen.len() {
                self.screen.remove(self.scroll_bottom);
            }
            self.screen.insert(self.cursor.row, Line::with_style(self.cols, self.current_style));
        }
    }

    pub fn delete_lines(&mut self, count: usize) {
        let count = count.min(self.scroll_bottom - self.cursor.row + 1);
        for _ in 0..count {
            if self.cursor.row < self.screen.len() {
                self.screen.remove(self.cursor.row);
            }
            self.screen.insert(self.scroll_bottom, Line::with_style(self.cols, self.current_style));
        }
    }

    pub fn insert_chars(&mut self, count: usize) {
        if let Some(line) = self.screen.get_mut(self.cursor.row) {
            for _ in 0..count {
                if self.cursor.col < line.len() {
                    line.cells.pop();
                    line.cells.insert(self.cursor.col, Cell { ch: ' ', style: self.current_style });
                }
            }
        }
    }

    pub fn delete_chars(&mut self, count: usize) {
        if let Some(line) = self.screen.get_mut(self.cursor.row) {
            for _ in 0..count {
                if self.cursor.col < line.len() {
                    line.cells.remove(self.cursor.col);
                    line.cells.push(Cell { ch: ' ', style: self.current_style });
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

    pub fn screen(&self) -> &[Line] {
        &self.screen
    }

    #[allow(dead_code)]
    pub fn scrollback(&self) -> &VecDeque<Line> {
        &self.scrollback
    }

    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    pub fn total_lines(&self) -> usize {
        self.scrollback.len() + self.rows
    }

    pub fn get_line(&self, index: usize) -> Option<&Line> {
        if index < self.scrollback.len() {
            self.scrollback.get(index)
        } else {
            self.screen.get(index - self.scrollback.len())
        }
    }

    #[allow(dead_code)]
    pub fn clear_scrollback(&mut self) {
        self.scrollback.clear();
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
}

