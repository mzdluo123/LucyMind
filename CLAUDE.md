# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概览

LucyMind 是一个 macOS 桌面应用：编排 git worktree，并在每个 worktree 里起一个 AI agent（`claude` / `codex`）的真实终端。核心流程是「新建 worktree → 跑 postCreate hook → 在 worktree 里启动 agent → 显示在内嵌终端」。用 Rust + GPUI（Zed 的 GUI 框架）实现。本项目用自己开发自己（dogfooding，见根目录 `.worktree.toml`）。

## 常用命令

```bash
# 构建 / 运行(从仓库根启动会自动把 cwd 当作目标仓库)
cargo run -p lucy-app          # 起 GUI 窗口(debug)
cargo build --release          # release 构建

# 打包成标准 macOS .app(补 Info.plist,让全屏/菜单栏/Dock 正常;裸二进制缺这些)
cargo bundle --release         # 需先 `cargo install cargo-bundle`

# 测试(逻辑全在 core / terminal,app 层无测试)
cargo test                     # 全部
cargo test -p lucy-core        # 仅 core
cargo test -p lucy-core --test git_test          # 单个测试文件
cargo test -p lucy-core --test git_test add_then  # 单个测试(名字前缀匹配)

# 质量门(rust-toolchain.toml 已 pin rustfmt + clippy)
cargo fmt
cargo clippy --all-targets

# 调日志:app 默认 `warn,lucy_*=info`,可用 RUST_LOG 覆盖
RUST_LOG=lucy_terminal=debug cargo run -p lucy-app
```

## app 层 UI 集成测试(`#[gpui::test]`)

app 层用 GPUI 的 `TestAppContext`(headless,无需真实 GPU/窗口)做 UI 集成测试,
覆盖 `WorkspaceView` 状态机(启动/worktree CRUD/agent 菜单/终端渲染)。测试在
`crates/app/tests/`,共享基建在 `tests/common/mod.rs`。

```bash
cargo test -p lucy-app          # 跑全部(含 #[gpui::test])
cargo test -p lucy-app --test smoke        # 单个测试文件
cargo test -p lucy-app --test new_worktree # 单个文件
```

**关键约定:**
- **每次新增功能都必须在同一个变更中补充相关自动化测试和端到端测试。** 只有实现、相关测试与端到端测试全部完成并通过后,该功能才算完成。
- `#[gpui::test]` 异步测试签名为 `async fn(cx: &mut TestAppContext)`。
- `tests/common/mod.rs` 提供 `temp_repo()`(临时 git 仓库)、`build_workspace(cx, repo)`
  (headless 构造 `WorkspaceView`)、`wait_for(cx, predicate, timeout)`(轮询异步完成)、
  `shutdown_workspace(cx, &workspace)`(停终端 + 排空,避免 leak-detection 误报)。
- `WorkspaceView`/`TerminalView` 的内部状态通过 `#[cfg(feature = "test-support")]`-gated
  `pub fn` accessor 暴露(如 `active_path`/`worktree_count`/`snapshot_text`)。该 feature
  仅在 `cargo test` 时启用(`[dev-dependencies] lucy-app = { features = ["test-support"] }`)。
- `TestPlatform` 未实现 `prompt_for_paths`(原生文件选择器)—— 测试用
  `WorkspaceView::new_for_test`(不弹 prompt)+ `set_repo_for_test` 注入仓库。
- registry 持久化路径用 `set_registry_path_for_test` 隔离到 tempdir,避免污染真实用户 session。
- **新增 UI 功能改动必须伴随 `#[gpui::test]`** —— `cargo test -p lucy-app` 是 UI 行为的自动化验证门禁。
- **OpenSpec 变更的 tasks.md 必须包含测试任务** —— 每个变更的 tasks 要有独立的测试任务组,覆盖三类:
  - **单元测试**(`#[test]`,在 `mod tests`):纯逻辑函数(命令构造、枚举映射、路径规范化等),无 PTY / 无 GPUI context,快且确定;
  - **UI 状态测试**(`#[gpui::test]`):用 accessor(`*_for_test`)验证状态机(open/close/no-op/edge case),不依赖像素渲染;
  - **集成测试**(`#[gpui::test]`):用 `wait_for` 轮询 PTY 输出 / 端到端流程(shell 启动、agent 命令发送、tab CRUD 跨 worktree)。
  测试任务要在实现任务之后、质量门之前,且每个测试任务标注测什么(不只写「加测试」)。

## 架构：三层 crate，依赖单向向下

分层的核心动机是**把两个 pre-1.0 的不稳定依赖（GPUI、alacritty_terminal）圈进隔离层**，让下层逻辑保持纯净、可移植、可单测。

- **`crates/core`（`lucy-core`）— 纯逻辑层，零 GUI/终端依赖。** 全部单测在此。
  - `git/` — worktree 编排：用 `std::process::Command` 直接调 `git` CLI（不用 libgit2），复用 `--porcelain` 机器可读输出。把 git 的硬限制（分支被别的 worktree 占用、有未提交改动拒删）转成清晰可引导的 `GitError` 变体，而非甩原始报错。`branch_checked_out_at` / `has_uncommitted_changes` 是创建/删除前的安全检查。
  - `config/` — `.worktree.toml` 解析与校验。加载流程：读文件 → 强类型 TOML 反序列化（缺失给默认）→ 语义校验（产出**警告**如未知 key，和**错误**如 sibling 缺 dir）。写别名用 `toml_edit` 做**格式保留**的局部改写。
  - `hooks/` — 生命周期钩子引擎。`PostCreate` 先跑 `[copy]` 文件复制，再顺序执行 `post_create` shell 命令；`PreRemove` 只跑命令。失败策略：`fail_fast=true`（默认）首个失败即停，`false` 记录并继续。
  - `agent/` — agent 启动规格（`AgentSpec`：命令/参数/env/cwd）。**纯数据，不含 PTY**——起 PTY 是 terminal 层的事。
  - `session/` — Session 注册表，记录「哪些 worktree 是本工具开的」。存 `~/Library/Application Support/LucyMind/`（不进 git，是个人运行时状态）。作用：关闭时只对本工具建的 session 提供「关闭」，避免误删用户手建的 worktree。

- **`crates/terminal`（`lucy-terminal`）— 终端内核适配层，基于 `alacritty_terminal`（Zed 同款内核），不依赖 GPUI。**
  - `session.rs` — 用 alacritty 自带 tty + `EventLoop` 起 PTY 子进程、驱动 `Term`。`Term` 包进 `Arc<FairMutex<>>` 供渲染/PTY 线程共享；后台线程读 PTY → 解析进 Term → 发 `Wakeup`。**关键：`Event::PtyWrite` 必须自动回环成 `Msg::Input` 写回 PTY，否则 vim/shell 卡死。** 暴露 `RenderSnapshot`（cell 网格快照）供 app 渲染。
  - `input.rs` — 按键 → 终端字节序列编码（含功能键、alt-screen 下滚轮转方向键等）。
  - `palette.rs` — 默认 256 色调色板（alacritty 内核不带默认配色）。

- **`crates/app`（`lucy-app`，二进制名 `lucy`）— GPUI 应用层。** 唯一引入 GPUI 与 `gpui-component` 的地方。
  - `main.rs` — 起窗口，仓库根取 cwd（`cargo run` 场景）；cwd 不是 git 仓库时（`.app` 双击）以空态启动并弹目录选择器。
  - `workspace.rs`（最大文件）— 端到端主流程装配：左侧栏（仓库 + worktree 列表 + 动作）+ 右侧终端区，把 core 的 git/hooks/agent 接成完整闭环。
  - `terminal_view.rs` — 终端渲染 + 输入。渲染走**自定义 `Element`**（非 canvas），因 IME 需在 paint 阶段调 `window.handle_input` 并缓存 bounds/cell 尺寸供鼠标坐标映射。输入分三路：普通/功能键 `on_key_down` 编码写 PTY；IME 预编辑走 `EntityInputHandler`（组合完成才送 PTY）；鼠标拖选 + Cmd+C 复制、Cmd+V 走 bracketed-paste。
  - `theme.rs` — 集中的语义色 token（冷深色 / 几乎无彩 / 扁平无阴影 / 2px 微圆角）。**改颜色/圆角/间距只在这里改，不在组件里散用 hex。**
  - `assets.rs` — SVG 图标用 `include_bytes!` 编译进二进制（原生 app 不能引在线 URL）。新增图标要同时在此登记路径。

## 关键约定与陷阱

- **路径必须先 `canon()` 规范化**（见 `workspace.rs`）再用作 terminals map 的 key 或 active 比较。macOS 有 `/private` 前缀、绝对/相对差异——不规范化会导致「点击时路径」≠「存入时路径」，同一 worktree 被当成两个（表现：不高亮当前项、点当前项又顶掉正在跑的会话）。
- **主仓（main）行**可点击、可设别名，但**不可关闭**（关闭会删主仓）。从子目录启动时用 `git rev-parse --show-toplevel` 解析真正的主仓根，否则 main 保护会失效。
- **worktree 默认建在仓库外兄弟目录**（`../{repo}-worktrees`），保持主仓干净、不触发 IDE/watcher 递归扫描。`{repo}` 仅用于生成目录名，绝不进 hook 命令。`.gitignore` 兜底忽略 `*-worktrees/` 防误建在项目内。
- **hook 上下文通过环境变量传递**（`$WORKTREE_PATH` / `$WORKTREE_BRANCH` / `$WORKTREE_NAME` / `$REPO_ROOT`），**不做模板占位符**——这是刻意的设计（5 个先例无一用模板）。
- **`claude` 基于 Ink，启动必须是真 TTY**，否则崩。terminal 层用真 PTY 提供，agent 规格里额外兜底 `TERM=xterm-256color`。
- **rust-toolchain 固定 `stable` channel（非具体版本）**：GPUI 的 pre-1.0 API 常依赖最新编译器，让 rustup 始终取最新 stable。
- **workspace 依赖策略**：稳定依赖集中在根 `Cargo.toml` 的 `[workspace.dependencies]`；GPUI 与 alacritty_terminal 刻意不放进去，分别由 app / terminal crate 各自引入，把不稳定性圈进隔离层。
