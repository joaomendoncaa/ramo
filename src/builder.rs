use crate::git::{GitCache, list_worktrees};
use crate::model::{
    Changes, Entry, EntryType, Goto, Opencode, TmuxPane, TmuxSession, WorktreeInfo,
};
use crate::opencode;
use crate::tmux;
use crate::util;
use rusqlite::Connection;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

const WORKTREE_CACHE_TTL_MS: u128 = 30_000;

pub struct TreeBuilder {
    git_cache: GitCache,
    oc_db: Mutex<Option<Connection>>,
    worktree_cache: Mutex<HashMap<PathBuf, (Vec<WorktreeInfo>, Instant)>>,
}

impl TreeBuilder {
    pub fn new() -> Self {
        TreeBuilder {
            git_cache: GitCache::new(),
            oc_db: Mutex::new(None),
            worktree_cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn build(&self, path_config: &str) -> Vec<Entry> {
        let sessions = tmux::list_sessions();
        let panes = tmux::list_panes();
        let oc_panes = tmux::opencode_panes(&panes);
        let oc_sessions = opencode::list_sessions_cached(&mut self.oc_db.lock().unwrap());
        let pane_sessions = match_panes_to_sessions(&oc_panes, &oc_sessions);

        let dirs = self.parse_directories(path_config);
        let covered_paths: Vec<PathBuf> = dirs.iter().map(|d| d.path.clone()).collect();
        let covered_names: Vec<String> = dirs.iter().map(|d| d.name.clone()).collect();

        let mut dir_entries: Vec<DirEntry> = Vec::with_capacity(dirs.len());
        for d in &dirs {
            dir_entries.push(self.build_dir_entry(d, &sessions, &panes, &pane_sessions));
        }

        for s in tmux::list_external_sessions(&sessions, &covered_paths, &covered_names) {
            dir_entries.push(self.external_dir_entry(&s, &panes));
        }

        let mut open: Vec<&DirEntry> = dir_entries.iter().filter(|e| e.is_open).collect();
        let mut closed: Vec<&DirEntry> = dir_entries.iter().filter(|e| !e.is_open).collect();
        open.sort_by(|a, b| b.name.cmp(&a.name));
        closed.sort_by(|a, b| b.name.cmp(&a.name));

        let total = open.len() + closed.len();
        let mut rows = Vec::new();
        let mut pos = 0;

        for entry in &closed {
            self.push_entry(entry, pos == total - 1, &mut rows);
            pos += 1;
        }
        for entry in &open {
            self.push_entry(entry, pos == total - 1, &mut rows);
            pos += 1;
        }

        rows
    }

    fn parse_directories(&self, path_config: &str) -> Vec<DirInfo> {
        let mut dirs = Vec::new();
        for raw in path_config.split(':') {
            if raw.is_empty() {
                continue;
            }
            let is_glob = raw.ends_with("/*");
            let base = if is_glob {
                raw.trim_end_matches("/*")
            } else {
                raw
            };
            let expanded = util::expand_tilde(base);
            if !expanded.is_dir() {
                continue;
            }

            if is_glob {
                if let Ok(entries) = std::fs::read_dir(&expanded) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            dirs.push(DirInfo {
                                name: path.file_name().unwrap().to_string_lossy().into(),
                                path,
                            });
                        }
                    }
                }
            } else {
                dirs.push(DirInfo {
                    name: expanded.file_name().unwrap().to_string_lossy().into(),
                    path: expanded,
                });
            }
        }
        dirs.sort_by(|a, b| a.name.cmp(&b.name));
        dirs
    }

    fn build_dir_entry(
        &self,
        dir: &DirInfo,
        sessions: &[TmuxSession],
        panes: &[TmuxPane],
        pane_sessions: &[PaneSession],
    ) -> DirEntry {
        let worktrees = self.dir_worktrees(dir, sessions, pane_sessions);
        let worktree_diffs: Vec<Changes> = worktrees
            .iter()
            .map(|wt| self.git_cache.diff(&wt.path))
            .collect();
        let main_diff = self.git_cache.diff(&dir.path);
        let changes = worktree_diffs.iter().fold(main_diff, |acc, d| acc.add(d));
        let sessions_here = self.dir_sessions(dir, &worktrees, pane_sessions);
        let worktree_sessions = self.worktree_sessions(&worktrees, pane_sessions);

        DirEntry {
            name: dir.name.clone(),
            path: dir.path.clone(),
            is_open: self.dir_is_open(dir, sessions, panes),
            changes,
            worktrees,
            worktree_diffs,
            sessions: sessions_here,
            worktree_sessions,
        }
    }

    fn external_dir_entry(&self, s: &TmuxSession, _panes: &[TmuxPane]) -> DirEntry {
        DirEntry {
            name: s.name.clone(),
            path: s.path.clone(),
            is_open: true,
            changes: Changes::default(),
            worktrees: vec![],
            worktree_diffs: vec![],
            sessions: vec![],
            worktree_sessions: vec![],
        }
    }

    fn dir_is_open(&self, dir: &DirInfo, sessions: &[TmuxSession], panes: &[TmuxPane]) -> bool {
        sessions
            .iter()
            .any(|s| s.name == dir.name || s.path == dir.path)
            || panes.iter().any(|p| is_in(&p.current_path, &dir.path))
    }

    // Worktrees (non-main) worth surfacing: have a session, an opencode
    // agent inside, or uncommitted diff.
    fn dir_worktrees(
        &self,
        dir: &DirInfo,
        sessions: &[TmuxSession],
        pane_sessions: &[PaneSession],
    ) -> Vec<WorktreeInfo> {
        let worktrees = {
            let mut cache = self.worktree_cache.lock().unwrap();
            let hit = cache
                .get(&dir.path)
                .filter(|(_, t)| t.elapsed().as_millis() < WORKTREE_CACHE_TTL_MS)
                .map(|(w, _)| w.clone());
            match hit {
                Some(w) => w,
                None => {
                    let w = list_worktrees(&dir.path);
                    cache.insert(dir.path.clone(), (w.clone(), Instant::now()));
                    w
                }
            }
        };
        worktrees
            .into_iter()
            .filter(|wt| !wt.is_main)
            .filter(|wt| {
                let diff = self.git_cache.diff(&wt.path);
                sessions.iter().any(|s| s.path == wt.path)
                    || pane_sessions
                        .iter()
                        .any(|ps| is_in(&ps.session.directory, &wt.path))
                    || !diff.has_none()
            })
            .collect()
    }

    // Opencode sessions whose directory is the dir itself *or* is under it
    // but not under any worktree (those go into `worktree_sessions`).
    fn dir_sessions(
        &self,
        dir: &DirInfo,
        worktrees: &[WorktreeInfo],
        pane_sessions: &[PaneSession],
    ) -> Vec<PaneSession> {
        pane_sessions
            .iter()
            .filter(|ps| {
                ps.session.directory == dir.path
                    || (is_in(&ps.session.directory, &dir.path)
                        && !worktrees
                            .iter()
                            .any(|wt| is_in(&ps.session.directory, &wt.path)))
            })
            .cloned()
            .collect()
    }

    fn worktree_sessions(
        &self,
        worktrees: &[WorktreeInfo],
        pane_sessions: &[PaneSession],
    ) -> Vec<Vec<PaneSession>> {
        worktrees
            .iter()
            .map(|wt| {
                pane_sessions
                    .iter()
                    .filter(|ps| is_in(&ps.session.directory, &wt.path))
                    .cloned()
                    .collect()
            })
            .collect()
    }

    fn push_entry(&self, entry: &DirEntry, is_last_dir: bool, rows: &mut Vec<Entry>) {
        let dir_idx = rows.len();

        rows.push(finalize_entry(Entry {
            kind: EntryType::Dir,
            label: entry.name.clone(),
            path: entry.path.clone(),
            changes: if entry.changes.has_none() {
                None
            } else {
                Some(entry.changes.clone())
            },
            is_open: entry.is_open,
            is_running: false,
            depth: 0,
            ancestors: vec![],
            is_last: is_last_dir,
            search_text: entry.name.clone(),
            goto: Some(Goto {
                session: entry.name.clone(),
                path: entry.path.clone(),
                window: None,
                pane: None,
            }),
            parent: None,
            connector: String::new(),
            search_text_lower: String::new(),
        }));

        let total_children = entry.sessions.len() + entry.worktrees.len();
        if total_children == 0 {
            return;
        }
        let mut child = 0;

        for ps in &entry.sessions {
            let is_last = child == total_children - 1;
            child += 1;
            rows.push(finalize_entry(Entry {
                kind: EntryType::Agent,
                label: ps.session.title.clone(),
                path: ps.session.directory.clone(),
                changes: None,
                is_open: false,
                is_running: ps.session.is_running,
                depth: 1,
                ancestors: vec![],
                is_last,
                search_text: format!("{} {}", entry.name, ps.session.title),
                goto: Some(Goto {
                    session: ps.pane.session_name.clone(),
                    path: ps.session.directory.clone(),
                    window: Some(ps.pane.window_index),
                    pane: Some(ps.pane.pane_index),
                }),
                parent: Some(dir_idx),
                connector: String::new(),
                search_text_lower: String::new(),
            }));
        }

        for (wi, wt) in entry.worktrees.iter().enumerate() {
            let is_last = child == total_children - 1;
            child += 1;
            let wt_idx = rows.len();
            let wt_name = wt
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let wt_diff = &entry.worktree_diffs[wi];

            rows.push(finalize_entry(Entry {
                kind: EntryType::Worktree,
                label: wt_name.clone(),
                path: wt.path.clone(),
                changes: if wt_diff.has_none() {
                    None
                } else {
                    Some(wt_diff.clone())
                },
                is_open: false,
                is_running: false,
                depth: 1,
                ancestors: vec![],
                is_last,
                search_text: format!("{} {}", entry.name, wt_name),
                goto: Some(Goto {
                    session: wt_name.clone(),
                    path: wt.path.clone(),
                    window: None,
                    pane: None,
                }),
                parent: Some(dir_idx),
                connector: String::new(),
                search_text_lower: String::new(),
            }));

            let wt_sessions: &_ = &entry.worktree_sessions[wi];
            let s_total = wt_sessions.len();
            for (si, ps) in wt_sessions.iter().enumerate() {
                rows.push(finalize_entry(Entry {
                    kind: EntryType::Agent,
                    label: ps.session.title.clone(),
                    path: ps.session.directory.clone(),
                    changes: None,
                    is_open: false,
                    is_running: ps.session.is_running,
                    depth: 2,
                    ancestors: vec![is_last_dir],
                    is_last: si == s_total - 1,
                    search_text: format!("{} {} {}", entry.name, wt_name, ps.session.title),
                    goto: Some(Goto {
                        session: ps.pane.session_name.clone(),
                        path: ps.session.directory.clone(),
                        window: Some(ps.pane.window_index),
                        pane: Some(ps.pane.pane_index),
                    }),
                    parent: Some(wt_idx),
                    connector: String::new(),
                    search_text_lower: String::new(),
                }));
            }
        }
    }
}

struct DirInfo {
    name: String,
    path: PathBuf,
}

struct DirEntry {
    name: String,
    path: PathBuf,
    is_open: bool,
    changes: Changes,
    worktrees: Vec<WorktreeInfo>,
    worktree_diffs: Vec<Changes>,
    sessions: Vec<PaneSession>,
    worktree_sessions: Vec<Vec<PaneSession>>,
}

#[derive(Clone)]
struct PaneSession {
    pane: TmuxPane,
    session: Opencode,
}

fn match_panes_to_sessions(panes: &[&TmuxPane], sessions: &[Opencode]) -> Vec<PaneSession> {
    let mut used: HashSet<String> = HashSet::new();
    let mut sorted: Vec<&TmuxPane> = panes.to_vec();
    sorted.sort_by_key(|b| std::cmp::Reverse(b.activity));

    sorted
        .into_iter()
        .filter_map(|pane| {
            let best = sessions
                .iter()
                .filter(|s| !used.contains(&s.id) && is_in(&pane.current_path, &s.directory))
                .max_by_key(|s| s.time_updated)?;
            used.insert(best.id.clone());
            Some(PaneSession {
                pane: pane.clone(),
                session: best.clone(),
            })
        })
        .collect()
}

fn is_in(path: &Path, base: &Path) -> bool {
    path == base || path.starts_with(base)
}

fn finalize_entry(mut entry: Entry) -> Entry {
    entry.compute_connector();
    entry.search_text_lower = entry.search_text.to_lowercase();
    entry
}
