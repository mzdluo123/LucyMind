//! Smoke 测试:验证 `#[gpui::test]` + `TestAppContext` + harness 在 Windows 上跑通。
//!
//! 这是 DE-RISK 步骤:确认 headless GPUI 测试基础设施可用,再写后续完整套件。

use gpui::TestAppContext;

use common::{build_workspace, shutdown_workspace, temp_repo};

mod common;

#[gpui::test]
async fn build_workspace_lists_main_worktree(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _window) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let count = cx.read(|cx| workspace.read(cx).worktree_count());
    assert!(
        count >= 1,
        "main worktree should be listed, got count={count}"
    );

    // active_path 在启动时是 None —— 没有终端被打开,用户需点 worktree 行或
    // 起 agent 才有 active。断言 repo 已加载即可。
    let repo_set = cx.read(|cx| workspace.read(cx).repo().map(|p| p.to_path_buf()));
    assert!(repo_set.is_some(), "repo should be set after startup");

    shutdown_workspace(cx, &workspace);
    cx.run_until_parked();
}

#[gpui::test]
async fn empty_state_when_no_candidate(cx: &mut TestAppContext) {
    // new_for_test(None) 不弹 prompt_for_paths(TestPlatform 未实现)。
    let (workspace, _window) = build_workspace(cx, None);
    cx.run_until_parked();

    let count = cx.read(|cx| workspace.read(cx).worktree_count());
    assert_eq!(count, 0, "no repo → no worktrees");

    let repo = cx.read(|cx| workspace.read(cx).repo().map(|p| p.to_path_buf()));
    assert!(repo.is_none(), "repo should be None in empty state");

    shutdown_workspace(cx, &workspace);
    cx.run_until_parked();
}
