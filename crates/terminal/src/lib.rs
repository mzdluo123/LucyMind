//! lucy-terminal — 终端内核适配层(基于 alacritty_terminal,Zed 同款内核)。
//!
//! 该 crate 不依赖 GPUI —— app 层在其上做渲染。职责:
//! - 用 alacritty 自带的 tty + EventLoop 起 PTY 子进程、驱动 `Term`
//! - 把内核事件(Wakeup / PtyWrite / Title / ChildExit)通过 channel 转出线程边界
//! - 提供默认调色板(alacritty 内核不带默认配色)
//! - 暴露可渲染快照(cell 网格,含宽字符标志)供 app 层绘制
//! - 把按键编码成终端字节序列(input 模块)
//!
//! - [`session`]  PTY + EventLoop 生命周期 + Term 共享句柄
//! - [`palette`]  默认 256 色 + fg/bg 调色板,Color → RGB 解析
//! - [`input`]    按键 → 终端字节序列编码

pub mod input;
pub mod palette;
pub mod session;

pub use session::{
    CursorPos, RenderCell, RenderSnapshot, TermDimensions, TermEvent, TerminalSession,
};
