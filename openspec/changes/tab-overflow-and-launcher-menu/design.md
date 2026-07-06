## Context

### 现状

**Tab 栏布局**(`crates/app/src/workspace/tabs.rs`):

```
┌──────────────────────────────────────────────────────────────┐
│ [Tab1] [Tab2] [+] ←── tab_list (flex_1, overflow_x_scroll) ──→ [Claude][Codex][OpenCode] │
└──────────────────────────────────────────────────────────────┘
```

- `tab_bar()` = `tab_list`(flex_1 + overflow_x_scroll)+ `agent_buttons`(flex_none)。
- `tab_list` 内:每个 tab `flex_none` + `min_w(80px)` + `max_w(200px)`,末尾 `+` 按钮。
- `agent_buttons`:迭代 `builtin_agents()`,每个 agent 一个按钮(图标 + display 名),`BTN_BG` 底 + `BORDER` 描边。

**问题**:
1. `+` 按钮在 `tab_list` 滚动区域内 — tab 多了被滚走,看不到也点不到。
2. `agent_buttons` 固定占位 — 三个按钮挤占 tab 宽度,视觉杂乱。
3. tab `flex_none` + `min_w(80px)` — tab 多时不能缩窄,只能滚动,内容溢出屏幕。
4. (已修复但方案不对)tab 改 `flex_1` + `min_w(40px)` 后 tab 被压到 40px 宽,标题完全不可读。应该用固定宽度 + 横向滚动。
5. 无法在外部文件管理器中打开 worktree 目录。

**`spawn_shell_tab`**(`mod.rs:298`):`TerminalView::new(cx, Some(cwd), None, env)` — `command=None` 始终用系统默认 shell(Windows 上是 powershell.exe)。无法选 cmd / PowerShell 7。

**`send_agent_command`**(`mod.rs:642`):往当前 active tab 的 shell PTY 发命令字符串。与 `new_terminal_tab` 分离 — agent 按钮发命令到已有 shell,不创建新 tab。

**菜单 overlay 模式**(`terminal_view.rs:681-800`):`context_menu` 用 `absolute().inset_0()` backdrop + `absolute().left/top` card + `on_mouse_down(stop_propagation)` 模式。launcher menu 复用此模式。

### 约束

- GPUI 的 `div().relative()` + 子 `div().absolute()` 实现定位菜单;`overflow_x_scroll()` 提供横向滚动。
- `TerminalView::new(cx, cwd, command, env)` 的 `command: Option<(String, Vec<String>)>` 已支持指定 shell 程序(`None` = 系统默认)。无需改 terminal crate。
- `TerminalSession::spawn` 在 Windows 上对 `.cmd` / `.ps1` shim 会用 `cmd.exe /C` 包装(`session.rs:156-167` 的 `needs_cmd_wrapper`)。`cmd.exe` / `powershell.exe` / `pwsh.exe` 都是 `.exe`,`needs_cmd_wrapper` 返回 false,直接执行。
- `builtin_agents()` 返回 `&[AgentBuiltin]`(core 层),含 `name` / `display` / `icon` / `command` / `args`。launcher menu 迭代它生成 agent 菜单项。
- `ShellKind` 是 app 层概念(core 层不需要知道 shell 类型选择)。`spawn_shell_tab` 在 app 层把 `ShellKind` 转成 `Option<(String, Vec<String>)>` 传给 `TerminalView::new`。
- `cx.notify()` 触发重绘;菜单状态变化(`launcher_menu_open`)后必须 `cx.notify()`。
- 现有测试用 `new_terminal_tab_for_test()`(无参数)建 tab。签名改了需适配。

## Goals / Non-Goals

**Goals:**

- `+` 按钮和「在文件管理器中打开」按钮始终可见(不被 tab 滚走),固定在 tab 栏右侧。
- 「在文件管理器中打开」按钮点击后用系统命令打开 active worktree 目录。
- 菜单分 New Tab(shell 类型选择)和 Launch Agent(agent 选择)两组。
- Windows 上可选 cmd / PowerShell / PowerShell 7;非 Windows 只有 Default Shell。
- 选 agent → 创建新 tab(Default shell)+ 立即发 agent 命令(每个 agent 独立 tab,可并行)。
- Tab 固定 200px 宽度(`flex_none` + `w(px(200.0))`):tab 多了不缩窄,而是 `overflow_x_scroll` 横向滚动(GPUI 自动把垂直鼠标滚轮转横向滚动)。
- 删除 agent 按钮行,tab 区获得全部宽度。
- 菜单点击外部 / Esc 关闭。
- 现有测试适配,新增 launcher menu + tab 溢出 + reveal 按钮 测试。

**Non-Goals:**

- 不做 tab 拖拽重排。
- 不做 tab 分屏。
- 不做 tab 右键菜单。
- 不做 shell 类型探测(不检查 pwsh 是否安装,直接列选项;选了不存在的 shell 由 PTY spawn 报错)。
- 不改 `send_agent_command` 的命令字符串构造逻辑(`agent_command_string` 不变)。
- 不改 `TerminalView` / `TerminalSession`(shell 类型通过 `command` 参数传入,已有接口)。
- 不做「发 agent 命令到已有 shell」的菜单项(agent 启动始终创建新 tab,避免发到正在跑命令的 shell)。

## Decisions

### D1:`+` 按钮和 reveal 按钮移出 `tab_list`,固定在 `tab_bar` 右端

**现状**:`+` 按钮是 `tab_list`(flex_1 + overflow_x_scroll)的最后一个 child,tab 多了被滚走。

**改为**:`tab_bar` 结构从 `[tab_list(+)] [agent_buttons]` 改为 `[tab_list] [+] [reveal]`:

```
┌────────────────────────────────────────────┬──┬──┐
│ [Tab1] [Tab2] [Tab3] [...]  ← scrollable  │+ │📁│
└────────────────────────────────────────────┴──┴──┘
```

- `tab_list` 只含 tab 项(不含 `+`),`flex_1` + `overflow_x_scroll` + `min_w_0`。
- `+` 按钮是 `tab_bar` 的直接 child(`flex_none`),始终可见。
- `reveal` 按钮(folder-open 图标)是 `tab_bar` 的直接 child(`flex_none`),在 `+` 右边,始终可见。
- `agent_buttons` 整个方法删除。

**备选(否决)**:把 `+` 留在 `tab_list` 内但用 `flex_none` 固定 — `overflow_x_scroll` 的 child 都在滚动流里,`flex_none` 不能把它拉出滚动流。必须移到滚动容器外。

### D2:Tab 自适应宽度 + 横向滚动(`flex_1` + `min_w`/`max_w`)

**现状**:tab `flex_none` + `min_w(80px)` + `max_w(200px)`,固定宽度,多了只能滚动。

**之前改为(有问题)**:tab `flex_none` + `w(px(200.0))` 固定 200px — 窗口窄时 3-4 个 tab 就溢出,没有考虑窗口大小。

**最终改为**:tab `flex_1` + `min_w(80px)` + `max_w(200px)`:

- 少量 tab + 宽窗口:每个 tab grow 到 `max_w(200px)`,填满可用宽度(与浏览器一致)。
- 中等数量:tab 等分可用宽度(如 5 tab / 700px = 每个 140px)。
- 大量 tab / 窄窗口:每个 tab shrink 到 `min_w(80px)`(可读:✕ + 几个字符),超出才 `overflow_x_scroll`。
- GPUI 的 `overflow_x_scroll`(`overflow.x = Scroll` + `overflow.y != Scroll`)自动把垂直鼠标滚轮转为横向滚动(div.rs:2424-2428)。用户鼠标悬停 tab 区滚轮即可翻阅。

`flex_1` = `flex-grow:1 + flex-shrink:1 + flex-basis:0%`,所有 tab 等分宽度。`min_w(80px)` 是 CSS flexbox 的硬下限 — tab 不会缩到 80px 以下,超出后触发滚动。

**为什么 `min_w` 用 80px 而非 40px**:40px 只显示 `✕`,标题完全不可读。80px 可显示 `✕` + 3-4 个字符,足够辨识。`min_w` 是 tab 缩窄的最后防线 — 到此宽度后不再缩,改用滚动。

**为什么不用固定 200px**:固定宽度不考虑窗口大小 — 窗口窄时 3-4 个 tab 就溢出需要滚动,而窗口宽时少量 tab 又不填满空间(视觉空旷)。`flex_1` 让 tab 适应窗口宽度,少时宽多时窄,80px 下限后才滚动。

### D3:`ShellKind` 枚举 + `spawn_shell_tab` 参数

```rust
/// 用户可选的 shell 类型(launcher menu 的 New Tab 组)。
enum ShellKind {
    Default,     // 系统默认 shell(command = None)
    #[cfg(windows)]
    Cmd,         // cmd.exe
    #[cfg(windows)]
    PowerShell,  // powershell.exe (Windows PowerShell 5.x)
    #[cfg(windows)]
    Pwsh,        // pwsh.exe (PowerShell 7+)
}

impl ShellKind {
    /// 转成 `TerminalView::new` 的 `command` 参数。
    fn command(&self) -> Option<(String, Vec<String>)> {
        match self {
            ShellKind::Default => None,
            #[cfg(windows)]
            ShellKind::Cmd => Some(("cmd.exe".into(), vec![])),
            #[cfg(windows)]
            ShellKind::PowerShell => Some(("powershell.exe".into(), vec![])),
            #[cfg(windows)]
            ShellKind::Pwsh => Some(("pwsh.exe".into(), vec![])),
        }
    }

    /// tab 标题回退(终端发 OSC 0/2 前显示)。
    fn label(&self) -> &'static str {
        match self {
            ShellKind::Default => "Shell",
            #[cfg(windows)]
            ShellKind::Cmd => "cmd",
            #[cfg(windows)]
            ShellKind::PowerShell => "PowerShell",
            #[cfg(windows)]
            ShellKind::Pwsh => "pwsh",
        }
    }
}
```

`spawn_shell_tab` 签名:`fn spawn_shell_tab(&self, wt_path: &Path, shell: ShellKind, cx: &mut Context<Self>) -> TerminalTab`。把 `shell.command()` 传给 `TerminalView::new` 的 `command` 参数,`shell.label()` 存入 `TerminalTab.title`。

**`new_terminal_tab` 签名**:`fn new_terminal_tab(&mut self, shell: ShellKind, cx: &mut Context<Self>)`。内部调 `spawn_shell_tab(active_path, shell, cx)`。

**为什么 `ShellKind` 是 app 层而非 core 层**:core 层的 `AgentSpec` 描述 agent 启动规格(命令 / 参数 / env),不关心 shell 选择。shell 选择是 UI 交互概念(launcher menu 的菜单项),app 层把 `ShellKind` 转成 `Option<(String, Vec<String>)>` 传给 `TerminalView::new` 即可,无需改 core。

**为什么不探测 pwsh 是否安装**:探测需要 `which` / `where` 命令或遍历 PATH,增加复杂度。选了不存在的 shell → PTY spawn 失败 → `TerminalView::new` 返回 `Err` → `spawn_shell_tab` panic(当前 `expect("failed to start shell terminal")`)。改进:把 `spawn_shell_tab` 的 `expect` 改为 `set_status("启动 shell 失败: ...", true)` 优雅降级。但这是独立改进,不在本变更范围(当前 `expect` 行为不变,只是新增 pwsh 选项后用户可能触发)。

### D4:Launcher menu 状态 + 渲染

**状态**:`WorkspaceView` 新增 `launcher_menu_open: bool`。

**`+` 按钮点击**:切换 `launcher_menu_open`(不直接 `new_terminal_tab`)。

**菜单渲染**:`render()` 末尾叠加 `launcher_menu` overlay(与 `confirm_dialog` / `settings_dialog` / `context_menu` 同模式):

```rust
if self.launcher_menu_open {
    root = root.child(self.launcher_menu(cx));
}
```

`launcher_menu` 结构:
```
backdrop: absolute().inset_0()  ← 点击关闭
  └─ card: absolute().top(px(32.0)).right_0()  ← tab 栏下方右对齐
       └─ "New Tab" 分组
            ├─ Default Shell    → new_terminal_tab(Default)
            ├─ Command Prompt   → new_terminal_tab(Cmd)         [Windows only]
            ├─ PowerShell       → new_terminal_tab(PowerShell)  [Windows only]
            └─ PowerShell 7     → new_terminal_tab(Pwsh)         [Windows only]
       └─ separator
       └─ "Launch Agent" 分组
            ├─ Claude           → launch_agent("claude")
            ├─ Codex            → launch_agent("codex")
            └─ OpenCode         → launch_agent("opencode")
```

- backdrop:`absolute().inset_0()`,`on_mouse_down(Left)` 关闭菜单 + `stop_propagation`。
- card:`absolute().top(px(32.0))`(tab 栏高 32px,菜单在下方)`.right_0()`(右对齐 `+` 按钮),`SURFACE` 底 + `BORDER` 描边 + `radius()` 圆角 + `py(space_xs)` + `flex_col` + `min_w(px(200.0))`。card 的 `on_mouse_down(Left)` `stop_propagation`(点菜单项不冒泡到 backdrop 关闭)。
- 分组标题:`TEXT_DIM` + `text_xs()` + `px(space_sm)` + `py(space_xs)`("New Tab" / "Launch Agent")。
- 菜单项:`px(space_md)` + `py(space_xs)` + `cursor_pointer` + `hover(BTN_BG_HOVER)` + `TEXT`,agent 项前缀 agent 图标(`crate::assets::agent_icon`)。
- 分隔线:`h_1()` + `bg(BORDER)` + `my(space_xs)`。
- 选中后:`launcher_menu_open = false` + `cx.notify()` + 执行对应动作。

**Esc 关闭**:`render` 的 `on_key_down` 检查 `launcher_menu_open`,Esc 时关闭(与 `context_menu` 同模式)。

**定位为什么用 `top(px(32.0)).right_0()`**:tab 栏在主区顶部,高 32px。`+` 按钮在 tab 栏右端。菜单出现在 tab 栏正下方、右对齐 `+` 按钮,与 `+` 按钮视觉关联。`right_0()` 相对于 root(整个窗口),tab 栏右端 = 窗口右端(主区占满右侧)。

**备选(否决)**:存储 `+` 按钮的点击坐标(像 `context_menu_pos`) — 需要在 `on_click` 里拿 `ev.position` 并存 `Point<Pixels>`,且坐标受 tab 栏滚动影响。固定 `top(32px).right_0()` 更简单可靠。

### D5:`launch_agent` = 新 tab + 发命令

```rust
fn launch_agent(&mut self, agent_name: &str, cx: &mut Context<Self>) {
    self.new_terminal_tab(ShellKind::Default, cx);
    self.send_agent_command(agent_name, cx);
}
```

`new_terminal_tab(Default)` 创建新 shell tab 并设为 active;`send_agent_command(name)` 往 active tab(刚创建的)发命令。

**为什么创建新 tab 而非发到当前 shell**:
1. 每个 agent 独立 tab — 可并行运行多个 agent,互不干扰。
2. 不发到正在跑命令的 shell — 避免 agent 命令混入前一个命令的输出。
3. 用户可关 tab 直接终止 agent(停 PTY = 停 agent + shell)。

**时序安全**:`new_terminal_tab` → `spawn_shell_tab` → `TerminalView::new` → `TerminalSession::spawn` 开 PTY。PTY 的写入端立即可用(OS 内核缓冲)。`send_agent_command` → `terminal.send_text` → `session.write_input` 写字节到 PTY buffer。shell 进程启动后从 PTY buffer 读到 agent 命令并执行。PTY 写不需等 shell 就绪(字节进 kernel buffer,shell readline 就绪后读取)。

**`send_agent_command` 不变**:仍发命令到 active tab(复用现有逻辑 + `agent_command_string`)。`launch_agent` 只是先建 tab 再调它。

### D6:删除 `agent_buttons` 方法 + `tab_list` 内的 `+` 按钮

- `tabs.rs` 的 `agent_buttons(&self, cx)` 方法整个删除。
- `tab_bar` 不再 `.child(self.agent_buttons(cx))`。
- `tab_list` 末尾的 `+` 按钮(`tabs.rs:57-76`)移到 `tab_bar`,改为切换 `launcher_menu_open`。
- `send_agent_command` 方法保留(`launch_agent` 调用它),但不再被 UI 按钮直接调用。

### D7:测试 accessor 适配

| accessor | 改动 |
|---|---|
| `new_terminal_tab_for_test()` | 签名改:加 `shell: ShellKind` 参数(测试传 `ShellKind::Default`) |
| `launcher_menu_open_for_test()` | 新增:读 / 写 `launcher_menu_open` 状态 |
| `launch_agent_for_test(name)` | 新增:调 `launch_agent`(= `new_terminal_tab(Default)` + `send_agent_command`) |

其他 accessor(`tab_count` / `active_tab_index` / `terminals_contains` / `terminal_at` / `switch_tab_for_test` / `close_tab_for_test` / `send_agent_command_for_test`)不变。

## Risks / Trade-offs

- **[pwsh 未安装]** → 选 PowerShell 7 后 `TerminalView::new` → `TerminalSession::spawn` → `tty::new` 失败 → 当前 `spawn_shell_tab` 用 `expect` 直接 panic。**缓解**:本变更不改 `expect` 行为(保持简单);未来可改为 `set_status("启动 shell 失败", true)` 优雅降级。用户选 pwsh 前应自行确认已安装。
- **[新 tab + 立即发 agent 命令的时序]** → PTY 字节进 kernel buffer,shell 启动后读取。理论上不会丢命令(PTY buffer 是 OS 级别的)。但 shell 启动 banner 可能混在 agent 命令前。**缓解**:shell readline 按行读取,agent 命令是完整一行(`command args\r`),banner 不影响命令解析。实测 VS Code terminal.sendText 同样模式,工作正常。
- **[`flex_1` tab 宽度在 GPUI 中的行为]** → GPUI 的 flex 实现应与 CSS flexbox 一致(`flex-grow:1; flex-shrink:1; flex-basis:0%`)。若 GPUI flex 实现有差异,tab 宽度可能不如预期。**缓解**:`overflow_x_scroll` 兜底,tab 不会超出 tab_list 边界。测试验证 `flex_1` + `min_w` / `max_w` 行为。
- **[launcher menu 定位依赖 tab 栏在主区顶部]** → `top(px(32.0))` 假设 tab 栏高 32px 且在主区顶部。若未来 tab 栏位置 / 高度变,菜单定位偏移。**缓解**:tab 栏高度是 `tab_bar` 的 `h(gpui::px(32.0))` 常量,改时同步改菜单 `top`。可提取为 `const TAB_BAR_H: f32 = 32.0`。
- **[Windows shell 选项硬编码]** → `cmd.exe` / `powershell.exe` / `pwsh.exe` 是 Windows 常见 shell,但不覆盖 Git Bash / WSL / nushell 等。**缓解**:`ShellKind` 枚举可扩展(加变体 + `command()` 分支)。本变更只覆盖用户明确要求的三种 Windows shell。
- **[菜单打开时 tab 栏可能消失]** → 若菜单打开后用户关掉最后一个 tab(`✕`),tab 栏 `h_0` 消失,但菜单 overlay 仍在(渲染在 root 层)。**缓解**:`close_tab` 里检查 `launcher_menu_open`,若 group 变空则关闭菜单。或菜单的 New Tab 项总是可用(`new_terminal_tab` 在无 active 时 no-op,但菜单项点击后先关菜单再执行,无 active 则 no-op)。

### D8:在文件管理器中打开 worktree 目录(reveal button)

**需求**:用户经常需要在外部文件管理器中查看 worktree 目录结构(Finder / Explorer)。

**实现**:`tab_bar` 在 `+` 按钮右边新增 `reveal` 按钮(folder-open 图标),点击调用系统命令打开 active worktree 目录:

```rust
fn reveal_in_file_manager(&self, cx: &mut Context<Self>) {
    let Some(path) = &self.active else { return };
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}
```

- 按钮在 `tab_bar` 中(`tab_list` 之外),`flex_none` 固定,不受 tab 滚动影响。
- 无 active worktree(空态)时 tab 栏本身 `h_0` 隐藏,reveal 按钮不渲染。
- 用 `spawn()`(非 `status()`),不阻塞 UI 线程。
- 路径用 `self.active`(已 `canon()` 规范化)。
- 图标用 `folder-open.svg`(已注册在 `assets.rs`)。

**备选(否决)**:把 reveal 放到侧边栏 worktree 项的右键菜单 — 右键菜单尚未实现,且 tab 栏更直观(用户正在看哪个 worktree 的终端就打开哪个)。
