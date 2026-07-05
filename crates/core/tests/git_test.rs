//! U3 集成测试:在真实临时 git 仓库上验证 worktree 编排。

use std::path::{Path, PathBuf};
use std::process::Command;

use lucy_core::git::{self, CreateMode, GitError};

/// 建一个带初始提交、默认分支为 `main` 的临时仓库,返回 (tempdir, repo_path)。
fn init_repo() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();

    run(&repo, &["init", "-q", "-b", "main"]);
    run(&repo, &["config", "user.name", "test"]);
    run(&repo, &["config", "user.email", "test@example.com"]);
    std::fs::write(repo.join("README.md"), "hello\n").unwrap();
    run(&repo, &["add", "-A"]);
    run(&repo, &["commit", "-q", "-m", "init"]);

    (dir, repo)
}

fn run(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?} failed");
}

/// 规范化路径后比较:macOS 上 git 返回 `/private/var/...`,tempdir 给 `/var/...`,
/// 二者指向同一目录但字符串不等,须 canonicalize 后再比。
fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

#[test]
fn creates_worktree_on_new_branch() {
    let (dir, repo) = init_repo();
    let wt = dir.path().join("wt-feature");

    git::add(
        &repo,
        &wt,
        &CreateMode::NewBranch {
            branch: "feature/x".into(),
            base: "main".into(),
        },
    )
    .expect("add worktree");

    assert!(wt.join("README.md").is_file(), "worktree checked out files");

    let list = git::list(&repo).expect("list");
    assert!(list.iter().any(|e| e.branch.as_deref() == Some("feature/x")));
}

#[test]
fn creates_worktree_on_existing_branch() {
    let (dir, repo) = init_repo();
    run(&repo, &["branch", "existing"]);
    let wt = dir.path().join("wt-existing");

    git::add(
        &repo,
        &wt,
        &CreateMode::ExistingBranch {
            branch: "existing".into(),
        },
    )
    .expect("add existing-branch worktree");

    assert!(wt.is_dir());
}

#[test]
fn creates_detached_worktree() {
    let (dir, repo) = init_repo();
    let wt = dir.path().join("wt-detached");

    git::add(&repo, &wt, &CreateMode::Detached { commitish: None }).expect("add detached");

    let list = git::list(&repo).expect("list");
    // detached 条目 branch 为 None。
    assert!(list.iter().any(|e| same_path(&e.path, &wt) && e.branch.is_none()));
}

#[test]
fn rejects_branch_already_checked_out() {
    let (dir, repo) = init_repo();
    run(&repo, &["branch", "shared"]);

    let wt1 = dir.path().join("wt1");
    git::add(
        &repo,
        &wt1,
        &CreateMode::ExistingBranch {
            branch: "shared".into(),
        },
    )
    .expect("first checkout ok");

    // 第二次检出同分支 → 明确的 BranchInUse,而非 git 原始报错。
    let wt2 = dir.path().join("wt2");
    let err = git::add(
        &repo,
        &wt2,
        &CreateMode::ExistingBranch {
            branch: "shared".into(),
        },
    )
    .expect_err("second checkout of same branch must fail");
    assert!(matches!(err, GitError::BranchInUse { .. }));
}

#[test]
fn remove_rejects_dirty_worktree_then_force_succeeds() {
    let (dir, repo) = init_repo();
    let wt = dir.path().join("wt-dirty");
    git::add(
        &repo,
        &wt,
        &CreateMode::NewBranch {
            branch: "dirty".into(),
            base: "main".into(),
        },
    )
    .unwrap();

    // 制造未提交改动。
    std::fs::write(wt.join("scratch.txt"), "uncommitted\n").unwrap();

    let err = git::remove(&repo, &wt, false).expect_err("dirty remove must be rejected");
    assert!(matches!(err, GitError::DirtyWorktree));
    assert!(wt.is_dir(), "worktree still present after rejected remove");

    // force 后成功。
    git::remove(&repo, &wt, true).expect("force remove");
    assert!(!wt.is_dir(), "worktree gone after force remove");
}

#[test]
fn remove_clean_worktree_succeeds() {
    let (dir, repo) = init_repo();
    let wt = dir.path().join("wt-clean");
    git::add(
        &repo,
        &wt,
        &CreateMode::NewBranch {
            branch: "clean".into(),
            base: "main".into(),
        },
    )
    .unwrap();

    git::remove(&repo, &wt, false).expect("clean remove ok");
    assert!(!wt.is_dir());
}

#[test]
fn lock_unlock_roundtrip() {
    let (dir, repo) = init_repo();
    let wt = dir.path().join("wt-lock");
    git::add(
        &repo,
        &wt,
        &CreateMode::NewBranch {
            branch: "locked-branch".into(),
            base: "main".into(),
        },
    )
    .unwrap();

    git::lock(&repo, &wt, Some("agent running")).expect("lock");
    let list = git::list(&repo).expect("list");
    assert!(list.iter().any(|e| same_path(&e.path, &wt) && e.locked));

    git::unlock(&repo, &wt).expect("unlock");
    let list = git::list(&repo).expect("list");
    assert!(list.iter().any(|e| same_path(&e.path, &wt) && !e.locked));
}

#[test]
fn random_branch_name_format() {
    let b = git::random_branch_name("lucy/session-");
    // 形如 lucy/session-<adj>-<color>-<animal>-<nature>。
    assert!(b.starts_with("lucy/session-"), "前缀不对: {b}");
    let tail = b.strip_prefix("lucy/session-").unwrap();
    let parts: Vec<&str> = tail.split('-').collect();
    assert_eq!(parts.len(), 4, "应为四个短单词: {b}");
    assert!(parts.iter().all(|p| !p.is_empty()), "词不应为空: {b}");
}

#[test]
fn random_branch_name_varies() {
    // 连续生成应有差异(极小概率相同,多取几个降低偶然)。
    let names: std::collections::HashSet<_> =
        (0..8).map(|_| git::random_branch_name("x-")).collect();
    assert!(names.len() >= 2, "随机分支名应有变化,得到: {names:?}");
}

#[test]
fn main_worktree_root_from_subdir() {
    let (_dir, repo) = init_repo();
    // 从子目录也应解析出主仓根,而非子目录本身。
    let subdir = repo.join("crates/app");
    std::fs::create_dir_all(&subdir).unwrap();

    let root = git::main_worktree_root(&subdir).expect("should resolve main root");
    assert!(same_path(&root, &repo), "{root:?} 应等于主仓 {repo:?}");
}

#[test]
fn main_worktree_root_from_inside_worktree() {
    let (dir, repo) = init_repo();
    let wt = dir.path().join("wt-a");
    git::add(
        &repo,
        &wt,
        &CreateMode::NewBranch {
            branch: "a".into(),
            base: "main".into(),
        },
    )
    .unwrap();
    // 在 worktree 内解析,应仍指向主仓(不是这个 worktree)。
    let root = git::main_worktree_root(&wt).expect("resolve");
    assert!(same_path(&root, &repo), "worktree 内应解析到主仓");
}

#[test]
fn branch_exists_detects() {
    let (_dir, repo) = init_repo();
    run(&repo, &["branch", "feature/x"]);
    assert!(git::branch_exists(&repo, "feature/x"));
    assert!(!git::branch_exists(&repo, "nonexistent"));
}

#[test]
fn list_reflects_multiple_worktrees() {
    let (dir, repo) = init_repo();
    for (i, br) in ["a", "b"].iter().enumerate() {
        let wt = dir.path().join(format!("wt-{i}"));
        git::add(
            &repo,
            &wt,
            &CreateMode::NewBranch {
                branch: (*br).into(),
                base: "main".into(),
            },
        )
        .unwrap();
    }
    let list = git::list(&repo).expect("list");
    // 主仓 + 2 个 worktree = 3。
    assert_eq!(list.len(), 3);
}
