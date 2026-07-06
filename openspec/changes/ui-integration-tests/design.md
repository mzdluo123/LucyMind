## Context

app 层（`crates/app`）是唯一引入 GPUI 的层，也是唯一无集成测试的层。现状：

- **测试覆盖**：`workspace/mod.rs` 6 个 + `terminal_view.rs` 4 个，全是纯函数单测（`canon`/`strip_verbatim_prefix`/`same_path`/`word_boundary`/`trim_end`），不构造任何 GPUI `Entity`/`Context`/`Window`。`WorkspaceView::new` 需 `&mut Context<Self>`、`TerminalView::new` 会 spawn 真实 PTY + `cx.spawn` 轮询、`InputState` 需 `gpui_component::init`——这些都无法在普通 `#[test]` 里构造。
- **GPUI 测试基础设施**（gpui 0.2.2，crates.io，源 `registry+https://github.com/rust-lang/crates.io-index`，即 Zed 官方发布）：提供完整第一方测试栈。
  - `#[gpui::test]` 宏（`pub use gpui_macros::test`，`src/gpui.rs:81`）：生成 `#[test]` 包装的 async 测试，签名为 `async fn(cx: &TestAppContext)`，支持多上下文（协作场景）。
  - `TestAppContext`（`src/app/test_context.rs`，`#[cfg(any(test, feature = "test-support"))]` gate）：`AppContext` 的测试实现。关键方法：`new`/`update`/`read`（访问 Entity 与 App 状态）、`add_window_view`（开窗口装 View，返回 `WindowHandle<V>`）、`run_until_parked`（跑完所有 pending async 任务——让 `cx.spawn` 的后台轮询跑完）、`simulate_keystrokes`/`simulate_input`/`dispatch_keystroke`/`dispatch_action`（键盘）、`simulate_click`/`simulate_mouse_move`/`simulate_mouse_down`/`simulate_mouse_up`/`simulate_modifiers_change`（鼠标）、`simulate_new_path_selection`/`simulate_prompt_answer`/`has_pending_prompt`（原生文件选择器/对话框——测 `open_repo_picker` 的关键）、`write_to_clipboard`/`read_from_clipboard`（测复制）、`notifications<T>`/`events<Evt,T>`/`next_event`（断言 `cx.notify()`/`emit`）、`spawn`。
  - `VisualTestContext`（窗口视图）：`update(&mut Window,&mut App)`、`draw<E>`（渲染元素拿结果）、`simulate_*`（窗口级鼠标/键盘）、`debug_bounds(selector)`（按选择器查元素 bounds）。
  - `TestPlatform`/`TestWindow`/`TestDispatcher`/`TestScreenCaptureSource`：headless 平台实现，**无需真实 GPU/显示器/窗口系统**，Windows/macOS/Linux 均可跑。
  - `text_system: Arc<TextSystem>`：`TestAppContext` 自带文本系统，`shape_line` 在 headless 下可用（`TerminalElement::paint` 的字体探针能工作）。
  - `test-support` feature 启用 `leak-detection`（entity 泄漏检测——测试结束未 drop 的 Entity 会让测试失败）。
- **terminal 层测试模式**：`crates/terminal/tests/session_test.rs` 用真实 PTY（`/bin/sh -c`、`/bin/cat`）+ `wait_for_text` 轮询（`drain_events` → `snapshot` → `contains`）+ 显式 `shutdown`。app 层 TerminalView 测试直接复用此模式。
- **core 层测试模式**：`crates/core/tests/*` 用 `tempfile::tempdir` + `git init`/`commit` 造临时仓库。app 层 `WorkspaceView::set_repo(temp_repo.path())` 直接注入，绕过 `open_repo_picker`。
- **app crate 结构**：纯 `[[bin]]`（`name = "lucy"`，`path = "src/main.rs"`），5 个私有 `mod`（`assets`/`terminal_view`/`theme`/`ui`/`workspace`），`WorkspaceView`/`TerminalView` 是 `pub struct` 但字段私有、子模块私有。集成测试（`tests/` 是外部 crate）无法 import 它们——必须 lib 化。

## Goals / Non-Goals

**Goals:**
- `cargo test` 覆盖 app 层全部 UI 状态机：启动（空态/有 repo）、worktree 列表渲染、新建+agent、切换、关闭（含脏工作树确认）、agent 菜单、别名、设置、终端渲染、终端输入、终端复制交互。
- headless、跨平台、确定性（`TestDispatcher` 种子化 + `run_until_parked` + temp repo 隔离）。
- 复用 core/terminal 已验证的测试模式（tempfile fixture、真实 PTY + `wait_for_text`）。
- harness 可复用、可扩展，新增 UI 功能时加测试成本低。

**Non-Goals:**
- **像素快照测试**：不对比渲染像素。`VisualTestContext::draw` 仅断言元素树结构（元素存在/可见性/文本），不截图比对——pre-1.0 GPUI 渲染细节易变，像素快照脆弱。
- **真实 agent（claude/codex）启动**：终端测试用 `/bin/sh`/`/bin/cat` 等通用命令，不起 claude（Ink 需真 TTY 且交互复杂）。agent 启动的端到端验证仍靠 dogfooding 手动。
- **macOS 专属系统集成**：不测 Dock/全屏/标题栏红绿灯/`cargo bundle` 产物。
- **性能/基准测试**：不测渲染帧率、PTY 吞吐。
- **core/terminal 层测试**：已有覆盖，不改。

## Decisions

### D1: 用 `TestAppContext` + `#[gpui::test]`（headless），不用真实窗口

`TestAppContext` 基于 `TestPlatform`（headless），无需真实 GPU/显示器，Windows/macOS/Linux 均可跑 `cargo test`。它提供完整的输入模拟（键盘/鼠标/文件选择器）、异步推进（`run_until_parked`）、状态/事件断言——满足"闭环验证 UI 状态"的需求，且确定性（`TestDispatcher` 种子化调度）。

**备选（否决）**：真实窗口 + 截图比对——跨平台不一致、CI 无显示器、渲染细节脆弱、慢。

### D2: lib+bin 分离，集成测试放 `crates/app/tests/`

app 目前是纯 `[[bin]]`，集成测试（外部 crate）无法 import 私有类型。新建 `src/lib.rs`，`pub mod` re-export 现有模块，`pub fn run()` 封装 `Application` 启动逻辑；`main.rs` 瘦身为 `fn main() { lucy_app::run() }`。集成测试放 `crates/app/tests/`，`use lucy_app::*`。

**备选（否决）**：inline `#[cfg(test)] mod tests`——能访问私有，但 harness 分散、源文件膨胀、与现有纯函数 inline 测试混用。lib 化是一次性小重构，换取 harness 集中（`tests/common/`）、测试与生产分离、可持续扩展。

### D3: dev-dependencies `gpui` 开 `test-support` + `tempfile`

`crates/app/Cargo.toml` 新增：
```toml
[dev-dependencies]
gpui = { version = "0.2.2", features = ["test-support"] }
tempfile = "3"
```
`test-support` 解锁 `pub mod test`（`TestAppContext`/`#[gpui::test]`/`VisualTestContext`）。注意：Cargo feature unification 会让 `cargo build`/`cargo run` 也带上 `test-support`（含 `leak-detection`）——`leak-detection` 仅加 backtrace 检测，运行时开销可忽略，可接受。

### D4: 状态/事件断言为主，元素树 `draw` 为辅，不做像素快照

- **主**：`cx.read(|cx| workspace.read(cx).active_path())` / `worktree_count()` / `is_agent_menu_open()` / `notifications::<()>()` 断言 `cx.notify()` 触发 / `events` 断言 emit——直接验证状态机。
- **辅**：`VisualTestContext::draw` 渲染元素树，断言结构（元素存在、文本内容、可见性），不截图。
- **不做**：像素对比。pre-1.0 GPUI 渲染细节跨版本易变，维护成本高。

### D5: 终端测试用真实 PTY（`/bin/sh`），复用 `wait_for_text` 模式

TerminalView 测试用 `TerminalSession::spawn` 起真实 `/bin/sh -c 'printf ...'`/`/bin/cat`，`run_until_parked` + 轮询 `workspace.read(cx).terminal_snapshot_text(path)` 断言输出。测试结束显式 `shutdown` 终端 + `run_until_parked` 排空，避免 `leak-detection` 误报。

**备选（否决）**：mock `TerminalSession`——TerminalView 的 polling/resize/输入编码/copy 是测 app 层包装，mock 绕过了真实 PTY 交互；terminal 层已验证 PTY 可靠，app 层应测真实集成。

### D6: temp git repo fixture + registry 路径隔离

`tests/common/mod.rs` 的 `temp_repo()`：`tempfile::tempdir()` + `git init` + `git commit --allow-empty -m init`（复用 core 层 `git_test.rs` 模式）。`WorkspaceView::set_repo(temp_repo.path())` 直接注入，绕过 `open_repo_picker`。`Registry` 存储路径用 `tempfile::tempdir()` 隔离，避免污染 `~/Library/Application Support/LucyMind/`。

### D7: `gpui_component::init(cx)` + `Root` 包裹

`build_workspace` harness 复刻 `main.rs` 的启动序列：`gpui_component::init(cx)`（`InputState` 等组件依赖的全局 theme/state）+ `WorkspaceView` 包进 `gpui_component::Root`（Input/弹层/焦点管理依赖 Root，否则渲染/聚焦 Input 会 panic）。测别名/设置对话框（含 `InputState`）必须如此。

### D8: 测试 accessor 用 `#[cfg(test)]`-gated `pub` 方法

`WorkspaceView`/`TerminalView` 字段私有，集成测试（外部 crate）需观察状态。加 `#[cfg(test)] pub fn active_path(&self) -> Option<&Path>` 等 accessor——仅测试构建可见，生产二进制不含。避免直接 `pub` 字段暴露内部。

### D9: 测试分层

1. **纯函数扩展**（现有 inline `#[cfg(test)] mod tests`）：`canon`/`word_boundary`/`keystroke_to_bytes`/`paint_grid` cell→text 等纯逻辑，继续 inline 单测。
2. **GPUI 状态机集成测试**（`tests/`）：`#[gpui::test]` + `TestAppContext`，覆盖 WorkspaceView 状态机与交互。主体。
3. **渲染元素树**（`tests/`，辅）：`VisualTestContext::draw` 断言元素结构（侧边栏 worktree 行数、菜单项、对话框可见性）。

## Risks / Trade-offs

- **[leak-detection 误报]** → `test-support` 启用 `leak-detection`：测试结束未 drop 的 `Entity`/`Subscription`/`Task` 会让测试失败。缓解：每个测试 `run_until_parked` 排空 + 显式 `shutdown` 终端 + drop Entity；harness 提供 `teardown` 辅助。TerminalView 的 16ms 轮询 `cx.spawn` 是长任务，必须显式 `shutdown` 终止。
- **[feature unification]** → dev-dep `test-support` 会 unify 到 `cargo build`/`run`（Cargo 全局 feature 合并）。`leak-detection` 仅加 backtrace，运行时开销可忽略；若不可接受，可拆独立 `lucy-app-test` crate（不在此 change 范围）。
- **[TestAppContext 跨平台 headless]** → `TestPlatform` 无真实 GPU，但 `TerminalElement::paint` 的 `shape_line` 字体探针依赖 `WindowTextSystem`——`TestAppContext` 自带 `text_system: Arc<TextSystem>`，headless 下可用。Windows 上 `Cascadia Mono` 可能未安装，GPUI 字体回退链处理（回退到系统默认等宽）。
- **[真实 PTY 跨平台]** → `/bin/sh`/`/bin/cat` 在 macOS/Linux 有；Windows CI 用 `cmd.exe`。harness 的 PTY 命令按 `cfg!(windows)` 选择。terminal 层 `session_test.rs` 已有 `/bin/sh` 模式，Windows 上需适配（本 change 范围：harness 按 platform 选命令）。
- **[pub 测试 accessor 暴露内部]** → `#[cfg(test)]`-gated，生产二进制不含，API 表面不膨胀。权衡可接受。
- **[GPUI pre-1.0 API 易变]** → `TestAppContext` API 跨 gpui 版本可能变。本 change pin `gpui = "0.2.2"`，升级时需同步 harness。
