//! lucy-terminal — 终端内核适配层。
//!
//! 隔离两个关注点:
//! - PTY 生命周期(portable-pty)
//! - 终端状态机(wezterm-term,U6 引入)
//!
//! 该 crate 不依赖 GPUI —— app 层在其上做渲染。
//!
//! - [`session`] PTY 生命周期 + 读泵 + Terminal 状态(U6)
//! - [`input`]   中性输入事件 → wezterm KeyCode/MouseEvent(U7)

pub mod input;
pub mod session;

/// U1 链接自检标记,U6 实现真实会话后移除。
#[doc(hidden)]
pub fn marker() -> &'static str {
    "terminal"
}
