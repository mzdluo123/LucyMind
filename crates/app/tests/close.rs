//! 关闭 worktree 流程:干净/脏/确认/取消/主仓守卫。

use std::time::Duration;

use gpui::TestAppContext;

use common::{
    build_workspace, shutdown_workspace, temp_repo_with_agent, wait_for, wait_for_shell_ready,
};

mod common;

/// 造一个 worktree 并在 workspace 里打开(用 fake agent),返回 worktree 路径。
fn create_worktree(
    cx: &mut TestAppContext,
    workspace: &gpui::Entity<lucy_app::workspace::WorkspaceView>,
    _repo: &std::path::Path,
) -> std::path::PathBuf {
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.new_worktree_for_test(cx));
    });
    wait_for(
        cx,
        |cx| cx.update(|cx| workspace.update(cx, |v, _| v.active_path().is_some())),
        Duration::from_secs(15),
    );
    cx.update(|cx| workspace.update(cx, |v, _| v.active_path().map(|p| p.to_path_buf()).unwrap()))
}

#[gpui::test]
async fn clean_worktree_closes_without_confirmation(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo_with_agent();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace, &repo);
    let count_before = cx.read(|cx| workspace.read(cx).worktree_count());

    // 干净 worktree → request_close 直接关(无 pending_close)。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| {
            v.request_close_for_test(wt_path.clone(), "test-branch".into(), cx);
        });
    });
    // do_close 后台跑 git remove,等它完成。
    wait_for(
        cx,
        |cx| cx.read(|cx| workspace.read(cx).worktree_count()) < count_before,
        Duration::from_secs(15),
    );

    // terminals 不含该路径。
    assert!(
        !cx.update(|cx| workspace.update(cx, |v, _| v.terminals_contains(&wt_path))),
        "terminal should be removed after close"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn dirty_worktree_prompts_confirmation(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo_with_agent();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace, &repo);

    // 造脏:在 worktree 里写未提交文件。
    std::fs::write(wt_path.join("dirty.txt"), "uncommitted\n").unwrap();

    // request_close → 脏 → 弹确认(pending_close)。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| {
            v.request_close_for_test(wt_path.clone(), "test-branch".into(), cx);
        });
    });
    cx.run_until_parked();

    assert!(
        cx.read(|cx| workspace.read(cx).has_pending_close()),
        "dirty worktree should trigger pending_close"
    );

    // 确认 → 执行关闭。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.confirm_close_for_test(cx));
    });
    wait_for(
        cx,
        |cx| !cx.read(|cx| workspace.read(cx).has_pending_close()),
        Duration::from_secs(15),
    );
    assert!(
        !cx.update(|cx| workspace.update(cx, |v, _| v.terminals_contains(&wt_path))),
        "terminal should be removed after confirm_close"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn cancel_close_keeps_worktree(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo_with_agent();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace, &repo);
    wait_for_shell_ready(cx, &workspace, &wt_path, Duration::from_secs(15));
    std::fs::write(wt_path.join("dirty.txt"), "uncommitted\n").unwrap();

    cx.update(|cx| {
        workspace.update(cx, |v, cx| {
            v.request_close_for_test(wt_path.clone(), "test-branch".into(), cx);
        });
    });
    cx.run_until_parked();
    assert!(cx.read(|cx| workspace.read(cx).has_pending_close()));

    // 取消 → terminal 仍在。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.cancel_close_for_test(cx));
    });
    cx.run_until_parked();
    assert!(!cx.read(|cx| workspace.read(cx).has_pending_close()));
    assert!(
        cx.update(|cx| workspace.update(cx, |v, _| v.terminals_contains(&wt_path))),
        "terminal should remain after cancel_close"
    );

    // 不仅 terminal 实体还在,底层 PTY 也必须仍能执行命令。
    let terminal = cx
        .update(|cx| workspace.update(cx, |v, _| v.terminal_at(&wt_path).cloned()))
        .expect("terminal should remain after cancel_close");
    cx.update(|cx| {
        terminal.update(cx, |terminal, _| {
            terminal.send_text("echo CANCEL_CLOSE_TERMINAL_\"\"ALIVE\r")
        });
    });
    wait_for(
        cx,
        |cx| {
            cx.update(|cx| terminal.update(cx, |terminal, _| terminal.poll_events_for_test()));
            cx.read(|cx| {
                terminal
                    .read(cx)
                    .snapshot_text()
                    .contains("CANCEL_CLOSE_TERMINAL_ALIVE")
            })
        },
        Duration::from_secs(15),
    );

    shutdown_workspace(cx, &workspace);
}
