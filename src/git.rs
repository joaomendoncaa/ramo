use crate::model::{Changes, WorktreeInfo};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Instant;

const CACHE_TTL_MS: u128 = 5000;
const WORKTREE_CACHE_TTL_MS: u128 = 30_000;

#[derive(Default, Serialize, Deserialize)]
pub struct DiskCache {
    pub diffs: Vec<(PathBuf, Changes)>,
    pub worktrees: Vec<(PathBuf, Vec<WorktreeInfo>)>,
}

struct TtlCache<T: Clone> {
    map: Mutex<HashMap<PathBuf, (T, Instant)>>,
    ttl_ms: u128,
}

impl<T: Clone> TtlCache<T> {
    fn new(ttl_ms: u128) -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
            ttl_ms,
        }
    }
    fn get_or_insert_with(&self, key: &Path, f: impl FnOnce() -> T) -> T {
        if let Ok(cache) = self.map.lock()
            && let Some((v, t)) = cache.get(key)
            && t.elapsed().as_millis() < self.ttl_ms
        {
            return v.clone();
        }
        let v = f();
        if let Ok(mut cache) = self.map.lock() {
            cache.insert(key.to_path_buf(), (v.clone(), Instant::now()));
        }
        v
    }
    fn load(&self, items: &[(PathBuf, T)]) {
        let stale = Instant::now() - std::time::Duration::from_millis((self.ttl_ms + 1) as u64);
        if let Ok(mut m) = self.map.lock() {
            for (k, v) in items {
                m.insert(k.clone(), (v.clone(), stale));
            }
        }
    }
    fn dump(&self) -> Vec<(PathBuf, T)> {
        self.map.lock().map(|c| c.iter().map(|(k, (v, _))| (k.clone(), v.clone())).collect()).unwrap_or_default()
    }
}

pub struct GitCache {
    diffs: TtlCache<Changes>,
    worktrees: TtlCache<Vec<WorktreeInfo>>,
}

impl GitCache {
    pub fn new() -> Self {
        Self {
            diffs: TtlCache::new(CACHE_TTL_MS),
            worktrees: TtlCache::new(WORKTREE_CACHE_TTL_MS),
        }
    }

    fn git_stdout(&self, path: &Path, args: &[&str]) -> Option<String> {
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

    pub fn diff(&self, path: &Path) -> Changes {
        self.diffs
            .get_or_insert_with(path, || self.compute_changes(path))
    }
    pub fn worktrees(&self, path: &Path) -> Vec<WorktreeInfo> {
        self.worktrees
            .get_or_insert_with(path, || self.list_worktrees(path))
    }

    pub fn load_disk(&self, cache: &DiskCache) {
        self.diffs.load(&cache.diffs);
        self.worktrees.load(&cache.worktrees);
    }
    pub fn to_disk(&self) -> DiskCache {
        DiskCache {
            diffs: self.diffs.dump(),
            worktrees: self.worktrees.dump(),
        }
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
