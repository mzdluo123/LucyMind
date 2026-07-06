//! 多 tab 终端 + agent 按钮发命令:tab CRUD、切换、关闭、OSC 标题、send_agent_command。

use std::time::Duration;

use gpui::TestAppContext;

use common::{build_workspace, shutdown_workspace, temp_repo, temp_repo_with_agent, wait_for};

mod common;

/// 辅助:建 worktree 并等 active_path 就绪,返回 worktree 路径。
fn create_worktree(
    cx: &mut TestAppContext,
    workspace: &gpui::Entity<lucy_app::workspace::WorkspaceView>,
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
async fn new_worktree_creates_shell_tab(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace);

    let (has_term, tab_count, active_idx) = cx.update(|cx| {
        workspace.update(cx, |v, _| {
            let has_term = v.terminals_contains(&wt_path);
            let tab_count = v.tab_count(&wt_path);
            let active_idx = v.active_tab_index();
            (has_term, tab_count, active_idx)
        })
    });
    assert!(has_term, "terminal should exist for new worktree");
    assert_eq!(tab_count, 1, "new worktree should have 1 shell tab");
    assert_eq!(active_idx, Some(0), "active tab should be 0");

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn new_tab_increments_tab_count(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace);
    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_path)),
        1,
        "initial tab count should be 1"
    );

    // 新建第二个 tab。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.new_terminal_tab_for_test(cx));
    });
    cx.run_until_parked();

    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_path)),
        2,
        "tab count should be 2 after new_terminal_tab"
    );
    assert_eq!(
        cx.read(|cx| workspace.read(cx).active_tab_index()),
        Some(1),
        "active tab should be 1 (newly created)"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn switch_tab_preserves_terminal(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace);

    // 建第二个 tab(active=1)。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.new_terminal_tab_for_test(cx));
    });
    cx.run_until_parked();

    // 记住 active tab(1)的终端 entity 指针。
    let term_idx1 = cx.read(|cx| {
        workspace
            .read(cx)
            .terminal_at(&wt_path)
            .map(|t| t.entity_id())
    });

    // 切到 tab 0。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.switch_tab_for_test(0, cx));
    });
    cx.run_until_parked();
    assert_eq!(
        cx.read(|cx| workspace.read(cx).active_tab_index()),
        Some(0),
        "active tab should be 0 after switch"
    );

    // tab 0 的终端应该不同于 tab 1。
    let term_idx0 = cx.read(|cx| {
        workspace
            .read(cx)
            .terminal_at(&wt_path)
            .map(|t| t.entity_id())
    });
    assert_ne!(
        term_idx0, term_idx1,
        "switching tabs should change the active terminal"
    );

    // 切回 tab 1,终端应该和之前一样。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.switch_tab_for_test(1, cx));
    });
    cx.run_until_parked();
    let term_idx1_again = cx.read(|cx| {
        workspace
            .read(cx)
            .terminal_at(&wt_path)
            .map(|t| t.entity_id())
    });
    assert_eq!(
        term_idx1, term_idx1_again,
        "switching back should restore the same terminal"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn close_non_active_tab(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace);

    // 建第二个 tab(active=1)。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.new_terminal_tab_for_test(cx));
    });
    cx.run_until_parked();
    let term_idx1 = cx.read(|cx| {
        workspace
            .read(cx)
            .terminal_at(&wt_path)
            .map(|t| t.entity_id())
    });

    // 关 tab 0(非 active)。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.close_tab_for_test(0, cx));
    });
    cx.run_until_parked();

    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_path)),
        1,
        "tab count should be 1 after close"
    );
    assert_eq!(
        cx.read(|cx| workspace.read(cx).active_tab_index()),
        Some(0),
        "active tab should be 0 (was 1, shifted down)"
    );

    // 剩余的 tab 应该是原来的 tab 1(现在 index 0)。
    let term_remaining = cx.read(|cx| {
        workspace
            .read(cx)
            .terminal_at(&wt_path)
            .map(|t| t.entity_id())
    });
    assert_eq!(
        term_idx1, term_remaining,
        "remaining tab should be the original tab 1"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn close_active_tab_falls_back(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace);

    // 建第二个 tab(active=1)。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.new_terminal_tab_for_test(cx));
    });
    cx.run_until_parked();

    // 记住 tab 0 的终端。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.switch_tab_for_test(0, cx));
    });
    cx.run_until_parked();
    let term_idx0 = cx.read(|cx| {
        workspace
            .read(cx)
            .terminal_at(&wt_path)
            .map(|t| t.entity_id())
    });

    // 切到 tab 1 并关掉它(active tab)。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| {
            v.switch_tab_for_test(1, cx);
            v.close_tab_for_test(1, cx);
        });
    });
    cx.run_until_parked();

    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_path)),
        1,
        "tab count should be 1 after close"
    );
    assert_eq!(
        cx.read(|cx| workspace.read(cx).active_tab_index()),
        Some(0),
        "active tab should fall back to 0"
    );

    // 剩余的应该是原来的 tab 0。
    let term_remaining = cx.read(|cx| {
        workspace
            .read(cx)
            .terminal_at(&wt_path)
            .map(|t| t.entity_id())
    });
    assert_eq!(
        term_idx0, term_remaining,
        "remaining tab should be the original tab 0"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn close_last_tab_empties_group(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace);
    let count_before = cx.read(|cx| workspace.read(cx).worktree_count());

    // 关最后一个 tab。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.close_tab_for_test(0, cx));
    });
    cx.run_until_parked();

    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_path)),
        0,
        "tab count should be 0 after closing last tab"
    );
    assert!(
        !cx.update(|cx| workspace.update(cx, |v, _| v.terminals_contains(&wt_path))),
        "terminals should not contain the path after group is emptied"
    );
    // worktree 本身不删(close_tab 不触发 git remove)。
    assert_eq!(
        cx.read(|cx| workspace.read(cx).worktree_count()),
        count_before,
        "worktree count should not change (close_tab != close worktree)"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn close_tab_does_not_delete_worktree(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace);

    // 关 tab → worktree 仍在,无 pending_close。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.close_tab_for_test(0, cx));
    });
    cx.run_until_parked();

    assert!(
        !cx.read(|cx| workspace.read(cx).has_pending_close()),
        "close_tab should not trigger pending_close"
    );
    // worktree 仍在列表里。
    let paths: Vec<std::path::PathBuf> = cx.read(|cx| workspace.read(cx).worktree_paths());
    assert!(
        paths.iter().any(|p| p == &wt_path),
        "worktree should still be in the list"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn switch_worktree_preserves_active_tab(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    // 建 worktree A → 2 tab(active=1)。
    let wt_a = create_worktree(cx, &workspace);
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.new_terminal_tab_for_test(cx));
    });
    cx.run_until_parked();
    assert_eq!(
        cx.read(|cx| workspace.read(cx).active_tab_index()),
        Some(1),
        "wt A: active tab should be 1"
    );

    // 建 worktree B → 1 tab(active=0)。
    let wt_b = create_worktree(cx, &workspace);
    assert_eq!(
        cx.read(|cx| workspace.read(cx).active_tab_index()),
        Some(0),
        "wt B: active tab should be 0"
    );
    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_b)),
        1,
        "wt B: tab count should be 1"
    );

    // 切回 A → active tab 恢复为 1。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.open_worktree_for_test(wt_a.clone(), cx));
    });
    cx.run_until_parked();
    assert_eq!(
        cx.read(|cx| workspace.read(cx).active_tab_index()),
        Some(1),
        "wt A: active tab should be restored to 1"
    );
    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_a)),
        2,
        "wt A: tab count should still be 2"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn terminal_title_updates_from_osc(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace);

    // 给 shell 发 OSC 0/2 标题序列。
    // Windows 默认 shell 是 PowerShell,echo 不输出原始字节;用 [Console]::Write
    // 直接写 stdout。Unix 用 printf 输出转义序列。
    let osc_cmd = if cfg!(windows) {
        "[Console]::Write([char]27 + ']0;MARKER_TITLE' + [char]7)\r\n"
    } else {
        "printf '\\033]0;MARKER_TITLE\\007'\n"
    };
    let term = cx.update(|cx| workspace.update(cx, |v, _| v.terminal_at(&wt_path).cloned()));
    if let Some(term) = term {
        cx.update(|cx| {
            term.update(cx, |t, _| t.send_text(osc_cmd));
        });
    }

    // 轮询 title 是否被更新(PTY reader 线程读到 OSC 序列 → Term 解析 →
    // TermEvent::Title → drain_events → view.title)。
    // GPUI test mock 时钟不推进 16ms 轮询循环,需手动 poll_events_for_test。
    wait_for(
        cx,
        |cx| {
            let term =
                cx.update(|cx| workspace.update(cx, |v, _| v.terminal_at(&wt_path).cloned()));
            term.is_some_and(|t| {
                cx.update(|cx| t.update(cx, |tv, _| tv.poll_events_for_test()));
                cx.read(|cx| t.read(cx).title().is_some_and(|t| t == "MARKER_TITLE"))
            })
        },
        Duration::from_secs(15),
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn tab_title_falls_back_to_shell(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace);

    // shell 不发 OSC 0/2 → title() 返回 None(静态回退 "Shell")。
    let title = cx.read(|cx| {
        workspace
            .read(cx)
            .terminal_at(&wt_path)
            .and_then(|t| t.read(cx).title())
            .map(|s| s.to_string())
    });
    assert!(
        title.is_none(),
        "title should be None when shell hasn't sent OSC 0/2"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn send_agent_command_writes_to_shell(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo_with_agent();
    // 覆盖 .worktree.toml:agent test 输出 MARKER_READY。
    let toml = if cfg!(windows) {
        "[worktree]\nlocation = \"sibling\"\ndir = \"../{repo}-worktrees\"\n\
         [agents.test]\ncommand = \"cmd.exe\"\nargs = [\"/c\", \"echo MARKER_READY\"]\n"
    } else {
        "[worktree]\nlocation = \"sibling\"\ndir = \"../{repo}-worktrees\"\n\
         [agents.test]\ncommand = \"sh\"\nargs = [\"-c\", \"printf MARKER_READY\"]\n"
    };
    std::fs::write(repo.join(".worktree.toml"), toml).unwrap();

    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace);

    // 等 shell 就绪(PTY spawn 有延迟)。
    std::thread::sleep(Duration::from_millis(500));

    // 发 agent 命令到 shell。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.send_agent_command_for_test("test", cx));
    });

    // 轮询 snapshot 是否包含 MARKER_READY。
    wait_for(
        cx,
        |cx| {
            let term =
                cx.update(|cx| workspace.update(cx, |v, _| v.terminal_at(&wt_path).cloned()));
            term.is_some_and(|t| cx.read(|cx| t.read(cx).snapshot_text().contains("MARKER_READY")))
        },
        Duration::from_secs(30),
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn send_agent_command_noop_without_terminal(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    // 无 active worktree / terminal → send_agent_command 应 no-op,不 panic。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.send_agent_command_for_test("claude", cx));
    });

    shutdown_workspace(cx, &workspace);
}
