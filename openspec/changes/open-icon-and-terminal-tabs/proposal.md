## Why

侧边栏仓库行的「Open…」按钮目前用文字按钮(`button("open-repo", "Open…")`),与同行的齿轮 / `+` 图标按钮风格不统一——后者是单色 SVG 图标 + group-hover 染色,视觉更轻。文字按钮在侧边栏窄空间里占位明显,换成图标后更紧凑、更编辑器风。

同时,右侧终端区目前一个 worktree 只能有一个终端(`terminals: HashMap<PathBuf, Entity<TerminalView>>`,一路径对应一终端)。用户常需要在同一 worktree 里同时跑 agent + 一个 shell(查 git status / 跑测试 / tail 日志),或并排两个 agent 会话。当前只能关掉重开,无法保留多个会话。Zed / VS Code / iTerm2 都有 tab 多终端,这是终端面板的基本能力。

此外,当前「建 worktree 即自动起 agent」的流程不够灵活——用户没有机会在 agent 启动前检查 / 修改环境,且 agent 以独立子进程起(退出即终),无法在同一终端里先跑几条命令再起 agent。改为「建 worktree → 开 shell → 用户在 tab 栏点 agent 按钮往 shell 里发命令」后,用户有更多控制空间:可以看到命令、Ctrl+C 回到 shell、同终端跑多个 agent。

## What Changes

### 1. Open 按钮换图标

- 仓库行的「Open…」文字按钮替换为 `folder-open.svg` 图标按钮(单色 SVG,group-hover 染色),与 WORKTREES 标题行的齿轮按钮、AGENTS 标题行的 `+` 按钮风格完全一致。
- 新增 `crates/app/assets/icons/folder-open.svg`(Lucide 风格,`stroke="currentColor"`)并在 `assets.rs` 登记。

### 2. 终端面板多 tab

- **数据模型**: `terminals` 从 `HashMap<PathBuf, Entity<TerminalView>>` 改为 `HashMap<PathBuf, TerminalGroup>`,每个 `TerminalGroup` 含 `tabs: Vec<TerminalTab>` + `active_tab: usize`。`TerminalTab` 含 `terminal: Entity<TerminalView>` + `title: String`(静态回退标题)。
- **Tab 标题动态化**: `TerminalView` 新增 `title: Option<String>` 字段,收到 `TermEvent::Title(t)`(OSC 0/2 协议)时存储;tab 栏渲染时优先取动态标题、回退到静态标题("Shell")。
- **Tab 栏**: 终端区顶部新增水平 tab 栏(仅当 active worktree 有终端时显示)。每个 tab = 标题 + `✕` 关闭按钮;末尾 `+` 按钮新建 shell 终端。active tab 高亮(冷白底 / 顶部标记线)。
- **新建 tab**: tab 栏 `+` 按钮在当前 worktree 起一个默认 shell 终端(复用 `open_worktree` 的 shell 起终端逻辑)。
- **切换 tab**: 点 tab 切到该终端。切 worktree 时保留各 group 的 active_tab。
- **关闭 tab**: 点 tab 上的 `✕` 只关该终端(停 PTY + 从 tabs 移除),worktree 仍在。关最后一个 tab 后 group 清空,终端区回到空态(select an action to begin),worktree 仍留在侧边栏。
- **关 worktree**: 从侧边栏关闭 worktree 时,先停该 group 内**所有** tab 的终端,再走既有 git remove 流程。

### 3. 工作流改为「建 worktree → 开 shell → tab 栏点 agent 按钮发命令」

- **`new_worktree_and_agent` 改为 `new_worktree`**: 侧边栏 `+` 按钮不再弹 agent 下拉菜单,而是直接建 worktree → 跑 postCreate hook → 开一个 **shell** 终端 tab(不自动起 agent)。删掉 `agent_menu_open` 状态和 `agent_menu()` 渲染。
- **Tab 栏 agent 按钮**: tab 栏右侧(与 `+` 按钮同行)渲染一排 agent 按钮(迭代 `builtin_agents()`:Claude / Codex / OpenCode),每个按钮显示 agent 图标 + 名字。点击后构造命令字符串(`command args\n`,来自 `AgentSpec::resolve`),通过 `TerminalView::send_text` 写入当前 active tab 的 shell PTY。用户可以看到命令、修改、Ctrl+C 回到 shell。
- **`TerminalView::send_text`**: 新增公开方法,向 PTY 写入文本(内部调 `session.write_input(bytes)`)。供 agent 按钮发命令、也可供未来其他场景(如快捷键发命令)。
- **Shell 已有 worktree 环境变量**: shell 终端在 `new_worktree` / `open_worktree` 创建时已注入 `TERM=xterm-256color` + worktree env(`WORKTREE_PATH` / `WORKTREE_BRANCH` 等),agent 命令在 shell 里执行时自动继承,无需额外处理。
- **Agent 按钮仅在有终端时可见**: active worktree 无终端(空态)时不显示 agent 按钮(无 shell 可发命令)。

## Capabilities

### New Capabilities

- `open-repo-icon`: 仓库行的「打开仓库」入口从文字按钮改为图标按钮,与侧边栏其他图标按钮(齿轮 / `+`)风格统一。
- `terminal-tabs`: 终端面板顶部的多 tab 栏——每个 worktree 可有多个终端(Shell),tab 栏展示 / 切换 / 新建 / 关闭;tab 栏右侧有 agent 按钮往当前 shell 发命令启动 agent。
- `shell-first-workflow`: 建 worktree 后开 shell(不自动起 agent),agent 通过 tab 栏按钮往 shell 发命令启动,用户有更多控制空间。

### Modified Capabilities

(无 —— `openspec/specs/` 当前为空,无既有 capability 的需求被改动。但本变更实质上替换了 `agent-launcher-menu` 变更引入的「侧边栏 `+` 弹 agent 下拉菜单」流程,改为「侧边栏 `+` 直接建 worktree + shell,agent 在 tab 栏发命令」。)

## Impact

- **`crates/app/src/workspace/mod.rs`**: `terminals` 字段类型改 `HashMap<PathBuf, TerminalGroup>`;`active` 不变;`new_worktree_and_agent` 重构为 `new_worktree`(建 worktree + 开 shell tab,不起 agent 子进程);`open_worktree` / `request_close` / `do_close` 适配多 tab;`render` 主区加 tab 栏;新增 `new_terminal_tab` / `close_tab` / `switch_tab` / `send_agent_command` 方法;删除 `agent_menu_open` 状态和 `agent_menu()` 渲染;测试 accessor 适配。
- **`crates/app/src/workspace/tabs.rs`**(新文件): tab 栏渲染方法(`impl WorkspaceView` 跨文件 impl),含 tab 列表 + `+` 新建按钮 + agent 按钮行;标题取 `terminal.read(cx).title()` 优先、静态 `TerminalTab.title` 回退。
- **`crates/app/src/terminal_view.rs`**: 新增 `title: Option<String>` 字段;事件循环 `TermEvent::Title(t)` 分支改为存储标题;新增 `pub fn title(&self) -> Option<&str>` 和 `pub fn send_text(&self, text: &str)`(写 PTY)。
- **`crates/app/src/workspace/sidebar.rs`**: 仓库行「Open…」按钮换图标;AGENTS 标题行 `+` 按钮改为直接调 `new_worktree`(不弹菜单);删除 `agent_menu()` 方法。
- **`crates/app/src/assets.rs`**: 登记 `folder-open.svg`。
- **`crates/app/assets/icons/folder-open.svg`**: 新增 Lucide 风格图标。
- **`crates/app/tests/`**: 现有测试适配新数据模型 + 新流程(`new_worktree_and_agent_for_test` → `new_worktree_for_test`);新增多 tab 测试 + agent 按钮发命令测试。
