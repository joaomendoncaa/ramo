use crate::clickable::Action;
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
                    KeyCode::Char('u') => {
                        self.move_cursor(-5);
                        return;
                    }
                    KeyCode::Char('d') => {
                        self.move_cursor(5);
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
        self.last_mouse_col = event.column;
        self.last_mouse_row = event.row;

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if self.check_clickables(event.column, event.row) {
                    return;
                }
                if let Some(idx) = self.get_index_from_mouse(event.column, event.row) {
                    self.cursor = idx;
                    self.goto();
                }
            }
            MouseEventKind::Down(MouseButton::Right) => {
                if self.check_clickables(event.column, event.row) {
                    return;
                }
                if self.get_index_from_mouse(event.column, event.row).is_some() {
                    self.command_mode = true;
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
                self.execute_kill_session();
            }
            KeyCode::Char('o') => {
                self.open_detached();
            }
            _ => {}
        }
    }

    fn check_clickables(&mut self, col: u16, row: u16) -> bool {
        let action = self
            .clickables
            .iter()
            .find(|c| c.contains(col, row))
            .map(|c| c.action);
        if let Some(action) = action {
            self.handle_click(action);
            true
        } else {
            false
        }
    }

    fn handle_click(&mut self, action: Action) {
        match action {
            Action::CommandMode => self.command_mode = true,
            Action::ExitCommandMode => self.command_mode = false,
            Action::KillSession => self.execute_kill_session(),
            Action::OpenDetached => self.open_detached(),
            Action::MovePrevious => self.move_cursor(-1),
            Action::MoveNext => self.move_cursor(1),
            Action::MoveUp => self.move_cursor(5),
            Action::MoveDown => self.move_cursor(-5),
            Action::ResetInput => {
                self.input.clear();
                self.input_cursor = 0;
                self.filter();
            }
            Action::Close => self.quit = true,
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
            KeyCode::Char('u') => {
                self.move_cursor(-5);
            }
            KeyCode::Char('d') => {
                self.move_cursor(5);
            }
            KeyCode::Char('c') | KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('a') => self.input_cursor = 0,
            KeyCode::Char('e') => self.input_cursor = self.input.len(),

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
        // Scroll bookkeeping runs in virtual-row space, but the cursor keeps
        // stepping over entries only — gaps are never landed on.
        let max_row = self.rows().len().saturating_sub(1);
        if dir < 0 {
            let step = (-dir) as usize;
            self.scroll = self.scroll.saturating_sub(step);
            self.cursor = self.cursor.saturating_sub(step);
        }
        if dir > 0 {
            let step = dir as usize;
            self.scroll = (self.scroll + step).min(max_row);
            let max = n.saturating_sub(1);
            self.cursor = (self.cursor + step).min(max);
        }
    }

    fn move_cursor(&mut self, amount: i32) {
        self.mouse_hover = false;

        let len = self.filtered.len();
        if len == 0 || amount == 0 {
            return;
        }

        if amount < 0 {
            let step = amount.unsigned_abs() as usize;
            self.cursor = self.cursor.checked_sub(step).unwrap_or(len - 1);
        }
        if amount > 0 {
            self.cursor = (self.cursor + amount as usize) % len;
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
        let rows = self.rows();
        if rows.is_empty() {
            return None;
        }

        let view_y = (row - area.y) as usize;
        let padding = vh.saturating_sub(rows.len());
        if view_y < padding {
            return None;
        }

        // Gap rows map to None: the pointer can't select or hover them,
        // just like the keyboard navigation skips them.
        match rows.get(self.scroll + (view_y - padding)) {
            Some(Some(p)) => Some(*p),
            _ => None,
        }
    }
}
