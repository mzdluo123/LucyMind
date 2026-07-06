## Why

当前 tab 栏有三个问题:

1. **Tab 溢出未解决**:`+` 按钮已移出滚动区,但 tab 用 `flex_1` + `min_w(40px)` 自适应缩窄,tab 多时被压到 40px 宽(几乎只剩 `✕`,标题完全不可读)。应该用 `flex_1` + `min_w(80px)` + `max_w(200px)`:tab 少时填满可用宽度(≤200px),tab 多时缩窄到 80px 下限(可读),超出后 `overflow_x_scroll` 横向滚动(GPUI `overflow_x_scroll` 自动把垂直滚轮转横向滚动)。

2. **Agent 按钮行太丑**:tab 栏右侧固定排三个 agent 按钮(Claude / Codex / OpenCode),不论是否需要都占位,视觉杂乱。应该合并到 `+` 按钮里,点击弹出下拉菜单,用户按需选择「开新 shell」或「启动 agent」。Windows 上还应支持选择 shell 类型(cmd / PowerShell / PowerShell 7)。

3. **无法在文件管理器中打开 worktree 目录**:用户经常需要在外部文件管理器(Finder / Explorer)中查看 worktree 目录结构,目前没有此入口。应在 `+` 按钮旁加一个「在文件管理器中打开」按钮。

## What Changes

### 1. Tab 栏布局重构 — 自适应宽度 tab + 横向滚动 + `+` / reveal 按钮固定右侧

- `+` 按钮从 `tab_list`(滚动区域内)移到 `tab_bar`(固定区域),始终可见,不被 tab 挤走。
- Tab 用 `flex_1` + `min_w(80px)` + `max_w(200px)`:tab 少时填满可用宽度(≤200px),tab 多时缩窄到 80px 下限(可读),超出后 `overflow_x_scroll` 横向滚动(GPUI 自动把垂直鼠标滚轮转为横向滚动)。考虑窗口大小:宽窗口 tab 少时 tab 撑宽,窄窗口 tab 多时缩窄后滚动。
- `+` 按钮旁新增「在文件管理器中打开」按钮(folder-open 图标),点击调用系统命令打开 active worktree 目录(macOS: `open`,Windows: `explorer`)。
- 删除 `agent_buttons` 行(合并到 `+` 下拉菜单),tab 区获得全部宽度。

### 2. `+` 按钮改为下拉菜单(launcher menu)

- `+` 按钮点击不再直接 `new_terminal_tab`,而是切换 `launcher_menu_open` 状态,弹出下拉菜单。
- 菜单分两组:
  - **New Tab**:Default Shell(系统默认)、Windows 上追加 Command Prompt(cmd.exe)、PowerShell(powershell.exe)、PowerShell 7(pwsh.exe,若安装)。
  - **Launch Agent**:迭代 `builtin_agents()`(Claude / Codex / OpenCode),每个 agent 一项。
- 选 New Tab 项 → `new_terminal_tab(shell_kind)`:创建新 shell tab,shell 类型由菜单项决定。
- 选 Launch Agent 项 → `launch_agent(name)`:创建新 shell tab(Default shell)+ 立即往新 tab 发送 agent 命令(`send_agent_command`)。每个 agent 有独立 tab,可并行运行。
- 菜单点击外部 / Esc 关闭(与 `context_menu` 同模式:backdrop + `stop_propagation`)。
- **BREAKING**(内部):`new_terminal_tab(cx)` 签名改为 `new_terminal_tab(shell: ShellKind, cx)`;`spawn_shell_tab` 增加 `shell: ShellKind` 参数决定 `command`。

### 3. 在文件管理器中打开 worktree 目录

- `+` 按钮旁新增「在文件管理器中打开」按钮(folder-open 图标),固定在 tab 栏右侧(`+` 按钮右边)。
- 点击调用系统命令打开 active worktree 的目录路径:
  - macOS: `open <path>`
  - Windows: `explorer <path>`
  - Linux: `xdg-open <path>`
- 无 active worktree(空态)时不渲染(tab 栏本身 `h_0` 隐藏)。
- 按钮在 `tab_bar` 中(`tab_list` 之外),不受 tab 滚动影响。

### 4. ShellKind 枚举

- 新增 `enum ShellKind { Default, Cmd, PowerShell, Pwsh }`(Windows 才有 `Cmd` / `PowerShell` / `Pwsh` 变体)。
- `ShellKind::command() -> Option<(String, Vec<String>)>`:`Default` → `None`(alacritty 默认 shell);`Cmd` → `("cmd.exe", [])`;`PowerShell` → `("powershell.exe", [])`;`Pwsh` → `("pwsh.exe", [])`。
- `spawn_shell_tab(wt_path, shell, cx)` 传 `shell.command()` 给 `TerminalView::new`。
- `TerminalTab.title` 改为反映 shell 类型("Shell" / "cmd" / "PowerShell" / "pwsh"),作为动态标题(OSC 0/2)的回退。

## Capabilities

### New Capabilities

- `launcher-menu`: `+` 按钮下拉菜单,统一「新建 tab」与「启动 agent」入口。菜单分 New Tab(可选 shell 类型)和 Launch Agent 两组,选中后创建新 tab(启动 agent 时新 tab + 发命令)。替代原 tab 栏右侧的 agent 按钮行。
- `tab-overflow`: tab 自适应宽度(`flex_1` + `min_w(80px)`/`max_w(200px)`,考虑窗口大小)+ `overflow_x_scroll` 横向滚动(GPUI 自动把垂直鼠标滚轮转横向滚动),`+` 按钮和「在文件管理器中打开」按钮移出滚动区固定右侧,解决 tab 多时溢出 / `+` 被滚走的问题。
- `reveal-in-file-manager`: tab 栏「在文件管理器中打开」按钮,点击用系统命令(`open` / `explorer` / `xdg-open`)打开 active worktree 目录。

### Modified Capabilities

(无 —— `openspec/specs/` 当前为空,无既有 capability 的需求被改动。但本变更实质上修改了 `open-icon-and-terminal-tabs` 变更引入的 tab 栏布局:agent 按钮行删除、`+` 按钮改为下拉菜单、tab 宽度自适应。)

## Impact

- **`crates/app/src/workspace/mod.rs`**:新增 `launcher_menu_open: bool` 状态 + `ShellKind` 枚举;`spawn_shell_tab` 增加 `shell: ShellKind` 参数;`new_terminal_tab(cx)` 改为 `new_terminal_tab(shell: ShellKind, cx)`;新增 `launch_agent(name, cx)`(= `new_terminal_tab(Default)` + `send_agent_command`);新增 `reveal_in_file_manager(cx)`(调系统命令打开 active worktree 目录);`render` 叠加 launcher menu overlay;Esc 关闭菜单。测试 accessor 适配 `new_terminal_tab_for_test` 签名。
- **`crates/app/src/workspace/tabs.rs`**:`tab_bar` 删除 `agent_buttons` 调用;`tab_list` 的 `+` 按钮移到 `tab_bar`(固定位置);tab `flex_none` + `min_w(80px)` 改为 `flex_1` + `min_w(80px)` + `max_w(200px)`(自适应宽度 + 横向滚动);`tab_bar` 新增「在文件管理器中打开」按钮(folder-open 图标);新增 `launcher_menu(&self, cx)` 渲染下拉菜单(New Tab 组 + Launch Agent 组 + backdrop);`+` 按钮点击改为切换 `launcher_menu_open`。
- **`crates/app/tests/`**:`new_terminal_tab_for_test` 签名改(加 `ShellKind` 参数);`send_agent_command_for_test` 不变(仍发命令到 active tab);新增 launcher menu 测试(打开 / 关闭 / 选 shell 类型 / 选 agent 创建新 tab + 发命令);新增 `reveal_in_file_manager` 测试。
