//! 生命周期事件与 hook 执行上下文。

use std::path::PathBuf;

/// worktree 生命周期事件。借用 devcontainer 心智模型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleEvent {
    /// worktree 创建后:装依赖、复制未跟踪文件。
    PostCreate,
    /// worktree 销毁前:停服务、清理、备份。
    PreRemove,
    // PostAttach 预留(计划 U10),MVP 不触发。
}

impl LifecycleEvent {
    /// 事件对应的显示名(日志/UI 用)。
    pub fn label(self) -> &'static str {
        match self {
            LifecycleEvent::PostCreate => "post_create",
            LifecycleEvent::PreRemove => "pre_remove",
        }
    }
}

/// hook 执行所需的上下文。用于注入环境变量(不做模板插值)。
#[derive(Debug, Clone)]
pub struct HookContext {
    /// 目标 worktree 的绝对路径 → `$WORKTREE_PATH`,同时作为命令 cwd。
    pub worktree_path: PathBuf,
    /// 分支名 → `$WORKTREE_BRANCH`(detached 时为空)。
    pub worktree_branch: String,
    /// worktree 目录名 → `$WORKTREE_NAME`。
    pub worktree_name: String,
    /// 主仓库根路径 → `$REPO_ROOT`(copy 动作的源目录)。
    pub repo_root: PathBuf,
}

impl HookContext {
    /// 组装注入给 hook 命令的环境变量键值对。
    pub fn env_vars(&self) -> Vec<(String, String)> {
        vec![
            (
                "WORKTREE_PATH".into(),
                self.worktree_path.to_string_lossy().into_owned(),
            ),
            ("WORKTREE_BRANCH".into(), self.worktree_branch.clone()),
            ("WORKTREE_NAME".into(), self.worktree_name.clone()),
            (
                "REPO_ROOT".into(),
                self.repo_root.to_string_lossy().into_owned(),
            ),
        ]
    }
}
