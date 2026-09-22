use crate::daemon;
use crate::integration::hyprland;
use crate::model::{EntryType, Payload};
use crate::integration::tmux;
use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

// Bar/widgets read agents and jump to them through here, so the tmux
// targeting stays identical to the picker's (`integration::tmux`).
pub fn run_agents() {
    let Some(bytes) = daemon::fetch_once() else {
        println!(r#"{{"ok": false, "error": "ramo daemon unreachable"}}"#);
        std::process::exit(1);
    };
    let Ok(payload) = serde_json::from_slice::<Payload>(&bytes) else {
        println!(r#"{{"ok": false, "error": "invalid daemon payload"}}"#);
        std::process::exit(1);
    };
    let mut agents = Vec::new();
    for entry in &payload.entries {
        if entry.kind != EntryType::Agent {
            continue;
        }
        let Some(goto) = &entry.goto else { continue };
        let (Some(window), Some(pane)) = (goto.window, goto.pane) else { continue };
        if goto.session.is_empty() {
            continue;
        }
        let (repo, branch) = entry
            .parent
            .and_then(|i| payload.entries.get(i))
            .map(|p| (p.label.clone(), p.branch.clone().unwrap_or_default()))
            .unwrap_or_default();
        agents.push(serde_json::json!({
            "id": format!("{}:{window}:{pane}", goto.session),
            "session": goto.session,
            "window": window,
            "pane": pane,
            "paneId": goto.pane_id.clone().unwrap_or_default(),
            "title": if entry.label.is_empty() { "New session".to_string() } else { entry.label.clone() },
            "repo": repo,
            "branch": branch,
            "state": if entry.is_running { "running" } else { "idle" },
            "additions": 0,
            "deletions": 0,
            "activityAt": 0,
            "createdAt": 0,
        }));
    }
    agents.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    let by_pane: HashMap<String, String> = agents
        .iter()
        .filter_map(|a| {
            let pane_id = a["paneId"].as_str()?;
            (!pane_id.is_empty()).then(|| (pane_id.to_string(), a["id"].as_str().unwrap_or_default().to_string()))
        })
        .collect();
    let by_index: HashSet<String> = agents
        .iter()
        .filter_map(|a| a["id"].as_str().map(str::to_string))
        .collect();
    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    println!(
        "{}",
        serde_json::json!({
            "ok": true,
            "generatedAt": generated_at,
            "agents": agents,
            "activeAgentId": detect_active(&by_pane, &by_index),
        })
    );
}

fn focused_client_session() -> Option<String> {
    let win = hyprland::active_window_pid()?;
    let out = tmux::tmux_lines(&["list-clients", "-F", "#{client_pid} #{client_session}"]);
    for line in &out {
        let mut parts = line.splitn(2, ' ');
        if let (Some(pid), Some(session)) = (parts.next(), parts.next())
            && let Ok(pid) = pid.parse::<u32>()
            && hyprland::is_descendant(pid, win)
        {
            return Some(session.to_string());
        }
    }
    None
}

fn detect_active(by_pane: &HashMap<String, String>, by_index: &HashSet<String>) -> Option<String> {
    let session = focused_client_session()?;
    let out = tmux::tmux_lines(&[
        "list-panes",
        "-t",
        &format!("={session}"),
        "-F",
        "#{pane_id} #{window_index} #{pane_index} #{pane_active}",
    ]);
    for line in &out {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() != 4 || parts[3] != "1" {
            continue;
        }
        if let Some(id) = by_pane.get(parts[0]) {
            return Some(id.clone());
        }
        let candidate = format!("{}:{}:{}", session, parts[1], parts[2]);
        return by_index.contains(&candidate).then_some(candidate);
    }
    None
}

fn tmux_clients() -> Vec<(u32, String, String)> {
    tmux::tmux_lines(&["list-clients", "-F", "#{client_pid} #{client_tty} #{client_session}"])
        .iter()
        .filter_map(|line| {
            let mut parts = line.splitn(3, ' ');
            Some((parts.next()?.parse().ok()?, parts.next()?.to_string(), parts.next()?.to_string()))
        })
        .collect()
}

fn switch_client(target: &str, tty: Option<&str>) -> bool {
    let target = format!("={target}");
    let mut args = vec!["switch-client"];
    if let Some(tty) = tty {
        args.extend(["-c", tty]);
    } else if std::env::var("TMUX").is_err() {
        return true;
    }
    args.extend(["-t", &target]);
    Command::new("tmux")
        .args(&args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn run_focus(session: &str, pane_id: &str, window: &str, pane: &str) {
    if session.is_empty() {
        std::process::exit(1);
    }
    let (w, p) = (window.parse::<usize>().ok(), pane.parse::<usize>().ok());
    if (w.is_none() || p.is_none()) && pane_id.is_empty() {
        eprintln!("bad agent target: {session} {pane_id} {window} {pane}");
        std::process::exit(1);
    }
    let target = tmux::resolve_session(session);
    let pane_opt = (!pane_id.is_empty()).then_some(pane_id);
    let (target, w, p, _) = tmux::fresh_target(&target, w, p, pane_opt);
    // Prefer the client in the focused window so its terminal comes
    // forward; cross-workspace there is none, so steal one already on
    // the target session, else any client — then focus *its* Hyprland
    // window, which pulls the workspace. Previously the focused window
    // was targeted unconditionally, so cross-workspace nothing switched
    // and focus landed back where it started.
    let clients = tmux_clients();
    let win = hyprland::active_window_pid();
    let picked = win
        .and_then(|w| clients.iter().find(|c| hyprland::is_descendant(c.0, w)))
        .or_else(|| clients.iter().find(|c| c.2 == target))
        .or_else(|| clients.first());
    let tty = picked.map(|c| c.1.clone());
    let address = picked.and_then(|c| hyprland::address_for_pid(c.0));
    if !switch_client(&target, tty.as_deref()) {
        std::process::exit(1);
    }
    tmux::select_agent_pane(&target, w, p, pane_opt);
    if let Some(address) = address {
        hyprland::focus_address(&address);
    }
    let (w, p) = (
        w.map(|v| v.to_string()).unwrap_or_default(),
        p.map(|v| v.to_string()).unwrap_or_default(),
    );
    println!(r#"{{"activeAgentId": "{session}:{w}:{p}"}}"#);
}
