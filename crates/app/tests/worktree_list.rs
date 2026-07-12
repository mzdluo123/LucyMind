//! worktree 列表渲染 + main 行不可关闭守卫。

use std::process::Command;

use gpui::{point, px, size, Bounds, Modifiers, Pixels, TestAppContext};

use common::{build_workspace, shutdown_workspace, temp_repo};

mod common;

/// 在 repo 里造 N 个额外 worktree(用 git worktree add)。
fn add_worktrees(repo: &std::path::Path, names: &[&str]) {
    for name in names {
        let wt = repo.parent().unwrap().join(name);
        let status = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["worktree", "add", "-b", name])
            .arg(&wt)
            .arg("HEAD")
            .status()
            .expect("git worktree add");
        assert!(status.success(), "git worktree add {name} failed");
    }
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
async fn list_count_matches_git_worktrees(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    add_worktrees(&repo, &["wt-a", "wt-b"]);
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let count = cx.read(|cx| workspace.read(cx).worktree_count());
    // main + 2 个 worktree = 3
    assert_eq!(count, 3, "main + 2 worktrees = 3, got {count}");

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn main_repo_is_not_closable(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    // 对 main 行 request_close → 不删主仓。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| {
            v.request_close_for_test(repo.clone(), "main".into(), cx);
        });
    });
    cx.run_until_parked();

    // main 仍在列表中。
    assert!(
        cx.read(|cx| workspace.read(cx).worktree_count()) >= 1,
        "main repo should not be removed"
    );
    // 状态是错误(主仓库不可关闭)。
    assert!(
        cx.read(|cx| workspace.read(cx).status_is_error()),
        "closing main repo should set error status"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn sidebar_actions_have_stable_compact_hit_targets(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, window) = build_workspace(cx, Some(repo));
    window.update(|_, cx| {
        workspace.update(cx, |view, _| view.set_sidebar_width_for_test(180.0));
    });

    let mut bounds = Vec::new();
    for selector in [
        "open-repo",
        "new-worktree-trigger",
        "open-settings",
        "worktree-actions-0",
    ] {
        let action_bounds = draw_and_get(window, &workspace, selector);
        assert_eq!(
            action_bounds.size,
            size(px(28.0), px(28.0)),
            "{selector} should use the shared compact hit target"
        );
        bounds.push(action_bounds);
    }
    assert_eq!(
        bounds[1].origin.y, bounds[2].origin.y,
        "new worktree and settings should be list-level header actions"
    );
    assert!(
        bounds[3].origin.y > bounds[1].origin.y,
        "row overflow should remain in the worktree row"
    );
    for action_bounds in bounds {
        assert!(
            action_bounds.right() <= px(180.0),
            "sidebar action should remain inside the 180px minimum width"
        );
    }

    shutdown_workspace(window, &workspace);
}

#[gpui::test]
async fn worktree_overflow_renames_through_rendered_ui(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, window) = build_workspace(cx, Some(repo.clone()));

    let trigger = draw_and_get(window, &workspace, "worktree-actions-0");
    window.simulate_click(trigger.center(), Modifiers::none());
    assert_eq!(
        window.read(|cx| {
            workspace
                .read(cx)
                .worktree_action_menu_for_test()
                .and_then(std::path::Path::file_name)
                .map(ToOwned::to_owned)
        }),
        repo.file_name().map(ToOwned::to_owned),
        "clicking overflow should open that row's menu"
    );

    let rename = draw_and_get(window, &workspace, "worktree-rename-0");
    window.simulate_click(rename.center(), Modifiers::none());
    assert_eq!(
        window.read(|cx| {
            workspace
                .read(cx)
                .editing_alias_for_test()
                .map(ToOwned::to_owned)
        }),
        Some("main".to_string()),
        "rename menu item should open the alias editor"
    );
    assert!(
        window.read(|cx| workspace.read(cx).worktree_action_menu_for_test().is_none()),
        "menu should close after choosing an action"
    );

    shutdown_workspace(window, &workspace);
}

#[gpui::test]
async fn worktree_overflow_close_reaches_confirmation(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    add_worktrees(&repo, &["wt-a"]);
    std::fs::write(repo.parent().unwrap().join("wt-a/dirty.txt"), "dirty\n")
        .expect("make worktree dirty");
    let (workspace, window) = build_workspace(cx, Some(repo));

    let trigger = draw_and_get(window, &workspace, "worktree-actions-1");
    window.simulate_click(trigger.center(), Modifiers::none());
    let close = draw_and_get(window, &workspace, "worktree-close-1");
    window.simulate_click(close.center(), Modifiers::none());

    assert!(
        window.read(|cx| workspace.read(cx).has_pending_close_for_test()),
        "close menu item should enter the existing confirmation flow"
    );
    assert!(
        window.read(|cx| workspace.read(cx).worktree_action_menu_for_test().is_none()),
        "menu should close before the confirmation dialog opens"
    );

    shutdown_workspace(window, &workspace);
}

#[gpui::test]
async fn worktree_overflow_supports_escape_and_enter(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, window) = build_workspace(cx, Some(repo));

    let trigger = draw_and_get(window, &workspace, "worktree-actions-0");
    window.simulate_click(trigger.center(), Modifiers::none());
    assert!(window.read(|cx| { workspace.read(cx).worktree_action_menu_for_test().is_some() }));

    window.simulate_keystrokes("escape");
    assert!(window.read(|cx| { workspace.read(cx).worktree_action_menu_for_test().is_none() }));

    window.simulate_keystrokes("enter");
    assert!(
        window.read(|cx| { workspace.read(cx).worktree_action_menu_for_test().is_some() }),
        "focused overflow trigger should activate with Enter"
    );

    shutdown_workspace(window, &workspace);
}
