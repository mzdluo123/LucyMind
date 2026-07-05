//! U4 测试:hook 引擎(顺序执行、环境变量、copy、fail 策略)。

use std::path::PathBuf;

use lucy_core::config::{CopySection, HookOptions, HooksSection};
use lucy_core::hooks::{self, HookContext, LifecycleEvent};

/// 建一对 (repo_root, worktree) 临时目录并返回 (tempdir, ctx)。
fn ctx() -> (tempfile::TempDir, HookContext) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    let wt = dir.path().join("wt");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::create_dir_all(&wt).unwrap();
    let ctx = HookContext {
        worktree_path: wt,
        worktree_branch: "feature/x".into(),
        worktree_name: "wt".into(),
        repo_root: repo,
    };
    (dir, ctx)
}

fn hooks_with(post_create: Vec<&str>, fail_fast: bool) -> HooksSection {
    HooksSection {
        post_create: post_create.into_iter().map(String::from).collect(),
        pre_remove: vec![],
        options: HookOptions { fail_fast },
    }
}

fn no_copy() -> CopySection {
    CopySection { files: vec![] }
}

#[test]
fn runs_commands_in_order() {
    let (_dir, c) = ctx();
    // 两条命令各写一个带序号的文件,再断言都在。
    let hooks = hooks_with(vec!["echo one > a.txt", "echo two > b.txt"], true);

    let run = hooks::run_event(LifecycleEvent::PostCreate, &hooks, &no_copy(), &c, |_| {});
    assert!(!run.had_failure());
    assert!(c.worktree_path.join("a.txt").is_file());
    assert!(c.worktree_path.join("b.txt").is_file());
}

#[test]
fn injects_worktree_env_vars() {
    let (_dir, c) = ctx();
    // 把环境变量写进文件,再读回断言。
    let hooks = hooks_with(vec!["echo \"$WORKTREE_BRANCH|$WORKTREE_NAME\" > env.txt"], true);

    hooks::run_event(LifecycleEvent::PostCreate, &hooks, &no_copy(), &c, |_| {});
    let content = std::fs::read_to_string(c.worktree_path.join("env.txt")).unwrap();
    assert_eq!(content.trim(), "feature/x|wt");
}

#[test]
fn command_runs_in_worktree_cwd() {
    let (_dir, c) = ctx();
    let hooks = hooks_with(vec!["pwd > where.txt"], true);
    hooks::run_event(LifecycleEvent::PostCreate, &hooks, &no_copy(), &c, |_| {});
    let content = std::fs::read_to_string(c.worktree_path.join("where.txt")).unwrap();
    // pwd 应等于 worktree 路径(canonicalize 消除 /private 差异)。
    let got = PathBuf::from(content.trim()).canonicalize().unwrap();
    let want = c.worktree_path.canonicalize().unwrap();
    assert_eq!(got, want);
}

#[test]
fn copies_declared_files_from_repo_root() {
    let (_dir, c) = ctx();
    std::fs::write(c.repo_root.join(".env"), "SECRET=1\n").unwrap();
    let copy = CopySection {
        files: vec![".env".into()],
    };
    let hooks = hooks_with(vec![], true);

    let run = hooks::run_event(LifecycleEvent::PostCreate, &hooks, &copy, &c, |_| {});
    assert!(!run.had_failure());
    let copied = std::fs::read_to_string(c.worktree_path.join(".env")).unwrap();
    assert_eq!(copied, "SECRET=1\n");
}

#[test]
fn missing_copy_source_is_skipped_not_fatal() {
    let (_dir, c) = ctx();
    let copy = CopySection {
        files: vec![".env.does-not-exist".into()],
    };
    let hooks = hooks_with(vec![], true);

    let run = hooks::run_event(LifecycleEvent::PostCreate, &hooks, &copy, &c, |_| {});
    // 源不存在 → 跳过并记成功,不致命。
    assert!(!run.had_failure());
}

#[test]
fn fail_fast_stops_after_first_failure() {
    let (_dir, c) = ctx();
    // 第一条失败(false 退出非零),第二条本应写文件 —— fail_fast 下不应执行。
    let hooks = hooks_with(vec!["false", "echo ran > second.txt"], true);

    let run = hooks::run_event(LifecycleEvent::PostCreate, &hooks, &no_copy(), &c, |_| {});
    assert!(run.had_failure());
    assert_eq!(run.steps.len(), 1, "第二条命令不应执行");
    assert!(!c.worktree_path.join("second.txt").exists());
}

#[test]
fn fail_open_continues_after_failure() {
    let (_dir, c) = ctx();
    // fail_fast=false:第一条失败后第二条仍执行。
    let hooks = hooks_with(vec!["false", "echo ran > second.txt"], false);

    let run = hooks::run_event(LifecycleEvent::PostCreate, &hooks, &no_copy(), &c, |_| {});
    assert!(run.had_failure()); // 整体有失败步骤
    assert_eq!(run.steps.len(), 2, "两条命令都应执行");
    assert!(c.worktree_path.join("second.txt").is_file());
}

#[test]
fn pre_remove_runs_its_commands() {
    let (_dir, c) = ctx();
    let hooks = HooksSection {
        post_create: vec![],
        pre_remove: vec!["echo cleanup > cleaned.txt".into()],
        options: HookOptions { fail_fast: true },
    };
    hooks::run_event(LifecycleEvent::PreRemove, &hooks, &no_copy(), &c, |_| {});
    assert!(c.worktree_path.join("cleaned.txt").is_file());
}

#[test]
fn on_step_callback_fires_per_step() {
    let (_dir, c) = ctx();
    let hooks = hooks_with(vec!["true", "true"], true);
    let mut count = 0;
    hooks::run_event(LifecycleEvent::PostCreate, &hooks, &no_copy(), &c, |_| count += 1);
    assert_eq!(count, 2);
}
