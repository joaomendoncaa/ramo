use ramo::cli::{Cli, Command, Daemon, Plugin};
use ramo::config::Config;
use ramo::picker::{self, Picker};
use ramo::terminal::Tui;
use ramo::integration::tmux;
use ramo::{agents, daemon, integration, model, purge, service};
use std::io;

fn main() -> io::Result<()> {
    let (mut config, mut feedbacks) = Config::new();
    let cli = Cli::new(&config);

    match cli.command {
        Command::Help => {
            println!("{}", cli.help());
            return Ok(());
        }
        Command::Daemon(Daemon::Start { detached }) => {
            if detached {
                return daemon::start_detached(cli.overrides);
            }
            return daemon::start_daemon(cli.overrides);
        }
        Command::Daemon(Daemon::Restart { detached }) => {
            return daemon::restart_daemon(cli.overrides, detached);
        }
        Command::Daemon(Daemon::Kill) | Command::Kill => {
            daemon::kill();
            return Ok(());
        }
        Command::Daemon(Daemon::Info) => {
            daemon::print_daemon_info();
            return Ok(());
        }
        Command::Agents => {
            agents::run_agents();
            return Ok(());
        }
        Command::Archive => {
            ramo::builder::run_archive_list();
            return Ok(());
        }
        Command::Unarchive { ids, all } => {
            ramo::builder::run_unarchive(&ids, all);
            return Ok(());
        }
        Command::Focus {
            session,
            pane_id,
            window,
            pane,
        } => {
            agents::run_focus(&session, &pane_id, &window, &pane);
            return Ok(());
        }
        Command::Daemon(Daemon::Logs) => {
            daemon::show_logs();
            return Ok(());
        }
        Command::Daemon(Daemon::Install) => {
            return service::install();
        }
        Command::Plugin(Plugin::Install) => {
            match integration::opencode::plugin_install() {
                Ok(()) => println!("plugin installed"),
                Err(e) => eprintln!("ramo plugin install: {e}"),
            }
            return Ok(());
        }
        Command::Daemon(Daemon::Uninstall) => {
            return service::uninstall();
        }
        Command::Purge { with_config } => {
            return purge::run(with_config);
        }
        Command::Config => {
            daemon::print_config();
            return Ok(());
        }
        Command::Unknown(ref cmd) => {
            println!("{}", cli.unknown(cmd));
            return Ok(());
        }
        _ => {}
    }

    feedbacks.extend(config.apply_overrides(&cli.overrides));

    let payload = daemon::fetch_once()
        .and_then(|bytes| serde_json::from_slice::<model::Payload>(&bytes).ok())
        .unwrap_or_else(|| daemon::preflight(&config, feedbacks));

    let mut screen = Tui::new()?;
    let mut picker = Picker::new(payload);

    picker.schedule_initial_fetch(cli.overrides);

    // Restore the tty (and close the popup) before switching sessions:
    // switching while the TUI still owns the terminal wedged the client.
    let exit_goto: Option<model::Goto> = loop {
        screen.draw(|f| picker.render(f, &config))?;
        screen.poll_and_handle_events(&mut picker)?;

        match picker.tick() {
            picker::Signal::Close => break None,
            picker::Signal::Goto(goto) => {
                if picker.quit {
                    break Some(goto);
                }
                perform_goto(&goto);
            }
            _ => {}
        }
    };
    drop(screen);
    if let Some(goto) = exit_goto {
        perform_goto(&goto);
    }

    Ok(())
}

fn perform_goto(goto: &model::Goto) {
    if tmux::is_current_session(&goto.session) {
        tmux::select_agent_pane(
            &tmux::resolve_session(&goto.session),
            goto.window,
            goto.pane,
            goto.pane_id.as_deref(),
        );
    } else {
        tmux::goto(&goto);
    }
}
