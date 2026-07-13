use crate::model::{Goto, TmuxPane, TmuxSession};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

pub fn list_sessions() -> Vec<TmuxSession> {
    tmux_output(
        &["list-sessions", "-F", "#{session_name}\t#{session_path}"],
        parse_session,
    )
}

pub fn list_panes() -> Vec<TmuxPane> {
    tmux_output(
        &[
            "list-panes",
            "-a",
            "-F",
            "#{session_name}\t#{window_index}\t#{pane_index}\t#{pane_current_command}\t#{pane_current_path}\t#{session_activity}",
        ],
        parse_pane,
    )
}

fn tmux_output<T>(args: &[&str], parse: impl Fn(&str) -> Option<T>) -> Vec<T> {
    Command::new("tmux")
        .args(args)
        .output()
        .map(|o| {
            if !o.status.success() {
                return vec![];
            }
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter_map(parse)
                .collect()
        })
        .unwrap_or_default()
}

fn parse_session(line: &str) -> Option<TmuxSession> {
    let p: Vec<&str> = line.split('\t').collect();
    (p.len() == 2).then(|| TmuxSession {
        name: p[0].into(),
        path: PathBuf::from(p[1]),
    })
}

fn parse_pane(line: &str) -> Option<TmuxPane> {
    let p: Vec<&str> = line.split('\t').collect();
    (p.len() == 6).then(|| TmuxPane {
        session_name: p[0].into(),
        window_index: p[1].parse().unwrap_or(0),
        pane_index: p[2].parse().unwrap_or(0),
        current_command: p[3].into(),
        current_path: PathBuf::from(p[4]),
        activity: p[5].parse().unwrap_or(0),
    })
}

pub fn opencode_panes(panes: &[TmuxPane]) -> Vec<&TmuxPane> {
    panes
        .iter()
        .filter(|p| p.current_command == "opencode")
        .collect()
}

// tmux sessions opened outside ramo are still surfaced in the picker
pub fn list_external_sessions(
    all_sessions: &[TmuxSession],
    covered_paths: &[PathBuf],
    covered_names: &[String],
) -> Vec<TmuxSession> {
    all_sessions
        .iter()
        .filter(|s| {
            !covered_paths
                .iter()
                .any(|p| s.path == *p || s.path.starts_with(p))
                && !covered_names.iter().any(|n| n == &s.name)
        })
        .cloned()
        .collect()
}

pub fn goto(action: &Goto) {
    let Goto {
        session,
        path,
        window,
        pane,
    } = action;

    let sanitized = session.replace([':', '.'], "_");
    if !has_session(&sanitized) && !new_session(&sanitized, path) {
        let _ = Command::new("tmux")
            .args([
                "display-message",
                &format!("ramo: failed to create session '{}'", session),
            ])
            .stderr(Stdio::null())
            .status();
        return;
    }
    switch_client(&sanitized);
    if let (Some(window), Some(pane)) = (window, pane) {
        select_pane(session, *window, *pane);
    }
}

pub fn open_detached(action: &Goto) {
    let Goto { session, path, .. } = action;
    let sanitized = session.replace([':', '.'], "_");
    if !has_session(&sanitized) {
        new_session(&sanitized, path);
    }
}

pub fn select_pane(session: &str, window: usize, pane: usize) {
    let _ = Command::new("tmux")
        .args(["select-window", "-t", &format!("{}:{}", session, window)])
        .stderr(Stdio::null())
        .status();
    let _ = Command::new("tmux")
        .args([
            "select-pane",
            "-t",
            &format!("{}:{}.{}", session, window, pane),
        ])
        .stderr(Stdio::null())
        .status();
}

pub fn is_current_session(name: String) -> bool {
    Command::new("tmux")
        .args(["display-message", "-p", "#{session_name}"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|s| s == name)
        .unwrap_or(false)
}

fn has_session(name: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", &format!("-t={}", name)])
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn new_session(name: &str, path: &Path) -> bool {
    Command::new("tmux")
        .args([
            "new-session",
            "-d",
            "-s",
            name,
            "-c",
            &path.to_string_lossy(),
        ])
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn switch_client(name: &str) {
    let cmd = if std::env::var("TMUX").is_ok() {
        "switch-client"
    } else {
        "attach"
    };
    for _ in 0..3 {
        let ok = Command::new("tmux")
            .args([cmd, "-t", name])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            return;
        }
        thread::sleep(Duration::from_millis(30));
    }
}

pub fn kill_session(name: &str) {
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", name])
        .stderr(Stdio::null())
        .status();
}

pub fn kill_window(session: &str, window: usize) {
    let _ = Command::new("tmux")
        .args(["kill-window", "-t", &format!("{}:{}", session, window)])
        .stderr(Stdio::null())
        .status();
}
