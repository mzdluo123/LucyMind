//! Git worktree 编排(封装 `git worktree` 子命令)。
//!
//! 通过 `Host` 抽象执行 git CLI(本机 `LocalHost` 或 WSL `WslHost`),
//! 不再直接调 `std::process::Command`。worktree 操作走 CLI 最稳,
//! 且能直接复用 `--porcelain` 的机器可读输出。

mod status;
mod worktree;

pub use status::*;
pub use worktree::*;

use std::path::Path;

use crate::host::{Host, HostCommand};

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

impl From<crate::host::HostError> for GitError {
    fn from(e: crate::host::HostError) -> Self {
        match e {
            crate::host::HostError::Io(io) => GitError::Spawn(io),
            other => GitError::Spawn(std::io::Error::other(other.to_string())),
        }
    }
}

/// 运行一条 git 子命令,成功返回 stdout,失败返回 [`GitError::Command`]。
///
/// 通过 `host.run` 执行 `git -C <repo> <args>`,支持本机和 WSL 后端。
pub(crate) fn run_git(host: &dyn Host, repo: &Path, args: &[&str]) -> Result<String, GitError> {
    let cmd = HostCommand {
        program: "git".into(),
        args: std::iter::once("-C".to_string())
            .chain(std::iter::once(repo.to_string_lossy().into_owned()))
            .chain(args.iter().map(|s| s.to_string()))
            .collect(),
        cwd: None,
        env: vec![],
    };
    let output = host.run(cmd)?;

    if output.success {
        Ok(output.stdout)
    } else {
        Err(GitError::Command {
            cmd: format!("git -C {} {}", repo.display(), args.join(" ")),
            stderr: output.stderr.trim().to_owned(),
        })
    }
}
