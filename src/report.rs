use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// tmux pane id -> (session id, reported at). Fed by the daemon's report
/// listener; read by the tree builder. Reporter-agnostic.
pub type ReportMap = Arc<Mutex<HashMap<String, (String, Instant)>>>;

// A pane id reused by a restarted tmux server must not ghost an old session.
const REPORT_TTL: Duration = Duration::from_secs(6 * 3600);
// Empty verdicts ("no session") expire fast so a dead reporter stops pinning.
const EMPTY_TTL: Duration = Duration::from_secs(30);

fn is_fresh(id: &str, at: &Instant) -> bool {
    at.elapsed() < if id.is_empty() { EMPTY_TTL } else { REPORT_TTL }
}

/// Record that `pane_id` shows `session_id` (empty = no session).
pub fn insert(reports: &ReportMap, pane_id: &str, session_id: &str) {
    if let Ok(mut map) = reports.lock() {
        map.insert(pane_id.to_string(), (session_id.to_string(), Instant::now()));
    }
}

/// Exact session id displayed in `pane_id`, if freshly reported.
/// `Some("")` means the pane is known session-less: show the synthetic row,
/// never a stale title.
pub fn reported_session(reports: &ReportMap, pane_id: &str) -> Option<String> {
    let map = reports.lock().ok()?;
    let (id, at) = map.get(pane_id)?;
    is_fresh(id, at).then(|| id.clone())
}

pub fn prune(reports: &ReportMap, live_panes: &HashSet<String>) {
    if let Ok(mut map) = reports.lock() {
        map.retain(|pane, (id, at)| is_fresh(id, at) && live_panes.contains(pane));
    }
}

fn persist_path() -> std::path::PathBuf {
    crate::logs::state_dir().join("reports.json")
}

fn ttl_for(id: &str) -> Duration {
    if id.is_empty() { EMPTY_TTL } else { REPORT_TTL }
}

// Reports live in memory but the daemon is routinely respawned — every
// picker open with flags kills it (`fetch_or_spawn`), plus idle timeouts
// and crashes. Without persistence each respawn wipes every pane→session
// binding and all live agents flash "New session" until the TUIs resend
// (up to 2s). Disk keeps the respawned daemon born accurate; dead panes
// are still pruned on the first build and entries expire by age on load.
pub fn save(reports: &ReportMap) {
    save_to(&persist_path(), reports);
}

pub fn load() -> ReportMap {
    load_from(&persist_path())
}

/// File-backed seam the daemon's default-path [`save`]/[`load`] use.
/// Integration tests simulate a daemon respawn through here without
/// touching the real state dir.
pub fn save_to(path: &std::path::Path, reports: &ReportMap) {
    let snapshot: HashMap<String, (String, u64)> = match reports.lock() {
        Ok(map) => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            map.iter()
                .map(|(k, (id, _))| (k.clone(), (id.clone(), now)))
                .collect()
        }
        Err(_) => return,
    };
    if snapshot.is_empty() {
        return;
    }
    if path.parent().is_some_and(|d| std::fs::create_dir_all(d).is_err()) {
        return;
    }
    if let Ok(bytes) = serde_json::to_vec(&snapshot) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, &bytes).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

pub fn load_from(path: &std::path::Path) -> ReportMap {
    let reports = ReportMap::default();
    let Ok(bytes) = std::fs::read(path) else {
        return reports;
    };
    let Ok(stored): Result<HashMap<String, (String, u64)>, _> =
        serde_json::from_slice(&bytes)
    else {
        return reports;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(mut map) = reports.lock() {
        for (pane, (id, at)) in stored {
            // Drop by age: a pane id reused after a tmux server restart
            // must not ghost an old session, and dead reporters stop pinning.
            if now.saturating_sub(at) < ttl_for(&id).as_secs() && !pane.is_empty() {
                map.insert(pane, (id, Instant::now()));
            }
        }
    }
    reports
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> std::path::PathBuf {
        let p = std::path::PathBuf::from("/tmp/opencode").join(format!(
            "ramo-reports-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        p
    }

    #[test]
    fn round_trip_keeps_fresh_drops_stale() {
        let path = tmp();
        let reports = ReportMap::default();
        insert(&reports, "%11", "ses-live");
        insert(&reports, "%22", "");
        save_to(&path, &reports);
        let back = load_from(&path);
        assert_eq!(reported_session(&back, "%11").as_deref(), Some("ses-live"));
        assert_eq!(reported_session(&back, "%22").as_deref(), Some(""));
        // Ancient entries never ghost, even for reused pane ids.
        let old: HashMap<String, (String, u64)> =
            [("%99".to_string(), ("ses-old".to_string(), 1))].into();
        std::fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
        assert_eq!(reported_session(&load_from(&path), "%99"), None);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_loads_empty() {
        assert!(load_from(&tmp()).lock().unwrap().is_empty());
        // Saving an empty map writes nothing and stays empty.
        let path = tmp();
        save_to(&path, &ReportMap::default());
        assert!(!path.exists());
    }
}

