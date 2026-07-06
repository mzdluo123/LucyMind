//! 启动状态机:有 repo / 空态 / 非 git 目录 / set_repo 注入。

use gpui::TestAppContext;

use common::{build_workspace, shutdown_workspace, temp_repo};

mod common;

#[gpui::test]
async fn startup_with_repo_lists_main(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let count = cx.read(|cx| workspace.read(cx).worktree_count());
    assert!(count >= 1, "main worktree should be listed");

    let repo_set = cx.read(|cx| workspace.read(cx).repo().map(|p| p.to_path_buf()));
    assert!(repo_set.is_some(), "repo should be set");
    // 不比较精确路径(canon 在 Windows 剥 \\?\ 前缀,与 canonicalize 不等)。
    // 只验证 repo 指向 temp repo 所在目录。
    assert!(
        repo_set.as_ref().is_some_and(|p| p.ends_with("repo")),
        "repo path should end with 'repo', got {:?}",
        repo_set
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn empty_state_when_no_candidate(cx: &mut TestAppContext) {
    let (workspace, _w) = build_workspace(cx, None);
    cx.run_until_parked();

    assert_eq!(cx.read(|cx| workspace.read(cx).worktree_count()), 0);
    assert!(cx
        .read(|cx| workspace.read(cx).repo().map(|p| p.to_path_buf()))
        .is_none());

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn non_git_candidate_enters_empty_state(cx: &mut TestAppContext) {
    // 非 git 目录 → main_worktree_root 返回 None → 空态(不弹 prompt,因 new_for_test)。
    let dir = tempfile::tempdir().unwrap();
    let non_git = dir.path().join("not-a-repo");
    std::fs::create_dir(&non_git).unwrap();

    let (workspace, _w) = build_workspace(cx, Some(non_git));
    cx.run_until_parked();

    assert_eq!(cx.read(|cx| workspace.read(cx).worktree_count()), 0);
    assert!(cx
        .read(|cx| workspace.read(cx).repo().map(|p| p.to_path_buf()))
        .is_none());

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn set_repo_for_test_loads_worktrees(cx: &mut TestAppContext) {
    // 空态启动后,手动注入 repo(set_repo_for_test 绕过 prompt_for_paths)。
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, None);
    cx.run_until_parked();

    // 注入 repo。
    cx.update(|cx| {
        workspace.update(cx, |v, _| v.set_repo_for_test(repo.clone()));
    });
    cx.run_until_parked();

    assert!(cx.read(|cx| workspace.read(cx).worktree_count()) >= 1);
    assert!(cx
        .read(|cx| workspace.read(cx).repo().map(|p| p.to_path_buf()))
        .is_some());

    shutdown_workspace(cx, &workspace);
}
