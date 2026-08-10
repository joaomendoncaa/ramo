use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

const SAMPLES: usize = 256;

pub struct Metrics {
    started: Instant,
    builds: u64,
    last_build_ms: u128,
    last_dirs_total: usize,
    last_dirs_refreshed: usize,
    samples: Vec<u128>,
    phases: Mutex<HashMap<&'static str, Phase>>,
    pub git_spawns: u64,
    pub git_diff_hits: u64,
    pub git_diff_misses: u64,
    pub worktree_hits: u64,
    pub worktree_misses: u64,
    pub pushes_sent: u64,
    pub pushes_skipped: u64,
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Phase {
    pub count: u64,
    pub total_ms: u128,
    pub max_ms: u128,
}

pub struct BuildTimings {
    pub tmux_ms: u128,
    pub oc_ms: u128,
    pub git_ms: u128,
    pub serialize_ms: u128,
    pub dirs_total: usize,
    pub dirs_refreshed: usize,
}

#[derive(Serialize, Deserialize)]
pub struct Stats {
    pub pid: u32,
    pub uptime_s: u64,
    pub rss_kb: u64,
    pub builds: u64,
    pub last_build_ms: u128,
    pub avg_build_ms: u128,
    pub p95_build_ms: u128,
    pub max_build_ms: u128,
    pub last_dirs_total: usize,
    pub last_dirs_refreshed: usize,
    pub phases: HashMap<String, Phase>,
    pub git: GitStats,
    pub pushes: PushStats,
}

#[derive(Default, Serialize, Deserialize)]
pub struct GitStats {
    pub spawns: u64,
    pub diff_cache_hits: u64,
    pub diff_cache_misses: u64,
    pub worktree_cache_hits: u64,
    pub worktree_cache_misses: u64,
}

#[derive(Default, Serialize, Deserialize)]
pub struct PushStats {
    pub sent: u64,
    pub skipped_identical: u64,
}

impl Metrics {
    pub fn new() -> Self {
        Metrics {
            started: Instant::now(),
            builds: 0,
            last_build_ms: 0,
            last_dirs_total: 0,
            last_dirs_refreshed: 0,
            samples: Vec::with_capacity(SAMPLES),
            phases: Mutex::new(HashMap::new()),
            git_spawns: 0,
            git_diff_hits: 0,
            git_diff_misses: 0,
            worktree_hits: 0,
            worktree_misses: 0,
            pushes_sent: 0,
            pushes_skipped: 0,
        }
    }

    pub fn record_build(&mut self, timings: &BuildTimings) {
        self.builds += 1;
        self.last_dirs_total = timings.dirs_total;
        self.last_dirs_refreshed = timings.dirs_refreshed;
        let total = timings.tmux_ms + timings.oc_ms + timings.git_ms + timings.serialize_ms;
        self.last_build_ms = total;
        if self.samples.len() == SAMPLES {
            self.samples.remove(0);
        }
        self.samples.push(total);
        self.record_phase("tmux", timings.tmux_ms);
        self.record_phase("opencode", timings.oc_ms);
        self.record_phase("git", timings.git_ms);
        self.record_phase("serialize", timings.serialize_ms);
        self.record_phase("total", total);
    }

    fn record_phase(&self, name: &'static str, ms: u128) {
        let mut phases = self.phases.lock().unwrap();
        let phase = phases.entry(name).or_default();
        phase.count += 1;
        phase.total_ms += ms;
        phase.max_ms = phase.max_ms.max(ms);
    }

    pub fn stats(&self) -> Stats {
        let mut sorted = self.samples.clone();
        sorted.sort_unstable();
        let p95 = sorted
            .get((sorted.len() as f64 * 0.95) as usize)
            .copied()
            .unwrap_or(0);
        let avg = if self.samples.is_empty() {
            0
        } else {
            self.samples.iter().sum::<u128>() / self.samples.len() as u128
        };
        let phases = self
            .phases
            .lock()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        Stats {
            pid: std::process::id(),
            uptime_s: self.started.elapsed().as_secs(),
            rss_kb: rss_kb(),
            builds: self.builds,
            last_build_ms: self.last_build_ms,
            avg_build_ms: avg,
            p95_build_ms: p95,
            max_build_ms: self.samples.iter().copied().max().unwrap_or(0),
            last_dirs_total: self.last_dirs_total,
            last_dirs_refreshed: self.last_dirs_refreshed,
            phases,
            git: GitStats {
                spawns: self.git_spawns,
                diff_cache_hits: self.git_diff_hits,
                diff_cache_misses: self.git_diff_misses,
                worktree_cache_hits: self.worktree_hits,
                worktree_cache_misses: self.worktree_misses,
            },
            pushes: PushStats {
                sent: self.pushes_sent,
                skipped_identical: self.pushes_skipped,
            },
        }
    }
}

fn rss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines().find_map(|l| {
                l.strip_prefix("VmRSS:")
                    .and_then(|v| v.trim().trim_end_matches(" kB").parse().ok())
            })
        })
        .unwrap_or(0)
}

pub fn stats_path() -> PathBuf {
    crate::logs::state_dir().join("stats.json")
}

pub fn save(stats: &Stats) {
    let path = stats_path();
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    let tmp = path.with_extension("json.tmp");
    if let Ok(bytes) = serde_json::to_vec_pretty(stats) {
        let _ = std::fs::write(&tmp, bytes);
        let _ = std::fs::rename(&tmp, path);
    }
}

pub fn print_stats() {
    let path = stats_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<Stats>(&content) {
            Ok(s) => {
                println!("ramo daemon stats (from {})", path.display());
                println!("  pid {}  uptime {}s  rss {}kB", s.pid, s.uptime_s, s.rss_kb);
                println!(
                    "  builds {}  last {}ms  avg {}ms  p95 {}ms  max {}ms",
                    s.builds, s.last_build_ms, s.avg_build_ms, s.p95_build_ms, s.max_build_ms
                );
                println!(
                    "  dirs     {} total  {} refreshed last build",
                    s.last_dirs_total, s.last_dirs_refreshed
                );
                println!("  phases (count, total, max):");
                for (name, p) in sorted_phases(&s.phases) {
                    println!(
                        "    {name:<10} n={:<5} total={}ms  max={}ms",
                        p.count, p.total_ms, p.max_ms
                    );
                }
                println!(
                    "  git      {} spawns  {} diff-hits  {} diff-misses  {} wt-hits  {} wt-misses",
                    s.git.spawns,
                    s.git.diff_cache_hits,
                    s.git.diff_cache_misses,
                    s.git.worktree_cache_hits,
                    s.git.worktree_cache_misses
                );
                println!(
                    "  pushes   {} sent  {} skipped-identical",
                    s.pushes.sent, s.pushes.skipped_identical
                );
            }
            Err(e) => eprintln!("ramo: failed to parse {}: {e}", path.display()),
        },
        Err(e) => eprintln!("ramo: no stats yet ({}: {e})", path.display()),
    }
}

fn sorted_phases(phases: &HashMap<String, Phase>) -> Vec<(&String, &Phase)> {
    let mut v: Vec<(&String, &Phase)> = phases.iter().collect();
    v.sort_by_key(|(_, p)| std::cmp::Reverse(p.total_ms));
    v
}
