//! Keeps a debug log of the current and the previous session on disk, so a
//! playback bug leaves a trace even when Canto was opened from the app menu
//! (where nothing reads its output).
//!
//! Files: `~/.local/state/canto/canto.log` (this session) and `canto.prev.log`
//! (the one before). `RUST_LOG` still wins over the default filter, and when it
//! is set the log also goes to stderr, as before.

use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Enough to follow playback and device changes without auth internals
/// (`canto_auth` stays at warn, so tokens never reach the file).
const DEFAULT_FILTER: &str = "canto=debug,canto_api=debug,warn";
/// A long session starts the file over past this size.
const MAX_BYTES: u64 = 20 * 1024 * 1024;
const CURRENT: &str = "canto.log";
const PREVIOUS: &str = "canto.prev.log";

pub fn log_dir() -> PathBuf {
    glib::user_state_dir().join("canto")
}

/// Starts the logger. Call once, before anything logs.
pub fn init() {
    let from_env = std::env::var_os("RUST_LOG").is_some();
    let mut builder = env_logger::Builder::new();
    if from_env {
        builder.parse_env("RUST_LOG");
    } else {
        builder.parse_filters(DEFAULT_FILTER);
    }
    builder
        .write_style(env_logger::WriteStyle::Never)
        .target(env_logger::Target::Pipe(Box::new(SessionLog::open(
            &log_dir(),
            MAX_BYTES,
            from_env,
        ))))
        .init();

    // A crash ends up in the file too, not only on a terminal nobody sees
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        error!("panic: {info}");
        default_hook(info);
    }));
}

struct SessionLog {
    path: PathBuf,
    file: Option<File>,
    written: u64,
    max_bytes: u64,
    echo: bool,
}

impl SessionLog {
    fn open(dir: &Path, max_bytes: u64, echo: bool) -> Self {
        let path = dir.join(CURRENT);
        let file = std::fs::create_dir_all(dir)
            .ok()
            .and_then(|_| start_file(&path));
        Self {
            path,
            file,
            written: 0,
            max_bytes,
            echo,
        }
    }
}

/// Moves the current file to the previous slot and opens a fresh one.
fn start_file(path: &Path) -> Option<File> {
    if path.exists() {
        let _ = std::fs::rename(path, path.with_file_name(PREVIOUS));
    }
    File::create(path).ok()
}

impl Write for SessionLog {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.echo {
            let _ = io::stderr().write_all(buf);
        }
        if self.file.is_some() && self.written + buf.len() as u64 > self.max_bytes {
            self.file = start_file(&self.path);
            self.written = 0;
        }
        if let Some(file) = self.file.as_mut() {
            if file.write_all(buf).is_ok() {
                self.written += buf.len() as u64;
            }
        }
        // Never fail the caller: a full disk must not break the app
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(file) = self.file.as_mut() {
            let _ = file.flush();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("canto-session-log-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn read(dir: &Path, name: &str) -> String {
        std::fs::read_to_string(dir.join(name)).unwrap_or_default()
    }

    #[test]
    fn keeps_the_previous_session() {
        let dir = scratch("prev");
        let mut first = SessionLog::open(&dir, MAX_BYTES, false);
        first.write_all(b"first session\n").unwrap();
        drop(first);
        let mut second = SessionLog::open(&dir, MAX_BYTES, false);
        second.write_all(b"second session\n").unwrap();
        assert_eq!(read(&dir, CURRENT), "second session\n");
        assert_eq!(read(&dir, PREVIOUS), "first session\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn long_session_starts_over() {
        let dir = scratch("cap");
        let mut log = SessionLog::open(&dir, 10, false);
        log.write_all(b"12345678\n").unwrap();
        log.write_all(b"abcdefgh\n").unwrap();
        assert_eq!(read(&dir, CURRENT), "abcdefgh\n");
        assert_eq!(read(&dir, PREVIOUS), "12345678\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_folder_is_created() {
        let dir = scratch("nested").join("a").join("b");
        let mut log = SessionLog::open(&dir, MAX_BYTES, false);
        log.write_all(b"ok\n").unwrap();
        assert_eq!(read(&dir, CURRENT), "ok\n");
        let _ = std::fs::remove_dir_all(dir.parent().unwrap().parent().unwrap());
    }
}
