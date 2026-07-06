//! git 状态查询:未提交改动检查、worktree 列表(用于分支占用检查)。
//!
//! 一律解析 `--porcelain` 输出(稳定、机器可读),不解析人类可读输出。

use std::path::{Path, PathBuf};

use crate::host::Host;

use super::{run_git, GitError};

/// 一个已存在的 worktree 条目(来自 `git worktree list --porcelain`)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeEntry {
    pub path: PathBuf,
    /// 检出的分支的短名(如 `feature/x`);detached 时为 None。
    pub branch: Option<String>,
    /// HEAD commit(40 位 sha);极少数损坏条目可能缺失。
    pub head: Option<String>,
    pub locked: bool,
}

/// 工作区是否有未提交改动(含未跟踪文件)。
///
/// 用于:创建前的脏工作区提示、删除前的安全检查(preRemove 核心价值)。
pub fn has_uncommitted_changes(host: &dyn Host, repo: impl AsRef<Path>) -> Result<bool, GitError> {
    let out = run_git(host, repo.as_ref(), &["status", "--porcelain"])?;
    Ok(!out.trim().is_empty())
}

/// 列出仓库的所有 worktree。
pub fn list_worktrees(
    host: &dyn Host,
    repo: impl AsRef<Path>,
) -> Result<Vec<WorktreeEntry>, GitError> {
    let out = run_git(host, repo.as_ref(), &["worktree", "list", "--porcelain"])?;
    Ok(parse_worktree_list(&out))
}

/// 检查某分支是否已被(除 `exclude_path` 外的)某个 worktree 检出。
///
/// git 硬限制:同一分支不能被多个 worktree 同时检出。创建前用它给出
/// 清晰的冲突错误,而非把 git 原始报错甩给用户。
pub fn branch_checked_out_at(
    host: &dyn Host,
    repo: impl AsRef<Path>,
    branch: &str,
    exclude_path: Option<&Path>,
) -> Result<Option<PathBuf>, GitError> {
    for entry in list_worktrees(host, repo)? {
        if entry.branch.as_deref() == Some(branch) {
            if let Some(ex) = exclude_path {
                if entry.path == ex {
                    continue;
                }
            }
            return Ok(Some(entry.path));
        }
    }
    Ok(None)
}

/// 解析 `git worktree list --porcelain` 输出。
///
/// 格式:每个 worktree 一段,段间空行分隔。段内每行 `key value` 或裸标记:
///   worktree /abs/path
///   HEAD <sha>
///   branch refs/heads/<name>   (或 `detached`)
///   locked [reason]
pub(crate) fn parse_worktree_list(text: &str) -> Vec<WorktreeEntry> {
    let mut entries = Vec::new();
    let mut cur: Option<WorktreeEntry> = None;

    for line in text.lines() {
        if line.is_empty() {
            if let Some(e) = cur.take() {
                entries.push(e);
            }
            continue;
        }
        if let Some(path) = line.strip_prefix("worktree ") {
            // 新段开始:先收尾上一段。
            if let Some(e) = cur.take() {
                entries.push(e);
            }
            cur = Some(WorktreeEntry {
                path: PathBuf::from(path),
                branch: None,
                head: None,
                locked: false,
            });
        } else if let Some(e) = cur.as_mut() {
            if let Some(sha) = line.strip_prefix("HEAD ") {
                e.head = Some(sha.to_string());
            } else if let Some(refname) = line.strip_prefix("branch ") {
                // refs/heads/<name> → <name>
                e.branch = Some(
                    refname
                        .strip_prefix("refs/heads/")
                        .unwrap_or(refname)
                        .to_string(),
                );
            } else if line == "locked" || line.starts_with("locked ") {
                e.locked = true;
            }
            // `detached`、`bare`、`prunable` 等标记:分支保持 None 即可。
        }
    }
    if let Some(e) = cur.take() {
        entries.push(e);
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multiple_worktrees() {
        let text = "\
worktree /home/u/proj
HEAD abc123
branch refs/heads/main

worktree /home/u/proj-worktrees/feature-x
HEAD def456
branch refs/heads/feature/x
locked

worktree /home/u/proj-worktrees/detached
HEAD 789aaa
detached
";
        let entries = parse_worktree_list(text);
        assert_eq!(entries.len(), 3);

        assert_eq!(entries[0].path, PathBuf::from("/home/u/proj"));
        assert_eq!(entries[0].branch.as_deref(), Some("main"));
        assert!(!entries[0].locked);

        assert_eq!(entries[1].branch.as_deref(), Some("feature/x"));
        assert!(entries[1].locked);

        assert_eq!(entries[2].branch, None); // detached
        assert_eq!(entries[2].head.as_deref(), Some("789aaa"));
    }

    #[test]
    fn empty_output_yields_no_entries() {
        assert!(parse_worktree_list("").is_empty());
    }
}
