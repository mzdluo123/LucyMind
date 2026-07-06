## 1. Core: builtin agent 注册表

- [x] 1.1 在 `crates/core/src/agent/mod.rs` 新增 `AgentBuiltin` 结构(`name`/`display`/`icon`/`command`/`args`)与 `pub fn builtin_agents() -> &'static [AgentBuiltin]`,返回 claude/codex/opencode 三条(含各自 bypass 参数)
- [x] 1.2 重写 `AgentSpec::builtin` 改为查 `builtin_agents()` 表,保持 `TERM=xterm-256color` + worktree env 注入不变
- [x] 1.3 更新 / 新增单测:`builtin_claude_and_codex_available_without_config` 扩展覆盖 opencode(opencode `command=="opencode"`、`args==["--auto"]`);新增 `builtin_agents_contains_all_three`
- [x] 1.4 `cargo test -p lucy-core` 通过

## 2. Core: 配置预设覆盖语义保持

- [x] 2.1 确认 `resolve` 仍 `from_config(...).or_else(builtin(...))`,config args 完全覆盖 builtin(不改合并逻辑);`config_preset_overrides_default_claude_args` 测试仍通过
- [x] 2.2 新增测试:codex builtin 含 `--full-auto`,但 `[agents.codex] args=["--yolo"]` 时 resolve 返回 `["--yolo"]`

## 3. App: 图标资产

- [x] 3.1 新增 `crates/app/assets/icons/plus.svg`(单色 `fill="currentColor"`,与现有图标风格一致)
- [x] 3.2 新增 `crates/app/assets/icons/opencode.svg`
- [x] 3.3 `crates/app/src/assets.rs`:`AssetSource::load` match 加 `icons/plus.svg`、`icons/opencode.svg`;`list()` 补两条;`agent_icon` 加 `"opencode" => Some("icons/opencode.svg")`

## 4. App: 启动按钮 + 下拉菜单 UI

- [x] 4.1 `WorkspaceView` 加状态字段 `agent_menu_open: bool`(`mod.rs`),`new`/构造处初始化为 `false`
- [x] 4.2 `sidebar.rs`:AGENTS 标题行改 flex 行,右侧加 `+` 图标按钮(id `agent-launcher`,点击置 `agent_menu_open=true`),删除原 per-agent `for` 循环
- [x] 4.3 新增菜单渲染方法(如 `agent_menu` in `sidebar.rs` 或新 `workspace` 子模块):`absolute()` overlay = 半透明遮罩(点击关)+ 卡片(迭代 `builtin_agents()` 每项:图标 + display,hover 高亮,点击 `new_worktree_and_agent(&name)` 并关菜单)
- [x] 4.4 `mod.rs::render` 在 root 叠加菜单 overlay(条件 `self.agent_menu_open`,与现有 modal overlay 同位置 `root.child(...)`)
- [x] 4.5 Esc 关闭:菜单打开时按 Esc 置 `agent_menu_open=false`(复用 `on_key_down` 或在菜单 overlay 上处理)
- [x] 4.6 `cargo build -p lucy-app` 通过

## 5. App: 菜单数据驱动

- [x] 5.1 菜单项迭代 `lucy_core::agent::builtin_agents()`,不再在 sidebar 硬编码 agent 数组
- [x] 5.2 确认新增 agent 仅改注册表 + 图标即可出现在菜单(手动验证:临时加一条假 agent 看菜单是否多一项,验证后移除)

## 6. Dogfood 配置

- [x] 6.1 `.worktree.toml`:`[agents.claude]` 补 `args = ["--dangerously-skip-permissions"]`
- [x] 6.2 `.worktree.toml`:`[agents.codex]` 补 `args = ["--full-auto"]`
- [x] 6.3 `.worktree.toml`:新增 `[agents.opencode]` 段 `command = "opencode"` + `args = ["--auto"]`

## 7. 验证

- [x] 7.1 `cargo fmt && cargo clippy --all-targets` 无 warning
- [x] 7.2 `cargo test` 全绿(agent/config/app 相关测试全通过;hooks_test 3 项为预存 Windows shell 问题,与本次改动无关)
- [ ] 7.3 `cargo run -p lucy-app`:侧边栏 AGENTS 区只有 `+` 按钮;点击弹出含 Claude/Codex/OpenCode 三项的菜单;点一项建 worktree 并起对应 agent;点遮罩/Esc 关菜单
