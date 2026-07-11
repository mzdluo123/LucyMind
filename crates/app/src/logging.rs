//! Command-line options and persistent application logging.

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const DEFAULT_FILTER: &str = "warn,lucy_app=info,lucy_terminal=info,lucy_core=info";
const DEBUG_FILTER: &str = "warn,lucy_app=debug,lucy_terminal=debug,lucy_core=debug";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct StartupOptions {
    pub debug_log: Option<Option<PathBuf>>,
    pub action: StartupAction,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub enum StartupAction {
    #[default]
    Run,
    Help,
    Version,
}

impl StartupOptions {
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = args.into_iter().peekable();

        while let Some(arg) = args.next() {
            let text = arg.to_string_lossy();
            match text.as_ref() {
                "--debug-log" => {
                    let path = args
                        .next_if(|next| !next.to_string_lossy().starts_with('-'))
                        .map(PathBuf::from);
                    options.debug_log = Some(path);
                }
                "--help" | "-h" => options.action = StartupAction::Help,
                "--version" | "-V" => options.action = StartupAction::Version,
                _ if text.starts_with("--debug-log=") => {
                    let value = &text["--debug-log=".len()..];
                    if value.is_empty() {
                        return Err("--debug-log 的路径不能为空".into());
                    }
                    options.debug_log = Some(Some(PathBuf::from(value)));
                }
                _ => return Err(format!("未知参数: {text}")),
            }
        }

        Ok(options)
    }
}

pub fn default_debug_log_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join("Library")
            .join("Logs")
            .join("LucyMind")
            .join("lucy.log");
    }

    #[cfg(target_os = "windows")]
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local_app_data)
            .join("LucyMind")
            .join("logs")
            .join("lucy.log");
    }

    if let Some(state_home) = std::env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(state_home).join("lucymind").join("lucy.log");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("lucymind")
            .join("lucy.log");
    }
    PathBuf::from("lucy.log")
}

/// Initializes terminal logging and, when requested, mirrors it to an append-only file.
pub fn init(debug_log: Option<&Path>) -> io::Result<Option<PathBuf>> {
    let mut builder = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(
        if debug_log.is_some() {
            DEBUG_FILTER
        } else {
            DEFAULT_FILTER
        },
    ));

    let log_path = if let Some(path) = debug_log {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        builder.target(env_logger::Target::Pipe(Box::new(TeeWriter::new(file))));
        Some(path.to_path_buf())
    } else {
        None
    };

    builder
        .try_init()
        .map_err(|error| io::Error::new(io::ErrorKind::AlreadyExists, error))?;
    Ok(log_path)
}

/// Ensures an unexpected crash is present in the persistent log as well as stderr.
pub fn install_panic_logger() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        log::error!("未捕获 panic: {panic_info}");
        previous(panic_info);
    }));
}

struct TeeWriter {
    file: File,
}

impl TeeWriter {
    fn new(file: File) -> Self {
        Self { file }
    }
}

impl Write for TeeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        io::stderr().write_all(buf)?;
        self.file.write_all(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        io::stderr().flush()?;
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn parses_debug_log_with_default_path() {
        let options = StartupOptions::parse(args(&["--debug-log"])).unwrap();
        assert_eq!(options.debug_log, Some(None));
        assert_eq!(options.action, StartupAction::Run);
    }

    #[test]
    fn parses_debug_log_with_explicit_path() {
        let separated = StartupOptions::parse(args(&["--debug-log", "logs/debug.log"])).unwrap();
        let equals = StartupOptions::parse(args(&["--debug-log=logs/debug.log"])).unwrap();

        assert_eq!(
            separated.debug_log,
            Some(Some(PathBuf::from("logs/debug.log")))
        );
        assert_eq!(separated, equals);
    }

    #[test]
    fn rejects_unknown_arguments_and_empty_paths() {
        assert!(StartupOptions::parse(args(&["--wat"])).is_err());
        assert!(StartupOptions::parse(args(&["--debug-log="])).is_err());
    }
}
