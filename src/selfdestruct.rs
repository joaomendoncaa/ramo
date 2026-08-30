use crate::config::Config;
use crate::daemon;
use crate::logs;
use crate::service;
use std::io::{self, Write};

pub fn run() -> io::Result<()> {
    print!("Are you sure you want to completely remove Ramo from your system? [y/N] ");
    io::stdout().flush()?;

    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    if !matches!(answer.trim().to_lowercase().as_str(), "y" | "yes") {
        println!("aborted");
        return Ok(());
    }

    let _ = service::uninstall();
    daemon::kill();

    let state = logs::state_dir();
    if let Err(e) = std::fs::remove_dir_all(&state)
        && e.kind() != io::ErrorKind::NotFound
    {
        eprintln!("failed to remove {}: {e}", state.display());
    }

    if let Some(dir) = Config::config_dir()
        && let Err(e) = std::fs::remove_dir_all(&dir)
        && e.kind() != io::ErrorKind::NotFound
    {
        eprintln!("failed to remove {}: {e}", dir.display());
    }

    if let Ok(exe) = std::env::current_exe()
        && let Err(e) = std::fs::remove_file(&exe)
        && e.kind() != io::ErrorKind::NotFound
    {
        eprintln!("failed to remove binary {}: {e}", exe.display());
    }

    println!("Ramo has been removed from your system");
    Ok(())
}
