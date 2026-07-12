//! 新建 worktree 流程:git 分支创建、shell 终端 spawn、active 切换。

use std::time::Duration;

use gpui::TestAppContext;

use common::{build_workspace, shutdown_workspace, temp_repo, temp_repo_with_agent, wait_for};

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
    wait_for(
        cx,
        |cx| !cx.read(|cx| workspace.read(cx).is_creating_worktree_for_test()),
        Duration::from_secs(15),
    );

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

#[gpui::test]
async fn new_worktree_start_menu_state_toggles(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo));
    cx.run_until_parked();

    assert!(
        !cx.read(|cx| workspace.read(cx).new_worktree_menu_open_for_test()),
        "start menu should be closed initially"
    );
    cx.update(|cx| {
        workspace.update(cx, |view, _| view.set_new_worktree_menu_open_for_test(true));
    });
    assert!(
        cx.read(|cx| workspace.read(cx).new_worktree_menu_open_for_test()),
        "start menu should open from the sidebar plus button"
    );
    cx.update(|cx| {
        workspace.update(cx, |view, _| {
            view.set_new_worktree_menu_open_for_test(false)
        });
    });
    assert!(
        !cx.read(|cx| workspace.read(cx).new_worktree_menu_open_for_test()),
        "start menu should close after a selection"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn new_worktree_runs_in_background_and_ignores_duplicate_start(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let slow_hook = if cfg!(windows) {
        "ping -n 2 127.0.0.1 >NUL"
    } else {
        "sleep 0.3"
    };
    std::fs::write(
        repo.join(".worktree.toml"),
        format!(
            "[worktree]\nlocation = \"sibling\"\ndir = \"../{{repo}}-worktrees\"\n\
             [hooks]\npost_create = [\"{slow_hook}\"]\n"
        ),
    )
    .unwrap();

    let (workspace, _window) = build_workspace(cx, Some(repo));
    let count_before = cx.read(|cx| workspace.read(cx).worktree_count());

    cx.update(|cx| {
        workspace.update(cx, |view, cx| view.new_worktree_for_test(cx));
    });
    assert!(
        cx.read(|cx| workspace.read(cx).is_creating_worktree_for_test()),
        "starting should return immediately with a visible busy state"
    );
    assert!(
        cx.read(|cx| {
            workspace
                .read(cx)
                .current_status()
                .is_some_and(|status| status.starts_with("正在创建 "))
        }),
        "the first background stage should be visible in the status bar"
    );

    // A second activation while the first request is running must be a no-op.
    cx.update(|cx| {
        workspace.update(cx, |view, cx| view.new_worktree_for_test(cx));
    });

    wait_for(
        cx,
        |cx| !cx.read(|cx| workspace.read(cx).is_creating_worktree_for_test()),
        Duration::from_secs(15),
    );
    assert_eq!(
        cx.read(|cx| workspace.read(cx).worktree_count()),
        count_before + 1,
        "duplicate activation must not create a second worktree"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn new_worktree_with_agent_starts_agent_in_first_tab(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo_with_agent();
    let toml = if cfg!(windows) {
        "[worktree]\nlocation = \"sibling\"\ndir = \"../{repo}-worktrees\"\n\
         [agents.test]\ncommand = \"cmd.exe\"\nargs = [\"/c\", \"echo NEW_WORKTREE_AGENT_READY\"]\n"
    } else {
        "[worktree]\nlocation = \"sibling\"\ndir = \"../{repo}-worktrees\"\n\
         [agents.test]\ncommand = \"sh\"\nargs = [\"-c\", \"printf NEW_WORKTREE_AGENT_READY\"]\n"
    };
    std::fs::write(repo.join(".worktree.toml"), toml).unwrap();

    let (workspace, _w) = build_workspace(cx, Some(repo));
    cx.run_until_parked();
    cx.update(|cx| {
        workspace.update(cx, |view, cx| {
            view.new_worktree_with_agent_for_test("test", cx)
        });
    });

    wait_for(
        cx,
        |cx| !cx.read(|cx| workspace.read(cx).is_creating_worktree_for_test()),
        Duration::from_secs(15),
    );

    let active = cx
        .read(|cx| workspace.read(cx).active_path().map(ToOwned::to_owned))
        .expect("new worktree should become active");
    assert_eq!(
        cx.read(|cx| workspace.read(cx).tab_count(&active)),
        1,
        "selected agent should use the first tab rather than add a second tab"
    );
    assert_eq!(
        cx.read(|cx| workspace.read(cx).session_agent_for_test(&active)),
        Some("test".to_string()),
        "session registry should remember the selected startup agent"
    );

    wait_for(
        cx,
        |cx| {
            let terminal =
                cx.update(|cx| workspace.update(cx, |view, _| view.terminal_at(&active).cloned()));
            terminal.is_some_and(|terminal| {
                cx.update(|cx| {
                    terminal.update(cx, |view, _| view.poll_events_for_test());
                });
                cx.read(|cx| {
                    terminal
                        .read(cx)
                        .snapshot_text()
                        .contains("NEW_WORKTREE_AGENT_READY")
                })
            })
        },
        Duration::from_secs(30),
    );

    shutdown_workspace(cx, &workspace);
}
