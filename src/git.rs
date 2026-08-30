use crate::model::{Changes, WorktreeInfo};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const CACHE_TTL_MS: u128 = 5000;
const WORKTREE_CACHE_TTL_MS: u128 = 30_000;

#[derive(Default, Serialize, Deserialize)]
pub struct DiskCache {
    pub diffs: Vec<(PathBuf, Changes)>,
    pub worktrees: Vec<(PathBuf, Vec<WorktreeInfo>)>,
}

pub struct GitCache {
    diffs: Mutex<HashMap<PathBuf, (Changes, Instant)>>,
    worktrees: Mutex<HashMap<PathBuf, (Vec<WorktreeInfo>, Instant)>>,
}

impl GitCache {
    pub fn new() -> Self {
        Self { diffs: Mutex::new(HashMap::new()), worktrees: Mutex::new(HashMap::new()) }
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
        if let Ok(c) = self.diffs.lock() {
            if let Some((v, t)) = c.get(path) {
                if t.elapsed().as_millis() < CACHE_TTL_MS {
                    return v.clone();
                }
            }
        }
        let v = self.compute_changes(path);
        if let Ok(mut c) = self.diffs.lock() {
            c.insert(path.to_path_buf(), (v.clone(), Instant::now()));
        }
        v
    }
    pub fn worktrees(&self, path: &Path) -> Vec<WorktreeInfo> {
        if let Ok(c) = self.worktrees.lock() {
            if let Some((v, t)) = c.get(path) {
                if t.elapsed().as_millis() < WORKTREE_CACHE_TTL_MS {
                    return v.clone();
                }
            }
        }
        let v = self.list_worktrees(path);
        if let Ok(mut c) = self.worktrees.lock() {
            c.insert(path.to_path_buf(), (v.clone(), Instant::now()));
        }
        v
    }

    pub fn load_disk(&self, cache: &DiskCache) {
        let stale = Instant::now() - Duration::from_millis((CACHE_TTL_MS + 1) as u64);
        if let Ok(mut m) = self.diffs.lock() {
            for (k, v) in &cache.diffs {
                m.insert(k.clone(), (v.clone(), stale));
            }
        }
        let stale2 = Instant::now() - Duration::from_millis((WORKTREE_CACHE_TTL_MS + 1) as u64);
        if let Ok(mut m) = self.worktrees.lock() {
            for (k, v) in &cache.worktrees {
                m.insert(k.clone(), (v.clone(), stale2));
            }
        }
    }
    pub fn to_disk(&self) -> DiskCache {
        DiskCache {
            diffs: self.diffs.lock().map(|c| c.iter().map(|(k, (v, _))| (k.clone(), v.clone())).collect()).unwrap_or_default(),
            worktrees: self.worktrees.lock().map(|c| c.iter().map(|(k, (v, _))| (k.clone(), v.clone())).collect()).unwrap_or_default(),
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
