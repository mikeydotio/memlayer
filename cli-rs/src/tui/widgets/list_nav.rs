/// Navigable list state — tracks selected index for a list of items.
pub struct ListNav {
    pub selected: usize,
    pub len: usize,
    pub offset: usize,
}

impl ListNav {
    pub fn new() -> Self {
        ListNav {
            selected: 0,
            len: 0,
            offset: 0,
        }
    }

    pub fn set_len(&mut self, len: usize) {
        self.len = len;
        if self.selected >= len && len > 0 {
            self.selected = len - 1;
        }
    }

    pub fn next(&mut self) {
        if self.len > 0 {
            self.selected = (self.selected + 1).min(self.len - 1);
        }
    }

    pub fn prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn home(&mut self) {
        self.selected = 0;
    }

    pub fn end(&mut self) {
        if self.len > 0 {
            self.selected = self.len - 1;
        }
    }

    /// Adjust offset for a visible window of `height` rows.
    pub fn visible_offset(&mut self, height: usize) -> usize {
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + height {
            self.offset = self.selected - height + 1;
        }
        self.offset
    }
}
