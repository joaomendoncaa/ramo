use crate::model::Opencode;
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;

pub fn list_sessions() -> Vec<Opencode> {
    try_v2().unwrap_or_default()
}

fn run(bin: &str, args: &[&str]) -> Option<Vec<u8>> {
    let out = Command::new(bin).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(out.stdout)
}

fn candidates() -> Vec<String> {
    let mut c = vec!["opencode2".to_string(), "opencode".to_string()];
    if let Ok(home) = std::env::var("HOME") {
        c.push(format!("{home}/.local/share/mise/shims/opencode2"));
        c.push(format!("{home}/.local/share/mise/shims/opencode"));
        c.push(format!("{home}/.opencode/bin/opencode"));
        c.push(format!("{home}/.opencode/bin/opencode2"));
    }
    c
}

fn try_v2() -> Option<Vec<Opencode>> {
    let v: serde_json::Value = candidates().iter().find_map(|b| {
        let d = run(b, &["api", "v2.session.list", "--param", "limit=500"])?;
        let val: serde_json::Value = serde_json::from_slice(&d).ok()?;
        val.get("data")?.as_array()?;
        Some(val)
    })?;
    let arr = v.get("data")?.as_array()?;
    let mut active: HashSet<String> = HashSet::new();
    for b in candidates() {
        if let Some(o) = run(&b, &["api", "v2.session.active"]) {
            if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&o) {
                if let Some(map) = val.get("data").and_then(|d| d.as_object()) {
                    active = map.keys().cloned().collect();
                    break;
                }
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
