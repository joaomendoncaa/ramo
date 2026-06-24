use crate::config::Config;

#[derive(Debug, Clone, PartialEq)]
pub enum Daemon {
    Info,
    Start,
    Kill,
    Logs,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Run,
    Daemon(Daemon),
    Kill,
    Help,
    Config,
    Unknown(String),
}

pub struct Cli {
    pub command: Command,
    pub overrides: Vec<(String, Option<String>)>,
}

impl Cli {
    pub fn new(_config: &Config) -> Self {
        let args: Vec<String> = std::env::args().collect();
        let mut command = Command::Run;
        let mut overrides = Vec::new();
        let mut i = 1;

        while i < args.len() {
            match args[i].as_str() {
                "help" | "--help" | "-h" => command = Command::Help,
                "daemon" => {
                    i += 1;
                    command = if i < args.len() {
                        match args[i].as_str() {
                            "start" => Command::Daemon(Daemon::Start),
                            "kill" => Command::Daemon(Daemon::Kill),
                            "logs" => Command::Daemon(Daemon::Logs),
                            _ => Command::Unknown(unknown_cmd(&args)),
                        }
                    } else {
                        Command::Daemon(Daemon::Info)
                    };
                }
                "kill" => command = Command::Kill,
                "config" => command = Command::Config,
                a if a.starts_with("--") => {
                    let rest = &a[2..];
                    if let Some((key, value)) = rest.split_once('=') {
                        overrides.push((key.to_string(), Some(value.to_string())));
                    } else {
                        overrides.push((rest.to_string(), None));
                    }
                }
                _ => command = Command::Unknown(unknown_cmd(&args)),
            }
            i += 1;
        }

        Cli { command, overrides }
    }

    pub fn unknown(&self, cmd: &str) -> String {
        format!(
            "
command `{}` doesn't exist

use `ramo help` to see usage
        ",
            cmd
        )
    }

    pub fn help(&self) -> String {
        format!(
            "

ramo                Open picker (starts daemon if not running yet)

ramo daemon         Print current daemon info
ramo daemon start   Start a daemon
ramo daemon kill    Kill running daemon
ramo daemon logs    Tail daemon logs

ramo kill           Alias for daemon kill

ramo config         Print config path and contents

ramo help           Print this help

Flags override config keys (e.g. --path=/custom/path)",
        )
    }
}

fn unknown_cmd(args: &[String]) -> String {
    let prog = std::path::Path::new(&args[0])
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("ramo");
    let rest = &args[1..];
    if rest.is_empty() {
        prog.to_string()
    } else {
        format!("{} {}", prog, rest.join(" "))
    }
}
