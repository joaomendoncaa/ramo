use std::path::PathBuf;

#[allow(dead_code)]
pub struct BuildTimings {
    pub tmux_ms: u128,
    pub oc_ms: u128,
    pub git_ms: u128,
    pub serialize_ms: u128,
    pub dirs_total: usize,
    pub dirs_refreshed: usize,
}

pub fn stats_path() -> PathBuf {
    crate::logs::state_dir().join("stats.json")
}

pub fn print_stats() {
    let path = stats_path();
    match std::fs::read_to_string(&path) {
        Ok(c) => println!("{c}"),
        Err(e) => eprintln!("ramo: no stats yet ({}: {e})", path.display()),
    }
}
