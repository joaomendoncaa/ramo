use crate::model::{Goto, TmuxPane, TmuxSession};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

pub struct Snapshot {
    pub sessions: Vec<TmuxSession>,
    pub panes: Vec<TmuxPane>,
}

// Sessions and panes come from a single tmux call: every session has at least
// one pane, so `list-panes -a` is a superset of `list-sessions`.
pub fn snapshot() -> Snapshot {
    let mut sessions: Vec<TmuxSession> = Vec::new();
    let mut panes: Vec<TmuxPane> = Vec::new();
    for line in tmux_lines(&[
        "list-panes",
        "-a",
        "-F",
        "#{session_name}\t#{session_path}\t#{window_index}\t#{pane_index}\t#{pane_id}\t#{pane_current_command}\t#{pane_current_path}\t#{session_activity}",
    ]) {
        let p: Vec<&str> = line.split('\t').collect();
        if p.len() != 8 {
            continue;
        }
        panes.push(TmuxPane {
            session_name: p[0].into(),
            window_index: p[2].parse().unwrap_or(0),
            pane_index: p[3].parse().unwrap_or(0),
            pane_id: p[4].into(),
            current_command: p[5].into(),
            current_path: PathBuf::from(p[6]),
            activity: p[7].parse().unwrap_or(0),
        });
        if !sessions.iter().any(|s| s.name == p[0]) {
            sessions.push(TmuxSession {
                name: p[0].into(),
                path: PathBuf::from(p[1]),
            });
        }
    }
    Snapshot { sessions, panes }
}

pub(crate) fn tmux_lines(args: &[&str]) -> Vec<String> {
    Command::new("tmux")
        .args(args)
        .output()
        .map(|o| {
            if !o.status.success() {
                return vec![];
            }
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub fn opencode_panes(panes: &[TmuxPane]) -> Vec<&TmuxPane> {
    // Process-only detection. Whether the pane is an opencode pane is
    // determined solely by what executable is running in it — either
    // `opencode` or `opencode2`. Window/pane titles are never consulted,
    // even as a fallback.
    panes.iter().filter(|p| is_opencode_command(&p.current_command)).collect()
}

fn is_opencode_command(cmd: &str) -> bool {
    // `pane_current_command` is usually the basename, but handle a full
    // path just in case (e.g. "/usr/local/bin/opencode2").
    if cmd == "opencode" || cmd == "opencode2" {
        return true;
    }
    if let Some(base) = cmd.rsplit('/').next() {
        return base == "opencode" || base == "opencode2";
    }
    false
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

// tmux rejects `:`/`.` in new session names, so ramo-created sessions
// live under the sanitized name. Pre-existing external sessions keep
// their raw name — resolve to whichever actually exists so we never
// shadow a live session with an empty duplicate.
pub fn sanitize_session(name: &str) -> String {
    name.replace([':', '.'], "_")
}

pub fn resolve_session(name: &str) -> String {
    if has_session(name) {
        name.to_string()
    } else {
        sanitize_session(name)
    }
}

// `=name` forces an exact session match so raw names containing
// `:`/`.` aren't parsed as window/pane separators.
fn exact(session: &str) -> String {
    format!("={session}")
}

pub fn goto(action: &Goto) {
    let Goto {
        session,
        path,
        window,
        pane,
        pane_id,
    } = action;

    let target = resolve_session(session);
    if !has_session(&target) && !new_session(&target, path) {
        let _ = Command::new("tmux")
            .args([
                "display-message",
                &format!("ramo: failed to create session '{}'", session),
            ])
            .stderr(Stdio::null())
            .status();
        return;
    }
    switch_client(&target);
    select_agent_pane(&target, *window, *pane, pane_id.as_deref());
}

pub fn open_detached(action: &Goto) {
    let Goto { session, path, .. } = action;
    let target = resolve_session(session);
    if !has_session(&target) {
        new_session(&target, path);
    }
}

// Coordinates go stale between list-build and Enter (panes open/close
// while agents work, shifting window/pane indexes). `%id` is stable for
// the pane's lifetime, so re-resolve the live coordinates from it at
// goto time; fall back to the stored ones when unknown or dead. The flag
// tells whether the coordinates are live: callers must never act on a
// dead pane's stored window index, it may now point at a stranger window.
pub(crate) fn fresh_target(
    session: &str,
    window: Option<usize>,
    pane: Option<usize>,
    pane_id: Option<&str>,
) -> (String, Option<usize>, Option<usize>, bool) {
    let Some(id) = pane_id.filter(|s| !s.is_empty()) else {
        return (session.to_string(), window, pane, false);
    };
    let out = Command::new("tmux")
        .args([
            "display-message",
            "-p",
            "-t",
            id,
            "-F",
            "#{session_name}\t#{window_index}\t#{pane_index}",
        ])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let f: Vec<&str> = out.split('\t').collect();
    if f.len() == 3 && !f[0].is_empty()
        && let (Ok(w), Ok(p)) = (f[1].parse(), f[2].parse())
    {
        return (f[0].to_string(), Some(w), Some(p), true);
    }
    (session.to_string(), window, pane, false)
}

// `%id` is stable for the pane's lifetime; indexes shift when panes
// open/close between list-build and click, so prefer it when known.
// `select-pane` alone leaves the attached client on its current window,
// so the live window is selected first. A dead pane's stored index is
// never trusted (it may name a stranger window now) — land on the
// session instead.
pub fn select_agent_pane(
    session: &str,
    window: Option<usize>,
    pane: Option<usize>,
    pane_id: Option<&str>,
) {
    let (session, window, pane, live) = fresh_target(session, window, pane, pane_id);
    let Some(id) = pane_id.filter(|s| !s.is_empty()) else {
        if let (Some(window), Some(pane)) = (window, pane) {
            select_pane(&session, window, pane);
        }
        return;
    };
    if live && let Some(window) = window {
        let _ = Command::new("tmux")
            .args(["select-window", "-t", &format!("={session}:{window}")])
            .stderr(Stdio::null())
            .status();
    }
    let _ = Command::new("tmux")
        .args(["select-pane", "-t", id])
        .stderr(Stdio::null())
        .status();
}

pub fn select_pane(session: &str, window: usize, pane: usize) {
    let _ = Command::new("tmux")
        .args(["select-window", "-t", &format!("={}:{}", session, window)])
        .stderr(Stdio::null())
        .status();
    let _ = Command::new("tmux")
        .args([
            "select-pane",
            "-t",
            &format!("={}:{}.{}", session, window, pane),
        ])
        .stderr(Stdio::null())
        .status();
}

pub fn current_session_name() -> Option<String> {
    Command::new("tmux")
        .args(["display-message", "-p", "#{session_name}"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn is_current_session(name: &str) -> bool {
    current_session_name().map(|s| s == name).unwrap_or(false)
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
            .args([cmd, "-t", &exact(name)])
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
        .args(["kill-session", "-t", &exact(name)])
        .stderr(Stdio::null())
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    // Dead/unknown pane id: no tmux lookup succeeds, stored coords win,
    // flagged as stale.
    #[test]
    fn fresh_target_falls_back_when_pane_dead() {
        let (s, w, p, live) = fresh_target("sess", Some(3), Some(1), Some("%999999"));
        assert_eq!((s.as_str(), w, p, live), ("sess", Some(3), Some(1), false));
        let (s, w, p, live) = fresh_target("sess", Some(3), Some(1), None);
        assert_eq!((s.as_str(), w, p, live), ("sess", Some(3), Some(1), false));
    }
}

pub fn kill_window(session: &str, window: usize) {
    let _ = Command::new("tmux")
        .args(["kill-window", "-t", &format!("={}:{}", session, window)])
        .stderr(Stdio::null())
        .status();
}

// Agent kill with live coordinates: a stored window index may have
// shifted since list-build, and killing the wrong window is worse
// than jumping to it. A dead pane kills nothing — its stored index may
// name a stranger window holding other agents, and the agent itself is
// already gone.
pub fn kill_agent(session: &str, window: Option<usize>, pane_id: Option<&str>) {
    let (session, window, _, live) = fresh_target(session, window, None, pane_id);
    if pane_id.is_some_and(|s| !s.is_empty()) && !live {
        return;
    }
    if let Some(w) = window {
        kill_window(&session, w);
    } else {
        kill_session(&session);
    }
}
