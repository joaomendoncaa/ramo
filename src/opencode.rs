use crate::model::Opencode;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::PathBuf;

fn db_path() -> PathBuf {
    std::env::var("XDG_DATA_HOME")
        .map(|x| PathBuf::from(x).join("opencode/opencode.db"))
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(|h| PathBuf::from(h).join(".local/share/opencode/opencode.db"))
                .unwrap_or_default()
        })
}

// A session can only be "running" if its most recent part was written within
// this window (agents write a part on every streaming chunk). Restricting the
// initial part scan to the window turns a 150k-row correlated-subquery crawl
// into a scan of the recent rows only.
const ACTIVE_WINDOW_MS: i64 = 10 * 60 * 1000;

// Parts are immutable and inserted in monotonic order, so running/idle
// decisions only need recomputing for parts written since the last scan.
// The initial scan covers a recency window (by time_created); every later
// scan reads only new rows (by rowid), which is index-backed and ~free.
#[derive(Default)]
pub struct OcTracker {
    last_rowid: i64,
    latest: HashMap<String, (i64, bool)>,
}

pub fn list_sessions_cached(
    conn: &mut Option<Connection>,
    tracker: &mut OcTracker,
) -> Vec<Opencode> {
    let path = db_path();
    if !path.exists() {
        *conn = None;
        return vec![];
    }

    if conn.is_none() {
        *conn = Connection::open_with_flags(
            &path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .ok();
    }

    match conn {
        Some(c) => query_sessions(c, tracker),
        None => vec![],
    }
}

fn query_sessions(conn: &Connection, tracker: &mut OcTracker) -> Vec<Opencode> {
    let _ = conn.pragma_update(None, "query_only", 1);

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    if tracker.last_rowid == 0 {
        // First scan: parts within the recency window, newest first. The
        // first row seen per session is its most recent part.
        let cutoff = now_ms - ACTIVE_WINDOW_MS;
        if let Ok(mut stmt) = conn.prepare(
            "SELECT session_id, time_created, data FROM part
             WHERE time_created >= ? ORDER BY time_created DESC",
        )
        && let Ok(rows) = stmt.query_map([cutoff], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        {
            for row in rows.flatten() {
                let (session_id, time_created, data) = row;
                record_part(tracker, session_id, time_created, &data);
            }
        }
        // Parts written during the window scan are picked up by the next
        // incremental scan, so anchoring on max(rowid) is safe.
        tracker.last_rowid = conn
            .query_row("SELECT max(rowid) FROM part", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0);
    } else {
        // Incremental: index-backed scan of only the rows written since last
        // time. ORDER BY rowid DESC means first sight per session is its
        // newest part of the batch.
        if let Ok(mut stmt) = conn.prepare(
            "SELECT rowid, session_id, time_created, data FROM part
             WHERE rowid > ? ORDER BY rowid DESC",
        )
        && let Ok(rows) = stmt.query_map([tracker.last_rowid], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        {
            for row in rows.flatten() {
                let (rowid, session_id, time_created, data) = row;
                tracker.last_rowid = tracker.last_rowid.max(rowid);
                record_part(tracker, session_id, time_created, &data);
            }
        }
    }

    let mut stmt = match conn.prepare(
        "SELECT s.id, s.title, s.directory, s.time_updated
         FROM session s WHERE s.time_archived IS NULL
         AND (s.agent IS NULL OR s.agent != 'Git Commit')
         ORDER BY s.time_updated DESC",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
        ))
    });

    rows.map(|rs| {
        rs.filter_map(|r| r.ok())
            .map(|(id, title, dir, updated)| Opencode {
                is_running: tracker
                    .latest
                    .get(&id)
                    .is_some_and(|(_, idle)| !idle),
                id,
                title,
                directory: PathBuf::from(dir),
                time_updated: updated,
            })
            .collect()
    })
    .unwrap_or_default()
}

fn record_part(tracker: &mut OcTracker, session_id: String, time_created: i64, data: &str) {
    match tracker.latest.get(&session_id) {
        Some((t, _)) if *t >= time_created => {}
        _ => {
            tracker
                .latest
                .insert(session_id, (time_created, is_idle(data)));
        }
    }
}

// Cheap idle detection: the fields we care about are short, fixed strings
// ("type":"step-finish", "reason":"stop") that serde/JSON.stringify always
// serialize adjacently. A substring scan is ~50x faster than deserializing
// every part's full data blob (which can be megabytes).
fn is_idle(part_data: &str) -> bool {
    part_data.contains("\"type\":\"step-finish\"") && part_data.contains("\"reason\":\"stop\"")
}
