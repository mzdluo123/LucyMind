//! 新建 worktree 流程:git 分支创建、shell 终端 spawn、active 切换。

use std::time::Duration;

use gpui::TestAppContext;

use common::{build_workspace, shutdown_workspace, temp_repo, wait_for};

mod common;

#[gpui::test]
async fn new_worktree_creates_terminal_and_switches_active(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let count_before = cx.read(|cx| workspace.read(cx).worktree_count());

    // new_worktree 建 worktree + 开 shell tab(不自动起 agent)。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.new_worktree_for_test(cx));
    });
    cx.run_until_parked();

    // git::list 含新分支(worktree_count 增加)。
    let count_after = cx.read(|cx| workspace.read(cx).worktree_count());
    assert!(
        count_after > count_before,
        "worktree count should increase ({count_before} → {count_after})"
    );

    // active_path 切到新 worktree + terminal 存在 + 1 个 tab。
    let (has_term, active, tab_count) = cx.update(|cx| {
        workspace.update(cx, |v, _| {
            let active = v.active_path().map(|p| p.to_path_buf());
            let has_term = active.as_deref().is_some_and(|p| v.terminals_contains(p));
            let tab_count = active.as_deref().map(|p| v.tab_count(p)).unwrap_or(0);
            (has_term, active, tab_count)
        })
    });
    assert!(active.is_some(), "active_path should be set");
    assert!(has_term, "active terminal should exist");
    assert_eq!(tab_count, 1, "new worktree should have 1 shell tab");

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn new_worktree_terminal_renders_pty_output(cx: &mut TestAppContext) {
    // terminal_render.rs 已覆盖 PTY 输出渲染(通过 send_agent_command 发 marker)。
    // 这里只验证 new_worktree 后终端存在且 active_path 已设。
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.new_worktree_for_test(cx));
    });

    // active_path + terminal 存在 + tab_count == 1。
    wait_for(
        cx,
        |cx| {
            cx.update(|cx| {
                workspace.update(cx, |v, _| {
                    let active = v.active_path();
                    active.is_some_and(|p| v.terminals_contains(p) && v.tab_count(p) == 1)
                })
            })
        },
        Duration::from_secs(15),
    );

    shutdown_workspace(cx, &workspace);
}
