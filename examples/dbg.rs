use ramo::builder::TreeBuilder;
use ramo::config::Config;
use ramo::integration::{opencode, tmux};
use ramo::report;
use std::collections::HashSet;
fn main() {
    println!("server_url: {:?}", opencode::server_url());
    let seen: HashSet<String> = HashSet::new();
    match opencode::sessions(&[], &seen) {
        Some(oc) => {
            println!("sessions: {}", oc.len());
            for s in oc.iter().filter(|s| s.is_running) {
                println!("  RUNNING {} dir={:?}", s.id, s.directory);
            }
        }
        None => println!("sessions: FETCH FAILED"),
    }
    let snap = tmux::snapshot();
    let config = Config { path: "/home/joao/lab/*:/home/joao/.config.jmmm.sh".into(), ..Config::default() };
    let b = TreeBuilder::new(report::ReportMap::default());
    let oc = opencode::sessions(&[], &seen).unwrap_or_default();
    let entries = b.build_with(&config, &snap.sessions, &snap.panes, &oc, None, opencode::server_url().as_deref());
    for e in entries.iter().filter(|e| e.kind == ramo::model::EntryType::Agent) {
        println!("ROW {:?} run={} url={:?}", e.label, e.is_running, e.goto.as_ref().and_then(|g| g.url.clone()));
    }
    println!("done");
}
