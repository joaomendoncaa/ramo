use crate::model::{Changes, WorktreeInfo};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Instant;

const CACHE_TTL_MS: u128 = 5000;

type Diffs = Mutex<HashMap<PathBuf, (Changes, Instant)>>;

pub struct GitCache {
    diffs: Diffs,
}

impl GitCache {
    pub fn new() -> Self {
        GitCache {
            diffs: Mutex::new(HashMap::new()),
        }
    }

    pub fn diff(&self, path: &Path) -> Changes {
        if let Ok(cache) = self.diffs.lock()
            && let Some((diff, t)) = cache.get(path)
            && t.elapsed().as_millis() < CACHE_TTL_MS
        {
            return diff.clone();
        }
        let diff = compute_changes(path);
        if let Ok(mut cache) = self.diffs.lock() {
            cache.insert(path.to_path_buf(), (diff.clone(), Instant::now()));
        }
        diff
    }
}

fn git_stdout(path: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(path.to_string_lossy().as_ref());
    cmd.args(args);
    let output = cmd.output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).to_string())
}

fn compute_changes(path: &Path) -> Changes {
    if !path.join(".git").exists() {
        return Changes::default();
    }

    let mut additions = 0i64;
    let mut deletions = 0i64;

    if let Some(out) = git_stdout(path, &["diff", "--numstat", "HEAD"]) {
        for line in out.lines() {
            let p: Vec<&str> = line.split_whitespace().collect();
            if p.len() >= 2 {
                additions += p[0].parse::<i64>().unwrap_or(0);
                deletions += p[1].parse::<i64>().unwrap_or(0);
            }
        }
    }

    if let Some(out) = git_stdout(path, &["ls-files", "--others", "--exclude-standard"]) {
        additions += out.lines().filter(|l| !l.is_empty()).count() as i64;
    }

    Changes {
        additions,
        deletions,
    }
}

pub fn list_worktrees(path: &Path) -> Vec<WorktreeInfo> {
    if !path.join(".git").exists() {
        return vec![];
    }

    let Some(out) = git_stdout(path, &["worktree", "list"]) else {
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

fn opencode_worktree_prefix() -> String {
    std::env::var("XDG_DATA_HOME")
        .map(|x| format!("{}/opencode/worktree", x))
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(|h| format!("{}/.local/share/opencode/worktree", h))
                .unwrap_or_default()
        })
}
