## Why

侧边栏 Agents 区目前为每个 agent 堆一个按钮(Claude、Codex),加一个就多一行,不随 agent 数量优雅伸缩。同时 `opencode` 尚未支持,且三个 agent 的「自动 / bypass 权限」默认不一致:`claude` 的 builtin `--dangerously-skip-permissions` 在 `.worktree.toml` 定义了 `[agents.claude]` 时会被静默丢弃(config 预设完全覆盖 builtin args),`codex` 根本没有 auto 模式参数。本工具的核心价值就是「在隔离 worktree 里一键起 agent 干活」,权限弹窗每次打断是反模式。

## What Changes

- **单按钮 + 下拉菜单取代多按钮**:Agents 区只留一个 `+` 启动按钮(放在 AGENTS 标题行右侧,与 WORKTREES 标题行的齿轮按钮对称),点击弹出下拉菜单列出可选 agent(Claude / Codex / OpenCode),选中后走既有 `new_worktree_and_agent` 流程。
- **新增 `opencode` agent**:builtin 注册 `opencode`(命令 `opencode`,参数 `["--auto"]` —— 自动批准非显式拒绝的权限请求)。
- **统一 auto/bypass 默认**:三个 agent 的 builtin 默认都带各自的自动工作 / bypass 权限参数(claude `--dangerously-skip-permissions`、codex `--full-auto`、opencode `--auto`),让零配置即「无人值守」。
- **agent 列表数据驱动**:菜单项来自 builtin agent 注册表(而非侧边栏硬编码数组),新增 agent 只改注册表 + 图标,不动 UI。
- **新增图标资源**:`plus.svg`(启动按钮)、`opencode.svg`(菜单项图标),并在 `assets.rs` 登记。
- **修复 dogfood 配置**:`.worktree.toml` 的 `[agents.*]` 预设显式带上各自 bypass 参数(因 config 预设会完全覆盖 builtin args,当前空 args 把 claude 的 bypass 也丢了)。

## Capabilities

### New Capabilities

- `agent-launcher`: agent 启动入口的 UI 与数据模型 —— 单个 `+` 按钮触发下拉菜单、菜单项来自 builtin agent 注册表、注册表为 terminal 层提供含 auto/bypass 参数的 AgentSpec。

### Modified Capabilities

(无 —— `openspec/specs/` 当前为空,无既有 capability 的需求被改动。)

## Impact

- **`crates/core/src/agent/mod.rs`**:`AgentSpec::builtin` 增加 `opencode` 分支与 `codex` 的 `--full-auto`;新增一个 builtin agent 注册表(名 / 显示名 / 图标 key / 命令 / 参数),供 UI 与 resolve 共用。
- **`crates/app/src/workspace/sidebar.rs`**:删除 per-agent 按钮循环,AGENTS 标题行加 `+` 按钮;下拉菜单 overlay 渲染。
- **`crates/app/src/workspace/mod.rs`**:新增 `agent_menu_open: bool` 状态;`render` 叠加菜单 overlay;点击外部 / 选中项 / Esc 关闭。
- **`crates/app/src/assets.rs`**:登记 `plus.svg`、`opencode.svg`;`agent_icon` 增加 opencode 映射。
- **`crates/app/assets/icons/`**:新增 `plus.svg`、`opencode.svg`。
- **`.worktree.toml`**(dogfood):三个 `[agents.*]` 预设补上各自 args。
- **测试**:`agent/mod.rs` 单测覆盖 opencode builtin、codex `--full-auto`、注册表枚举。
