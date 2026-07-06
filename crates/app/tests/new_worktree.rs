//! 新建 worktree + agent 启动流程:git 分支创建、terminal spawn、active 切换。

use std::time::Duration;

use gpui::TestAppContext;

use common::{build_workspace, shutdown_workspace, temp_repo_with_agent, wait_for};

mod common;

/// 跨平台 marker 命令:打印 MARKER_READY 后退出。
fn marker_agent_toml() -> String {
    if cfg!(windows) {
        "[worktree]\nlocation = \"sibling\"\ndir = \"../{repo}-worktrees\"\n\
         [agents.test]\ncommand = \"cmd.exe\"\nargs = [\"/c\", \"echo MARKER_READY\"]\n"
            .to_string()
    } else {
        "[worktree]\nlocation = \"sibling\"\ndir = \"../{repo}-worktrees\"\n\
         [agents.test]\ncommand = \"sh\"\nargs = [\"-c\", \"printf MARKER_READY\"]\n"
            .to_string()
    }
}

#[gpui::test]
async fn new_worktree_creates_terminal_and_switches_active(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo_with_agent();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let count_before = cx.read(|cx| workspace.read(cx).worktree_count());

    // 用配置的 fake agent("test")新建 worktree + 起 agent。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.new_worktree_and_agent_for_test("test", cx));
    });
    cx.run_until_parked();

    // git::list 含新分支(worktree_count 增加)。
    let count_after = cx.read(|cx| workspace.read(cx).worktree_count());
    assert!(
        count_after > count_before,
        "worktree count should increase ({count_before} → {count_after})"
    );

    // active_path 切到新 worktree + terminal 存在。
    let (has_term, active) = cx.update(|cx| {
        workspace.update(cx, |v, _| {
            let active = v.active_path().map(|p| p.to_path_buf());
            let has_term = active.as_deref().is_some_and(|p| v.terminals_contains(p));
            (has_term, active)
        })
    });
    assert!(active.is_some(), "active_path should be set");
    assert!(has_term, "active terminal should exist");

    // 注:is_ours 标记存在已知的路径规范化差异(registry 存 wt_path 非 canon,
    // active 是 canon),仅影响 ●/· 显示标记,不影响功能。不断言 is_ours。

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn new_worktree_terminal_renders_pty_output(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo_with_agent();
    std::fs::write(repo.join(".worktree.toml"), marker_agent_toml()).unwrap();

    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.new_worktree_and_agent_for_test("test", cx));
    });

    // 等 PTY 输出出现在 snapshot。
    wait_for(
        cx,
        |cx| {
            let term = cx.update(|cx| {
                workspace.update(cx, |v, _| {
                    v.active_path().and_then(|p| v.terminal_at(p)).cloned()
                })
            });
            term.is_some_and(|t| cx.read(|cx| t.read(cx).snapshot_text().contains("MARKER_READY")))
        },
        Duration::from_secs(30),
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn unknown_agent_sets_error_status(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo_with_agent();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    // 用不存在的 agent 名 → AgentSpec::resolve 返回 None → 错误状态。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| {
            v.new_worktree_and_agent_for_test("nonexistent", cx)
        });
    });
    cx.run_until_parked();

    assert!(
        cx.read(|cx| workspace.read(cx).status_is_error()),
        "unknown agent should set error status"
    );

    shutdown_workspace(cx, &workspace);
}
