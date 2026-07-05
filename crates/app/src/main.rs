//! lucy — worktree + agent 编排桌面工具入口。
//!
//! U1 阶段为占位:GPUI 窗口装配在 U8-spike 引入。此处仅确认 workspace
//! 链接正确(依赖 core / terminal 两个 crate)。

fn main() {
    // 引用两个 crate 以确认链接;U8-spike 起真正的 GPUI 窗口。
    let _ = lucy_core::config::placeholder_marker();
    let _ = lucy_terminal::marker();
    println!("lucy: scaffold ok (GPUI window comes in U8-spike)");
}
