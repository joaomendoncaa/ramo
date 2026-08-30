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
    "esc,ctrl-c,ctrl-q".into()
}
fn default_bind_nav_up() -> String {
    "up,ctrl-p".into()
}
fn default_bind_nav_down() -> String {
    "down,ctrl-n".into()
}
fn default_bind_nav_page_up() -> String {
    "ctrl-u".into()
}
fn default_bind_nav_page_down() -> String {
    "ctrl-d".into()
}
fn default_bind_input_left() -> String {
    "left".into()
}
fn default_bind_input_right() -> String {
    "right".into()
}
fn default_bind_input_home() -> String {
    "ctrl-a".into()
}
fn default_bind_input_end() -> String {
    "ctrl-e".into()
}
fn default_bind_input_kill_line() -> String {
    "ctrl-k".into()
}
fn default_bind_input_delete_word() -> String {
    "ctrl-w".into()
}
fn default_bind_input_clear() -> String {
    "ctrl-r".into()
}
fn default_bind_input_word_left() -> String {
    "alt-b".into()
}
fn default_bind_input_word_right() -> String {
    "alt-f".into()
}
fn default_bind_input_backspace() -> String {
    "backspace".into()
}
fn default_bind_input_delete() -> String {
    "delete".into()
}
fn default_bind_command_exit() -> String {
    "esc,backspace".into()
}
fn default_bind_command_open_detached() -> String {
    "o".into()
}
fn default_bind_help_enter() -> String {
    "enter".into()
}
fn default_bind_help_exit() -> String {
    "esc".into()
}

fn strip_inline_value(s: &str) -> String {
    if let Some(idx) = s.find('#') {
        s[..idx].trim().to_string()
    } else {
        s.trim().to_string()
    }
}

macro_rules! config {
    ( $( $field:ident : $ty:ty = $default:expr => $key:literal : $kind:ident $(: $serde:literal)? ),* $(,)? ) => {
        #[derive(Clone, Debug, Serialize, Deserialize)]
        pub struct Config {
            $(
                $(#[serde(default = $serde)])?
                pub $field: $ty,
            )*
        }

        impl Default for Config {
            fn default() -> Self {
                Self {
                    $($field: $default,)*
                }
            }
        }

        impl Config {
            pub fn value_string(&self, key: &str) -> Option<String> {
                match key {
                    $($key => Some(self.$field.to_string()),)*
                    _ => None,
                }
            }

            fn set_field(&mut self, key: &str, value: &str, feedbacks: &mut Vec<FeedbackEntry>) -> bool {
                match key {
                    $($key => config!(@set $kind, &mut self.$field, key, value, feedbacks),)*
                    _ => false,
                }
            }

            fn reset_field(&mut self, key: &str) -> bool {
                let d = Config::default();
                match key {
                    $($key => { self.$field = d.$field; true },)*
                    _ => false,
                }
            }
        }
    };
    (@set string, $slot:expr, $key:expr, $value:expr, $fb:expr) => { { *$slot = $value.to_string(); true } };
    (@set char, $slot:expr, $key:expr, $value:expr, $fb:expr) => { set_char($slot, $key, $value, $fb) };
    (@set bool, $slot:expr, $key:expr, $value:expr, $fb:expr) => { set_bool($slot, $key, $value, $fb) };
    (@set u64, $slot:expr, $key:expr, $value:expr, $fb:expr) => { set_u64($slot, $key, $value, $fb) };
    (@set path, $slot:expr, $key:expr, $value:expr, $fb:expr) => { set_path($slot, $key, $value, $fb) };
}

config! {
    path: String = { std::env::var("HOME").map(|h| format!("{h}/Projects/*")).unwrap_or_else(|_| "~/Projects/*".to_string()) } => "path" : path,
    path_worktrees: String = String::new() => "path-worktrees" : string,
    bind_jumpto: String = "enter".to_string() => "bind-jumpto" : string,
    bind_command_mode: char = ':' => "bind-command-mode" : char,
    bind_help: char = '?' => "bind-help" : char : "default_bind_help",
    bind_command_session_kill: String = "k".to_string() => "bind-command-session-kill" : string,
    bind_command_worktree_new: String = "n".to_string() => "bind-command-worktree-new" : string,
    bind_command_worktree_delete: String = "d".to_string() => "bind-command-worktree-delete" : string,
    bind_quit: String = "esc,ctrl-c,ctrl-q".to_string() => "bind-quit" : string : "default_bind_quit",
    bind_nav_up: String = "up,ctrl-p".to_string() => "bind-nav-up" : string : "default_bind_nav_up",
    bind_nav_down: String = "down,ctrl-n".to_string() => "bind-nav-down" : string : "default_bind_nav_down",
    bind_nav_page_up: String = "ctrl-u".to_string() => "bind-nav-page-up" : string : "default_bind_nav_page_up",
    bind_nav_page_down: String = "ctrl-d".to_string() => "bind-nav-page-down" : string : "default_bind_nav_page_down",
    bind_input_left: String = "left".to_string() => "bind-input-left" : string : "default_bind_input_left",
    bind_input_right: String = "right".to_string() => "bind-input-right" : string : "default_bind_input_right",
    bind_input_home: String = "ctrl-a".to_string() => "bind-input-home" : string : "default_bind_input_home",
    bind_input_end: String = "ctrl-e".to_string() => "bind-input-end" : string : "default_bind_input_end",
    bind_input_kill_line: String = "ctrl-k".to_string() => "bind-input-kill-line" : string : "default_bind_input_kill_line",
    bind_input_delete_word: String = "ctrl-w".to_string() => "bind-input-delete-word" : string : "default_bind_input_delete_word",
    bind_input_clear: String = "ctrl-r".to_string() => "bind-input-clear" : string : "default_bind_input_clear",
    bind_input_word_left: String = "alt-b".to_string() => "bind-input-word-left" : string : "default_bind_input_word_left",
    bind_input_word_right: String = "alt-f".to_string() => "bind-input-word-right" : string : "default_bind_input_word_right",
    bind_input_backspace: String = "backspace".to_string() => "bind-input-backspace" : string : "default_bind_input_backspace",
    bind_input_delete: String = "delete".to_string() => "bind-input-delete" : string : "default_bind_input_delete",
    bind_command_exit: String = "esc,backspace".to_string() => "bind-command-exit" : string : "default_bind_command_exit",
    bind_command_open_detached: String = "o".to_string() => "bind-command-open-detached" : string : "default_bind_command_open_detached",
    bind_help_enter: String = "enter".to_string() => "bind-help-enter" : string : "default_bind_help_enter",
    bind_help_exit: String = "esc".to_string() => "bind-help-exit" : string : "default_bind_help_exit",
    auto_close: bool = true => "auto-close" : bool,
    daemon_timeout: u64 = 1800 => "daemon-timeout" : u64,
    hide_changes_inactive: bool = false => "hide-changes-inactive" : bool,
    hide_changes_active: bool = false => "hide-changes-active" : bool,
    hide_changes_worktree: bool = false => "hide-changes-worktree" : bool,
    hide_hints_footer: bool = false => "hide-hints-footer" : bool,
    hide_hints_branches_active: bool = false => "hide-hints-branches-active" : bool,
    hide_hints_branches_inactive: bool = false => "hide-hints-branches-inactive" : bool,
    hide_hints_remotes_active: bool = false => "hide-hints-remotes-active" : bool,
    hide_hints_remotes_inactive: bool = false => "hide-hints-remotes-inactive" : bool,
    style_icon_daemon_loading: String = "─╲│╱".to_string() => "style-icon-daemon-loading" : string,
    style_icon_daemon_ready: String = "✓".to_string() => "style-icon-daemon-ready" : string,
    style_icon_active: String = "*".to_string() => "style-icon-active" : string,
    style_icon_worktree: String = "⑂".to_string() => "style-icon-worktree" : string,
    style_icon_agent_idle: String = "✓".to_string() => "style-icon-agent-idle" : string,
    style_icon_agent_running: String = "⠋⠙⠹⠸⢰⣰⣠⣄⣆⡆⠇⠏".to_string() => "style-icon-agent-running" : string,
    style_icon_input: String = "▸".to_string() => "style-icon-input" : string,
    style_entries_gap: u64 = 0 => "style-entries-gap" : u64,
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

    pub fn write_target() -> PathBuf {
        Self::config_path().unwrap_or_else(|| Self::config_base().join("ramo/config"))
    }

    pub fn is_default_value(key: &str, new_value: &str) -> bool {
        let def = Config::default();
        let Some(def_val) = def.value_string(key) else {
            return false;
        };
        let new_trim = new_value.trim();
        if key == "path" || key == "path-worktrees" {
            return util::expand_tilde(new_trim) == util::expand_tilde(&def_val);
        }
        def_val == new_trim
    }

    pub fn new() -> (Self, Vec<FeedbackEntry>) {
        Self::load(&[])
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

    /// Check if a key event matches any of the comma-separated binds in `spec`.
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
}

fn token_matches(token: &str, key: KeyEvent) -> bool {
    let t = token.trim().to_lowercase();
    if t.is_empty() {
        return false;
    }
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
    fb.push(FeedbackEntry {
        level: FeedbackType::Error,
        message: msg,
    });
}
fn set_bool(slot: &mut bool, key: &str, value: &str, fb: &mut Vec<FeedbackEntry>) -> bool {
    match parse_bool(key, value) {
        Ok(v) => *slot = v,
        Err(m) => push_err(fb, m),
    }
    true
}
fn set_u64(slot: &mut u64, key: &str, value: &str, fb: &mut Vec<FeedbackEntry>) -> bool {
    match parse_u64(key, value) {
        Ok(v) => *slot = v,
        Err(m) => push_err(fb, m),
    }
    true
}
fn set_char(slot: &mut char, key: &str, value: &str, fb: &mut Vec<FeedbackEntry>) -> bool {
    match parse_char(key, value) {
        Ok(v) => *slot = v,
        Err(m) => push_err(fb, m),
    }
    true
}
fn set_path(slot: &mut String, key: &str, value: &str, fb: &mut Vec<FeedbackEntry>) -> bool {
    match parse_path(key, value) {
        Ok(()) => *slot = value.to_string(),
        Err(m) => push_err(fb, m),
    }
    true
}
