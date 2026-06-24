use crate::model::Opencode;
use rusqlite::Connection;
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

pub fn list_sessions_cached(conn: &mut Option<Connection>) -> Vec<Opencode> {
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
        Some(c) => query_sessions(c),
        None => vec![],
    }
}

fn query_sessions(conn: &Connection) -> Vec<Opencode> {
    let _ = conn.pragma_update(None, "query_only", 1);

    let mut stmt = match conn.prepare(
        "SELECT s.id, s.title, s.directory, s.time_updated,
                (SELECT p.data FROM part p
                 WHERE p.session_id = s.id
                 ORDER BY p.time_created DESC, p.id DESC LIMIT 1)
         FROM session s WHERE s.time_archived IS NULL
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
            r.get::<_, Option<String>>(4)?,
        ))
    });

    rows.map(|rs| {
        rs.filter_map(|r| r.ok())
            .map(|(id, title, dir, updated, last_part)| Opencode {
                is_running: last_part.is_some_and(|d| !is_idle(&d)),
                id,
                title,
                directory: PathBuf::from(dir),
                time_updated: updated,
            })
            .collect()
    })
    .unwrap_or_default()
}

#[derive(serde::Deserialize)]
struct PartData {
    #[serde(rename = "type")]
    typ: Option<String>,
    reason: Option<String>,
}

fn is_idle(part_data: &str) -> bool {
    serde_json::from_str::<PartData>(part_data)
        .ok()
        .is_some_and(|d| {
            d.typ.as_deref() == Some("step-finish") && d.reason.as_deref() == Some("stop")
        })
}
