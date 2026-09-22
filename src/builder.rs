use crate::config::Config;
use crate::git::GitCache;
use crate::integration::opencode;
use crate::model::{
    Changes, Entry, EntryType, Goto, Opencode, TmuxPane, TmuxSession, WorktreeInfo,
};
use crate::report;
use crate::integration::tmux;
use crate::util;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct TreeBuilder {
    git_cache: GitCache,
    reports: report::ReportMap,
    // Session ids ever shown live since tracking began. Only these can
    // go dormant — the API's full history stays out of the list until
    // ramo actually sees a session open. Survives restarts via cache.json.
    seen: Mutex<HashSet<String>>,
    // Session ids the user hid (`ramo archive` / ctrl-a). Dormant rows
    // for these stay out; live panes always show. Loaded from disk so
    // daemon restarts keep respecting hides.
    archived: Mutex<HashSet<String>>,
}

impl TreeBuilder {
    pub fn new(reports: report::ReportMap) -> Self {
        TreeBuilder {
            git_cache: GitCache::new(),
            reports,
            seen: Mutex::new(HashSet::new()),
            archived: Mutex::new(
                load_archived_from(&archived_path()).into_keys().collect(),
            ),
        }
    }

    pub fn load_seen(&self, ids: &[String]) {
        self.seen.lock().unwrap().extend(ids.iter().cloned());
    }

    pub fn seen_snapshot(&self) -> Vec<String> {
        self.seen.lock().unwrap().iter().cloned().collect()
    }

    pub fn reports(&self) -> report::ReportMap {
        self.reports.clone()
    }

    /// Hide a session id from dormant rows (test seam; the picker and
    /// CLI persist through [`save_archived_to`] directly, the daemon
    /// picks file state up at construction). Memory-only by design.
    pub fn archive(&self, id: &str) {
        self.archived.lock().unwrap().insert(id.to_string());
    }

    pub fn load_disk_cache(&self, cache: &crate::git::DiskCache) {
        self.git_cache.load_disk(cache);
    }
    pub fn to_disk_cache(&self) -> crate::git::DiskCache {
        self.git_cache.to_disk()
    }

    pub fn build(&self, config: &Config) -> Option<Vec<Entry>> {
        let (tmux_snapshot, oc_sessions, current) = std::thread::scope(|s| {
            let tmux_h = s.spawn(tmux::snapshot);
            let oc_h = s.spawn(|| {
                opencode::sessions(&config.opencode_agents_ignored)
            });
            let cur_h = s.spawn(tmux::current_session_name);
            (tmux_h.join().unwrap(), oc_h.join().unwrap(), cur_h.join().unwrap())
        });
        // A transient `opencode api` failure must not wipe the list for a
        // frame (every pane would flash synthetic, then snap back). The
        // daemon keeps serving the previous payload instead.
        let oc_sessions = oc_sessions?;
        Some(self.build_with(
            config,
            &tmux_snapshot.sessions,
            &tmux_snapshot.panes,
            &oc_sessions,
            current.as_deref(),
        ))
    }

    /// Pure assembly from already-fetched inputs. Headless tests drive
    /// the whole agent pipeline through here with fake tmux panes,
    /// fake API sessions and real reports — no tmux/opencode binaries.
    /// `current_session` is the tmux session the picker was opened from
    /// (used only to pick the jump target when several panes show one
    /// agent session); tests pass `None` unless pinning that rule.
    pub fn build_with(
        &self,
        config: &Config,
        sessions: &[TmuxSession],
        panes: &[TmuxPane],
        oc_sessions: &[Opencode],
        current_session: Option<&str>,
    ) -> Vec<Entry> {

        let oc_panes = tmux::opencode_panes(panes);
        // Drop reports for dead panes so a reused pane id can't ghost.
        report::prune(
            &self.reports,
            &panes.iter().map(|p| p.pane_id.clone()).collect(),
        );
        let (pane_sessions, bound) =
            match_panes_to_sessions(&oc_panes, oc_sessions, &self.reports, current_session);
        // Sessions no live pane binds (exited to shell, pane closed,
        // `/new` moved the pane on) stay listed as dormant rows under
        // their directory — but only sessions ramo has actually seen
        // open. Everything older than tracking stays out until opened.
        self.seen.lock().unwrap().extend(bound.iter().cloned());
        let seen = self.seen.lock().unwrap();
        let archived = self.archived.lock().unwrap();
        let mut dormant: Vec<Opencode> = oc_sessions
            .iter()
            .filter(|s| {
                !bound.contains(&s.id) && seen.contains(&s.id) && !archived.contains(&s.id)
            })
            .cloned()
            .collect();
        drop(seen);
        drop(archived);
        // Stable by id: sorting by recency reordered rows under the
        // cursor whenever any session updated mid-picker.
        dormant.sort_by(|a, b| a.id.cmp(&b.id));

        let dirs = self.parse_directories(&config.path);
        let open: HashSet<PathBuf> = dirs
            .iter()
            .filter(|d| dir_is_open(d, sessions, panes))
            .map(|d| d.path.clone())
            .collect();

        let git_data = self.git_phase(&dirs, &open, config);

        // A scanned dir that is itself a linked worktree of another scanned
        // repo is no top-level dir: it already shows as that repo's child
        // (otherwise the glob re-adds every new sibling worktree on top,
        // each even listing itself as its own child).
        let keep = keep_mask(&dirs, &git_data);
        let mut kept_dirs = Vec::with_capacity(dirs.len());
        let mut kept_git = Vec::with_capacity(git_data.len());
        for (k, (d, g)) in keep.into_iter().zip(dirs.into_iter().zip(git_data)) {
            if k {
                kept_dirs.push(d);
                kept_git.push(g);
            }
        }
        let (dirs, git_data) = (kept_dirs, kept_git);

        let mut covered_paths: Vec<PathBuf> = dirs.iter().map(|d| d.path.clone()).collect();
        // Sessions living in a worktree are represented as its child, never as external.
        covered_paths.extend(
            git_data
                .iter()
                .flat_map(|g| g.worktrees.iter().map(|w| w.path.clone())),
        );
        let covered_names: Vec<String> = dirs.iter().map(|d| d.name.clone()).collect();

        let branches: Vec<Option<String>> = {
            let n = dirs.len();
            if n == 0 {
                Vec::new()
            } else if n == 1 {
                vec![
                    self.git_cache
                        .branch(&dirs[0].path)
                        .filter(|b| b != "master" && b != "main")
                        .filter(|_| {
                            let active = open.contains(&dirs[0].path);
                            !(active && config.hide_hints_branches_active
                                || !active && config.hide_hints_branches_inactive)
                        }),
                ]
            } else {
                let threads = std::thread::available_parallelism()
                    .map(|p| p.get())
                    .unwrap_or(4)
                    .min(n)
                    .min(8);
                let chunk = n.div_ceil(threads);
                let mut out: Vec<Option<Option<String>>> = (0..n).map(|_| None).collect();
                std::thread::scope(|s| {
                    for (chunk_idx, out_chunk) in out.chunks_mut(chunk).enumerate() {
                        let start = chunk_idx * chunk;
                        let dirs = &dirs;
                        let open = &open;
                        let config = config;
                        let cache = &self.git_cache;
                        s.spawn(move || {
                            for (i, slot) in out_chunk.iter_mut().enumerate() {
                                let idx = start + i;
                                if idx >= dirs.len() {
                                    break;
                                }
                                let b = cache
                                    .branch(&dirs[idx].path)
                                    .filter(|v| v != "master" && v != "main")
                                    .filter(|_| {
                                        let active = open.contains(&dirs[idx].path);
                                        !(active && config.hide_hints_branches_active
                                            || !active && config.hide_hints_branches_inactive)
                                    });
                                *slot = Some(b);
                            }
                        });
                    }
                });
                out.into_iter().map(|o| o.unwrap()).collect()
            }
        };

        let mut dir_entries: Vec<DirEntry> = Vec::with_capacity(dirs.len());
        for ((d, git), branch) in dirs.iter().zip(git_data.iter()).zip(branches) {
            // Closed dirs show no dormant agents: rows with no goto that
            // nobody can jump to are stale-feeling noise.
            let visible: &[Opencode] =
                if open.contains(&d.path) { &dormant } else { &[] };
            dir_entries.push(build_dir_entry(
                d,
                git,
                branch,
                sessions,
                panes,
                &pane_sessions,
                visible,
            ));
        }
        for s in tmux::list_external_sessions(sessions, &covered_paths, &covered_names) {
            dir_entries.push(external_dir_entry(&s, &panes));
        }

        let (mut open_roots, mut quiet_roots) = split_roots(&mut dir_entries);
        open_roots.sort_by(|a, b| b.label.cmp(&a.label));
        quiet_roots.sort_by(|a, b| b.label.cmp(&a.label));

        let mut open_entries: Vec<&DirEntry> = dir_entries.iter().filter(|e| e.is_open).collect();
        let mut closed: Vec<&DirEntry> = dir_entries.iter().filter(|e| !e.is_open).collect();
        open_entries.sort_by(|a, b| b.name.cmp(&a.name));
        closed.sort_by(|a, b| b.name.cmp(&a.name));

        let total = open_entries.len() + closed.len() + open_roots.len() + quiet_roots.len();
        let mut rows = Vec::new();
        let mut pos = 0;
        // Closed dirs and quiet worktree roots share the closed section, by name.
        let (mut i, mut j) = (0, 0);
        while i < closed.len() || j < quiet_roots.len() {
            let take_dir = match (closed.get(i), quiet_roots.get(j)) {
                (Some(d), Some(w)) => d.name >= w.label,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => break,
            };
            if take_dir {
                push_entry(closed[i], pos == total - 1, &mut rows);
                i += 1;
            } else {
                push_wt_root(&quiet_roots[j], pos == total - 1, &mut rows);
                j += 1;
            }
            pos += 1;
        }
        // Open dirs and hoisted worktree roots share the open section, by name.
        let (mut i, mut j) = (0, 0);
        while i < open_entries.len() || j < open_roots.len() {
            let take_dir = match (open_entries.get(i), open_roots.get(j)) {
                (Some(d), Some(w)) => d.name >= w.label,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => break,
            };
            if take_dir {
                push_entry(open_entries[i], pos == total - 1, &mut rows);
                i += 1;
            } else {
                push_wt_root(&open_roots[j], pos == total - 1, &mut rows);
                j += 1;
            }
            pos += 1;
        }

        rows
    }

    fn git_phase(&self, dirs: &[DirInfo], open: &HashSet<PathBuf>, config: &Config) -> Vec<DirGit> {
        // Non-main worktrees are first-class: always listed, like directories.
        let wts_of = |dir: &DirInfo| -> (
            Vec<WorktreeInfo>,
            HashMap<PathBuf, Changes>,
            Vec<Option<String>>,
        ) {
            let worktrees: Vec<WorktreeInfo> = self
                .git_cache
                .worktrees(&dir.path)
                .into_iter()
                .filter(|wt| !wt.is_main)
                .collect();
            let worktree_diffs: HashMap<PathBuf, Changes> = if config.hide_changes_worktree {
                HashMap::new()
            } else {
                worktrees
                    .iter()
                    .map(|wt| (wt.path.clone(), self.git_cache.diff(&wt.path)))
                    .collect()
            };
            let hide_branch = if open.contains(&dir.path) {
                config.hide_hints_branches_active
            } else {
                config.hide_hints_branches_inactive
            };
            let worktree_branches: Vec<Option<String>> = worktrees
                .iter()
                .map(|wt| {
                    self.git_cache
                        .branch(&wt.path)
                        .filter(|b| b != "master" && b != "main")
                        .filter(|_| !hide_branch)
                })
                .collect();
            (worktrees, worktree_diffs, worktree_branches)
        };
        let n = dirs.len();
        if n == 0 {
            return Vec::new();
        }
        if n == 1 {
            let dir = &dirs[0];
            let (worktrees, worktree_diffs, worktree_branches) = wts_of(dir);
            let main_diff = if !open.contains(&dir.path) && config.hide_changes_inactive {
                Changes::default()
            } else {
                self.git_cache.diff(&dir.path)
            };
            return vec![DirGit {
                worktrees,
                worktree_diffs,
                worktree_branches,
                main_diff,
            }];
        }
        let threads = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4)
            .min(n)
            .min(8);
        let chunk = n.div_ceil(threads);
        let mut out: Vec<Option<DirGit>> = (0..n).map(|_| None).collect();
        std::thread::scope(|s| {
            for (chunk_idx, out_chunk) in out.chunks_mut(chunk).enumerate() {
                let start = chunk_idx * chunk;
                let dirs = dirs;
                let open = open;
                let config = config;
                let cache = &self.git_cache;
                s.spawn(move || {
                    for (i, slot) in out_chunk.iter_mut().enumerate() {
                        let idx = start + i;
                        if idx >= dirs.len() {
                            break;
                        }
                        let dir = &dirs[idx];
                        let (worktrees, worktree_diffs, worktree_branches) = wts_of(dir);
                        let main_diff = if !open.contains(&dir.path) && config.hide_changes_inactive
                        {
                            Changes::default()
                        } else {
                            cache.diff(&dir.path)
                        };
                        *slot = Some(DirGit {
                            worktrees,
                            worktree_diffs,
                            worktree_branches,
                            main_diff,
                        });
                    }
                });
            }
        });
        out.into_iter().map(|o| o.unwrap()).collect()
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
    worktree_branches: Vec<Option<String>>,
    main_diff: Changes,
}

fn build_dir_entry(
    dir: &DirInfo,
    git: &DirGit,
    branch: Option<String>,
    sessions: &[TmuxSession],
    panes: &[TmuxPane],
    pane_sessions: &[PaneSession],
    dormant: &[Opencode],
) -> DirEntry {
    let mut worktrees: Vec<WtEntry> = git
        .worktrees
        .iter()
        .enumerate()
        .map(|(wi, wt)| {
            let wt_sessions: Vec<PaneSession> = pane_sessions
                .iter()
                .filter(|ps| is_in(&ps.session.directory, &wt.path))
                .cloned()
                .collect();
            WtEntry {
                is_open: sessions.iter().any(|s| s.path == wt.path) || !wt_sessions.is_empty(),
                diff: git
                    .worktree_diffs
                    .get(&wt.path)
                    .cloned()
                    .unwrap_or_default(),
                branch: git.worktree_branches.get(wi).cloned().flatten(),
                sessions: wt_sessions,
                dormant: dormant
                    .iter()
                    .filter(|s| is_in(&s.directory, &wt.path))
                    .cloned()
                    .collect(),
                info: wt.clone(),
            }
        })
        .collect();
    worktrees.sort_by_key(|w| wt_label(&w.info));
    let changes = worktrees
        .iter()
        .map(|w| &w.diff)
        .fold(git.main_diff.clone(), |acc, d| acc.add(d));
    DirEntry {
        name: dir.name.clone(),
        path: dir.path.clone(),
        is_open: dir_is_open(dir, sessions, panes),
        changes,
        branch,
        sessions: dir_sessions(dir, &git.worktrees, pane_sessions),
        dormant: dormant
            .iter()
            .filter(|s| {
                is_in(&s.directory, &dir.path)
                    && !git
                        .worktrees
                        .iter()
                        .any(|wt| is_in(&s.directory, &wt.path))
            })
            .cloned()
            .collect(),
        worktrees,
    }
}

fn external_dir_entry(s: &TmuxSession, _panes: &[TmuxPane]) -> DirEntry {
    DirEntry {
        name: s.name.clone(),
        path: s.path.clone(),
        is_open: true,
        changes: Changes::default(),
        branch: None,
        worktrees: vec![],
        sessions: vec![],
        dormant: vec![],
    }
}

fn dir_is_open(dir: &DirInfo, sessions: &[TmuxSession], panes: &[TmuxPane]) -> bool {
    sessions
        .iter()
        .any(|s| s.name == dir.name || s.path == dir.path)
        || panes.iter().any(|p| is_in(&p.current_path, &dir.path))
}

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

fn wt_label(wt: &WorktreeInfo) -> String {
    wt.path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn push_agent_row(
    rows: &mut Vec<Entry>,
    ps: &PaneSession,
    depth: usize,
    ancestors: Vec<bool>,
    is_last: bool,
    search_text: String,
    parent: Option<usize>,
) {
    // Synthetic rows (no report yet) carry no session id — keyed by pane.
    let session_id = (!ps.session.id.starts_with("synthetic:")).then(|| ps.session.id.clone());
    rows.push(finalize_entry(Entry {
        kind: EntryType::Agent,
        label: ps.session.title.clone(),
        path: ps.session.directory.clone(),
        changes: None,
        branch: None,
        is_open: false,
        is_running: ps.session.is_running,
        pending: false,
        depth,
        ancestors,
        is_last,
        search_text,
        goto: Some(Goto {
            session: ps.pane.session_name.clone(),
            path: ps.session.directory.clone(),
            window: Some(ps.pane.window_index),
            pane: Some(ps.pane.pane_index),
            pane_id: Some(ps.pane.pane_id.clone()),
        }),
        parent,
        connector: String::new(),
        search_text_lower: String::new(),
        session_id,
    }));
}

fn push_dormant_row(
    rows: &mut Vec<Entry>,
    session: &Opencode,
    depth: usize,
    ancestors: Vec<bool>,
    is_last: bool,
    search_text: String,
    parent: Option<usize>,
) {
    // Present but not open in any pane: no goto, Enter is a no-op.
    rows.push(finalize_entry(Entry {
        kind: EntryType::Agent,
        label: session.title.clone(),
        path: session.directory.clone(),
        changes: None,
        branch: None,
        is_open: false,
        is_running: false,
        pending: false,
        depth,
        ancestors,
        is_last,
        search_text,
        goto: None,
        parent,
        connector: String::new(),
        search_text_lower: String::new(),
        session_id: Some(session.id.clone()),
    }));
}

fn push_entry(entry: &DirEntry, is_last_dir: bool, rows: &mut Vec<Entry>) {
    let dir_idx = rows.len();
    let search_text = entry
        .branch
        .as_ref()
        .map(|b| format!("{} {b}", entry.name))
        .unwrap_or_else(|| entry.name.clone());
    rows.push(finalize_entry(Entry {
        kind: EntryType::Dir,
        label: entry.name.clone(),
        path: entry.path.clone(),
        changes: if entry.changes.has_none() {
            None
        } else {
            Some(entry.changes.clone())
        },
        branch: entry.branch.clone(),
        is_open: entry.is_open,
        is_running: false,
        pending: false,
        depth: 0,
        ancestors: vec![],
        is_last: is_last_dir,
        search_text,
        goto: Some(Goto {
            session: entry.name.clone(),
            path: entry.path.clone(),
            window: None,
            pane: None,
            pane_id: None,
        }),
        parent: None,
        connector: String::new(),
        search_text_lower: String::new(),
        session_id: None,
    }));
    let total_children = entry.sessions.len() + entry.dormant.len() + entry.worktrees.len();
    if total_children == 0 {
        return;
    }
    let mut child = 0;
    for ps in &entry.sessions {
        let is_last = child == total_children - 1;
        child += 1;
        push_agent_row(
            rows,
            ps,
            1,
            vec![],
            is_last,
            format!("{} {}", entry.name, ps.session.title),
            Some(dir_idx),
        );
    }
    for s in &entry.dormant {
        let is_last = child == total_children - 1;
        child += 1;
        push_dormant_row(
            rows,
            s,
            1,
            vec![],
            is_last,
            format!("{} {}", entry.name, s.title),
            Some(dir_idx),
        );
    }
    for w in &entry.worktrees {
        let is_last = child == total_children - 1;
        child += 1;
        let wt_idx = rows.len();
        let wt_name = wt_label(&w.info);
        rows.push(finalize_entry(Entry {
            kind: EntryType::Worktree,
            label: wt_name.clone(),
            path: w.info.path.clone(),
            changes: if w.diff.has_none() {
                None
            } else {
                Some(w.diff.clone())
            },
            branch: w.branch.clone(),
            is_open: w.is_open,
            is_running: false,
            pending: false,
            depth: 1,
            ancestors: vec![],
            is_last,
            search_text: format!(
                "{} {} {}",
                entry.name,
                wt_name,
                w.branch.clone().unwrap_or_default()
            ),
            goto: Some(Goto {
                session: wt_name.clone(),
                path: w.info.path.clone(),
                window: None,
                pane: None,
                pane_id: None,
            }),
            parent: Some(dir_idx),
            connector: String::new(),
            search_text_lower: String::new(),
            session_id: None,
        }));
        let s_total = w.sessions.len();
        let d_total = w.dormant.len();
        for (si, ps) in w.sessions.iter().enumerate() {
            push_agent_row(
                rows,
                ps,
                2,
                vec![is_last_dir],
                si == s_total - 1 && d_total == 0,
                format!("{} {} {}", entry.name, wt_name, ps.session.title),
                Some(wt_idx),
            );
        }
        for (di, s) in w.dormant.iter().enumerate() {
            push_dormant_row(
                rows,
                s,
                2,
                vec![is_last_dir],
                di == d_total - 1,
                format!("{} {} {}", entry.name, wt_name, s.title),
                Some(wt_idx),
            );
        }
    }
}

// A worktree nests under its parent only when their open-state matches;
// otherwise it becomes a root in the matching section: open worktree of a
// closed dir → open `* ⑂` root, quiet worktree of an open dir → closed `⑂` root.
struct WtRoot {
    from_dir: String,
    label: String,
    wt: WtEntry,
}

fn split_roots(dir_entries: &mut [DirEntry]) -> (Vec<WtRoot>, Vec<WtRoot>) {
    let mut open_roots = Vec::new();
    let mut quiet_roots = Vec::new();
    for de in dir_entries.iter_mut() {
        let (stay, go): (Vec<WtEntry>, Vec<WtEntry>) = std::mem::take(&mut de.worktrees)
            .into_iter()
            .partition(|w| w.is_open == de.is_open);
        de.worktrees = stay;
        for w in go {
            let root = WtRoot {
                from_dir: de.name.clone(),
                label: wt_label(&w.info),
                wt: w,
            };
            if de.is_open {
                quiet_roots.push(root);
            } else {
                open_roots.push(root);
            }
        }
    }
    (open_roots, quiet_roots)
}

fn push_wt_root(root: &WtRoot, is_last_root: bool, rows: &mut Vec<Entry>) {
    let wt_idx = rows.len();
    rows.push(finalize_entry(Entry {
        kind: EntryType::Worktree,
        label: root.label.clone(),
        path: root.wt.info.path.clone(),
        changes: if root.wt.diff.has_none() {
            None
        } else {
            Some(root.wt.diff.clone())
        },
        branch: root.wt.branch.clone(),
        is_open: root.wt.is_open,
        is_running: false,
        pending: false,
        depth: 0,
        ancestors: vec![],
        is_last: is_last_root,
        search_text: format!(
            "{} {} {}",
            root.from_dir,
            root.label,
            root.wt.branch.clone().unwrap_or_default()
        ),
        goto: Some(Goto {
            session: root.label.clone(),
            path: root.wt.info.path.clone(),
            window: None,
            pane: None,
            pane_id: None,
        }),
        parent: None,
        connector: String::new(),
        search_text_lower: String::new(),
        session_id: None,
    }));
    let s_total = root.wt.sessions.len();
    let d_total = root.wt.dormant.len();
    for (si, ps) in root.wt.sessions.iter().enumerate() {
        push_agent_row(
            rows,
            ps,
            1,
            vec![],
            si == s_total - 1 && d_total == 0,
            format!("{} {} {}", root.from_dir, root.label, ps.session.title),
            Some(wt_idx),
        );
    }
    for (di, s) in root.wt.dormant.iter().enumerate() {
        push_dormant_row(
            rows,
            s,
            1,
            vec![],
            di == d_total - 1,
            format!("{} {} {}", root.from_dir, root.label, s.title),
            Some(wt_idx),
        );
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
    branch: Option<String>,
    worktrees: Vec<WtEntry>,
    sessions: Vec<PaneSession>,
    dormant: Vec<Opencode>,
}

struct WtEntry {
    info: WorktreeInfo,
    diff: Changes,
    branch: Option<String>,
    is_open: bool,
    sessions: Vec<PaneSession>,
    dormant: Vec<Opencode>,
}

#[derive(Clone)]
struct PaneSession {
    pane: TmuxPane,
    session: Opencode,
}

fn synthetic(p: &TmuxPane) -> Opencode {
    // Stable for the pane's lifetime (window/pane indexes shift).
    Opencode {
        id: format!("synthetic:{}", p.pane_id),
        title: "New session".into(),
        directory: p.current_path.clone(),
        time_updated: p.activity,
        time_viewed: 0,
        is_running: false,
    }
}

fn match_panes_to_sessions(
    panes: &[&TmuxPane],
    sessions: &[Opencode],
    reports: &report::ReportMap,
    current_session: Option<&str>,
) -> (Vec<PaneSession>, HashSet<String>) {
    // Binding is report-or-nothing. A pane is an opencode pane iff
    // `tmux::opencode_panes` classified it by `pane_current_command`;
    // the TUI plugin report then names its session (hook authority: the
    // plugin sees `/new` and session switches that emit nothing
    // server-side). No recency guessing — a pane with no usable report
    // gets a synthetic row, never another session's title. Sessions no
    // pane binds stay listed as dormant rows under their directory.
    //
    // Stable order by pane identity: sorting by activity reordered rows
    // under the cursor whenever any session ticked mid-picker.
    let mut sorted = panes.to_vec();
    sorted.sort_by(|a, b| {
        a.session_name
            .cmp(&b.session_name)
            .then(a.pane_id.cmp(&b.pane_id))
    });
    let mut consumed = HashSet::new();
    let mut out = Vec::with_capacity(sorted.len());
    for p in &sorted {
        let reported = (!p.pane_id.is_empty())
            .then(|| report::reported_session(reports, &p.pane_id))
            .flatten();
        // Known session-less pane (fresh TUI): synthetic row. Unknown
        // reported id (ended/no longer listed): synthetic too.
        let best = match reported.as_deref() {
            Some("") | None => None,
            Some(id) => sessions.iter().find(|s| s.id == id),
        };
        if let Some(s) = best {
            consumed.insert(s.id.clone());
            out.push(PaneSession {
                pane: (*p).clone(),
                session: s.clone(),
            });
        } else {
            out.push(PaneSession {
                pane: (*p).clone(),
                session: synthetic(p),
            });
        }
    }
    (collapse_shared(out, current_session), consumed)
}

// One row per session: N screens on the same session rendered N identical
// rows (only reachable via moves + multi-viewer switches, but it reads as
// a glitch). The jump target ranks: viewer in the tmux session the picker
// was opened from (the latest terminal by construction — constant while
// open, so it never flips rows mid-picker) > viewer at home (cwd under
// the session dir) > stable-first. A closed winner simply loses the next
// rebuild and the survivor takes the row. Synthetics are per-pane
// unknowns and never merge.
fn collapse_shared(sessions: Vec<PaneSession>, current: Option<&str>) -> Vec<PaneSession> {
    let rank = |ps: &PaneSession| {
        let here = current.is_some_and(|c| ps.pane.session_name == c);
        let home = is_in(&ps.pane.current_path, &ps.session.directory);
        (!here, !home)
    };
    // Input is already stable-sorted; strict improvement keeps the first
    // of equals, so ties stay deterministic across rebuilds.
    let mut best: HashMap<String, (usize, (bool, bool))> = HashMap::new();
    for (i, ps) in sessions.iter().enumerate() {
        if ps.session.id.starts_with("synthetic:") {
            continue;
        }
        let replace = match best.get(&ps.session.id) {
            None => true,
            Some(&(_, br)) => rank(ps) < br,
        };
        if replace {
            best.insert(ps.session.id.clone(), (i, rank(ps)));
        }
    }
    sessions
        .into_iter()
        .enumerate()
        .filter(|(i, ps)| {
            ps.session.id.starts_with("synthetic:") || best.get(&ps.session.id) == Some(&(*i, rank(ps)))
        })
        .map(|(_, ps)| ps)
        .collect()
}

fn is_in(path: &Path, base: &Path) -> bool {
    path == base || path.starts_with(base)
}

fn keep_mask(dirs: &[DirInfo], git_data: &[DirGit]) -> Vec<bool> {
    let linked: HashSet<PathBuf> = git_data
        .iter()
        .flat_map(|g| g.worktrees.iter().map(|w| w.path.clone()))
        .collect();
    dirs.iter().map(|d| !linked.contains(&d.path)).collect()
}

fn finalize_entry(mut entry: Entry) -> Entry {
    entry.compute_connector();
    entry.search_text_lower = entry.search_text.to_lowercase();
    entry
}

// Hidden sessions live here as id -> title. The picker and CLI own the
// file (possibly from another process); the daemon reads it at
// construction and the picker re-reads it on every payload.
pub fn archived_path() -> PathBuf {
    crate::logs::state_dir().join("archived.json")
}

pub fn load_archived_from(path: &Path) -> HashMap<String, String> {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_archived_to(path: &Path, map: &HashMap<String, String>) {
    if path.parent().is_some_and(|d| std::fs::create_dir_all(d).is_err()) {
        return;
    }
    if let Ok(bytes) = serde_json::to_vec(map) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, &bytes).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

pub fn run_archive_list() {
    let map = load_archived_from(&archived_path());
    if map.is_empty() {
        println!("no archived sessions");
        return;
    }
    let mut ids: Vec<&String> = map.keys().collect();
    ids.sort();
    for id in ids {
        println!("{id} {}", map[id].as_str());
    }
}

pub fn run_unarchive(ids: &[String], all: bool) {
    let path = archived_path();
    if all {
        let _ = std::fs::remove_file(&path);
        println!("unarchived all sessions");
        return;
    }
    let mut map = load_archived_from(&path);
    let mut restored = Vec::new();
    for id in ids {
        if map.remove(id).is_some() {
            restored.push(id.clone());
        }
    }
    save_archived_to(&path, &map);
    if restored.is_empty() {
        println!("nothing unarchived");
    } else {
        println!("unarchived {}", restored.join(" "));
    }
}
