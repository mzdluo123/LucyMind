//! 终端键盘输入的 GPUI -> PTY 端到端回归测试。

use std::time::Duration;

use gpui::{Focusable, KeyDownEvent, Keystroke, TestAppContext};

use common::{build_workspace, shutdown_workspace, temp_repo, wait_for, wait_for_shell_ready};

mod common;

#[gpui::test]
async fn windows_space_keydown_and_text_commit_write_one_space(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, window) = build_workspace(cx, Some(repo.clone()));

    window.update(|_window, cx| {
        workspace.update(cx, |view, cx| view.open_worktree_for_test(repo.clone(), cx));
    });
    wait_for(
        window,
        |cx| cx.read(|cx| workspace.read(cx).active_path().is_some()),
        Duration::from_secs(10),
    );

    let active = window.update(|_window, cx| {
        workspace
            .read(cx)
            .active_path()
            .expect("active terminal path")
            .to_path_buf()
    });
    wait_for_shell_ready(window, &workspace, &active, Duration::from_secs(15));

    let terminal = window.update(|_window, cx| {
        workspace
            .read(cx)
            .terminal_at(&active)
            .expect("active terminal")
            .clone()
    });
    window.update(|window, cx| {
        window.focus(&terminal.focus_handle(cx));
        window.refresh();
    });
    window.run_until_parked();

    terminal.update(window, |terminal, _| {
        terminal.send_text("echo LUCY_SPACE_LEFT")
    });

    // GPUI/Windows 的 VK_SPACE KeyDown 没有 key_char,随后 WM_CHAR 再提交文本。
    // 原始 KeyDown 与模拟的文本提交组合在修复前会向 PTY 写入两个空格。
    window.simulate_event(KeyDownEvent {
        keystroke: Keystroke::parse("space").unwrap(),
        is_held: false,
    });
    window.simulate_keystrokes("space");

    terminal.update(window, |terminal, _| {
        terminal.send_text("LUCY_SPACE_RIGHT\r")
    });
    wait_for(
        window,
        |cx| {
            cx.read(|cx| {
                terminal
                    .read(cx)
                    .snapshot_text()
                    .contains("LUCY_SPACE_RIGHT")
            })
        },
        Duration::from_secs(10),
    );

    let output = window.update(|_window, cx| terminal.read(cx).snapshot_text());
    assert!(
        output.contains("LUCY_SPACE_LEFT LUCY_SPACE_RIGHT"),
        "terminal did not receive the expected command: {output:?}"
    );
    assert!(
        !output.contains("LUCY_SPACE_LEFT  LUCY_SPACE_RIGHT"),
        "space was written through both keydown and text commit: {output:?}"
    );

    shutdown_workspace(window, &workspace);
}
