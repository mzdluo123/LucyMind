//! lucy — worktree + agent 编排桌面工具入口(bin)。
//!
//! 仅调 [`lucy_app::run`]。所有逻辑在 lib(`src/lib.rs` + `workspace`/`terminal_view`
//! 等模块),供集成测试(`tests/`)导入。

fn main() {
    lucy_app::run();
}
