//! Headless end-to-end tests for the agent tracking pipeline.
//!
//! The pipeline is `tmux panes + opencode API sessions + TUI reports →
//! entries`. All three inputs are faked here: no tmux server, no opencode
//! binary, no sockets. Temp dirs stand in for scanned projects (no `.git`
//! inside, so the git cache never shells out).
//!
//! Contract under test — the thing ramo must do every time it opens:
//! live panes always appear with the exact reported session, in a stable
//! order, under stable identities, and rebuilds are byte-identical.

use ramo::builder::TreeBuilder;
use ramo::config::Config;
use ramo::model::{EntryType, Opencode, TmuxPane, TmuxSession};
use ramo::report;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp_root() -> PathBuf {
    let id = N.fetch_add(1, Ordering::SeqCst);
    let base = std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp/opencode"));
    let dir = base.join(format!("ramo-agents-{}-{id}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

struct Fixture {
    _root: PathBuf,
    config: Config,
    alpha: PathBuf,
    beta: PathBuf,
}

fn fixture() -> Fixture {
    let root = tmp_root();
    let alpha = root.join("alpha");
    let beta = root.join("beta");
    std::fs::create_dir_all(&alpha).unwrap();
    std::fs::create_dir_all(&beta).unwrap();
    let config = Config {
        path: format!("{}/*", root.display()),
        ..Config::default()
    };
    Fixture {
        _root: root,
        config,
        alpha,
        beta,
    }
}

fn pane(
    session: &str,
    window: usize,
    index: usize,
    pane_id: &str,
    dir: &Path,
) -> TmuxPane {
    TmuxPane {
        session_name: session.into(),
        window_index: window,
        pane_index: index,
        pane_id: pane_id.into(),
        current_command: "opencode2".into(),
        current_path: dir.to_path_buf(),
        activity: 1_700_000_000,
    }
}

fn tmux_session(name: &str, dir: &Path) -> TmuxSession {
    TmuxSession {
        name: name.into(),
        path: dir.to_path_buf(),
    }
}

fn sess(id: &str, title: &str, dir: &Path, running: bool) -> Opencode {
    Opencode {
        id: id.into(),
        title: title.into(),
        directory: dir.to_path_buf(),
        time_updated: 1_700_000_100,
        time_viewed: 0,
        is_running: running,
    }
}

fn builder_with_reports(pairs: &[(&str, &str)]) -> TreeBuilder {
    let reports = report::ReportMap::default();
    for (pane_id, session_id) in pairs {
        report::insert(&reports, pane_id, session_id);
    }
    TreeBuilder::new(reports)
}

fn agents_of(entries: &[ramo::model::Entry]) -> Vec<&ramo::model::Entry> {
    entries
        .iter()
        .filter(|e| e.kind == EntryType::Agent)
        .collect()
}

fn live_agents_of(entries: &[ramo::model::Entry]) -> Vec<&ramo::model::Entry> {
    agents_of(entries)
        .into_iter()
        .filter(|e| e.goto.is_some())
        .collect()
}

#[test]
fn live_panes_bind_exact_reported_sessions() {
    let fx = fixture();
    let b = builder_with_reports(&[("%11", "ses-a"), ("%22", "ses-b")]);
    let entries = b.build_with(
        &fx.config,
        &[tmux_session("alpha", &fx.alpha), tmux_session("beta", &fx.beta)],
        &[
            pane("alpha", 1, 0, "%11", &fx.alpha),
            pane("beta", 2, 0, "%22", &fx.beta),
        ],
        &[sess("ses-a", "Alpha work", &fx.alpha, true), sess("ses-b", "Beta work", &fx.beta, false)],
    );
    let live = live_agents_of(&entries);
    assert_eq!(live.len(), 2, "both panes listed: {entries:?}");
    let by_pane: HashMap<_, _> = live
        .iter()
        .map(|e| (e.goto.as_ref().unwrap().pane_id.clone().unwrap(), *e))
        .collect();
    assert_eq!(by_pane["%11"].label, "Alpha work");
    assert_eq!(by_pane["%22"].label, "Beta work");
    assert_eq!(by_pane["%11"].session_id.as_deref(), Some("ses-a"));
    assert!(by_pane["%11"].is_running);
    assert!(!by_pane["%22"].is_running);
    // Grouped under their own directories, never crossed.
    for e in live {
        let parent = e.parent.and_then(|i| entries.get(i)).unwrap();
        assert_eq!(parent.path, e.path, "agent sits under its dir");
    }
}

#[test]
fn missing_report_is_synthetic_never_another_title() {
    let fx = fixture();
    let b = builder_with_reports(&[("%11", "ses-a")]);
    let entries = b.build_with(
        &fx.config,
        &[tmux_session("alpha", &fx.alpha)],
        &[
            pane("alpha", 1, 0, "%11", &fx.alpha),
            pane("alpha", 1, 1, "%99", &fx.alpha),
        ],
        &[sess("ses-a", "Alpha work", &fx.alpha, false)],
    );
    let live = live_agents_of(&entries);
    assert_eq!(live.len(), 2);
    let unknown: Vec<_> = live.iter().filter(|e| e.label == "Alpha work").collect();
    assert_eq!(unknown.len(), 1, "unreported pane must not borrow a title");
    let synth = live.iter().find(|e| e.label == "New session").unwrap();
    assert_eq!(synth.session_id, None);
    assert_eq!(
        synth.goto.as_ref().unwrap().pane_id.as_deref(),
        Some("%99")
    );
}

#[test]
fn unknown_reported_id_is_synthetic() {
    let fx = fixture();
    let b = builder_with_reports(&[("%11", "ses-gone")]);
    let entries = b.build_with(
        &fx.config,
        &[tmux_session("alpha", &fx.alpha)],
        &[pane("alpha", 1, 0, "%11", &fx.alpha)],
        &[sess("ses-a", "Alpha work", &fx.alpha, false)],
    );
    let live = live_agents_of(&entries);
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].label, "New session", "ended id must not ghost");
}

#[test]
fn index_shift_keeps_identity_order_and_keys() {
    let fx = fixture();
    let b = builder_with_reports(&[("%11", "ses-a"), ("%22", "ses-b")]);
    let sessions = vec![tmux_session("alpha", &fx.alpha)];
    let api = vec![
        sess("ses-a", "Alpha work", &fx.alpha, false),
        sess("ses-b", "Beta work", &fx.alpha, false),
    ];
    // Note: ses-b lives in alpha too, so both panes share one dir.
    let before = b.build_with(
        &fx.config,
        &sessions,
        &[
            pane("alpha", 1, 0, "%11", &fx.alpha),
            pane("alpha", 1, 1, "%22", &fx.alpha),
        ],
        &api,
    );
    // Panes open/close around them: window/pane indexes shift, pane ids
    // (stable for the pane lifetime) do not.
    let after = b.build_with(
        &fx.config,
        &sessions,
        &[
            pane("alpha", 3, 2, "%11", &fx.alpha),
            pane("alpha", 3, 5, "%22", &fx.alpha),
        ],
        &api,
    );
    let keys = |es: &[ramo::model::Entry]| {
        live_agents_of(es)
            .iter()
            .map(|e| e.stable_key())
            .collect::<Vec<_>>()
    };
    assert_eq!(keys(&before), keys(&after), "order + identity survive shifts");
    assert_eq!(
        live_agents_of(&after)
            .iter()
            .map(|e| e.goto.as_ref().unwrap().pane_id.clone().unwrap())
            .collect::<Vec<_>>(),
        vec!["%11".to_string(), "%22".to_string()]
    );
}

#[test]
fn rebuild_is_byte_stable() {
    let fx = fixture();
    let b = builder_with_reports(&[("%11", "ses-a")]);
    let sessions = vec![tmux_session("alpha", &fx.alpha)];
    let panes = vec![pane("alpha", 1, 0, "%11", &fx.alpha)];
    let api = vec![sess("ses-a", "Alpha work", &fx.alpha, true)];
    let a = b.build_with(&fx.config, &sessions, &panes, &api);
    let c = b.build_with(&fx.config, &sessions, &panes, &api);
    assert_eq!(
        serde_json::to_vec(&a).unwrap(),
        serde_json::to_vec(&c).unwrap(),
        "identical inputs, identical entries — no flicker source"
    );
}

#[test]
fn recency_change_does_not_reorder() {
    let fx = fixture();
    let b = builder_with_reports(&[("%11", "ses-a"), ("%22", "ses-b")]);
    let sessions = vec![tmux_session("alpha", &fx.alpha)];
    // Both sessions seen live, then both panes close: two dormant rows.
    let live_panes = vec![
        pane("alpha", 1, 0, "%11", &fx.alpha),
        pane("alpha", 1, 1, "%22", &fx.alpha),
    ];
    let mut api = vec![
        sess("ses-a", "Alpha work", &fx.alpha, false),
        sess("ses-b", "Beta work", &fx.alpha, false),
    ];
    let _ = b.build_with(&fx.config, &sessions, &live_panes, &api);
    let order = |es: &[ramo::model::Entry]| {
        agents_of(es)
            .iter()
            .map(|e| e.session_id.clone().unwrap())
            .collect::<Vec<_>>()
    };
    api[0].time_updated = 1_800_000_000;
    api[1].time_updated = 1_900_000_001;
    let first = b.build_with(&fx.config, &sessions, &[], &api);
    assert_eq!(order(&first), vec!["ses-a".to_string(), "ses-b".to_string()]);
    // ses-a gets a new message — recency would flip it on top.
    api[0].time_updated = 1_999_999_999;
    let second = b.build_with(&fx.config, &sessions, &[], &api);
    assert_eq!(
        order(&second),
        order(&first),
        "timestamp updates must not reorder rows under the cursor"
    );
}

#[test]
fn closed_pane_becomes_dormant_once_seen() {
    let fx = fixture();
    let b = builder_with_reports(&[("%11", "ses-a")]);
    let sessions = vec![tmux_session("alpha", &fx.alpha)];
    let live_panes = vec![pane("alpha", 1, 0, "%11", &fx.alpha)];
    let api = vec![
        sess("ses-a", "Alpha work", &fx.alpha, false),
        sess("ses-never", "Never opened", &fx.alpha, false),
    ];
    let live = b.build_with(&fx.config, &sessions, &live_panes, &api);
    assert!(live_agents_of(&live).iter().any(|e| e.label == "Alpha work"));
    // Pane closed (or `/new` moved on): no panes at all now.
    let after = b.build_with(&fx.config, &sessions, &[], &api);
    let agents = agents_of(&after);
    assert_eq!(agents.len(), 1, "seen session lingers once, history stays out");
    assert_eq!(agents[0].label, "Alpha work");
    assert_eq!(agents[0].goto, None);
    assert_eq!(agents[0].session_id.as_deref(), Some("ses-a"));
}

#[test]
fn daemon_respawn_keeps_live_bindings() {
    // The picker respawns the daemon on every flagged open, wiping
    // in-memory reports. A respawned daemon must be born accurate —
    // no "New session" flash while TUIs resend.
    let fx = fixture();
    let sessions = vec![tmux_session("alpha", &fx.alpha)];
    let panes = vec![pane("alpha", 1, 0, "%11", &fx.alpha)];
    let api = vec![sess("ses-a", "Alpha work", &fx.alpha, true)];

    let a = builder_with_reports(&[("%11", "ses-a")]);
    let before = a.build_with(&fx.config, &sessions, &panes, &api);
    assert_eq!(live_agents_of(&before).len(), 1);

    let path = fx._root.join("reports.json");
    report::save_to(&path, &a.reports());
    let b = TreeBuilder::new(report::load_from(&path));
    let after = b.build_with(&fx.config, &sessions, &panes, &api);

    let titles = |es: &[ramo::model::Entry]| {
        live_agents_of(es)
            .iter()
            .map(|e| e.label.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(titles(&after), vec!["Alpha work".to_string()]);
    assert_eq!(
        serde_json::to_vec(&before).unwrap(),
        serde_json::to_vec(&after).unwrap(),
        "respawned daemon builds byte-identical entries"
    );
}

#[test]
fn unseen_history_stays_out() {
    let fx = fixture();
    let b = builder_with_reports(&[]);
    let entries = b.build_with(
        &fx.config,
        &[tmux_session("alpha", &fx.alpha)],
        &[],
        &[sess("ses-old", "Ancient history", &fx.alpha, false)],
    );
    assert!(
        agents_of(&entries).is_empty(),
        "full API history must not flood the list before ramo sees it open"
    );
}

#[test]
fn non_opencode_panes_are_not_agents() {
    let fx = fixture();
    let b = builder_with_reports(&[]);
    let mut shell = pane("alpha", 1, 0, "%11", &fx.alpha);
    shell.current_command = "nvim".into();
    let entries = b.build_with(
        &fx.config,
        &[tmux_session("alpha", &fx.alpha)],
        &[shell],
        &[sess("ses-a", "Alpha work", &fx.alpha, false)],
    );
    assert!(
        agents_of(&entries).is_empty(),
        "process-exact classification: editors/shells are not agents"
    );
}
