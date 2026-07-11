//! End-to-end verification that UI errors also reach the process logger.

use std::sync::Mutex;

use gpui::TestAppContext;
use log::{Level, LevelFilter, Log, Metadata, Record};

use common::{build_workspace, shutdown_workspace};

mod common;

static LOGGER: CapturingLogger = CapturingLogger {
    messages: Mutex::new(Vec::new()),
};

struct CapturingLogger {
    messages: Mutex<Vec<String>>,
}

impl Log for CapturingLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= Level::Error
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            self.messages
                .lock()
                .unwrap()
                .push(record.args().to_string());
        }
    }

    fn flush(&self) {}
}

#[gpui::test]
async fn workspace_error_status_is_emitted_to_the_process_logger(cx: &mut TestAppContext) {
    log::set_logger(&LOGGER).expect("test logger should initialize once");
    log::set_max_level(LevelFilter::Error);

    let (workspace, _window) = build_workspace(cx, None);
    cx.run_until_parked();

    cx.update(|cx| {
        workspace.update(cx, |view, cx| view.new_worktree_for_test(cx));
    });

    let messages = LOGGER.messages.lock().unwrap().clone();
    assert!(
        messages
            .iter()
            .any(|message| message.contains("请先打开一个 git 仓库")),
        "UI error did not reach logger: {messages:?}"
    );

    shutdown_workspace(cx, &workspace);
}
