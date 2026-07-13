use crate::clickable::{Action, Clickable};
use crate::config::Config;
use crate::daemon;
use crate::logs;
use crate::model::{Entry, EntryType, FeedbackEntry, Goto, Payload};
use crate::tmux;
use ratatui::layout::Rect;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

const SPINNER_MS: u128 = 30;
const SPINUP_TAU_MS: f64 = 2000.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
    Close,
    Goto(Goto),
    None,
}

pub struct Picker {
    pub(crate) entries: Vec<Entry>,
    pub(crate) filtered: Vec<usize>,
    pub(crate) cursor: usize,
    pub(crate) input: String,
    pub(crate) input_cursor: usize,
    pub(crate) spinner: usize,
    started_at: Instant,
    last_spinner: Instant,
    pub(crate) quit: bool,
    pub(crate) pending_goto: Option<Goto>,
    tx: mpsc::Sender<Option<Payload>>,
    rx: mpsc::Receiver<Option<Payload>>,
    pub(crate) slot_entries: Rect,
    pub(crate) scroll: usize,
    pub(crate) mouse_hover: bool,
    pub(crate) auto_close: bool,
    pub(crate) command_mode: bool,
    pub(crate) reveal_open: bool,
    pub(crate) config: Config,
    pub(crate) feedbacks: Vec<FeedbackEntry>,
    pub(crate) entries_found: usize,
    pub(crate) clickables: Vec<Clickable>,
    pub(crate) last_mouse_col: u16,
    pub(crate) last_mouse_row: u16,
}

impl Picker {
    pub fn new(payload: Payload) -> Self {
        logs::init("picker").ok();

        let (tx, rx) = mpsc::channel::<Option<Payload>>();
        let auto_close = payload.config.auto_close;
        let entries_found = payload.entries_found.max(payload.entries.len());
        daemon::listen(tx.clone());
        let mut picker = Picker {
            cursor: 0,
            entries: payload.entries,
            filtered: Vec::new(),
            input: String::new(),
            input_cursor: 0,
            spinner: 0,
            started_at: Instant::now(),
            last_spinner: Instant::now(),
            quit: false,
            pending_goto: None,
            tx,
            rx,
            slot_entries: Rect::default(),
            scroll: 0,
            mouse_hover: false,
            command_mode: false,
            clickables: Vec::new(),
            last_mouse_col: 0,
            last_mouse_row: 0,
            reveal_open: true,
            auto_close,
            config: payload.config,
            feedbacks: payload.feedbacks,
            entries_found,
        };
        picker.filtered = picker.filtered();
        picker.cursor = picker.find_initial_cursor();
        picker
    }

    pub fn tick(&mut self) -> Signal {
        let now = Instant::now();
        let spinner_ms = if self.entries.is_empty() {
            let elapsed = now.duration_since(self.started_at).as_millis() as f64;
            (10.0 + 70.0 * (-elapsed / SPINUP_TAU_MS).exp()) as u128
        } else {
            SPINNER_MS
        };
        if now.duration_since(self.last_spinner).as_millis() >= spinner_ms {
            self.spinner = self.spinner.wrapping_add(1);
            self.last_spinner = now;
        }

        while let Ok(result) = self.rx.try_recv() {
            if let Some(payload) = result {
                let is_empty = self.entries.is_empty();
                let path_changed = self.config.path != payload.config.path
                    || self.config.path_worktrees != payload.config.path_worktrees;
                self.entries_found = payload.entries_found.max(payload.entries.len());
                self.entries = payload.entries;
                self.feedbacks = payload.feedbacks.clone();
                self.config = payload.config;
                self.auto_close = self.config.auto_close;
                self.filtered = self.filtered();
                if (is_empty && !self.filtered.is_empty()) || path_changed {
                    self.cursor = self.find_initial_cursor();
                    self.reveal_open = true;
                } else if self.cursor >= self.filtered.len() {
                    self.cursor = self.filtered.len().saturating_sub(1);
                }
            }
        }

        if let Some(goto) = self.pending_goto.take() {
            return Signal::Goto(goto);
        }
        if self.quit {
            Signal::Close
        } else {
            Signal::None
        }
    }

    pub(crate) fn schedule_refresh(&self) {
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = daemon::fetch_once()
                .and_then(|bytes| serde_json::from_slice::<Payload>(&bytes).ok());
            let _ = tx.send(result);
        });
    }

    pub fn render(&mut self, frame: &mut ratatui::Frame, _config: &Config) {
        crate::renderer::Renderer::render(frame, self);
    }

    pub(crate) fn schedule_initial_fetch(&mut self, overrides: Vec<(String, Option<String>)>) {
        let tx = self.tx.clone();
        thread::spawn(move || {
            let payload = daemon::initial_fetch(&overrides);
            let _ = tx.send(Some(payload));
        });
    }

    pub fn cursor_entry_buttons(&self) -> Vec<(String, Action)> {
        if !self.command_mode {
            return vec![];
        }
        let Some(&idx) = self.filtered.get(self.cursor) else {
            return vec![];
        };
        let entry = &self.entries[idx];
        let mut buttons = Vec::new();
        if entry.is_open || entry.kind == EntryType::Agent {
            let key = self.config.bind_command_session_kill.to_uppercase();
            buttons.push((format!("{key} Kill Session"), Action::KillSession));
        } else if entry.goto.is_some() {
            buttons.push(("O Open Detached".to_string(), Action::OpenDetached));
        }
        buttons
    }

    pub fn execute_kill_session(&mut self) {
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

    pub fn open_detached(&mut self) {
        if let Some(&idx) = self.filtered.get(self.cursor) {
            let entry = &self.entries[idx];
            if !entry.is_open
                && entry.kind != EntryType::Agent
                && let Some(goto) = &entry.goto
            {
                tmux::open_detached(goto);
                let mut cur = Some(idx);
                while let Some(i) = cur {
                    self.entries[i].is_open = true;
                    cur = self.entries[i].parent;
                }
                self.command_mode = false;
                self.schedule_refresh();
            }
        }
    }
}
