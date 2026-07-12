//! Tab 栏常显快捷 launcher:UI 布局与真实点击端到端行为。

use std::time::Duration;

use gpui::{point, px, size, Bounds, Modifiers, Pixels, TestAppContext};

use common::{build_workspace, shutdown_workspace, temp_repo, wait_for, wait_for_shell_ready};

mod common;

fn create_worktree(
    cx: &mut TestAppContext,
    workspace: &gpui::Entity<lucy_app::workspace::WorkspaceView>,
) -> std::path::PathBuf {
    cx.update(|cx| workspace.update(cx, |view, cx| view.new_worktree_for_test(cx)));
    wait_for(
        cx,
        |cx| cx.read(|cx| workspace.read(cx).active_path().is_some()),
        Duration::from_secs(15),
    );
    cx.read(|cx| {
        workspace
            .read(cx)
            .active_path()
            .expect("worktree should be active")
            .to_path_buf()
    })
}

fn draw_and_get(
    window: &mut gpui::VisualTestContext,
    workspace: &gpui::Entity<lucy_app::workspace::WorkspaceView>,
    selector: &'static str,
) -> Bounds<Pixels> {
    window.draw(
        point(px(0.0), px(0.0)),
        size(px(1100.0), px(680.0)),
        |_, _| workspace.clone(),
    );
    window
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} should be rendered"))
}

#[gpui::test]
async fn quick_launchers_are_expanded_as_compact_icons_by_default(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, window) = build_workspace(cx, Some(repo));
    let _wt_path = create_worktree(window, &workspace);

    for selector in [
        "quick-launch-codex",
        "quick-launch-claude",
        "quick-launch-terminal",
    ] {
        let bounds = draw_and_get(window, &workspace, selector);
        assert_eq!(
            bounds.size,
            size(px(32.0), px(31.0)),
            "{selector} should fill the tab bar inside its 1px bottom border"
        );
    }

    shutdown_workspace(window, &workspace);
}

#[gpui::test]
async fn quick_launcher_clicks_run_codex_claude_and_terminal(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    #[cfg(not(windows))]
    std::fs::write(
        repo.join(".worktree.toml"),
        "[agents.codex]\ncommand = \"printf\"\nargs = [\"QUICK_CODEX\"]\n\
         [agents.claude]\ncommand = \"printf\"\nargs = [\"QUICK_CLAUDE\"]\n",
    )
    .unwrap();
    #[cfg(windows)]
    std::fs::write(
        repo.join(".worktree.toml"),
        "[agents.codex]\ncommand = \"cmd.exe\"\nargs = [\"/c\", \"echo QUICK_CODEX\"]\n\
         [agents.claude]\ncommand = \"cmd.exe\"\nargs = [\"/c\", \"echo QUICK_CLAUDE\"]\n",
    )
    .unwrap();

    let (workspace, window) = build_workspace(cx, Some(repo));
    let wt_path = create_worktree(window, &workspace);
    wait_for_shell_ready(window, &workspace, &wt_path, Duration::from_secs(15));

    for (selector, marker) in [
        ("quick-launch-codex", "QUICK_CODEX"),
        ("quick-launch-claude", "QUICK_CLAUDE"),
    ] {
        let before = window.read(|cx| workspace.read(cx).tab_count(&wt_path));
        let bounds = draw_and_get(window, &workspace, selector);
        window.simulate_click(bounds.center(), Modifiers::none());
        wait_for(
            window,
            |cx| cx.read(|cx| workspace.read(cx).tab_count(&wt_path)) == before + 1,
            Duration::from_secs(15),
        );
        wait_for(
            window,
            |cx| {
                let terminal = cx.read(|cx| workspace.read(cx).terminal_at(&wt_path).cloned());
                terminal.is_some_and(|terminal| {
                    cx.update(|cx| {
                        terminal.update(cx, |terminal, _| terminal.poll_events_for_test())
                    });
                    cx.read(|cx| terminal.read(cx).snapshot_text().contains(marker))
                })
            },
            Duration::from_secs(15),
        );
    }

    let before = window.read(|cx| workspace.read(cx).tab_count(&wt_path));
    let terminal_bounds = draw_and_get(window, &workspace, "quick-launch-terminal");
    window.simulate_click(terminal_bounds.center(), Modifiers::none());
    wait_for(
        window,
        |cx| cx.read(|cx| workspace.read(cx).tab_count(&wt_path)) == before + 1,
        Duration::from_secs(15),
    );
    wait_for_shell_ready(window, &workspace, &wt_path, Duration::from_secs(15));

    let terminal = window
        .read(|cx| workspace.read(cx).terminal_at(&wt_path).cloned())
        .expect("terminal quick launcher should activate a terminal");
    window.update(|_window, cx| {
        terminal.update(cx, |terminal, _| {
            terminal.send_text("echo QUICK_TERMINAL\r")
        });
    });
    wait_for(
        window,
        |cx| {
            cx.update(|cx| terminal.update(cx, |terminal, _| terminal.poll_events_for_test()));
            cx.read(|cx| terminal.read(cx).snapshot_text().contains("QUICK_TERMINAL"))
        },
        Duration::from_secs(15),
    );

    shutdown_workspace(window, &workspace);
}
