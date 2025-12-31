use crate::terminal::buffer::TerminalBuffer;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionPoint {
    pub line: usize,
    pub col: usize,
}

impl SelectionPoint {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

#[derive(Debug, Clone)]
pub struct Selection {
    start: SelectionPoint,
    end: SelectionPoint,
    active: bool,
}

impl Selection {
    pub fn new(line: usize, col: usize) -> Self {
        let point = SelectionPoint::new(line, col);
        Self {
            start: point,
            end: point,
            active: true,
        }
    }

    pub fn update(&mut self, line: usize, col: usize) {
        self.end = SelectionPoint::new(line, col);
    }

    pub fn finish(&mut self) {
        self.active = false;
    }

    #[allow(dead_code)]
    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    pub fn normalized(&self) -> (SelectionPoint, SelectionPoint) {
        if self.start.line < self.end.line
            || (self.start.line == self.end.line && self.start.col <= self.end.col)
        {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }

    pub fn contains(&self, line: usize, col: usize) -> bool {
        let (start, end) = self.normalized();
        if line < start.line || line > end.line {
            return false;
        }
        if line == start.line && line == end.line {
            return col >= start.col && col <= end.col;
        }
        if line == start.line {
            return col >= start.col;
        }
        if line == end.line {
            return col <= end.col;
        }
        true
    }

    pub fn get_text(&self, buffer: &TerminalBuffer) -> String {
        if self.is_empty() {
            return String::new();
        }
        let (start, end) = self.normalized();
        buffer.get_text_range(start.line, start.col, end.line, end.col)
    }
}

#[derive(Debug, Default)]
pub struct SelectionManager {
    selection: Option<Selection>,
}

impl SelectionManager {
    pub fn new() -> Self {
        Self { selection: None }
    }

    pub fn start(&mut self, line: usize, col: usize) {
        self.selection = Some(Selection::new(line, col));
    }

    pub fn update(&mut self, line: usize, col: usize) {
        if let Some(sel) = &mut self.selection {
            sel.update(line, col);
        }
    }

    pub fn finish(&mut self) {
        if let Some(sel) = &mut self.selection {
            sel.finish();
            if sel.is_empty() {
                self.selection = None;
            }
        }
    }

    pub fn clear(&mut self) {
        self.selection = None;
    }

    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    #[allow(dead_code)]
    pub fn has_selection(&self) -> bool {
        self.selection.as_ref().map_or(false, |s| !s.is_empty())
    }

    pub fn get_text(&self, buffer: &TerminalBuffer) -> Option<String> {
        self.selection.as_ref().map(|s| s.get_text(buffer))
    }

    #[allow(dead_code)]
    pub fn is_selecting(&self) -> bool {
        self.selection.as_ref().map_or(false, |s| s.is_active())
    }
}

