// External surfaces ramo talks to: agents (session listing plus TUI
// plugin installer and assets, see assets/opencode/), the tmux
// multiplexer, and the Hyprland compositor.
pub mod hyprland;
pub mod opencode;
pub mod tmux;
