//! Git worktree 编排(封装 `git worktree` 子命令)。
//!
//! 用 `std::process::Command` 直接调 git CLI(不引 libgit2)——worktree
//! 操作走 CLI 最稳,且能直接复用 `--porcelain` 的机器可读输出。

mod status;
mod worktree;

pub use status::*;
pub use worktree::*;

use std::path::Path;
use std::process::Command;

/// git 操作错误。
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("执行 git 失败: {0}")]
    Spawn(#[from] std::io::Error),

    #[error("git 命令返回非零: {cmd}\n{stderr}")]
    Command { cmd: String, stderr: String },

    /// 分支已被其它 worktree 检出(git 硬限制)。带引导信息。
    #[error("分支 `{branch}` 已在 {path} 检出;请改用其它分支名或 detached 模式")]
    BranchInUse { branch: String, path: String },

    /// 目标 worktree 有未提交改动,拒绝删除(除非 force)。
    #[error("worktree 有未提交改动,拒绝删除;确认后可强制删除")]
    DirtyWorktree,
}

/// 运行一条 git 子命令,成功返回 stdout,失败返回 [`GitError::Command`]。
pub(crate) fn run_git(repo: &Path, args: &[&str]) -> Result<String, GitError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(GitError::Command {
            cmd: format!("git -C {} {}", repo.display(), args.join(" ")),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }
}
