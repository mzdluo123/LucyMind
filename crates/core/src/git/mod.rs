//! Git worktree 编排(封装 `git worktree` 子命令)。完整实现见 U3。
mod status;
mod worktree;

#[allow(unused_imports)] // stub 模块,U3 填实后移除
pub use status::*;
#[allow(unused_imports)]
pub use worktree::*;
