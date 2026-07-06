//! UI 集成测试共享基建。
//!
//! - [`temp_repo`] 造临时 git 仓库(复用 core 层 git_test 模式)。
//! - [`build_workspace`] 用 `TestAppContext` 构造 `WorkspaceView`(headless,无需真实窗口)。
//! - [`wait_for`] 轮询 `run_until_parked` + 谓词,等异步(git/PTY)完成。
//! - [`shutdown_workspace`] 停所有终端 + 排空,避免 `leak-detection` 误报。
//! - [`fake_agent_command`] 跨平台 shell 命令,替代真实 claude/codex。
//!
//! 各测试文件按需 import,未用到的函数可能触发 dead_code —— 加 allow 避免噪音。
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{Entity, TestAppContext, VisualTestContext};

use lucy_app::workspace::WorkspaceView;
use lucy_core::host::Host;

/// 建一个带初始提交(`main` 分支)的临时 git 仓库,返回 (tempdir, repo_path)。
/// tempdir 析构时自动清理。
pub fn temp_repo() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git_run(&repo, &["init", "-q", "-b", "main"]);
    git_run(&repo, &["config", "user.name", "test"]);
    git_run(&repo, &["config", "user.email", "test@example.com"]);
    std::fs::write(repo.join("README.md"), "hello\n").unwrap();
    git_run(&repo, &["add", "-A"]);
    git_run(&repo, &["commit", "-q", "-m", "init"]);
    (dir, repo)
}

/// 同 [`temp_repo`],但额外写一个 `.worktree.toml` 配置 `[agents.test]`
/// 指向跨平台 shell 命令(避免依赖真实 claude/codex)。
pub fn temp_repo_with_agent() -> (tempfile::TempDir, PathBuf) {
    let (dir, repo) = temp_repo();
    let (cmd, args) = fake_agent_command();
    let args_str = args
        .iter()
        .map(|a| format!("\"{a}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let toml = format!(
        "[worktree]\nlocation = \"sibling\"\ndir = \"../{{repo}}-worktrees\"\n\
         [agents.test]\ncommand = \"{cmd}\"\nargs = [{args_str}]\n"
    );
    std::fs::write(repo.join(".worktree.toml"), toml).unwrap();
    (dir, repo)
}

fn git_run(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?} failed in {repo:?}");
}

/// 构造一个 headless `WorkspaceView`(包在窗口里),用给定候选仓库路径初始化。
///
/// 用 `new_for_test`(不弹 `open_repo_picker` —— TestPlatform 未实现
/// `prompt_for_paths`)。`gpui_component::init` 复刻 main.rs 启动序列。
/// registry 持久化路径隔离到 tempdir,避免污染真实用户 session。
pub fn build_workspace(
    cx: &mut TestAppContext,
    candidate: Option<PathBuf>,
) -> (Entity<WorkspaceView>, &mut VisualTestContext) {
    build_workspace_with_host(cx, candidate, Arc::new(lucy_core::host::LocalHost))
}

/// 同 [`build_workspace`],但接收自定义 Host(如 `WslHost` 或 `MockHost`)。
pub fn build_workspace_with_host(
    cx: &mut TestAppContext,
    candidate: Option<PathBuf>,
    host: Arc<dyn Host>,
) -> (Entity<WorkspaceView>, &mut VisualTestContext) {
    let registry_dir = tempfile::tempdir().expect("registry tempdir");
    let registry_path = registry_dir.path().join("sessions.json");
    // tempdir 析构清理,但 Entity 可能比 tempdir 活更久 —— 把路径记下,
    // 测试结束 shutdown_workspace 后手动删即可。
    let (workspace, window) = cx.add_window_view(|_window, cx| {
        gpui_component::init(cx);
        let mut v = WorkspaceView::new_for_test_with_host(cx, candidate, host.clone());
        v.set_registry_path_for_test(registry_path);
        v
    });
    std::mem::forget(registry_dir);
    (workspace, window)
}

/// 轮询直到谓词成立或超时。每次循环 `run_until_parked` 排空异步任务
/// (git/PTY/cx.spawn),让后台操作完成后再检查谓词。
///
/// `run_until_parked` 只排空**已就绪**任务;PTY 的 16ms 轮询 timer 在 sleep
/// 期间未就绪,会立即返回。故循环里显式 sleep 让 timer 触发,下一轮
/// `run_until_parked` 才能排空 PTY 事件 + 刷新 snapshot。
///
/// - sleep 20ms 略大于 PTY 的 16ms 周期,避免错相位导致轮空。
/// - 默认超时 30s:CI 机器负载高时 PTY 子进程 spawn + 首次输出可能 >10s。
pub fn wait_for<F>(cx: &mut TestAppContext, mut predicate: F, timeout: Duration)
where
    F: FnMut(&mut TestAppContext) -> bool,
{
    let start = Instant::now();
    loop {
        cx.run_until_parked();
        if predicate(cx) {
            return;
        }
        if start.elapsed() > timeout {
            panic!("wait_for timed out after {timeout:?} (predicate never held)");
        }
        // 让 PTY 轮询(16ms)和其他定时器有机会推进。
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// 停掉 workspace 内所有终端 + 排空异步,避免 `leak-detection` 误报。
///
/// TerminalView 的 `cx.spawn` 轮询循环是长任务,不显式 shutdown 会在测试结束
/// 后仍持有 Entity → leak-detection 失败。
pub fn shutdown_workspace(cx: &mut TestAppContext, workspace: &Entity<WorkspaceView>) {
    cx.update(|cx| {
        workspace.update(cx, |view, cx| {
            view.shutdown_all_terminals_for_test(cx);
        });
    });
    // 排空轮询循环 + 后台 git 任务。
    cx.run_until_parked();
}

/// 跨平台 fake agent 命令:Unix 用 `/bin/sh`,Windows 用 `cmd.exe`。
/// 供 `new_worktree_and_agent` 测试用,避免真实 claude/codex 依赖。
pub fn fake_agent_command() -> (String, Vec<String>) {
    if cfg!(windows) {
        ("cmd.exe".to_string(), vec!["/Q".into(), "/K".into()])
    } else {
        ("/bin/sh".to_string(), vec![])
    }
}
