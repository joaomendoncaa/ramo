use crate::model::{Changes, WorktreeInfo};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

const CACHE_TTL_MS: u128 = 5000;
const WORKTREE_CACHE_TTL_MS: u128 = 30_000;

type Diffs = Mutex<HashMap<PathBuf, (Changes, Instant)>>;

#[derive(Default, Serialize, Deserialize)]
pub struct DiskCache {
    pub diffs: Vec<(PathBuf, Changes)>,
    pub worktrees: Vec<(PathBuf, Vec<WorktreeInfo>)>,
}

pub struct GitCache {
    diffs: Diffs,
    worktrees: Mutex<HashMap<PathBuf, (Vec<WorktreeInfo>, Instant)>>,
    pub spawns: AtomicU64,
    pub diff_hits: AtomicU64,
    pub diff_misses: AtomicU64,
    pub worktree_hits: AtomicU64,
    pub worktree_misses: AtomicU64,
}

impl GitCache {
    pub fn new() -> Self {
        GitCache {
            diffs: Mutex::new(HashMap::new()),
            worktrees: Mutex::new(HashMap::new()),
            spawns: AtomicU64::new(0),
            diff_hits: AtomicU64::new(0),
            diff_misses: AtomicU64::new(0),
            worktree_hits: AtomicU64::new(0),
            worktree_misses: AtomicU64::new(0),
        }
    }

    fn git_stdout(&self, path: &Path, args: &[&str]) -> Option<String> {
        self.spawns.fetch_add(1, Ordering::Relaxed);
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(path.to_string_lossy().as_ref());
        cmd.args(args);
        let output = cmd.output().ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn compute_changes(&self, path: &Path) -> Changes {
        if !path.join(".git").exists() {
            return Changes::default();
        }

        let mut additions = 0i64;
        let mut deletions = 0i64;

        if let Some(out) = self.git_stdout(path, &["diff", "--numstat", "HEAD"]) {
            for line in out.lines() {
                let p: Vec<&str> = line.split_whitespace().collect();
                if p.len() >= 2 {
                    additions += p[0].parse::<i64>().unwrap_or(0);
                    deletions += p[1].parse::<i64>().unwrap_or(0);
                }
            }
        }

        if let Some(out) = self.git_stdout(path, &["ls-files", "--others", "--exclude-standard"]) {
            additions += out.lines().filter(|l| !l.is_empty()).count() as i64;
        }

        Changes {
            additions,
            deletions,
        }
    }

    fn list_worktrees(&self, path: &Path) -> Vec<WorktreeInfo> {
        if !path.join(".git").exists() {
            return vec![];
        }

        let Some(out) = self.git_stdout(path, &["worktree", "list"]) else {
            return vec![];
        };

        let oc_prefix = opencode_worktree_prefix();

        out.lines()
            .enumerate()
            .filter_map(|(i, line)| {
                let path = PathBuf::from(line.split_whitespace().next()?);
                if path.starts_with(&oc_prefix) {
                    return None;
                }
                Some(WorktreeInfo {
                    path,
                    is_main: i == 0,
                })
            })
            .collect()
    }

    // Fresh value if within TTL, otherwise computes (and records a miss).
    pub fn diff(&self, path: &Path) -> Changes {
        if let Ok(cache) = self.diffs.lock()
            && let Some((diff, t)) = cache.get(path)
            && t.elapsed().as_millis() < CACHE_TTL_MS
        {
            self.diff_hits.fetch_add(1, Ordering::Relaxed);
            return diff.clone();
        }
        self.diff_misses.fetch_add(1, Ordering::Relaxed);
        let diff = self.compute_changes(path);
        if let Ok(mut cache) = self.diffs.lock() {
            cache.insert(path.to_path_buf(), (diff.clone(), Instant::now()));
        }
        diff
    }

    // Cached value regardless of age (used for stagger-refreshed dirs).
    pub fn diff_stale(&self, path: &Path) -> Option<Changes> {
        self.diffs
            .lock()
            .ok()
            .and_then(|c| c.get(path).map(|(d, _)| d.clone()))
    }

    // Refreshes the cache entry without the TTL check (used for dirs picked by
    // the rotation) and reports whether the value was already fresh.
    pub fn refresh_diff(&self, path: &Path) -> Changes {
        if let Ok(cache) = self.diffs.lock()
            && let Some((diff, t)) = cache.get(path)
            && t.elapsed().as_millis() < CACHE_TTL_MS
        {
            self.diff_hits.fetch_add(1, Ordering::Relaxed);
            return diff.clone();
        }
        self.diff(path)
    }

    pub fn worktrees(&self, path: &Path) -> Vec<WorktreeInfo> {
        if let Ok(cache) = self.worktrees.lock()
            && let Some((w, t)) = cache.get(path)
            && t.elapsed().as_millis() < WORKTREE_CACHE_TTL_MS
        {
            self.worktree_hits.fetch_add(1, Ordering::Relaxed);
            return w.clone();
        }
        self.worktree_misses.fetch_add(1, Ordering::Relaxed);
        let w = self.list_worktrees(path);
        if let Ok(mut cache) = self.worktrees.lock() {
            cache.insert(path.to_path_buf(), (w.clone(), Instant::now()));
        }
        w
    }

    pub fn worktrees_stale(&self, path: &Path) -> Option<Vec<WorktreeInfo>> {
        self.worktrees
            .lock()
            .ok()
            .and_then(|c| c.get(path).map(|(w, _)| w.clone()))
    }

    pub fn refresh_worktrees(&self, path: &Path) -> Vec<WorktreeInfo> {
        if let Ok(cache) = self.worktrees.lock()
            && let Some((w, t)) = cache.get(path)
            && t.elapsed().as_millis() < WORKTREE_CACHE_TTL_MS
        {
            self.worktree_hits.fetch_add(1, Ordering::Relaxed);
            return w.clone();
        }
        self.worktrees(path)
    }

    pub fn load_disk(&self, cache: &DiskCache) {
        let now = Instant::now();
        let stale = now - std::time::Duration::from_millis(CACHE_TTL_MS as u64 + 1);
        let wt_stale = now - std::time::Duration::from_millis(WORKTREE_CACHE_TTL_MS as u64 + 1);
        if let Ok(mut diffs) = self.diffs.lock() {
            for (path, changes) in &cache.diffs {
                diffs.insert(path.clone(), (changes.clone(), stale));
            }
        }
        if let Ok(mut wts) = self.worktrees.lock() {
            for (path, w) in &cache.worktrees {
                wts.insert(path.clone(), (w.clone(), wt_stale));
            }
        }
    }

    pub fn to_disk(&self) -> DiskCache {
        let diffs = self
            .diffs
            .lock()
            .ok()
            .map(|c| {
                c.iter()
                    .map(|(p, (d, _))| (p.clone(), d.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let worktrees = self
            .worktrees
            .lock()
            .ok()
            .map(|c| {
                c.iter()
                    .map(|(p, (w, _))| (p.clone(), w.clone()))
                    .collect()
            })
            .unwrap_or_default();
        DiskCache { diffs, worktrees }
    }
}

fn opencode_worktree_prefix() -> String {
    std::env::var("XDG_DATA_HOME")
        .map(|x| format!("{}/opencode/worktree", x))
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(|h| format!("{}/.local/share/opencode/worktree", h))
                .unwrap_or_default()
        })
}
