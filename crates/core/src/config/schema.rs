//! `.worktree.toml` 的强类型 schema。占位骨架,完整实现见 U2。

/// U1 链接自检标记,U2 实现真实 schema 后移除。
#[doc(hidden)]
pub fn placeholder_marker() -> &'static str {
    "config"
}
