use crate::model::Opencode;
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};

/// Exact `agents-disabled` token for this source.
pub const SOURCE_ID: &str = "t3code/clankers";

/// Statuses that count as live. Mirrors `bin/clankers-agents` (`$rs`).
fn is_live(status: &str) -> bool {
    matches!(status, "preparing" | "queued" | "starting" | "running")
}

/// Every visible thread: not deleted, not archived, not a subagent.
/// Single query — no paging, no server round-trip (unlike opencode).
const QUERY: &str = "
SELECT t.thread_id,
    json_extract(t.payload_json, '$.title'),
    p.workspace_root,
    json_extract(t.payload_json, '$.worktreePath'),
    COALESCE(r.status, ''),
    COALESCE(CAST(strftime('%s', json_extract(t.payload_json, '$.updatedAt')) AS INTEGER), 0)
FROM orchestration_v2_projection_threads t
LEFT JOIN projection_projects p ON p.project_id = t.project_id
LEFT JOIN orchestration_v2_projection_runs r ON r.thread_id = t.thread_id
    AND r.ordinal = (SELECT MAX(ordinal) FROM orchestration_v2_projection_runs WHERE thread_id = t.thread_id)
WHERE t.deleted_at IS NULL AND t.archived_at IS NULL
    AND (json_extract(t.payload_json, '$.lineage.relationshipToParent') IS NULL
        OR json_extract(t.payload_json, '$.lineage.relationshipToParent') != 'subagent')
    -- Sidebar union (pinned + active + snoozed): unsettled threads, plus
    -- settled ones still snoozed. Settled rest never enters ramo — the
    -- equivalent of archived.
    AND ((json_extract(t.payload_json, '$.settledOverride') IS NULL
        OR json_extract(t.payload_json, '$.settledOverride') != 'settled')
        OR (json_extract(t.payload_json, '$.snoozedUntil') IS NOT NULL
            AND strftime('%s', json_extract(t.payload_json, '$.snoozedUntil')) > strftime('%s', 'now')))";

fn home_dir() -> PathBuf {
    if let Ok(h) = std::env::var("T3CODE_HOME")
        && !h.is_empty()
    {
        return PathBuf::from(h);
    }
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/root".into())).join(".clankers")
}

fn thread_url(home: &Path, id: &str) -> String {
    let udata = home.join("userdata");
    let origin = std::fs::read_to_string(udata.join("server-runtime.json"))
        .ok()
        .and_then(|s| {
            serde_json::from_str::<serde_json::Value>(&s)
                .ok()?
                .get("origin")?
                .as_str()
                .map(str::to_string)
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:3773".to_string());
    let env = std::fs::read_to_string(udata.join("environment-id"))
        .map(|s| s.trim().to_string())
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "local".to_string());
    format!("{}/{}/{id}", origin.trim_end_matches('/'), env)
}

pub fn sessions() -> Vec<Opencode> {
    sessions_from_home(&home_dir())
}

fn sessions_from_home(home: &Path) -> Vec<Opencode> {
    let db = home.join("userdata").join("statev2.sqlite");
    let Ok(conn) = Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_ONLY) else {
        return Vec::new();
    };
    let mut stmt = match conn.prepare(QUERY) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = match stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
        ))
    }) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for row in rows.flatten() {
        let (id, title, workspace_root, worktree_path, status, updated_secs) = row;
        if id.is_empty() {
            continue;
        }
        // Worktree threads live in the worktree, not the project root.
        let dir = worktree_path
            .filter(|p| !p.trim().is_empty())
            .or(workspace_root)
            .filter(|p| !p.trim().is_empty());
        let Some(dir) = dir else { continue };
        out.push(Opencode {
            id: id.clone(),
            title: title
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| "New session".to_string()),
            directory: PathBuf::from(dir),
            time_updated: updated_secs.saturating_mul(1000),
            time_viewed: 0,
            is_running: is_live(&status),
            url: Some(thread_url(home, &id)),
        });
    }
    out
}
