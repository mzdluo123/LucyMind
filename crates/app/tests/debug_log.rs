//! End-to-end coverage for persistent debug logging through the real binary.

use std::process::Command;

#[test]
fn debug_log_flag_creates_a_log_file_during_startup() {
    let temp = tempfile::tempdir().unwrap();
    let log_path = temp.path().join("nested").join("lucy-debug.log");

    let output = Command::new(env!("CARGO_BIN_EXE_lucy"))
        .arg(format!("--debug-log={}", log_path.display()))
        .arg("--version")
        .output()
        .expect("run lucy binary");

    assert!(
        output.status.success(),
        "lucy failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("lucy "));

    let log = std::fs::read_to_string(&log_path).expect("debug log should exist");
    assert!(
        log.contains("debug logging enabled:"),
        "startup record missing from log: {log}"
    );
    assert!(log.contains(&log_path.display().to_string()));
}

#[test]
fn debug_log_flag_without_path_uses_the_platform_log_directory() {
    let temp = tempfile::tempdir().unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_lucy"));
    command.arg("--debug-log").arg("--version");

    #[cfg(target_os = "macos")]
    let log_path = {
        command.env("HOME", temp.path());
        temp.path()
            .join("Library")
            .join("Logs")
            .join("LucyMind")
            .join("lucy.log")
    };

    #[cfg(target_os = "windows")]
    let log_path = {
        command.env("LOCALAPPDATA", temp.path());
        temp.path().join("LucyMind").join("logs").join("lucy.log")
    };

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let log_path = {
        command.env("XDG_STATE_HOME", temp.path());
        temp.path().join("lucymind").join("lucy.log")
    };

    let output = command.output().expect("run lucy binary");
    assert!(
        output.status.success(),
        "lucy failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let log = std::fs::read_to_string(&log_path).expect("default debug log should exist");
    assert!(log.contains("debug logging enabled:"));
}
