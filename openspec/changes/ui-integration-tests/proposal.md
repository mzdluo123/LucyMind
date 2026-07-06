## Why

app 层（`crates/app`）目前只有 10 个纯函数单元测试（`canon`/`same_path`/`word_boundary`/`trim_end`），**零 GPUI 集成测试**。`WorkspaceView` 的状态机——启动、空态、worktree 列表、新建/切换/关闭、agent 菜单、别名编辑、设置对话框、终端渲染与输入——完全靠 `cargo run -p lucy-app` 后人工点按钮、看渲染、试交互来验证。

三个已合并的 change（`agent-launcher-menu`、`terminal-copy`、`terminal-render-fix`）的 `tasks.md` 末尾都有一条手动验证任务（"7.3 `cargo run -p lucy-app`：侧边栏…点击…验证…"）至今未勾——这就是闭环缺口：**UI 行为的正确性没有自动化门禁，回归只能靠人眼**。

GPUI 0.2.2（crates.io，即 Zed 官方发布）提供了完整的第一方测试基础设施：`#[gpui::test]` 宏 + `TestAppContext`（headless、跨平台、无需真实 GPU/窗口），能构造 Entity、开窗口、模拟键盘/鼠标/文件选择器输入、跑完异步任务（`run_until_parked`）、断言状态与事件、读取剪贴板、渲染元素树（`VisualTestContext::draw`）。本 change 用它建立 app 层的 UI 集成测试，让 `cargo test` 成为 UI 状态验证的门禁。

## What Changes

- **app crate lib+bin 分离**：新建 `crates/app/src/lib.rs`，把 `main.rs` 的 5 个私有 `mod` 改为 `pub mod` 并 re-export，`fn main()` 瘦身为 `lucy_app::run()`。集成测试（`crates/app/tests/`）得以 `use lucy_app::*` 导入被测类型。
- **测试 accessor**：`WorkspaceView`/`TerminalView` 的关键字段与方法通过 `#[cfg(test)]`-gated `pub` accessor（`active_path`/`worktree_count`/`is_agent_menu_open`/`snapshot_text` 等）暴露给集成测试，生产二进制可见性不变。
- **dev-dependencies**：`crates/app/Cargo.toml` 新增 `[dev-dependencies]`：`gpui = { version = "0.2.2", features = ["test-support"] }`（解锁 `TestAppContext`/`#[gpui::test]`/`VisualTestContext`）、`tempfile = "3"`。
- **测试 harness**：`crates/app/tests/common/mod.rs` 共享基建——`temp_repo()`（tempfile + `git init` + commit，复用 core 层模式）、`build_workspace(cx, repo)`（`gpui_component::init` + `Root` 包裹 + `WorkspaceView::new`）、`wait_for(cx, predicate, timeout)`（基于 `run_until_parked` 轮询）、registry 路径隔离到 tempdir。
- **集成测试套件**：`crates/app/tests/` 下按功能域分文件——启动/空态、worktree 列表、新建+agent、切换、关闭、agent 菜单、别名、设置、终端渲染、终端输入、终端复制交互。全部用 `#[gpui::test]` + `TestAppContext`；终端测试用真实 `/bin/sh` PTY（复用 terminal 层 `wait_for_text` 模式）+ `run_until_parked` + 显式 `shutdown`。
- **闭环门禁**：`cargo test`（含 `#[gpui::test]`）全绿 = UI 状态验证通过；`cargo fmt && cargo clippy --all-targets` 覆盖测试代码。CLAUDE.md 测试段补充 app 层说明。

## Capabilities

### New Capabilities

- `ui-integration-tests`: app 层 GPUI 集成测试基础设施与套件——lib+bin 分离、TestAppContext 驱动的 headless 测试 harness、按功能域覆盖 WorkspaceView/TerminalView 状态机与交互的测试用例，使 `cargo test` 成为 UI 行为的自动化验证门禁。

### Modified Capabilities

(无 —— `openspec/specs/` 当前为空，无既有 capability 的需求被改动。)

## Impact

- **`crates/app/Cargo.toml`**：新增 `[dev-dependencies]`（`gpui` 开 `test-support`、`tempfile`）；新增 `[lib]` 目标（`name = "lucy_app"`，`path = "src/lib.rs"`）。
- **`crates/app/src/lib.rs`**（新建）：re-export `pub mod workspace/terminal_view/theme/ui/assets`；`pub fn run()` 搬入 `main.rs` 的 `Application::new().with_assets(Assets).run(...)` 逻辑。
- **`crates/app/src/main.rs`**：瘦身为 `fn main() { lucy_app::run(); }`。
- **`crates/app/src/workspace/mod.rs`**：`WorkspaceView` 加 `#[cfg(test)]`-gated 测试 accessor；子模块 `dialogs/settings/sidebar/status_bar` 视测试需要改 `pub(crate)`。
- **`crates/app/src/terminal_view.rs`**：`TerminalView` 加 `#[cfg(test)]`-gated accessor（`snapshot_text`/`is_exited`/`selection_text` 等）。
- **`crates/app/tests/common/mod.rs`**（新建）：共享 harness。
- **`crates/app/tests/`**（新建）：`startup.rs`、`worktree_list.rs`、`new_worktree.rs`、`switch.rs`、`close.rs`、`agent_menu.rs`、`alias.rs`、`settings.rs`、`terminal_render.rs`、`terminal_input.rs`、`terminal_copy.rs`。
- **`CLAUDE.md`**：测试段补 app 层 `#[gpui::test]` 说明与运行命令。
