//! lucy-core — worktree + agent 编排的纯逻辑层。
//!
//! 该 crate 刻意不依赖任何 GUI 或终端内核(GPUI / wezterm-term),
//! 以保持可移植与可单测。UI 层(app)与终端适配层(terminal)在其之上构建。
//!
//! 模块划分见 [Output Structure] 计划:
//! - [`git`]    worktree 编排(封装 `git worktree` 子命令)
//! - [`config`] `.worktree.toml` 解析与校验
//! - [`hooks`]  生命周期钩子引擎
//! - [`agent`]  agent 启动规格(纯数据,不含 PTY)

pub mod agent;
pub mod config;
pub mod git;
pub mod hooks;
