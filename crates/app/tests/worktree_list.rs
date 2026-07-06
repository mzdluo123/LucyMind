//! worktree 列表渲染 + main 行不可关闭守卫。

use std::process::Command;

use gpui::TestAppContext;

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
