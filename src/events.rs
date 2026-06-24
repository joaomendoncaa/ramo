use crate::model::EntryType;
use crate::picker::Picker;
use crate::tmux;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

impl Picker {
    pub fn handle_input(&mut self, key: KeyEvent) {
        if self.command_mode {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Char('p') => {
                        self.move_cursor(-1);
                        return;
                    }
                    KeyCode::Char('n') => {
                        self.move_cursor(1);
                        return;
                    }
                    KeyCode::Char('r') => {
                        self.input.clear();
                        self.input_cursor = 0;
                        self.filter();
                        return;
                    }
                    KeyCode::Char('c') | KeyCode::Char('q') => {
                        self.quit = true;
                        return;
                    }
                    _ => {}
                }
            }
            self.handle_command_key(key);
            return;
        }

        self.mouse_hover = false;

        if key.code == KeyCode::Char(self.config.bind_command_mode) && key.modifiers.is_empty() {
            self.command_mode = true;
            return;
        }

        if key.modifiers.contains(KeyModifiers::ALT)
            && !key.modifiers.contains(KeyModifiers::CONTROL)
        {
            self.handle_mod_alt(key);
            return;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL)
            && !key.modifiers.contains(KeyModifiers::ALT)
            && self.handle_mod_ctrl(key)
        {
            return;
        }

        self.handle_key(key);
    }

    pub fn handle_mouse(&mut self, event: MouseEvent) {
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(idx) = self.get_index_from_mouse(event.column, event.row) {
                    self.cursor = idx;
                    self.goto();
                }
            }
            MouseEventKind::Moved => {
                if let Some(idx) = self.get_index_from_mouse(event.column, event.row) {
                    self.cursor = idx;
                    self.mouse_hover = true;
                }
            }
            MouseEventKind::ScrollDown => {
                self.mouse_hover = false;
                self.scroll_view(1);
            }
            MouseEventKind::ScrollUp => {
                self.mouse_hover = false;
                self.scroll_view(-1);
            }
            _ => {}
        }
    }

    fn handle_command_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Backspace => {
                self.command_mode = false;
            }
            KeyCode::Char('k') => {
                if let Some(&idx) = self.filtered.get(self.cursor) {
                    let entry = &self.entries[idx];
                    if (entry.is_open || entry.kind == EntryType::Agent)
                        && let Some(goto) = &entry.goto
                    {
                        if let Some(window) = &goto.window {
                            tmux::kill_window(&goto.session, *window);
                        } else {
                            let sanitized = goto.session.replace([':', '.'], "_");
                            tmux::kill_session(&sanitized);
                        }
                        self.command_mode = false;
                        self.schedule_refresh();
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_mod_alt(&mut self, key: KeyEvent) {
        const ALT_COUNT: usize = 4;
        match key.code {
            KeyCode::Char(c) if c.is_ascii_digit() => {
                if let Some(n) = c.to_digit(10)
                    && n >= 1
                    && n as usize <= ALT_COUNT
                {
                    self.jump_above(n as usize);
                }
            }
            KeyCode::Char('b') => self.move_word(-1),
            KeyCode::Char('f') => self.move_word(1),
            _ => {}
        }
    }

    // Returns true if the key was handled (caller should stop).
    fn handle_mod_ctrl(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('p') => self.move_cursor(-1),
            KeyCode::Char('n') => self.move_cursor(1),
            KeyCode::Char('c') | KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('a') => self.input_cursor = 0,
            KeyCode::Char('e') => self.input_cursor = self.input.len(),
            KeyCode::Char('u') => {
                self.input.drain(0..self.input_cursor);
                self.input_cursor = 0;
                self.filter();
            }
            KeyCode::Char('k') => {
                self.input.drain(self.input_cursor..);
                self.filter();
            }
            KeyCode::Char('w') => self.delete_word_back(),
            KeyCode::Char('r') => {
                self.input.clear();
                self.input_cursor = 0;
                self.filter();
            }
            _ => return false,
        }
        true
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.move_cursor(-1),
            KeyCode::Down => self.move_cursor(1),
            KeyCode::Enter => self.goto(),
            KeyCode::Esc => self.quit = true,
            KeyCode::Left => {
                if self.input_cursor > 0 {
                    self.input_cursor -= 1;
                }
            }
            KeyCode::Right => {
                if self.input_cursor < self.input.len() {
                    self.input_cursor += 1;
                }
            }
            KeyCode::Char(c) => {
                self.input.insert(self.input_cursor, c);
                self.input_cursor += 1;
                self.filter();
            }
            KeyCode::Backspace => {
                if self.input_cursor > 0 {
                    self.input_cursor -= 1;
                    self.input.remove(self.input_cursor);
                    self.filter();
                }
            }
            KeyCode::Delete if self.input_cursor < self.input.len() => {
                self.input.remove(self.input_cursor);
                self.filter();
            }
            _ => {}
        }
    }

    fn scroll_view(&mut self, dir: i32) {
        let n = self.filtered.len();
        if n == 0 {
            return;
        }
        if dir < 0 {
            let step = (-dir) as usize;
            self.scroll = self.scroll.saturating_sub(step);
            self.cursor = self.cursor.saturating_sub(step);
        }
        if dir > 0 {
            let step = dir as usize;
            let max = n.saturating_sub(1);
            self.scroll = (self.scroll + step).min(max);
            self.cursor = (self.cursor + step).min(max);
        }
    }

    fn move_cursor(&mut self, dir: i32) {
        self.mouse_hover = false;
        let len = self.filtered.len();
        if len == 0 {
            return;
        }
        if dir < 0 {
            self.cursor = self.cursor.checked_sub(1).unwrap_or(len - 1);
        }
        if dir > 0 {
            self.cursor = (self.cursor + 1) % len;
        }
    }

    fn activate_entry(&mut self, idx: usize) {
        let goto = self.entries[idx].goto.clone();
        let Some(goto) = goto else { return };

        self.pending_goto = Some(goto.clone());

        let mut cur = Some(idx);
        while let Some(i) = cur {
            self.entries[i].is_open = true;
            cur = self.entries[i].parent;
        }

        let is_current = tmux::is_current_session(goto.session);
        if self.auto_close || is_current {
            self.quit = true;
        } else {
            self.schedule_refresh();
        }
    }

    fn goto(&mut self) {
        if let Some(&idx) = self.filtered.get(self.cursor) {
            self.activate_entry(idx);
        }
    }

    // Alt+1..Alt+4: open the entry `n` rows above the cursor. Handier than
    // it sounds — a quick "jump back through recent sessions" gesture.
    // TODO  does this need to exist? if so, missing UI hints
    fn jump_above(&mut self, n: usize) {
        if n > self.cursor {
            return;
        }
        if let Some(&idx) = self.filtered.get(self.cursor - n) {
            self.activate_entry(idx);
        }
    }

    fn move_word(&mut self, dir: i32) {
        let bytes = self.input.as_bytes();
        if dir < 0 {
            let mut i = self.input_cursor;
            while i > 0 && bytes[i - 1].is_ascii_whitespace() {
                i -= 1;
            }
            while i > 0 && !bytes[i - 1].is_ascii_whitespace() {
                i -= 1;
            }
            self.input_cursor = i;
        } else {
            let mut i = self.input_cursor;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            self.input_cursor = i;
        }
    }

    fn delete_word_back(&mut self) {
        let bytes = self.input.as_bytes();
        let mut i = self.input_cursor;
        while i > 0 && bytes[i - 1].is_ascii_whitespace() {
            i -= 1;
        }
        while i > 0 && !bytes[i - 1].is_ascii_whitespace() {
            i -= 1;
        }
        self.input.drain(i..self.input_cursor);
        self.input_cursor = i;
        self.filter();
    }

    fn get_index_from_mouse(&self, column: u16, row: u16) -> Option<usize> {
        let area = self.slot_entries;
        if column < area.x
            || column >= area.x.saturating_add(area.width)
            || row < area.y
            || row >= area.y.saturating_add(area.height)
        {
            return None;
        }

        let vh = area.height as usize;
        let n = self.filtered.len();
        if n == 0 {
            return None;
        }

        let view_y = (row - area.y) as usize;
        let padding = vh.saturating_sub(n);
        if view_y < padding {
            return None;
        }

        let scroll = self.scroll;
        let entry_relative = view_y - padding;
        let idx = scroll + entry_relative;

        if idx < n { Some(idx) } else { None }
    }
}
