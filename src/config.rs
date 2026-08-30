use crate::model::{FeedbackEntry, FeedbackType};
use crate::util;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn default_bind_help() -> char {
    '?'
}
fn default_bind_quit() -> String {
    "esc,ctrl-c,ctrl-q".to_string()
}
fn default_bind_nav_up() -> String {
    "up,ctrl-p".to_string()
}
fn default_bind_nav_down() -> String {
    "down,ctrl-n".to_string()
}
fn default_bind_nav_page_up() -> String {
    "ctrl-u".to_string()
}
fn default_bind_nav_page_down() -> String {
    "ctrl-d".to_string()
}
fn default_bind_input_left() -> String {
    "left".to_string()
}
fn default_bind_input_right() -> String {
    "right".to_string()
}
fn default_bind_input_home() -> String {
    "ctrl-a".to_string()
}
fn default_bind_input_end() -> String {
    "ctrl-e".to_string()
}
fn default_bind_input_kill_line() -> String {
    "ctrl-k".to_string()
}
fn default_bind_input_delete_word() -> String {
    "ctrl-w".to_string()
}
fn default_bind_input_clear() -> String {
    "ctrl-r".to_string()
}
fn default_bind_input_word_left() -> String {
    "alt-b".to_string()
}
fn default_bind_input_word_right() -> String {
    "alt-f".to_string()
}
fn default_bind_input_backspace() -> String {
    "backspace".to_string()
}
fn default_bind_input_delete() -> String {
    "delete".to_string()
}
fn default_bind_command_exit() -> String {
    "esc,backspace".to_string()
}
fn default_bind_command_open_detached() -> String {
    "o".to_string()
}
fn default_bind_help_enter() -> String {
    "enter".to_string()
}
fn default_bind_help_exit() -> String {
    "esc".to_string()
}

fn strip_inline_value(s: &str) -> String {
    if let Some(idx) = s.find('#') {
        s[..idx].trim().to_string()
    } else {
        s.trim().to_string()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub path: String,
    pub path_worktrees: String,
    pub bind_jumpto: String,
    pub bind_command_mode: char,
    #[serde(default = "default_bind_help")]
    pub bind_help: char,
    pub bind_command_session_kill: String,
    pub bind_command_worktree_new: String,
    pub bind_command_worktree_delete: String,
    #[serde(default = "default_bind_quit")]
    pub bind_quit: String,
    #[serde(default = "default_bind_nav_up")]
    pub bind_nav_up: String,
    #[serde(default = "default_bind_nav_down")]
    pub bind_nav_down: String,
    #[serde(default = "default_bind_nav_page_up")]
    pub bind_nav_page_up: String,
    #[serde(default = "default_bind_nav_page_down")]
    pub bind_nav_page_down: String,
    #[serde(default = "default_bind_input_left")]
    pub bind_input_left: String,
    #[serde(default = "default_bind_input_right")]
    pub bind_input_right: String,
    #[serde(default = "default_bind_input_home")]
    pub bind_input_home: String,
    #[serde(default = "default_bind_input_end")]
    pub bind_input_end: String,
    #[serde(default = "default_bind_input_kill_line")]
    pub bind_input_kill_line: String,
    #[serde(default = "default_bind_input_delete_word")]
    pub bind_input_delete_word: String,
    #[serde(default = "default_bind_input_clear")]
    pub bind_input_clear: String,
    #[serde(default = "default_bind_input_word_left")]
    pub bind_input_word_left: String,
    #[serde(default = "default_bind_input_word_right")]
    pub bind_input_word_right: String,
    #[serde(default = "default_bind_input_backspace")]
    pub bind_input_backspace: String,
    #[serde(default = "default_bind_input_delete")]
    pub bind_input_delete: String,
    #[serde(default = "default_bind_command_exit")]
    pub bind_command_exit: String,
    #[serde(default = "default_bind_command_open_detached")]
    pub bind_command_open_detached: String,
    #[serde(default = "default_bind_help_enter")]
    pub bind_help_enter: String,
    #[serde(default = "default_bind_help_exit")]
    pub bind_help_exit: String,
    pub auto_close: bool,
    pub daemon_timeout: u64,
    pub hide_changes_inactive: bool,
    pub hide_changes_active: bool,
    pub hide_changes_worktree: bool,
    pub hide_hints_footer: bool,
    pub hide_hints_branches_active: bool,
    pub hide_hints_branches_inactive: bool,
    pub hide_hints_remotes_active: bool,
    pub hide_hints_remotes_inactive: bool,
    pub style_icon_daemon_loading: String,
    pub style_icon_daemon_ready: String,
    pub style_icon_active: String,
    pub style_icon_worktree: String,
    pub style_icon_agent_idle: String,
    pub style_icon_agent_running: String,
    pub style_icon_input: String,
    pub style_entries_gap: u64,
}

impl Default for Config {
    fn default() -> Self {
        let default_path = std::env::var("HOME")
            .map(|h| format!("{h}/Projects/*"))
            .unwrap_or_else(|_| "~/Projects/*".to_string());
        Config {
            path: default_path,
            path_worktrees: String::new(),
            bind_jumpto: "enter".to_string(),
            bind_command_mode: ':',
            bind_help: '?',
            bind_command_session_kill: "k".to_string(),
            bind_command_worktree_new: "n".to_string(),
            bind_command_worktree_delete: "d".to_string(),
            bind_quit: default_bind_quit(),
            bind_nav_up: default_bind_nav_up(),
            bind_nav_down: default_bind_nav_down(),
            bind_nav_page_up: default_bind_nav_page_up(),
            bind_nav_page_down: default_bind_nav_page_down(),
            bind_input_left: default_bind_input_left(),
            bind_input_right: default_bind_input_right(),
            bind_input_home: default_bind_input_home(),
            bind_input_end: default_bind_input_end(),
            bind_input_kill_line: default_bind_input_kill_line(),
            bind_input_delete_word: default_bind_input_delete_word(),
            bind_input_clear: default_bind_input_clear(),
            bind_input_word_left: default_bind_input_word_left(),
            bind_input_word_right: default_bind_input_word_right(),
            bind_input_backspace: default_bind_input_backspace(),
            bind_input_delete: default_bind_input_delete(),
            bind_command_exit: default_bind_command_exit(),
            bind_command_open_detached: default_bind_command_open_detached(),
            bind_help_enter: default_bind_help_enter(),
            bind_help_exit: default_bind_help_exit(),
            auto_close: true,
            daemon_timeout: 1800,
            hide_changes_inactive: false,
            hide_changes_active: false,
            hide_changes_worktree: false,
            hide_hints_footer: false,
            hide_hints_branches_active: false,
            hide_hints_branches_inactive: false,
            hide_hints_remotes_active: false,
            hide_hints_remotes_inactive: false,
            style_icon_daemon_loading: "─╲│╱".to_string(),
            style_icon_daemon_ready: "✓".to_string(),
            style_icon_active: "*".to_string(),
            style_icon_worktree: "⑂".to_string(),
            style_icon_agent_idle: "✓".to_string(),
            style_icon_agent_running: "⠋⠙⠹⠸⢰⣰⣠⣄⣆⡆⠇⠏".to_string(),
            style_icon_input: "▸".to_string(),
            style_entries_gap: 0,
        }
    }
}

impl Config {
    pub fn daemon_timeout_duration(&self) -> Duration {
        Duration::from_secs(self.daemon_timeout)
    }

    pub(crate) fn config_base() -> PathBuf {
        match std::env::var("XDG_CONFIG_HOME") {
            Ok(x) if !x.is_empty() => PathBuf::from(x),
            _ => PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config"),
        }
    }

    pub fn config_path() -> Option<PathBuf> {
        let ramo_dir = Self::config_base().join("ramo");
        for name in ["config", "config.ramo"] {
            let p = ramo_dir.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
        None
    }

    pub fn config_dir() -> Option<PathBuf> {
        let dir = Self::config_base().join("ramo");
        dir.is_dir().then_some(dir)
    }

    pub fn candidate_paths() -> Vec<PathBuf> {
        let base = Self::config_base().join("ramo");
        ["config", "config.ramo"]
            .into_iter()
            .map(|n| base.join(n))
            .collect()
    }

    pub fn write_target() -> PathBuf {
        if let Some(p) = Self::config_path() {
            return p;
        }
        // No file yet — first candidate wins (mkdir on write)
        Self::candidate_paths()
            .into_iter()
            .next()
            .unwrap_or_else(|| Self::config_base().join("ramo/config"))
    }

    pub fn value_string(&self, key: &str) -> Option<String> {
        match key {
            "path" => Some(self.path.clone()),
            "path-worktrees" => Some(self.path_worktrees.clone()),
            "bind-jumpto" => Some(self.bind_jumpto.clone()),
            "bind-command-mode" => Some(self.bind_command_mode.to_string()),
            "bind-help" => Some(self.bind_help.to_string()),
            "bind-command-session-kill" => Some(self.bind_command_session_kill.clone()),
            "bind-command-worktree-new" => Some(self.bind_command_worktree_new.clone()),
            "bind-command-worktree-delete" => Some(self.bind_command_worktree_delete.clone()),
            "bind-quit" => Some(self.bind_quit.clone()),
            "bind-nav-up" => Some(self.bind_nav_up.clone()),
            "bind-nav-down" => Some(self.bind_nav_down.clone()),
            "bind-nav-page-up" => Some(self.bind_nav_page_up.clone()),
            "bind-nav-page-down" => Some(self.bind_nav_page_down.clone()),
            "bind-input-left" => Some(self.bind_input_left.clone()),
            "bind-input-right" => Some(self.bind_input_right.clone()),
            "bind-input-home" => Some(self.bind_input_home.clone()),
            "bind-input-end" => Some(self.bind_input_end.clone()),
            "bind-input-kill-line" => Some(self.bind_input_kill_line.clone()),
            "bind-input-delete-word" => Some(self.bind_input_delete_word.clone()),
            "bind-input-clear" => Some(self.bind_input_clear.clone()),
            "bind-input-word-left" => Some(self.bind_input_word_left.clone()),
            "bind-input-word-right" => Some(self.bind_input_word_right.clone()),
            "bind-input-backspace" => Some(self.bind_input_backspace.clone()),
            "bind-input-delete" => Some(self.bind_input_delete.clone()),
            "bind-command-exit" => Some(self.bind_command_exit.clone()),
            "bind-command-open-detached" => Some(self.bind_command_open_detached.clone()),
            "bind-help-enter" => Some(self.bind_help_enter.clone()),
            "bind-help-exit" => Some(self.bind_help_exit.clone()),
            "auto-close" => Some(self.auto_close.to_string()),
            "daemon-timeout" => Some(self.daemon_timeout.to_string()),
            "hide-changes-inactive" => Some(self.hide_changes_inactive.to_string()),
            "hide-changes-active" => Some(self.hide_changes_active.to_string()),
            "hide-changes-worktree" => Some(self.hide_changes_worktree.to_string()),
            "hide-hints-footer" => Some(self.hide_hints_footer.to_string()),
            "hide-hints-branches-active" => Some(self.hide_hints_branches_active.to_string()),
            "hide-hints-branches-inactive" => Some(self.hide_hints_branches_inactive.to_string()),
            "hide-hints-remotes-active" => Some(self.hide_hints_remotes_active.to_string()),
            "hide-hints-remotes-inactive" => Some(self.hide_hints_remotes_inactive.to_string()),
            "style-icon-daemon-loading" => Some(self.style_icon_daemon_loading.clone()),
            "style-icon-daemon-ready" => Some(self.style_icon_daemon_ready.clone()),
            "style-icon-active" => Some(self.style_icon_active.clone()),
            "style-icon-worktree" => Some(self.style_icon_worktree.clone()),
            "style-icon-agent-idle" => Some(self.style_icon_agent_idle.clone()),
            "style-icon-agent-running" => Some(self.style_icon_agent_running.clone()),
            "style-icon-input" => Some(self.style_icon_input.clone()),
            "style-entries-gap" => Some(self.style_entries_gap.to_string()),
            _ => None,
        }
    }

    pub fn is_default_value(key: &str, new_value: &str) -> bool {
        let def = Config::default();
        let Some(def_val) = def.value_string(key) else {
            return false;
        };
        let new_trim = new_value.trim();
        // For path keys, compare after tilde expansion so "~/Projects/*" matches default
        if key == "path" || key == "path-worktrees" {
            return util::expand_tilde(new_trim) == util::expand_tilde(&def_val);
        }
        // For bool and u64, also consider semantic equality via parsing
        // but keep string equality as fallback for simplicity
        def_val == new_trim
    }

    pub fn new() -> (Self, Vec<FeedbackEntry>) {
        match Self::config_path() {
            Some(path) => Self::load_from_file(&path),
            None => {
                eprintln!(
                    "ramo: no config file found in $XDG_CONFIG_HOME/ramo/ or $HOME/.config/ramo/ \
                     — using defaults"
                );
                (Config::default(), vec![])
            }
        }
    }

    pub fn load(overrides: &[(String, Option<String>)]) -> (Self, Vec<FeedbackEntry>) {
        match Self::config_path() {
            Some(path) => {
                let (mut c, mut fb) = Self::load_from_file(&path);
                fb.extend(c.apply_overrides(overrides));
                (c, fb)
            }
            None => {
                eprintln!(
                    "ramo: no config file found in $XDG_CONFIG_HOME/ramo/ or $HOME/.config/ramo/ \
                     — using defaults"
                );
                let mut c = Config::default();
                let fb = c.apply_overrides(overrides);
                (c, fb)
            }
        }
    }

    pub fn load_from_file(path: &Path) -> (Self, Vec<FeedbackEntry>) {
        match std::fs::read_to_string(path) {
            Ok(content) => Self::parse_content(path, &content),
            Err(e) => (
                Config::default(),
                vec![FeedbackEntry {
                    level: FeedbackType::Error,
                    message: format!("cannot read config file '{}': {e}", path.display()),
                }],
            ),
        }
    }

    pub(crate) fn parse_content(path: &Path, content: &str) -> (Self, Vec<FeedbackEntry>) {
        let mut config = Config::default();
        let mut feedbacks = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let (key, value) = match trimmed.split_once('=') {
                Some((k, v)) => (k.trim(), v.trim()),
                None => (trimmed, ""),
            };

            // strip inline comment after value (e.g. `k # comment` -> `k`)
            let value = strip_inline_value(value);
            if value.is_empty() {
                if !config.reset_field(key) {
                    feedbacks.push(FeedbackEntry {
                        level: FeedbackType::Warning,
                        message: format!("'{key}' isn't a valid config key in {}", path.display()),
                    });
                }
                continue;
            }

            config.set_field(key, &value, &mut feedbacks);
        }

        (config, feedbacks)
    }

    pub fn apply_overrides(
        &mut self,
        overrides: &[(String, Option<String>)],
    ) -> Vec<FeedbackEntry> {
        let mut feedbacks = Vec::new();
        for (key, val) in overrides {
            match val {
                Some(v) => {
                    if !self.set_field(key, v, &mut feedbacks) {
                        feedbacks.push(FeedbackEntry {
                            level: FeedbackType::Warning,
                            message: format!("'{key}' isn't a valid flag"),
                        });
                    }
                }
                None => match key.as_str() {
                    "auto-close"
                    | "hide-changes-inactive"
                    | "hide-changes-active"
                    | "hide-changes-worktree"
                    | "hide-hints-footer"
                    | "hide-hints-branches-active"
                    | "hide-hints-branches-inactive"
                    | "hide-hints-remotes-active"
                    | "hide-hints-remotes-inactive" => {
                        self.set_field(key, "true", &mut feedbacks);
                    }
                    _ => feedbacks.push(FeedbackEntry {
                        level: FeedbackType::Error,
                        message: format!("--{key} requires a value (e.g. --{key}=value)"),
                    }),
                },
            }
        }
        feedbacks
    }

    fn set_field(&mut self, key: &str, value: &str, feedbacks: &mut Vec<FeedbackEntry>) -> bool {
        match key {
            "path" => set_path(&mut self.path, key, value, feedbacks),
            "path-worktrees" => { self.path_worktrees = value.to_string(); true }
            "bind-jumpto" => { self.bind_jumpto = value.to_string(); true }
            "bind-command-mode" => set_char(&mut self.bind_command_mode, key, value, feedbacks),
            "bind-help" => set_char(&mut self.bind_help, key, value, feedbacks),
            "bind-command-session-kill" => { self.bind_command_session_kill = value.to_string(); true }
            "bind-command-worktree-new" => { self.bind_command_worktree_new = value.to_string(); true }
            "bind-command-worktree-delete" => { self.bind_command_worktree_delete = value.to_string(); true }
            "bind-quit" => { self.bind_quit = value.to_string(); true }
            "bind-nav-up" => { self.bind_nav_up = value.to_string(); true }
            "bind-nav-down" => { self.bind_nav_down = value.to_string(); true }
            "bind-nav-page-up" => { self.bind_nav_page_up = value.to_string(); true }
            "bind-nav-page-down" => { self.bind_nav_page_down = value.to_string(); true }
            "bind-input-left" => { self.bind_input_left = value.to_string(); true }
            "bind-input-right" => { self.bind_input_right = value.to_string(); true }
            "bind-input-home" => { self.bind_input_home = value.to_string(); true }
            "bind-input-end" => { self.bind_input_end = value.to_string(); true }
            "bind-input-kill-line" => { self.bind_input_kill_line = value.to_string(); true }
            "bind-input-delete-word" => { self.bind_input_delete_word = value.to_string(); true }
            "bind-input-clear" => { self.bind_input_clear = value.to_string(); true }
            "bind-input-word-left" => { self.bind_input_word_left = value.to_string(); true }
            "bind-input-word-right" => { self.bind_input_word_right = value.to_string(); true }
            "bind-input-backspace" => { self.bind_input_backspace = value.to_string(); true }
            "bind-input-delete" => { self.bind_input_delete = value.to_string(); true }
            "bind-command-exit" => { self.bind_command_exit = value.to_string(); true }
            "bind-command-open-detached" => { self.bind_command_open_detached = value.to_string(); true }
            "bind-help-enter" => { self.bind_help_enter = value.to_string(); true }
            "bind-help-exit" => { self.bind_help_exit = value.to_string(); true }
            "auto-close" => set_bool(&mut self.auto_close, key, value, feedbacks),
            "daemon-timeout" => set_u64(&mut self.daemon_timeout, key, value, feedbacks),
            "hide-changes-inactive" => set_bool(&mut self.hide_changes_inactive, key, value, feedbacks),
            "hide-changes-active" => set_bool(&mut self.hide_changes_active, key, value, feedbacks),
            "hide-changes-worktree" => set_bool(&mut self.hide_changes_worktree, key, value, feedbacks),
            "hide-hints-footer" => set_bool(&mut self.hide_hints_footer, key, value, feedbacks),
            "hide-hints-branches-active" => set_bool(&mut self.hide_hints_branches_active, key, value, feedbacks),
            "hide-hints-branches-inactive" => set_bool(&mut self.hide_hints_branches_inactive, key, value, feedbacks),
            "hide-hints-remotes-active" => set_bool(&mut self.hide_hints_remotes_active, key, value, feedbacks),
            "hide-hints-remotes-inactive" => set_bool(&mut self.hide_hints_remotes_inactive, key, value, feedbacks),
            "style-icon-daemon-loading" => { self.style_icon_daemon_loading = value.to_string(); true }
            "style-icon-daemon-ready" => { self.style_icon_daemon_ready = value.to_string(); true }
            "style-icon-active" => { self.style_icon_active = value.to_string(); true }
            "style-icon-worktree" => { self.style_icon_worktree = value.to_string(); true }
            "style-icon-agent-idle" => { self.style_icon_agent_idle = value.to_string(); true }
            "style-icon-agent-running" => { self.style_icon_agent_running = value.to_string(); true }
            "style-icon-input" => { self.style_icon_input = value.to_string(); true }
            "style-entries-gap" => set_u64(&mut self.style_entries_gap, key, value, feedbacks),
            _ => false,
        }
    }

    fn reset_field(&mut self, key: &str) -> bool {
        let d = Config::default();
        match key {
            "path" => { self.path = d.path; true }
            "path-worktrees" => { self.path_worktrees = d.path_worktrees; true }
            "bind-jumpto" => { self.bind_jumpto = d.bind_jumpto; true }
            "bind-command-mode" => { self.bind_command_mode = d.bind_command_mode; true }
            "bind-help" => { self.bind_help = d.bind_help; true }
            "bind-command-session-kill" => { self.bind_command_session_kill = d.bind_command_session_kill; true }
            "bind-command-worktree-new" => { self.bind_command_worktree_new = d.bind_command_worktree_new; true }
            "bind-command-worktree-delete" => { self.bind_command_worktree_delete = d.bind_command_worktree_delete; true }
            "bind-quit" => { self.bind_quit = d.bind_quit; true }
            "bind-nav-up" => { self.bind_nav_up = d.bind_nav_up; true }
            "bind-nav-down" => { self.bind_nav_down = d.bind_nav_down; true }
            "bind-nav-page-up" => { self.bind_nav_page_up = d.bind_nav_page_up; true }
            "bind-nav-page-down" => { self.bind_nav_page_down = d.bind_nav_page_down; true }
            "bind-input-left" => { self.bind_input_left = d.bind_input_left; true }
            "bind-input-right" => { self.bind_input_right = d.bind_input_right; true }
            "bind-input-home" => { self.bind_input_home = d.bind_input_home; true }
            "bind-input-end" => { self.bind_input_end = d.bind_input_end; true }
            "bind-input-kill-line" => { self.bind_input_kill_line = d.bind_input_kill_line; true }
            "bind-input-delete-word" => { self.bind_input_delete_word = d.bind_input_delete_word; true }
            "bind-input-clear" => { self.bind_input_clear = d.bind_input_clear; true }
            "bind-input-word-left" => { self.bind_input_word_left = d.bind_input_word_left; true }
            "bind-input-word-right" => { self.bind_input_word_right = d.bind_input_word_right; true }
            "bind-input-backspace" => { self.bind_input_backspace = d.bind_input_backspace; true }
            "bind-input-delete" => { self.bind_input_delete = d.bind_input_delete; true }
            "bind-command-exit" => { self.bind_command_exit = d.bind_command_exit; true }
            "bind-command-open-detached" => { self.bind_command_open_detached = d.bind_command_open_detached; true }
            "bind-help-enter" => { self.bind_help_enter = d.bind_help_enter; true }
            "bind-help-exit" => { self.bind_help_exit = d.bind_help_exit; true }
            "auto-close" => { self.auto_close = d.auto_close; true }
            "daemon-timeout" => { self.daemon_timeout = d.daemon_timeout; true }
            "hide-changes-inactive" => { self.hide_changes_inactive = d.hide_changes_inactive; true }
            "hide-changes-active" => { self.hide_changes_active = d.hide_changes_active; true }
            "hide-changes-worktree" => { self.hide_changes_worktree = d.hide_changes_worktree; true }
            "hide-hints-footer" => { self.hide_hints_footer = d.hide_hints_footer; true }
            "hide-hints-branches-active" => { self.hide_hints_branches_active = d.hide_hints_branches_active; true }
            "hide-hints-branches-inactive" => { self.hide_hints_branches_inactive = d.hide_hints_branches_inactive; true }
            "hide-hints-remotes-active" => { self.hide_hints_remotes_active = d.hide_hints_remotes_active; true }
            "hide-hints-remotes-inactive" => { self.hide_hints_remotes_inactive = d.hide_hints_remotes_inactive; true }
            "style-icon-daemon-loading" => { self.style_icon_daemon_loading = d.style_icon_daemon_loading; true }
            "style-icon-daemon-ready" => { self.style_icon_daemon_ready = d.style_icon_daemon_ready; true }
            "style-icon-active" => { self.style_icon_active = d.style_icon_active; true }
            "style-icon-worktree" => { self.style_icon_worktree = d.style_icon_worktree; true }
            "style-icon-agent-idle" => { self.style_icon_agent_idle = d.style_icon_agent_idle; true }
            "style-icon-agent-running" => { self.style_icon_agent_running = d.style_icon_agent_running; true }
            "style-icon-input" => { self.style_icon_input = d.style_icon_input; true }
            "style-entries-gap" => { self.style_entries_gap = d.style_entries_gap; true }
            _ => false,
        }
    }

    /// Check if a key event matches any of the comma-separated binds in `spec`.
    /// Spec examples: "enter", "esc,ctrl-c", "up,ctrl-p", "alt-b", "o", "?"
    pub fn key_matches(spec: &str, key: KeyEvent) -> bool {
        for token in spec.split(',') {
            let t = token.trim();
            if t.is_empty() {
                continue;
            }
            if token_matches(t, key) {
                return true;
            }
        }
        false
    }

    /// Convenience: does `key` match the bind for this config entry?
    #[allow(dead_code)]
    pub fn matches(&self, bind: &str, key: KeyEvent) -> bool {
        let Some(spec) = self.value_string(bind) else {
            return false;
        };
        Self::key_matches(&spec, key)
    }
}

fn token_matches(token: &str, key: KeyEvent) -> bool {
    let t = token.trim().to_lowercase();
    if t.is_empty() {
        return false;
    }
    // split modifiers: all parts except last are modifiers, last is key
    let parts: Vec<&str> = t.split('-').collect();
    let (mods, key_part) = if parts.len() == 1 {
        (Vec::new(), parts[0])
    } else {
        (parts[..parts.len() - 1].to_vec(), parts[parts.len() - 1])
    };
    let mut need_ctrl = false;
    let mut need_alt = false;
    let mut need_shift = false;
    for m in mods {
        match m {
            "ctrl" | "control" => need_ctrl = true,
            "alt" => need_alt = true,
            "shift" => need_shift = true,
            _ => return false,
        }
    }
    let has_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let has_alt = key.modifiers.contains(KeyModifiers::ALT);
    let has_shift = key.modifiers.contains(KeyModifiers::SHIFT);
    if need_ctrl != has_ctrl {
        return false;
    }
    if need_alt != has_alt {
        return false;
    }
    if need_shift && !has_shift {
        return false;
    }
    // allow extra shift for plain keys (e.g. "?" needs shift, but we ignore if not required)
    match key_part {
        "enter" => key.code == KeyCode::Enter,
        "esc" | "escape" => key.code == KeyCode::Esc,
        "up" => key.code == KeyCode::Up,
        "down" => key.code == KeyCode::Down,
        "left" => key.code == KeyCode::Left,
        "right" => key.code == KeyCode::Right,
        "backspace" | "bs" => key.code == KeyCode::Backspace,
        "delete" | "del" => key.code == KeyCode::Delete,
        "tab" => key.code == KeyCode::Tab,
        "space" => key.code == KeyCode::Char(' '),
        _ => {
            if key_part.chars().count() == 1 {
                let ch = key_part.chars().next().unwrap();
                if let KeyCode::Char(c) = key.code {
                    c.to_ascii_lowercase() == ch
                } else {
                    false
                }
            } else {
                false
            }
        }
    }
}

fn parse_path(name: &str, value: &str) -> Result<(), String> {
    for raw in value.split(':') {
        if raw.is_empty() {
            continue;
        }
        let base = raw.trim_end_matches("/*");
        let expanded = util::expand_tilde(base);
        if !expanded.is_dir() {
            return Err(format!(
                "'{name}' has invalid value {value:?} — segment '{raw}' does not point to a real directory (resolved to {}), falling back to default",
                expanded.display()
            ));
        }
    }
    Ok(())
}

fn parse_char(name: &str, value: &str) -> Result<char, String> {
    let mut chars = value.chars();
    let Some(c) = chars.next() else {
        return Err(format!(
            "'{name}' has invalid value {value:?} — expected a single character, falling back to default"
        ));
    };
    if chars.next().is_some() {
        return Err(format!(
            "'{name}' has invalid value {value:?} — expected a single character, falling back to default"
        ));
    }
    Ok(c)
}

fn parse_bool(name: &str, value: &str) -> Result<bool, String> {
    match value.trim() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(format!(
            "'{name}' has invalid value {value:?} — expected true/false, 1/0, yes/no, or on/off, falling back to default"
        )),
    }
}

fn parse_u64(name: &str, value: &str) -> Result<u64, String> {
    match value.trim().parse::<u64>() {
        Ok(n) => Ok(n),
        Err(_) => Err(format!(
            "'{name}' has invalid value {value:?} — expected a non-negative integer, falling back to default"
        )),
    }
}

fn push_err(fb: &mut Vec<FeedbackEntry>, msg: String) {
    fb.push(FeedbackEntry { level: FeedbackType::Error, message: msg });
}
fn set_bool(slot: &mut bool, key: &str, value: &str, fb: &mut Vec<FeedbackEntry>) -> bool {
    match parse_bool(key, value) { Ok(v) => *slot = v, Err(m) => push_err(fb, m) } true
}
fn set_u64(slot: &mut u64, key: &str, value: &str, fb: &mut Vec<FeedbackEntry>) -> bool {
    match parse_u64(key, value) { Ok(v) => *slot = v, Err(m) => push_err(fb, m) } true
}
fn set_char(slot: &mut char, key: &str, value: &str, fb: &mut Vec<FeedbackEntry>) -> bool {
    match parse_char(key, value) { Ok(v) => *slot = v, Err(m) => push_err(fb, m) } true
}
fn set_path(slot: &mut String, key: &str, value: &str, fb: &mut Vec<FeedbackEntry>) -> bool {
    match parse_path(key, value) { Ok(()) => *slot = value.to_string(), Err(m) => push_err(fb, m) } true
}
