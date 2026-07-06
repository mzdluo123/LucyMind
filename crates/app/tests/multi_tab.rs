//! 多 tab 终端 + agent 按钮发命令:tab CRUD、切换、关闭、OSC 标题、send_agent_command。

use std::time::Duration;

use gpui::TestAppContext;

use common::{
    build_workspace, shutdown_workspace, temp_repo, temp_repo_with_agent, wait_for,
    wait_for_shell_ready,
};
use lucy_app::workspace::ShellKind;

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
        workspace.update(cx, |v, cx| {
            v.new_terminal_tab_for_test(ShellKind::Default, cx)
        });
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
        workspace.update(cx, |v, cx| {
            v.new_terminal_tab_for_test(ShellKind::Default, cx)
        });
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
        workspace.update(cx, |v, cx| {
            v.new_terminal_tab_for_test(ShellKind::Default, cx)
        });
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
        workspace.update(cx, |v, cx| {
            v.new_terminal_tab_for_test(ShellKind::Default, cx)
        });
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
        workspace.update(cx, |v, cx| {
            v.new_terminal_tab_for_test(ShellKind::Default, cx)
        });
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

    // 等 shell 就绪后再发 OSC 序列(避免命令在 shell 启动前被缓冲,CI 慢机器上更可靠)。
    wait_for_shell_ready(cx, &workspace, &wt_path, Duration::from_secs(15));

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

    // 等 shell 就绪(PTY spawn 有延迟,CI 机器负载高时可能 >500ms)。
    wait_for_shell_ready(cx, &workspace, &wt_path, Duration::from_secs(15));

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

#[gpui::test]
async fn switch_tab_out_of_bounds_is_noop(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let _wt_path = create_worktree(cx, &workspace);

    // 建第二个 tab(active=1)。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| {
            v.new_terminal_tab_for_test(ShellKind::Default, cx)
        });
    });
    cx.run_until_parked();

    // switch_tab(99) 越界 → no-op,active 仍是 1。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.switch_tab_for_test(99, cx));
    });
    cx.run_until_parked();
    assert_eq!(
        cx.read(|cx| workspace.read(cx).active_tab_index()),
        Some(1),
        "out-of-bounds switch should be no-op"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn close_tab_out_of_bounds_is_noop(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace);

    // close_tab(99) 越界 → no-op,tab 数不变。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.close_tab_for_test(99, cx));
    });
    cx.run_until_parked();
    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_path)),
        1,
        "out-of-bounds close should be no-op"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn new_terminal_tab_noop_without_active(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    // 无 active worktree → new_terminal_tab 应 no-op,不 panic。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| {
            v.new_terminal_tab_for_test(ShellKind::Default, cx)
        });
    });
    cx.run_until_parked();
    assert!(
        cx.read(|cx| workspace.read(cx).active_tab_index())
            .is_none(),
        "no tab should exist without active worktree"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn send_agent_command_unknown_sets_error_status(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let _wt_path = create_worktree(cx, &workspace);

    // 发不存在的 agent 名 → 设置错误状态。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| {
            v.send_agent_command_for_test("nonexistent_agent", cx)
        });
    });
    cx.run_until_parked();

    let status = cx.read(|cx| workspace.read(cx).current_status().map(|s| s.to_string()));
    assert!(
        status
            .as_deref()
            .is_some_and(|s| s.contains("nonexistent_agent")),
        "should set error status for unknown agent, got: {status:?}"
    );
    assert!(
        cx.read(|cx| workspace.read(cx).status_is_error()),
        "status should be marked as error"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn open_worktree_creates_first_tab(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    // 先建一个 worktree(会创建 group + tab)。
    let wt_a = create_worktree(cx, &workspace);
    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_a)),
        1,
        "wt A should have 1 tab"
    );

    // 切到主仓(active 变成主仓路径,主仓无 group)。
    let main_path = cx.read(|cx| workspace.read(cx).worktree_paths()[0].clone());
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.open_worktree_for_test(main_path.clone(), cx));
    });
    cx.run_until_parked();
    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&main_path)),
        1,
        "open_worktree on main repo (no group) should create first tab"
    );

    // 切回 wt_a → tab 仍在。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.open_worktree_for_test(wt_a.clone(), cx));
    });
    cx.run_until_parked();
    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_a)),
        1,
        "wt A tab should be preserved"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn close_tab_then_new_tab_recreates_group(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace);

    // 关最后一个 tab → group 被移除。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.close_tab_for_test(0, cx));
    });
    cx.run_until_parked();
    assert!(
        !cx.update(|cx| workspace.update(cx, |v, _| v.terminals_contains(&wt_path))),
        "group should be removed after closing last tab"
    );

    // new_terminal_tab 应重新创建 group(active 仍是 wt_path)。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| {
            v.new_terminal_tab_for_test(ShellKind::Default, cx)
        });
    });
    cx.run_until_parked();
    assert!(
        cx.update(|cx| workspace.update(cx, |v, _| v.terminals_contains(&wt_path))),
        "group should be recreated by new_terminal_tab"
    );
    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_path)),
        1,
        "should have 1 tab after recreating group"
    );

    shutdown_workspace(cx, &workspace);
}

// ---- 10. UI 状态测试:launcher menu + ShellKind + launch_agent ----

#[gpui::test]
async fn launcher_menu_closed_by_default(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    assert!(
        !cx.read(|cx| workspace.read(cx).launcher_menu_open_for_test()),
        "launcher menu should be closed by default"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn launcher_menu_open_close(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    cx.update(|cx| {
        workspace.update(cx, |v, _| v.set_launcher_menu_open_for_test(true));
    });
    assert!(
        cx.read(|cx| workspace.read(cx).launcher_menu_open_for_test()),
        "menu should be open after set true"
    );

    cx.update(|cx| {
        workspace.update(cx, |v, _| v.set_launcher_menu_open_for_test(false));
    });
    assert!(
        !cx.read(|cx| workspace.read(cx).launcher_menu_open_for_test()),
        "menu should be closed after set false"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn new_terminal_tab_default_shell(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace);
    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_path)),
        1,
        "initial tab count should be 1"
    );

    cx.update(|cx| {
        workspace.update(cx, |v, cx| {
            v.new_terminal_tab_for_test(ShellKind::Default, cx)
        });
    });
    cx.run_until_parked();

    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_path)),
        2,
        "tab count should be 2"
    );
    assert_eq!(
        cx.read(|cx| workspace.read(cx).active_tab_index()),
        Some(1),
        "active tab should be 1 (newly created)"
    );

    let title = cx.read(|cx| workspace.read(cx).tab_title_for_test(&wt_path));
    assert_eq!(title.as_deref(), Some("Shell"), "default shell title");

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn new_terminal_tab_multiple_shells(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace);

    for i in 0..3 {
        cx.update(|cx| {
            workspace.update(cx, |v, cx| {
                v.new_terminal_tab_for_test(ShellKind::Default, cx)
            });
        });
        cx.run_until_parked();
        assert_eq!(
            cx.read(|cx| workspace.read(cx).tab_count(&wt_path)),
            2 + i,
            "tab count after {} new tabs",
            i + 1
        );
    }

    assert!(
        cx.read(|cx| { workspace.read(cx).terminal_at(&wt_path).is_some() }),
        "active terminal should exist"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn new_terminal_tab_noop_without_active_shell(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    cx.update(|cx| {
        workspace.update(cx, |v, cx| {
            v.new_terminal_tab_for_test(ShellKind::Default, cx)
        });
    });
    cx.run_until_parked();
    assert!(
        cx.read(|cx| workspace.read(cx).active_tab_index())
            .is_none(),
        "no tab should exist without active worktree"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn launch_agent_creates_new_tab(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo_with_agent();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace);
    let initial_count = cx.read(|cx| workspace.read(cx).tab_count(&wt_path));

    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.launch_agent_for_test("test", cx));
    });
    cx.run_until_parked();

    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_path)),
        initial_count + 1,
        "launch_agent should create a new tab"
    );
    assert_eq!(
        cx.read(|cx| workspace.read(cx).active_tab_index()),
        Some(initial_count),
        "active tab should be the newly created one"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn launch_agent_unknown_sets_error(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo_with_agent();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace);
    let initial_count = cx.read(|cx| workspace.read(cx).tab_count(&wt_path));

    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.launch_agent_for_test("nonexistent", cx));
    });
    cx.run_until_parked();

    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_path)),
        initial_count + 1,
        "tab should be created even for unknown agent"
    );
    assert!(
        cx.read(|cx| workspace.read(cx).status_is_error()),
        "should set error status for unknown agent"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn launch_agent_noop_without_active(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo_with_agent();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.launch_agent_for_test("test", cx));
    });
    cx.run_until_parked();
    assert!(
        cx.read(|cx| workspace.read(cx).active_tab_index())
            .is_none(),
        "no tab should exist without active worktree"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn multiple_launch_agent_creates_separate_tabs(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo_with_agent();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace);
    let initial_count = cx.read(|cx| workspace.read(cx).tab_count(&wt_path));

    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.launch_agent_for_test("test", cx));
    });
    cx.run_until_parked();
    let id1 = cx.read(|cx| {
        workspace
            .read(cx)
            .terminal_at(&wt_path)
            .map(|t| t.entity_id())
    });

    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.launch_agent_for_test("test", cx));
    });
    cx.run_until_parked();
    let id2 = cx.read(|cx| {
        workspace
            .read(cx)
            .terminal_at(&wt_path)
            .map(|t| t.entity_id())
    });

    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_path)),
        initial_count + 2,
        "should have 2 new tabs from 2 launch_agent calls"
    );
    assert_ne!(id1, id2, "each launch_agent should create a separate tab");

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn close_tab_after_launch_agent(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo_with_agent();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace);
    let initial_count = cx.read(|cx| workspace.read(cx).tab_count(&wt_path));

    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.launch_agent_for_test("test", cx));
    });
    cx.run_until_parked();
    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_path)),
        initial_count + 1,
    );

    let active = cx.read(|cx| workspace.read(cx).active_tab_index()).unwrap();
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.close_tab_for_test(active, cx));
    });
    cx.run_until_parked();
    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_path)),
        initial_count,
        "tab count should decrease after close"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn switch_worktree_does_not_affect_launcher_menu(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_a = create_worktree(cx, &workspace);
    let wt_b = create_worktree(cx, &workspace);

    cx.update(|cx| {
        workspace.update(cx, |v, _| v.set_launcher_menu_open_for_test(true));
    });
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.open_worktree_for_test(wt_b.clone(), cx));
    });
    cx.run_until_parked();
    assert!(
        cx.read(|cx| workspace.read(cx).launcher_menu_open_for_test()),
        "menu should stay open after switching worktree"
    );

    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.open_worktree_for_test(wt_a.clone(), cx));
    });
    cx.run_until_parked();
    assert!(
        cx.read(|cx| workspace.read(cx).launcher_menu_open_for_test()),
        "menu should stay open after switching back"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn close_last_tab_does_not_crash_launcher(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace);

    cx.update(|cx| {
        workspace.update(cx, |v, _| v.set_launcher_menu_open_for_test(true));
    });
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.close_tab_for_test(0, cx));
    });
    cx.run_until_parked();

    assert!(
        cx.read(|cx| workspace.read(cx).launcher_menu_open_for_test()),
        "menu state should survive close_last_tab"
    );

    cx.update(|cx| {
        workspace.update(cx, |v, cx| {
            v.new_terminal_tab_for_test(ShellKind::Default, cx)
        });
    });
    cx.run_until_parked();
    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_path)),
        1,
        "group should be recreated"
    );

    shutdown_workspace(cx, &workspace);
}

// ---- 11. 集成测试:ShellKind + launch_agent 端到端 ----

#[gpui::test]
async fn shell_kind_default_spawns_shell(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace);

    // 等 shell 就绪后再发命令(CI 机器负载高时 shell spawn 可能 >500ms)。
    wait_for_shell_ready(cx, &workspace, &wt_path, Duration::from_secs(15));

    let term = cx.update(|cx| workspace.update(cx, |v, _| v.terminal_at(&wt_path).cloned()));
    if let Some(term) = term {
        cx.update(|cx| {
            term.update(cx, |t, _| t.send_text("echo MARKER_SHELL\r"));
        });
    }

    wait_for(
        cx,
        |cx| {
            let term =
                cx.update(|cx| workspace.update(cx, |v, _| v.terminal_at(&wt_path).cloned()));
            term.is_some_and(|t| {
                cx.update(|cx| t.update(cx, |tv, _| tv.poll_events_for_test()));
                cx.read(|cx| t.read(cx).snapshot_text().contains("MARKER_SHELL"))
            })
        },
        Duration::from_secs(15),
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn shell_kind_label_as_fallback_title(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace);

    let title = cx.read(|cx| workspace.read(cx).tab_title_for_test(&wt_path));
    assert_eq!(title.as_deref(), Some("Shell"), "Default shell label");

    cx.update(|cx| {
        workspace.update(cx, |v, cx| {
            v.new_terminal_tab_for_test(ShellKind::Default, cx)
        });
    });
    cx.run_until_parked();

    let title = cx.read(|cx| workspace.read(cx).tab_title_for_test(&wt_path));
    assert_eq!(
        title.as_deref(),
        Some("Shell"),
        "Default shell label on new tab"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn tab_flex_shrink_many_tabs(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace);

    for _ in 0..9 {
        cx.update(|cx| {
            workspace.update(cx, |v, cx| {
                v.new_terminal_tab_for_test(ShellKind::Default, cx)
            });
        });
        cx.run_until_parked();
    }

    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_path)),
        10,
        "should have 10 tabs"
    );

    let mut ids = Vec::new();
    for i in 0..10 {
        cx.update(|cx| {
            workspace.update(cx, |v, cx| v.switch_tab_for_test(i, cx));
        });
        cx.run_until_parked();
        let id = cx.read(|cx| {
            workspace
                .read(cx)
                .terminal_at(&wt_path)
                .map(|t| t.entity_id())
        });
        assert!(id.is_some(), "tab {i} should have a terminal");
        ids.push(id.unwrap());
    }
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 10, "all 10 terminals should be distinct");

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn launch_agent_sends_command_to_new_tab(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo_with_agent();
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

    // 等 shell 就绪后再发 agent 命令(CI 机器负载高时 shell spawn 可能 >500ms)。
    wait_for_shell_ready(cx, &workspace, &wt_path, Duration::from_secs(15));

    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.launch_agent_for_test("test", cx));
    });

    wait_for(
        cx,
        |cx| {
            let term =
                cx.update(|cx| workspace.update(cx, |v, _| v.terminal_at(&wt_path).cloned()));
            term.is_some_and(|t| {
                cx.update(|cx| t.update(cx, |tv, _| tv.poll_events_for_test()));
                cx.read(|cx| t.read(cx).snapshot_text().contains("MARKER_READY"))
            })
        },
        Duration::from_secs(30),
    );

    shutdown_workspace(cx, &workspace);
}

// ---- 12. 集成测试:launcher menu 交互 ----

#[gpui::test]
async fn launcher_menu_state_is_workspace_level(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_a = create_worktree(cx, &workspace);
    let wt_b = create_worktree(cx, &workspace);

    cx.update(|cx| {
        workspace.update(cx, |v, _| v.set_launcher_menu_open_for_test(true));
    });
    assert!(
        cx.read(|cx| workspace.read(cx).launcher_menu_open_for_test()),
        "menu should be open"
    );

    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.open_worktree_for_test(wt_b.clone(), cx));
    });
    cx.run_until_parked();
    assert!(
        cx.read(|cx| workspace.read(cx).launcher_menu_open_for_test()),
        "menu should stay open after switching to wt B"
    );

    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.open_worktree_for_test(wt_a.clone(), cx));
    });
    cx.run_until_parked();
    assert!(
        cx.read(|cx| workspace.read(cx).launcher_menu_open_for_test()),
        "menu should stay open after switching back to wt A"
    );

    shutdown_workspace(cx, &workspace);
}

// ---- 15. 测试:reveal_in_file_manager ----

#[gpui::test]
async fn reveal_in_file_manager_noop_without_active(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    // 无 active worktree → no-op,不 panic。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.reveal_in_file_manager_for_test(cx));
    });

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn reveal_in_file_manager_with_active(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let _wt_path = create_worktree(cx, &workspace);

    // 有 active worktree → spawn 系统命令(不阻塞),不 panic。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.reveal_in_file_manager_for_test(cx));
    });

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn reveal_in_file_manager_after_close(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace);

    // 关掉最后一个 tab(group 移除,active 仍指向 wt_path)。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.close_tab_for_test(0, cx));
    });
    cx.run_until_parked();

    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_path)),
        0,
        "group should be removed after closing last tab"
    );

    // reveal_in_file_manager 仍可用(active 仍指向 wt_path),不 panic。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.reveal_in_file_manager_for_test(cx));
    });

    shutdown_workspace(cx, &workspace);
}

// ---- 16. 测试:tab 溢出 —— 大量 tab 不撑爆 tab 栏 ----

/// 大量 tab(20+)不应撑爆 tab 栏:tab_count 正确、切换正常、launcher 菜单正常、
/// 关闭后 tab_count 递减。验证 `overflow_hidden` + `overflow_x_scroll` 布局修复
/// 不影响 tab CRUD 状态机。
#[gpui::test]
async fn tab_overflow_many_tabs(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let wt_path = create_worktree(cx, &workspace);

    // 初始:1 个 shell tab(新建 worktree 自带)。
    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_path)),
        1,
        "should start with 1 tab"
    );

    // 再加 19 个 tab → 共 20 个(远超 tab 栏宽度,触发横向滚动)。
    for _ in 0..19 {
        cx.update(|cx| {
            workspace.update(cx, |v, cx| {
                v.new_terminal_tab_for_test(ShellKind::Default, cx)
            });
        });
        cx.run_until_parked();
    }
    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_path)),
        20,
        "should have 20 tabs after adding 19"
    );

    // 切换到最后一个 tab(索引 19),验证 active_tab_index 正确。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.switch_tab_for_test(19, cx));
    });
    cx.run_until_parked();
    assert_eq!(
        cx.read(|cx| workspace.read(cx).active_tab_index()),
        Some(19),
        "active tab should be 19 after switch"
    );

    // 切回第一个 tab。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.switch_tab_for_test(0, cx));
    });
    cx.run_until_parked();
    assert_eq!(
        cx.read(|cx| workspace.read(cx).active_tab_index()),
        Some(0),
        "active tab should be 0 after switch back"
    );

    // launcher 菜单在大量 tab 下仍能正常开关。
    cx.update(|cx| {
        workspace.update(cx, |v, _| v.set_launcher_menu_open_for_test(true));
    });
    cx.run_until_parked();
    assert!(
        cx.read(|cx| workspace.read(cx).launcher_menu_open_for_test()),
        "launcher menu should open with 20 tabs"
    );
    cx.update(|cx| {
        workspace.update(cx, |v, _| v.set_launcher_menu_open_for_test(false));
    });
    cx.run_until_parked();
    assert!(
        !cx.read(|cx| workspace.read(cx).launcher_menu_open_for_test()),
        "launcher menu should close with 20 tabs"
    );

    // 关闭中间的 tab(索引 10),验证 tab_count 递减且 active 调整。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.close_tab_for_test(10, cx));
    });
    cx.run_until_parked();
    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_path)),
        19,
        "should have 19 tabs after closing one"
    );

    // 逐个关闭剩余 tab,验证不 panic。
    for _ in 0..19 {
        cx.update(|cx| {
            workspace.update(cx, |v, cx| v.close_tab_for_test(0, cx));
        });
        cx.run_until_parked();
    }
    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&wt_path)),
        0,
        "group should be removed after closing all tabs"
    );

    shutdown_workspace(cx, &workspace);
}
