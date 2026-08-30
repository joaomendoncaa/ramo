use crate::model::Opencode;
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;

pub fn list_sessions() -> Vec<Opencode> {
    if let Some(v) = try_v2() {
        return v;
    }
    if let Some(v) = try_v1() {
        return v;
    }
    vec![]
}

fn run(bin: &str, args: &[&str]) -> Option<Vec<u8>> {
    let out = Command::new(bin).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(out.stdout)
}

fn try_v2() -> Option<Vec<Opencode>> {
    // opencode2 api handles auth/port discovery internally
    let data = run("opencode2", &["api", "v2.session.list", "--param", "limit=100"])?;
    let v: serde_json::Value = serde_json::from_slice(&data).ok()?;
    let arr = v.get("data")?.as_array()?;
    let mut active: HashSet<String> = HashSet::new();
    if let Some(o) = run("opencode2", &["api", "v2.session.active"]) {
        if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&o) {
            if let Some(map) = val.get("data").and_then(|d| d.as_object()) {
                active = map.keys().cloned().collect();
            }
        }
    }
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let id = item.get("id")?.as_str()?.to_string();
        let title = item.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string();
        if title == "Git Commit" {
            continue;
        }
        let dir = item
            .get("location")
            .and_then(|l| l.get("directory"))
            .and_then(|d| d.as_str())?;
        let updated = item
            .get("time")
            .and_then(|t| t.get("updated"))
            .and_then(|u| u.as_i64())
            .unwrap_or(0);
        out.push(Opencode {
            id: id.clone(),
            title,
            directory: PathBuf::from(dir),
            time_updated: updated,
            is_running: active.contains(&id),
        });
    }
    Some(out)
}

fn try_v1() -> Option<Vec<Opencode>> {
    let data = run("opencode", &["session", "list", "--format", "json", "-n", "100"])
        .or_else(|| run("opencode", &["session", "list", "--format", "json"]))?;
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&data).ok()?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let id = item.get("id")?.as_str()?.to_string();
        let title = item.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string();
        if title == "Git Commit" {
            continue;
        }
        let dir = item.get("directory")?.as_str()?;
        let updated = item.get("updated").and_then(|u| u.as_i64()).unwrap_or(0);
        out.push(Opencode {
            id,
            title,
            directory: PathBuf::from(dir),
            time_updated: updated,
            is_running: false, // v1 CLI has no running flag; derived via tmux pane match in builder
        });
    }
    Some(out)
}
