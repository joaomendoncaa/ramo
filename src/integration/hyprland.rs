use std::collections::HashMap;
use std::process::Command;

fn ppid(pid: u32) -> Option<u32> {
    let data = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    data.rsplit(')').next()?.split_whitespace().nth(1)?.parse().ok()
}

pub(crate) fn is_descendant(mut current: u32, ancestor: u32) -> bool {
    for _ in 0..32 {
        if current == ancestor {
            return true;
        }
        let Some(parent) = ppid(current) else {
            return false;
        };
        if parent == 0 || parent == current {
            return false;
        }
        current = parent;
    }
    current == ancestor
}

fn hyprctl_json(args: &[&str]) -> Option<serde_json::Value> {
    let out = Command::new("hyprctl").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

pub fn active_window_pid() -> Option<u32> {
    let v = hyprctl_json(&["activewindow", "-j"])?;
    u32::try_from(v.get("pid")?.as_u64()?).ok().filter(|p| *p != 0)
}

pub fn address_for_pid(pid: u32) -> Option<String> {
    let clients = hyprctl_json(&["clients", "-j"])?;
    let by_pid: HashMap<u32, String> = clients
        .as_array()?
        .iter()
        .filter_map(|c| {
            Some((
                u32::try_from(c.get("pid")?.as_u64()?).ok()?,
                c.get("address")?.as_str()?.to_string(),
            ))
        })
        .collect();
    let mut current = pid;
    for _ in 0..32 {
        if let Some(addr) = by_pid.get(&current)
            && !addr.is_empty()
        {
            return Some(addr.clone());
        }
        current = ppid(current)?;
        if current == 0 {
            return None;
        }
    }
    None
}

pub fn focus_address(address: &str) -> bool {
    Command::new("hyprctl")
        .args([
            "dispatch",
            &format!("hl.dsp.focus({{ window = 'address:{address}' }})"),
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
