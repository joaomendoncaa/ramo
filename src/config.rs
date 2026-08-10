use crate::model::{FeedbackEntry, FeedbackType};
use crate::util;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub path: String,
    pub path_worktrees: String,
    pub bind_jumpto: String,
    pub bind_command_mode: char,
    pub bind_command_session_kill: String,
    pub bind_command_worktree_new: String,
    pub bind_command_worktree_delete: String,
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
            bind_command_session_kill: "k".to_string(),
            bind_command_worktree_new: "n".to_string(),
            bind_command_worktree_delete: "d".to_string(),
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

            if value.is_empty() {
                if !config.reset_field(key) {
                    feedbacks.push(FeedbackEntry {
                        level: FeedbackType::Warning,
                        message: format!("'{key}' isn't a valid config key in {}", path.display()),
                    });
                }
                continue;
            }

            config.set_field(key, value, &mut feedbacks);
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
            "path" => match parse_path(key, value) {
                Err(msg) => {
                    feedbacks.push(FeedbackEntry {
                        level: FeedbackType::Error,
                        message: msg,
                    });
                    true
                }
                Ok(()) => {
                    self.path = value.to_string();
                    true
                }
            },
            "path-worktrees" => {
                self.path_worktrees = value.to_string();
                true
            }
            "bind-jumpto" => {
                self.bind_jumpto = value.to_string();
                true
            }
            "bind-command-mode" => match parse_char(key, value) {
                Ok(c) => {
                    self.bind_command_mode = c;
                    true
                }
                Err(msg) => {
                    feedbacks.push(FeedbackEntry {
                        level: FeedbackType::Error,
                        message: msg,
                    });
                    true
                }
            },
            "bind-command-session-kill" => {
                self.bind_command_session_kill = value.to_string();
                true
            }
            "bind-command-worktree-new" => {
                self.bind_command_worktree_new = value.to_string();
                true
            }
            "bind-command-worktree-delete" => {
                self.bind_command_worktree_delete = value.to_string();
                true
            }
            "auto-close" => match parse_bool(key, value) {
                Ok(b) => {
                    self.auto_close = b;
                    true
                }
                Err(msg) => {
                    feedbacks.push(FeedbackEntry {
                        level: FeedbackType::Error,
                        message: msg,
                    });
                    true
                }
            },
            "daemon-timeout" => match parse_u64(key, value) {
                Ok(n) => {
                    self.daemon_timeout = n;
                    true
                }
                Err(msg) => {
                    feedbacks.push(FeedbackEntry {
                        level: FeedbackType::Error,
                        message: msg,
                    });
                    true
                }
            },
            "hide-changes-inactive" => match parse_bool(key, value) {
                Ok(b) => {
                    self.hide_changes_inactive = b;
                    true
                }
                Err(msg) => {
                    feedbacks.push(FeedbackEntry {
                        level: FeedbackType::Error,
                        message: msg,
                    });
                    true
                }
            },
            "hide-changes-active" => match parse_bool(key, value) {
                Ok(b) => {
                    self.hide_changes_active = b;
                    true
                }
                Err(msg) => {
                    feedbacks.push(FeedbackEntry {
                        level: FeedbackType::Error,
                        message: msg,
                    });
                    true
                }
            },
            "hide-changes-worktree" => match parse_bool(key, value) {
                Ok(b) => {
                    self.hide_changes_worktree = b;
                    true
                }
                Err(msg) => {
                    feedbacks.push(FeedbackEntry {
                        level: FeedbackType::Error,
                        message: msg,
                    });
                    true
                }
            },
            "hide-hints-footer" => match parse_bool(key, value) {
                Ok(b) => {
                    self.hide_hints_footer = b;
                    true
                }
                Err(msg) => {
                    feedbacks.push(FeedbackEntry {
                        level: FeedbackType::Error,
                        message: msg,
                    });
                    true
                }
            },
            "hide-hints-branches-active" => match parse_bool(key, value) {
                Ok(b) => {
                    self.hide_hints_branches_active = b;
                    true
                }
                Err(msg) => {
                    feedbacks.push(FeedbackEntry {
                        level: FeedbackType::Error,
                        message: msg,
                    });
                    true
                }
            },
            "hide-hints-branches-inactive" => match parse_bool(key, value) {
                Ok(b) => {
                    self.hide_hints_branches_inactive = b;
                    true
                }
                Err(msg) => {
                    feedbacks.push(FeedbackEntry {
                        level: FeedbackType::Error,
                        message: msg,
                    });
                    true
                }
            },
            "hide-hints-remotes-active" => match parse_bool(key, value) {
                Ok(b) => {
                    self.hide_hints_remotes_active = b;
                    true
                }
                Err(msg) => {
                    feedbacks.push(FeedbackEntry {
                        level: FeedbackType::Error,
                        message: msg,
                    });
                    true
                }
            },
            "hide-hints-remotes-inactive" => match parse_bool(key, value) {
                Ok(b) => {
                    self.hide_hints_remotes_inactive = b;
                    true
                }
                Err(msg) => {
                    feedbacks.push(FeedbackEntry {
                        level: FeedbackType::Error,
                        message: msg,
                    });
                    true
                }
            },
            "style-icon-daemon-loading" => {
                self.style_icon_daemon_loading = value.to_string();
                true
            }
            "style-icon-daemon-ready" => {
                self.style_icon_daemon_ready = value.to_string();
                true
            }
            "style-icon-active" => {
                self.style_icon_active = value.to_string();
                true
            }
            "style-icon-worktree" => {
                self.style_icon_worktree = value.to_string();
                true
            }
            "style-icon-agent-idle" => {
                self.style_icon_agent_idle = value.to_string();
                true
            }
            "style-icon-agent-running" => {
                self.style_icon_agent_running = value.to_string();
                true
            }
            "style-icon-input" => {
                self.style_icon_input = value.to_string();
                true
            }
            _ => false,
        }
    }

    fn reset_field(&mut self, key: &str) -> bool {
        let default = Config::default();
        match key {
            "path" => {
                self.path = default.path;
                true
            }
            "path-worktrees" => {
                self.path_worktrees = default.path_worktrees;
                true
            }
            "bind-jumpto" => {
                self.bind_jumpto = default.bind_jumpto;
                true
            }
            "bind-command-mode" => {
                self.bind_command_mode = default.bind_command_mode;
                true
            }
            "bind-command-session-kill" => {
                self.bind_command_session_kill = default.bind_command_session_kill;
                true
            }
            "bind-command-worktree-new" => {
                self.bind_command_worktree_new = default.bind_command_worktree_new;
                true
            }
            "bind-command-worktree-delete" => {
                self.bind_command_worktree_delete = default.bind_command_worktree_delete;
                true
            }
            "auto-close" => {
                self.auto_close = default.auto_close;
                true
            }
            "daemon-timeout" => {
                self.daemon_timeout = default.daemon_timeout;
                true
            }
            "hide-changes-inactive" => {
                self.hide_changes_inactive = default.hide_changes_inactive;
                true
            }
            "hide-changes-active" => {
                self.hide_changes_active = default.hide_changes_active;
                true
            }
            "hide-changes-worktree" => {
                self.hide_changes_worktree = default.hide_changes_worktree;
                true
            }
            "hide-hints-footer" => {
                self.hide_hints_footer = default.hide_hints_footer;
                true
            }
            "hide-hints-branches-active" => {
                self.hide_hints_branches_active = default.hide_hints_branches_active;
                true
            }
            "hide-hints-branches-inactive" => {
                self.hide_hints_branches_inactive = default.hide_hints_branches_inactive;
                true
            }
            "hide-hints-remotes-active" => {
                self.hide_hints_remotes_active = default.hide_hints_remotes_active;
                true
            }
            "hide-hints-remotes-inactive" => {
                self.hide_hints_remotes_inactive = default.hide_hints_remotes_inactive;
                true
            }
            "style-icon-daemon-loading" => {
                self.style_icon_daemon_loading = default.style_icon_daemon_loading;
                true
            }
            "style-icon-daemon-ready" => {
                self.style_icon_daemon_ready = default.style_icon_daemon_ready;
                true
            }
            "style-icon-active" => {
                self.style_icon_active = default.style_icon_active;
                true
            }
            "style-icon-worktree" => {
                self.style_icon_worktree = default.style_icon_worktree;
                true
            }
            "style-icon-agent-idle" => {
                self.style_icon_agent_idle = default.style_icon_agent_idle;
                true
            }
            "style-icon-agent-running" => {
                self.style_icon_agent_running = default.style_icon_agent_running;
                true
            }
            "style-icon-input" => {
                self.style_icon_input = default.style_icon_input;
                true
            }
            _ => false,
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
