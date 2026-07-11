//! lucy — worktree + agent 编排桌面工具入口(bin)。
//!
//! 仅调 [`lucy_app::run`]。所有逻辑在 lib(`src/lib.rs` + `workspace`/`terminal_view`
//! 等模块),供集成测试(`tests/`)导入。

use lucy_app::logging::{self, StartupAction, StartupOptions};

fn main() {
    if let Err(error) = try_main() {
        eprintln!("lucy: {error}");
        std::process::exit(2);
    }
}

fn try_main() -> Result<(), String> {
    let options = StartupOptions::parse(std::env::args_os().skip(1))?;
    let debug_log_path = options
        .debug_log
        .map(|path| path.unwrap_or_else(logging::default_debug_log_path));

    logging::init(debug_log_path.as_deref()).map_err(|error| format!("初始化日志失败: {error}"))?;
    logging::install_panic_logger();

    if let Some(path) = &debug_log_path {
        log::info!(target: "lucy_app", "debug logging enabled: {}", path.display());
    }

    match options.action {
        StartupAction::Run => lucy_app::run(),
        StartupAction::Help => print_help(),
        StartupAction::Version => println!("lucy {}", env!("CARGO_PKG_VERSION")),
    }
    Ok(())
}

fn print_help() {
    println!(
        "LucyMind - Git worktree + AI agent desktop app\n\n\
         Usage: lucy [OPTIONS]\n\n\
         Options:\n  \
           --debug-log [PATH]  Append debug logs to PATH (or the platform default)\n  \
         -h, --help              Print help\n  \
         -V, --version           Print version"
    );
}
