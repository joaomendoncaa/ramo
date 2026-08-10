use crate::config::Config;
use crate::git::GitCache;
use crate::metrics::BuildTimings;
use crate::model::{
    Changes, Entry, EntryType, Goto, Opencode, TmuxPane, TmuxSession, WorktreeInfo,
};
use crate::opencode;
use crate::tmux;
use crate::util;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

// How many dirs (beyond the always-open ones) get their git state refreshed
// per build. Keeps per-cycle cost bounded no matter how many projects exist;
// a full pass over N dirs takes ~N/STAGGER_BUDGET cycles.
const STAGGER_BUDGET: usize = 8;

pub struct TreeBuilder {
    git_cache: GitCache,
    oc_db: Mutex<Option<Connection>>,
    oc_tracker: Mutex<opencode::OcTracker>,
    rotation: Mutex<VecDeque<PathBuf>>,
}

impl TreeBuilder {
    pub fn new() -> Self {
        TreeBuilder {
            git_cache: GitCache::new(),
            oc_db: Mutex::new(None),
            oc_tracker: Mutex::new(opencode::OcTracker::default()),
            rotation: Mutex::new(VecDeque::new()),
        }
    }

    pub fn git_cache(&self) -> &GitCache {
        &self.git_cache
    }

    pub fn load_disk_cache(&self, cache: &crate::git::DiskCache) {
        self.git_cache.load_disk(cache);
    }

    pub fn to_disk_cache(&self) -> crate::git::DiskCache {
        self.git_cache.to_disk()
    }

    pub fn build(&self, config: &Config) -> (Vec<Entry>, BuildTimings) {
        let tmux_start = Instant::now();
        let snap = tmux::snapshot();
        let sessions = snap.sessions;
        let panes = snap.panes;
        let tmux_ms = tmux_start.elapsed().as_millis();

        let oc_start = Instant::now();
        let oc_sessions = opencode::list_sessions_cached(
            &mut self.oc_db.lock().unwrap(),
            &mut self.oc_tracker.lock().unwrap(),
        );
        let oc_ms = oc_start.elapsed().as_millis();
        let oc_panes = tmux::opencode_panes(&panes);
        let pane_sessions = match_panes_to_sessions(&oc_panes, &oc_sessions);

        let dirs = self.parse_directories(&config.path);
        let open: HashSet<PathBuf> = dirs
            .iter()
            .filter(|d| dir_is_open(d, &sessions, &panes))
            .map(|d| d.path.clone())
            .collect();

        let git_start = Instant::now();
        let git_data = self.git_phase(&dirs, &open, config);
        let git_ms = git_start.elapsed().as_millis();
        let dirs_refreshed = git_data.iter().filter(|g| g.fresh).count();

        let covered_paths: Vec<PathBuf> = dirs.iter().map(|d| d.path.clone()).collect();
        let covered_names: Vec<String> = dirs.iter().map(|d| d.name.clone()).collect();

        let mut dir_entries: Vec<DirEntry> = Vec::with_capacity(dirs.len());
        for (d, git) in dirs.iter().zip(git_data.iter()) {
            dir_entries.push(build_dir_entry(
                d,
                git,
                &sessions,
                &panes,
                &pane_sessions,
            ));
        }

        for s in tmux::list_external_sessions(&sessions, &covered_paths, &covered_names) {
            dir_entries.push(external_dir_entry(&s, &panes));
        }

        let mut open_entries: Vec<&DirEntry> = dir_entries.iter().filter(|e| e.is_open).collect();
        let mut closed: Vec<&DirEntry> = dir_entries.iter().filter(|e| !e.is_open).collect();
        open_entries.sort_by(|a, b| b.name.cmp(&a.name));
        closed.sort_by(|a, b| b.name.cmp(&a.name));

        let total = open_entries.len() + closed.len();
        let mut rows = Vec::new();
        let mut pos = 0;

        for entry in &closed {
            push_entry(entry, pos == total - 1, &mut rows);
            pos += 1;
        }
        for entry in &open_entries {
            push_entry(entry, pos == total - 1, &mut rows);
            pos += 1;
        }

        (
            rows,
            BuildTimings {
                tmux_ms,
                oc_ms,
                git_ms,
                serialize_ms: 0,
                dirs_total: dirs.len(),
                dirs_refreshed,
            },
        )
    }

    // Per-dir git data for one build. `fresh` dirs (open ones plus a rotating
    // slice) get recomputed; the rest reuse cached values so per-build cost
    // stays bounded.
    fn git_phase(
        &self,
        dirs: &[DirInfo],
        open: &HashSet<PathBuf>,
        config: &Config,
    ) -> Vec<DirGit> {
        let refresh = self.next_refresh_set(dirs, open);

        // Phase 1: refresh worktree lists for the refresh set, in parallel.
        let refresh_paths: Vec<PathBuf> = refresh.iter().cloned().collect();
        let worker_count = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .min(8);
        if !refresh_paths.is_empty() {
            std::thread::scope(|s| {
                for chunk in chunk_ranges(refresh_paths.len(), worker_count) {
                    let paths = &refresh_paths;
                    let cache = &self.git_cache;
                    s.spawn(move || {
                        for p in &paths[chunk] {
                            let _ = cache.refresh_worktrees(p);
                        }
                    });
                }
            });
        }

        // Phase 2: gather worktree paths (fresh lists for the refresh set,
        // stale otherwise, fresh listing as a cold-cache fallback).
        let mut worktree_paths: Vec<PathBuf> = Vec::new();
        let mut all_worktrees: Vec<Vec<WorktreeInfo>> = Vec::with_capacity(dirs.len());
        for dir in dirs {
            let listed = if refresh.contains(&dir.path) {
                self.git_cache.worktrees(&dir.path)
            } else {
                self.git_cache
                    .worktrees_stale(&dir.path)
                    .unwrap_or_else(|| self.git_cache.worktrees(&dir.path))
            };
            for wt in &listed {
                if !wt.is_main {
                    worktree_paths.push(wt.path.clone());
                }
            }
            all_worktrees.push(listed);
        }

        // Phase 3: diff the refresh set in parallel — worktrees always
        // (needed to decide which ones surface), main dirs when shown or
        // open. Non-refresh dirs keep using their cached values.
        let skip_main: HashSet<&Path> = dirs
            .iter()
            .filter(|d| !open.contains(&d.path) && config.hide_changes_inactive)
            .map(|d| d.path.as_path())
            .collect();
        let mut diff_targets: Vec<PathBuf> = refresh
            .iter()
            .filter(|p| !skip_main.contains(p.as_path()))
            .cloned()
            .collect();
        diff_targets.extend(worktree_paths.iter().cloned());
        if !diff_targets.is_empty() {
            std::thread::scope(|s| {
                for chunk in chunk_ranges(diff_targets.len(), worker_count) {
                    let targets = &diff_targets;
                    let cache = &self.git_cache;
                    s.spawn(move || {
                        for p in &targets[chunk] {
                            let _ = cache.refresh_diff(p);
                        }
                    });
                }
            });
        }

        // Assemble per-dir results.
        let mut out = Vec::with_capacity(dirs.len());
        for (i, dir) in dirs.iter().enumerate() {
            let fresh = refresh.contains(&dir.path);
            let worktree_diffs: HashMap<PathBuf, Changes> = all_worktrees[i]
                .iter()
                .filter(|wt| !wt.is_main)
                .map(|wt| {
                    (
                        wt.path.clone(),
                        self.git_cache.diff_stale(&wt.path).unwrap_or_default(),
                    )
                })
                .collect();
            let main_diff = if skip_main.contains(dir.path.as_path()) {
                Changes::default()
            } else {
                self.git_cache
                    .diff_stale(&dir.path)
                    .unwrap_or_else(|| self.git_cache.diff(&dir.path))
            };
            out.push(DirGit {
                worktrees: all_worktrees[i].clone(),
                worktree_diffs,
                main_diff,
                fresh,
            });
        }
        out
    }

    fn next_refresh_set(&self, dirs: &[DirInfo], open: &HashSet<PathBuf>) -> HashSet<PathBuf> {
        let mut set: HashSet<PathBuf> = open.clone();
        let mut rotation = self.rotation.lock().unwrap();
        rotation.retain(|p| dirs.iter().any(|d| d.path == *p));
        for d in dirs {
            if !rotation.contains(&d.path) {
                rotation.push_back(d.path.clone());
            }
        }
        for _ in 0..STAGGER_BUDGET {
            let Some(p) = rotation.pop_front() else { break };
            set.insert(p.clone());
            rotation.push_back(p);
        }
        set
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
}

struct DirGit {
    worktrees: Vec<WorktreeInfo>,
    worktree_diffs: HashMap<PathBuf, Changes>,
    main_diff: Changes,
    fresh: bool,
}

fn build_dir_entry(
    dir: &DirInfo,
    git: &DirGit,
    sessions: &[TmuxSession],
    panes: &[TmuxPane],
    pane_sessions: &[PaneSession],
) -> DirEntry {
    let worktrees: Vec<WorktreeInfo> = git
        .worktrees
        .iter()
        .filter(|wt| !wt.is_main)
        .filter(|wt| {
            let diff = git
                .worktree_diffs
                .get(&wt.path)
                .cloned()
                .unwrap_or_default();
            sessions.iter().any(|s| s.path == wt.path)
                || pane_sessions
                    .iter()
                    .any(|ps| is_in(&ps.session.directory, &wt.path))
                || !diff.has_none()
        })
        .cloned()
        .collect();
    let worktree_diffs: Vec<Changes> = worktrees
        .iter()
        .map(|wt| {
            git.worktree_diffs
                .get(&wt.path)
                .cloned()
                .unwrap_or_default()
        })
        .collect();
    let changes = worktree_diffs
        .iter()
        .fold(git.main_diff.clone(), |acc, d| acc.add(d));
    let sessions_here = dir_sessions(dir, &worktrees, pane_sessions);
    let worktree_sessions = worktree_sessions(&worktrees, pane_sessions);

    DirEntry {
        name: dir.name.clone(),
        path: dir.path.clone(),
        is_open: dir_is_open(dir, sessions, panes),
        changes,
        worktrees,
        worktree_diffs,
        sessions: sessions_here,
        worktree_sessions,
    }
}

fn external_dir_entry(s: &TmuxSession, _panes: &[TmuxPane]) -> DirEntry {
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

fn dir_is_open(dir: &DirInfo, sessions: &[TmuxSession], panes: &[TmuxPane]) -> bool {
    sessions
        .iter()
        .any(|s| s.name == dir.name || s.path == dir.path)
        || panes.iter().any(|p| is_in(&p.current_path, &dir.path))
}

// Opencode sessions whose directory is the dir itself *or* is under it
// but not under any worktree (those go into `worktree_sessions`).
fn dir_sessions(
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

fn push_entry(entry: &DirEntry, is_last_dir: bool, rows: &mut Vec<Entry>) {
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

fn chunk_ranges(len: usize, workers: usize) -> Vec<std::ops::Range<usize>> {
    let workers = workers.max(1);
    let chunk = len.div_ceil(workers);
    (0..workers)
        .map(|i| (i * chunk).min(len)..((i + 1) * chunk).min(len))
        .filter(|r| !r.is_empty())
        .collect()
}
