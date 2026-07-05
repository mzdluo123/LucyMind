//! worktree add / remove / lock / unlock。
//!
//! 创建前做安全检查(分支占用),删除前做安全检查(未提交改动)——
//! 把 git 的硬限制转成清晰的、可引导的错误,而非直接甩 git 原始报错。

use std::path::{Path, PathBuf};

use super::{
    branch_checked_out_at, has_uncommitted_changes, list_worktrees, run_git, GitError,
    WorktreeEntry,
};

/// worktree 创建模式。
#[derive(Debug, Clone)]
pub enum CreateMode {
    /// 基于 `base` 新建分支 `branch`(最常用):`git worktree add -b <branch> <path> <base>`。
    NewBranch { branch: String, base: String },
    /// 检出已有分支:`git worktree add <path> <branch>`。
    ExistingBranch { branch: String },
    /// detached HEAD:`git worktree add --detach <path> [<commitish>]`。
    Detached { commitish: Option<String> },
}

/// 在 `path` 处创建一个 worktree。
///
/// - `NewBranch` / `ExistingBranch`:创建前检查目标分支是否已被其它 worktree 检出,
///   命中则返回 [`GitError::BranchInUse`]。
pub fn add(repo: impl AsRef<Path>, path: impl AsRef<Path>, mode: &CreateMode) -> Result<(), GitError> {
    let repo = repo.as_ref();
    let path = path.as_ref();

    // 分支占用检查(仅对涉及具名分支的模式)。
    let branch = match mode {
        CreateMode::NewBranch { branch, .. } | CreateMode::ExistingBranch { branch } => {
            Some(branch.as_str())
        }
        CreateMode::Detached { .. } => None,
    };
    if let Some(branch) = branch {
        if let Some(existing) = branch_checked_out_at(repo, branch, None)? {
            return Err(GitError::BranchInUse {
                branch: branch.to_string(),
                path: existing.display().to_string(),
            });
        }
    }

    let path_str = path.to_string_lossy();
    let path_str: &str = &path_str;
    let mut args: Vec<&str> = vec!["worktree", "add"];
    match mode {
        CreateMode::NewBranch { branch, base } => {
            args.extend(["-b", branch.as_str(), path_str, base.as_str()]);
            run_git(repo, &args)?;
        }
        CreateMode::ExistingBranch { branch } => {
            args.extend([path_str, branch.as_str()]);
            run_git(repo, &args)?;
        }
        CreateMode::Detached { commitish } => {
            args.extend(["--detach", path_str]);
            if let Some(c) = commitish {
                args.push(c.as_str());
            }
            run_git(repo, &args)?;
        }
    }
    Ok(())
}

/// 列出仓库所有 worktree(转发 [`list_worktrees`],便于从本模块统一入口调用)。
pub fn list(repo: impl AsRef<Path>) -> Result<Vec<WorktreeEntry>, GitError> {
    list_worktrees(repo)
}

/// 删除一个 worktree。
///
/// 未 `force` 时,先检查目标 worktree 的未提交改动,非空则拒绝
/// ([`GitError::DirtyWorktree`])——这是 preRemove 语义的安全底线。
pub fn remove(
    repo: impl AsRef<Path>,
    worktree_path: impl AsRef<Path>,
    force: bool,
) -> Result<(), GitError> {
    let repo = repo.as_ref();
    let wt = worktree_path.as_ref();

    if !force && has_uncommitted_changes(wt)? {
        return Err(GitError::DirtyWorktree);
    }

    let path_str = wt.to_string_lossy();
    let mut args: Vec<&str> = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(&path_str);
    run_git(repo, &args)?;
    Ok(())
}

/// 锁定 worktree(防止被 prune / 误删)。agent 运行期间应加锁。
pub fn lock(
    repo: impl AsRef<Path>,
    worktree_path: impl AsRef<Path>,
    reason: Option<&str>,
) -> Result<(), GitError> {
    let path_str = worktree_path.as_ref().to_string_lossy();
    let mut args: Vec<&str> = vec!["worktree", "lock"];
    if let Some(r) = reason {
        args.extend(["--reason", r]);
    }
    args.push(&path_str);
    run_git(repo.as_ref(), &args)?;
    Ok(())
}

/// 解锁 worktree。
pub fn unlock(repo: impl AsRef<Path>, worktree_path: impl AsRef<Path>) -> Result<(), GitError> {
    let path_str = worktree_path.as_ref().to_string_lossy();
    run_git(repo.as_ref(), &["worktree", "unlock", &path_str])?;
    Ok(())
}

/// 检测仓库是否使用 submodule。worktree 对 submodule 支持弱,
/// UI 层应据此给降级提示(不假装完全支持)。
pub fn uses_submodules(repo: impl AsRef<Path>) -> bool {
    repo.as_ref().join(".gitmodules").is_file()
}

/// 便捷:清理已被手动删除但元数据残留的 worktree 记录。
pub fn prune(repo: impl AsRef<Path>) -> Result<(), GitError> {
    run_git(repo.as_ref(), &["worktree", "prune"])?;
    Ok(())
}

/// 给定 sibling 父目录与分支名,计算 worktree 路径(分支名做文件系统安全清理)。
pub fn sibling_worktree_path(parent_dir: &Path, branch: &str) -> PathBuf {
    parent_dir.join(sanitize_branch_for_path(branch))
}

/// 把分支名清理成文件系统安全的目录名(`feature/x` → `feature-x`)。
pub fn sanitize_branch_for_path(branch: &str) -> String {
    branch
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_branch_names() {
        assert_eq!(sanitize_branch_for_path("feature/x"), "feature-x");
        assert_eq!(sanitize_branch_for_path("fix/a:b"), "fix-a-b");
        assert_eq!(sanitize_branch_for_path("main"), "main");
    }

    #[test]
    fn builds_sibling_path() {
        let p = sibling_worktree_path(Path::new("/tmp/proj-worktrees"), "feature/x");
        assert_eq!(p, PathBuf::from("/tmp/proj-worktrees/feature-x"));
    }
}
