//! worktree add / remove / lock / unlock。
//!
//! 创建前做安全检查(分支占用),删除前做安全检查(未提交改动)——
//! 把 git 的硬限制转成清晰的、可引导的错误,而非直接甩 git 原始报错。

use std::path::{Path, PathBuf};

use crate::host::Host;

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
pub fn add(
    host: &dyn Host,
    repo: impl AsRef<Path>,
    path: impl AsRef<Path>,
    mode: &CreateMode,
) -> Result<(), GitError> {
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
        if let Some(existing) = branch_checked_out_at(host, repo, branch, None)? {
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
            run_git(host, repo, &args)?;
        }
        CreateMode::ExistingBranch { branch } => {
            args.extend([path_str, branch.as_str()]);
            run_git(host, repo, &args)?;
        }
        CreateMode::Detached { commitish } => {
            args.extend(["--detach", path_str]);
            if let Some(c) = commitish {
                args.push(c.as_str());
            }
            run_git(host, repo, &args)?;
        }
    }
    Ok(())
}

/// 列出仓库所有 worktree(转发 [`list_worktrees`],便于从本模块统一入口调用)。
pub fn list(host: &dyn Host, repo: impl AsRef<Path>) -> Result<Vec<WorktreeEntry>, GitError> {
    list_worktrees(host, repo)
}

/// 删除一个 worktree。
///
/// 未 `force` 时,先检查目标 worktree 的未提交改动,非空则拒绝
/// ([`GitError::DirtyWorktree`])——这是 preRemove 语义的安全底线。
pub fn remove(
    host: &dyn Host,
    repo: impl AsRef<Path>,
    worktree_path: impl AsRef<Path>,
    force: bool,
) -> Result<(), GitError> {
    let repo = repo.as_ref();
    let wt = worktree_path.as_ref();

    if !force && has_uncommitted_changes(host, wt)? {
        return Err(GitError::DirtyWorktree);
    }

    let path_str = wt.to_string_lossy();
    let mut args: Vec<&str> = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(&path_str);
    run_git(host, repo, &args)?;
    Ok(())
}

/// 锁定 worktree(防止被 prune / 误删)。agent 运行期间应加锁。
pub fn lock(
    host: &dyn Host,
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
    run_git(host, repo.as_ref(), &args)?;
    Ok(())
}

/// 解锁 worktree。
pub fn unlock(
    host: &dyn Host,
    repo: impl AsRef<Path>,
    worktree_path: impl AsRef<Path>,
) -> Result<(), GitError> {
    let path_str = worktree_path.as_ref().to_string_lossy();
    run_git(host, repo.as_ref(), &["worktree", "unlock", &path_str])?;
    Ok(())
}

/// 检测仓库是否使用 submodule。worktree 对 submodule 支持弱,
/// UI 层应据此给降级提示(不假装完全支持)。
pub fn uses_submodules(host: &dyn Host, repo: impl AsRef<Path>) -> bool {
    host.exists(&host.join_path(repo.as_ref(), ".gitmodules"))
}

/// 从任意子目录解析出**主仓库工作树根**(`git rev-parse --show-toplevel`)。
///
/// 注意:在 worktree 内运行时,`--show-toplevel` 返回的是该 worktree 的根,
/// 不是主仓。要拿主仓根用 [`main_worktree_root`]。这里用于"从项目任意子目录
/// 启动也能定位仓库",而不是盲信当前目录。
pub fn toplevel(host: &dyn Host, dir: impl AsRef<Path>) -> Option<PathBuf> {
    run_git(host, dir.as_ref(), &["rev-parse", "--show-toplevel"])
        .ok()
        .map(|s| PathBuf::from(s.trim()))
}

/// 解析出**主仓库**根(不是当前 worktree 根)。用 `git worktree list` 的第一条
/// (git 保证第一条是主工作树)。从子目录/worktree 内启动都能拿到正确主仓。
pub fn main_worktree_root(host: &dyn Host, dir: impl AsRef<Path>) -> Option<PathBuf> {
    let list = list_worktrees(host, dir.as_ref()).ok()?;
    list.into_iter().next().map(|e| e.path)
}

/// 某本地分支是否已存在(含被 worktree 删除后残留的孤儿分支)。
pub fn branch_exists(host: &dyn Host, repo: impl AsRef<Path>, branch: &str) -> bool {
    // `git branch --list <name>` 存在则输出该分支行,否则空。
    run_git(host, repo.as_ref(), &["branch", "--list", branch])
        .map(|out| !out.trim().is_empty())
        .unwrap_or(false)
}

/// 生成一个 `prefix` + 四个随机短单词(`-` 拼)的分支名,如
/// `lucy/session-brave-cyan-fox-moon`。四词组合空间极大、几乎不撞,故**不做
/// git 探测**(旧的逐个递增探测在大仓库下要几百 ms)。极小概率撞名交给 git add
/// 报错兜底(调用方已处理 add 失败)。
pub fn random_branch_name(prefix: &str) -> String {
    let mut seed = seed_from_time();
    let mut pick = |list: &[&'static str]| -> &'static str {
        // xorshift 简单伪随机,不引 rand 依赖。
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        list[(seed as usize) % list.len()]
    };
    format!(
        "{prefix}{}-{}-{}-{}",
        pick(ADJECTIVES),
        pick(COLORS),
        pick(ANIMALS),
        pick(NATURE),
    )
}

/// 随机种子:系统时间纳秒 XOR 一个进程内自增计数 —— 保证同一纳秒内连续调用
/// 也不同(否则紧密循环里会拿到同种子、同名)。无需可复现,不引 rand。
fn seed_from_time() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9);
    let c = COUNTER.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed);
    (nanos ^ c).max(1) // 避免种子为 0(xorshift 会卡死在 0)
}

const ADJECTIVES: &[&str] = &[
    "brave", "calm", "swift", "bold", "keen", "wise", "lucky", "quiet", "bright", "eager",
    "gentle", "jolly", "merry", "noble", "proud", "sunny", "witty", "zesty", "cosmic", "electric",
];
const COLORS: &[&str] = &[
    "amber", "azure", "coral", "crimson", "cyan", "gold", "indigo", "jade", "lime", "magenta",
    "olive", "pearl", "ruby", "sage", "teal", "violet", "ivory", "onyx", "slate", "rose",
];
const ANIMALS: &[&str] = &[
    "fox", "owl", "wolf", "hawk", "lynx", "otter", "seal", "crane", "raven", "moth", "koi", "elk",
    "bear", "swan", "wren", "ibis", "puma", "orca", "toad", "yak",
];
const NATURE: &[&str] = &[
    "moon", "reef", "dune", "peak", "grove", "creek", "cliff", "tide", "mist", "spark", "ember",
    "frost", "storm", "vale", "fjord", "atoll", "delta", "ridge", "bloom", "leaf",
];

/// 便捷:清理已被手动删除但元数据残留的 worktree 记录。
pub fn prune(host: &dyn Host, repo: impl AsRef<Path>) -> Result<(), GitError> {
    run_git(host, repo.as_ref(), &["worktree", "prune"])?;
    Ok(())
}

/// 给定 sibling 父目录与分支名,计算 worktree 路径(分支名做文件系统安全清理)。
pub fn sibling_worktree_path(host: &dyn Host, parent_dir: &Path, branch: &str) -> PathBuf {
    host.join_path(parent_dir, &sanitize_branch_for_path(branch))
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
        let p = sibling_worktree_path(
            &crate::host::LocalHost,
            Path::new("/tmp/proj-worktrees"),
            "feature/x",
        );
        assert_eq!(p, PathBuf::from("/tmp/proj-worktrees/feature-x"));
    }
}
