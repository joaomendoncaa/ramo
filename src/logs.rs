use log::{LevelFilter, Log, Metadata, Record, SetLoggerError};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

pub fn state_dir() -> PathBuf {
    std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/state")
        })
        .join("ramo")
}

struct Logger {
    file: Option<Mutex<File>>,
    level: LevelFilter,
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let timestamp = {
            let dur = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            let secs = dur.as_secs();
            let ms = dur.subsec_millis();
            let total_mins = secs / 60;
            let h = (total_mins / 60) % 24;
            let m = total_mins % 60;
            let s = secs % 60;
            format!("{h:02}:{m:02}:{s:02}.{ms:03}")
        };

        if let Some(ref file) = self.file
            && let Ok(mut f) = file.lock()
        {
            let _ = writeln!(f, "{timestamp} [{}] {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

fn set_logger(logger: Logger) -> Result<(), SetLoggerError> {
    let level = logger.level;
    let logger: &'static Logger = Box::leak(Box::new(logger));
    log::set_logger(logger)?;
    log::set_max_level(level);
    Ok(())
}

pub fn init(name: &str) -> Result<(), SetLoggerError> {
    let dir = state_dir();
    std::fs::create_dir_all(&dir).ok();

    let path = dir.join(format!("{name}.log"));

    if let Ok(meta) = std::fs::metadata(&path)
        && meta.len() > 1024 * 1024
    {
        std::fs::write(&path, "").ok();
    }

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok();

    set_logger(Logger {
        file: file.map(Mutex::new),
        level: LevelFilter::Debug,
    })
}
