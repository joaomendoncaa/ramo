// External surfaces ramo talks to: agents (session listing plus TUI
// plugin installer and assets, see assets/opencode/), the tmux
// multiplexer, and the Hyprland compositor.
pub mod clankers;
pub mod hyprland;
pub mod opencode;
pub mod tmux;

/// Strict exact-token check for `agents-disabled` (e.g. `opencode`,
/// `t3code/clankers`). Unknown tokens are ignored.
pub fn source_disabled(disabled: &[String], id: &str) -> bool {
    disabled.iter().any(|x| x == id)
}
