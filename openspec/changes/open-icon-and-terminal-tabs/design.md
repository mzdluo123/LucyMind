## Context

### 现状

**Open 按钮**(`crates/app/src/workspace/sidebar.rs:54-77`):仓库行是 flex 行 = 仓库名(`flex_1` + 省略号)+ `button("open-repo", "Open…")`。`button` 组件(`crates/app/src/ui/button.rs`)渲染为深灰底 + 细描边 + 文字的矩形按钮。同侧边栏其他操作入口(齿轮 `settings.svg`、`+` `plus.svg`)都是单色 SVG 图标 + `group-hover` 染色,无描边无背景,视觉更轻。文字 Open 按钮在这群图标中显得笨重。

**终端模型**(`crates/app/src/workspace/mod.rs:123-125`):
```rust
terminals: HashMap<PathBuf, Entity<TerminalView>>,
active: Option<PathBuf>,
```
一路径对应一个终端。`open_worktree`(点击 worktree 行)若该路径无终端则起一个默认 shell,有则切过去。`new_worktree_and_agent`(`+` 菜单)建新 worktree + 起 agent 终端(独立子进程)。`request_close` 停该路径的终端 + 删 worktree。**无法在同一路径下有多个终端**。

**工作流**(`mod.rs:323-418`):侧边栏 AGENTS `+` 按钮弹下拉菜单(Claude / Codex / OpenCode),选中后 `new_worktree_and_agent` 建 worktree → 跑 hook → **直接 spawn agent 子进程**(`TerminalView::new(cx, cwd, Some((command, args)), env)`)。用户无法在 agent 启动前检查环境、无法在同终端先跑命令再起 agent、agent 退出即终端死。

终端区渲染(`mod.rs:705-722`):`active` 路径对应的终端直接 `child(term.clone())` 填满 `flex_1`;无 active 则空态文字。无 tab 栏。

### 约束

- GPUI 的 `svg()` 是单色 mask,需 `text_color` 染色;多色 SVG 会被填成单色剪影。图标须 `fill="currentColor"` / `stroke="currentColor"` 的 Lucide 风格。
- 路径必须先 `canon()` 规范化再作 terminals map key(见 `mod.rs:50-53`),否则同一 worktree 被当成两个。
- `TerminalView` 的 `cx.spawn` 轮询循环是长任务,关闭 tab 时必须调 `shutdown()` 停 PTY,否则 leak-detection 报错(见 `tests/common/mod.rs:120-128`)。
- `claude` 基于 Ink,必须真 TTY(已由 terminal 层 PTY + `TERM=xterm-256color` 兜底)。
- 现有测试用 `terminal_at(path)` / `terminals_contains(path)` / `active_path()` 观察状态机内部(见 `mod.rs:545-693` 的 `#[cfg(feature = "test-support")]` accessor)。数据模型改了,这些 accessor 语义需保持或适配。
- tab 标题:`TerminalView` 当前收到 `TermEvent::Title(_)` 只标记 dirty,不存储标题(见 `terminal_view.rs:101`)。alacritty 内核已解析 OSC 0/2 转义序列并通过 `TermEvent::Title(String)` 转发(见 `session.rs:31,104,295`),只是 app 层丢弃了。本变更改为存储标题,tab 栏优先显示动态标题、回退静态标题(见 D7)。
- shell 终端在 `open_worktree` / `new_worktree` 创建时已注入 `TERM=xterm-256color` + worktree env(`WORKTREE_PATH` / `WORKTREE_BRANCH` 等,见 `mod.rs:264-277`)。agent 命令在 shell 里执行时自动继承这些环境变量,无需额外处理。
- `TerminalSession::write_input(&self, bytes: Vec<u8>)`(见 `session.rs:267`)已暴露向 PTY 写字节的接口。`TerminalView` 的 `session` 字段是私有的,需加一个公开方法 `send_text` 供 `WorkspaceView` 发 agent 命令。

## Goals / Non-Goals

**Goals:**

- Open 按钮换成 `folder-open.svg` 图标,与齿轮 / `+` 按钮风格统一(无背景无描边,group-hover 染色)。
- 每个 worktree 可有多个终端(Shell),tab 栏展示 / 切换 / 新建 / 关闭。
- Tab 栏只在 active worktree 有终端时显示;无终端时空态文字(不变)。
- 关 tab 只关该终端,不删 worktree;关 worktree 停其所有 tab。
- 切 worktree 时保留各 group 的 active_tab(切回来恢复)。
- Tab 标题跟随终端 OSC 0/2 协议动态更新(shell / agent 发 `\x1b]0;<title>\x07` 改标题),无动态标题时回退静态名("Shell")。
- **建 worktree 后开 shell(不自动起 agent)**,tab 栏右侧有 agent 按钮往当前 shell 发命令启动 agent,用户有更多控制空间。
- 现有测试适配新模型,新增多 tab 行为测试。

**Non-Goals:**

- 不做 tab 拖拽重排(Zed 支持,但 MVP 不需要)。
- 不做 tab 分屏(左右 / 上下拆分)。
- 不做 tab 右键菜单(关闭其他 / 关闭右侧 / 重命名)。
- 不做跨 worktree 的全局 tab 列表(tab 只属于当前 worktree 的 group)。
- 不改 `TerminalView` 的渲染 / 输入 / 选区 / 复制(只加 `title` 字段 + 事件处理 + `send_text` 方法);tab 栏是 `WorkspaceView` 层的容器,`TerminalView` 不感知 tab。
- 不改 terminal crate(`session.rs` / `input.rs` / `palette.rs`)—— OSC 0/2 已由 alacritty 内核解析并通过 `TermEvent::Title` 转发,`write_input` 已暴露,无需改。
- 不做 agent 命令的参数编辑 / 预览(按钮直接发 `AgentSpec::resolve` 的 command + args,用户可在 shell 里 Ctrl+U 清行再手动输入修改)。

## Decisions

### D1:Open 按钮用 `folder-open.svg` 图标,与齿轮 / `+` 同风格

仓库行的 `button("open-repo", "Open…")` 替换为与齿轮按钮完全同构的图标按钮:无背景无描边的 `div`,`group-hover` 染色,`cursor_pointer`,点击触发 `open_repo_picker`。

图标用 Lucide 的 `folder-open`(打开的文件夹),语义最贴切(打开仓库)。新增 `crates/app/assets/icons/folder-open.svg`,`assets.rs` 的 `load` / `list` 登记路径。

`button` 组件不再用于仓库行(它带背景+描边,太重)。仓库行与 WORKTREES 标题行齿轮、AGENTS 标题行 `+` 统一为「纯图标 + group-hover」风格。

**备选(否决)**:用 `button().icon("icons/folder-open.svg")` —— `button` 组件仍带 `BTN_BG` 背景 + `BORDER` 描边 + padding,比齿轮 / `+` 按钮重。直接用 `div` + `svg` 与现有图标按钮一致。

### D2:数据模型 — `TerminalGroup` + `TerminalTab`

```rust
struct TerminalTab {
    terminal: Entity<TerminalView>,
    title: String,  // 静态回退标题,所有 tab 都是 "Shell"(因为 worktree 只开 shell)
}

struct TerminalGroup {
    tabs: Vec<TerminalTab>,
    active_tab: usize,  // 当前展示的 tab 索引
}

// WorkspaceView 字段:
terminals: HashMap<PathBuf, TerminalGroup>,  // key = canon(worktree_path)
active: Option<PathBuf>,  // 当前 active worktree(不变)
```

`active` 仍是 worktree 路径(不是 tab),因为侧边栏 worktree 列表的高亮 /点击逻辑都基于 worktree 路径。tab 级 active 存在 `TerminalGroup.active_tab` 里,切 worktree 时自动恢复该 group 的 active_tab。

**为什么不用扁平 `Vec<Tab>` + `active_tab: TabId`**:扁平列表需要额外维护「tab 属于哪个 worktree」字段,且切 worktree 时要过滤 tab 列表,渲染和查找都更复杂。`HashMap<PathBuf, TerminalGroup>` 天然按 worktree 分组,渲染 tab 栏时一次查找即可。

**为什么 `active_tab` 存 `usize` 索引而非 `Entity` ID**:tab 数量少(通常 1-3 个),`usize` 索引简单直接。关闭 tab 时调整索引(见 D6)。用 `Entity` 做 ID 需要额外比较逻辑,无收益。

### D3:Tab 栏渲染 — 新文件 `workspace/tabs.rs`

新文件 `crates/app/src/workspace/tabs.rs`,作为 `impl WorkspaceView` 的跨文件 impl(与 `sidebar.rs` / `status_bar.rs` / `dialogs.rs` / `settings.rs` 同模式)。暴露 `pub(super) fn tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement`。

`mod.rs` 的 `render` 主区从:
```rust
let main = div().flex_1().h_full().flex().flex_col()
    .child(term_area)
    .child(self.status_bar());
```
改为:
```rust
let main = div().flex_1().h_full().flex().flex_col()
    .child(self.tab_bar(cx))   // 有 tab 时显示 tab 栏(含 agent 按钮),无则返回空 div(h_0)
    .child(term_area)
    .child(self.status_bar());
```

### D4:Tab 栏视觉设计 + Agent 按钮行

Tab 栏高度 ~32px,水平排列,背景 `SURFACE`(与侧边栏同层),底部 1px `BORDER` 描边分隔 tab 栂与终端区。结构:`[Tab1] [Tab2] [+] <flex_spacer> [Claude] [Codex] [OpenCode]`。

**Tab 区**(左侧,`flex_row` + `overflow_x_scroll`):
- 每个 tab:`flex_none` + `px(space_sm)` + `py(space_xs)` + `gap(space_xs)` + `flex_row` + `items_center`,标题(`SharedString`,单行省略)+ `✕` 关闭按钮(`TEXT_FAINT` → hover `STATE_ERROR`)。active tab 顶部 2px `TEXT_BRIGHT` 标记线 + `SURFACE_RAISED` 背景;inactive `SURFACE` 底 + `TEXT_DIM` 字,hover `BTN_BG_HOVER`。`min_w` / `max_w` / `overflow_hidden` / `text_ellipsis`。
- 末尾 `+` 按钮:复用 `icons/plus.svg`,`TEXT_FAINT` → group-hover `TEXT`,点击在当前 worktree 新建 shell tab。

**Agent 按钮区**(右侧,`flex_none` + `flex_row` + `gap(space_xs)` + `pr(space_sm)`):
- 迭代 `builtin_agents()`(Claude / Codex / OpenCode),每个按钮 = agent 图标(`crate::assets::agent_icon`) + display 名,`BTN_BG` 底 + `BORDER` 描边 + `TEXT` 字,hover `BTN_BG_HOVER`,点击调 `send_agent_command(name, cx)`。
- 仅在 active worktree 有终端(tab 栏显示)时渲染;空态时不显示(无 shell 可发命令)。

tab 栏无 tab 时(active worktree 无终端)整体不显示(`h_0` / 不渲染),终端区直接展示空态文字。agent 按钮随 tab 栏一起隐藏。

### D5:新建 tab — `+` 按钮与 `new_worktree`(原 `new_worktree_and_agent`)的关系

**工作流变更**:侧边栏 `+` 按钮不再弹 agent 下拉菜单,直接调 `new_worktree(cx)`:
1. 建 worktree(git add + postCreate hook,不变)
2. 创建 group,append **shell** tab(不是 agent tab):`TerminalView::new(cx, Some(cwd), None, env)` —— `command=None` 表示起默认 shell
3. active = 该 worktree 路径,active_tab = 0
4. git lock + session 注册 + persist(不变,但 `Session.agent` 字段记 `None` 而非 agent 名)

用户在新 shell 里可通过 tab 栏的 agent 按钮发命令启动 agent,或手动输入任何命令。

**`open_worktree(path)`**(点击 worktree 行)行为不变:
1. 若该路径无 group → 创建 group + 一个 shell tab
2. 若该路径有 group → 只切 active 到该 worktree(恢复其 active_tab)
3. 不新建 tab(切回去看已有的)

**Tab 栏 `+` 按钮**(`new_terminal_tab`):
1. 取 active worktree 路径(无 active 则 no-op)
2. 在该路径的 group 里 append 一个 shell tab,active_tab 指向新 tab
3. 若 group 不存在(理论上不会,因为 `+` 只在有终端时显示)→ 先创建 group

所有 tab 都是 shell tab(静态标题 "Shell")。agent 通过 tab 栏按钮发命令启动,在 shell 内运行(不是独立子进程)。

### D6:关闭 tab — 只关终端,不删 worktree

`close_tab(tab_index)`:
1. 取 active worktree 的 group,取 `tabs[tab_index]`
2. 调 `terminal.shutdown()` 停 PTY
3. `tabs.remove(tab_index)`
4. 调整 `active_tab`:若删的是 active,`active_tab` 回退到 `min(tab_index, tabs.len()-1)`(删最后一个则回退到前一个);若删的在 active 之前,`active_tab -= 1`
5. 若 `tabs.is_empty()` → 从 `self.terminals` 移除该 group(worktree 仍在侧边栏,终端区回到空态)

不弹「未提交改动」确认(tab 关闭只是停终端,不删 worktree 目录 / 不丢 git 改动)。worktree 目录和文件不受影响。

**关 worktree**(`request_close` / `do_close`):先遍历 group 内所有 tab 调 `shutdown()`,再移除 group,再走既有 git remove 流程。现有 `request_close` 只停一个终端,改为停 group 内所有。

### D7:Tab 标题跟随终端 OSC 0/2 协议动态更新,回退静态标题

终端程序(shell / vim / agent)通过 **OSC 0/2** 转义序列设置窗口/标签页标题:`ESC ] 0 ; <title> BEL`(`\x1b]0;<title>\x07`)或 `ESC ] 2 ; <title> BEL`。alacritty 内核已解析此序列并通过 `TermEvent::Title(String)` 转发(见 `crates/terminal/src/session.rs:31,104,295`)。

当前 `TerminalView` 在事件循环(`terminal_view.rs:101`)把 `TermEvent::Title(_)` 与 `Wakeup`/`Bell` 一起只标记 `dirty=true`,**丢弃了标题字符串**。改动:

1. `TerminalView` 新增字段 `title: Option<String>`(动态标题,None = 未收到过)。
2. 事件循环 `TermEvent::Title(t)` 分支改为 `view.title = Some(t); dirty = true;`(存储标题 + 标记重绘)。
3. 新增普通 `pub fn title(&self) -> Option<&str>`(非 test-support gate,因为 tab 栏渲染要用)。
4. `TerminalTab` 保留静态 `title: String`(创建时确定,所有 tab 都是 "Shell"),作为**回退**——终端未发 OSC 0/2 时显示静态标题,发了之后动态标题覆盖。
5. tab 栏渲染时:`tab.terminal.read(cx).title().unwrap_or(&tab.title)`(动态优先,静态回退)。

**为什么保留静态回退**:shell 启动时不一定立刻发 OSC 0/2,空标题不如 "Shell"。shell 启动后通常发当前目录(如 `~/code/LucyMind`),比静态 "Shell" 更有用。用户通过 tab 栏按钮发 `claude` 命令后,claude 会发自己的标题(如 "claude — LucyMind"),tab 标题随之更新。

**`TerminalView::new` 初始化**:`title: None`。shell 启动后若发 OSC 0/2 则覆盖;不发则 `None`,tab 栏用静态 `TerminalTab.title`。

**渲染时机**:tab 栏在 `WorkspaceView::render` → `tab_bar(&self, cx)` 里渲染,此时可 `tab.terminal.read(cx)` 读当前 title。`TerminalView` 收到 `Title` 事件后 `cx.notify()` 触发重绘,`WorkspaceView` 作为父 Entity 也会重绘(子 Entity `notify` 冒泡到父),tab 标题随之刷新。

**备选(否决)**:在 `TerminalTab` 里缓存 title,每次 `Title` 事件回写到 `TerminalTab`——需要 `WorkspaceView` 持有 `WeakEntity<TerminalView>` 反向引用或在事件循环里更新父,增加耦合。直接在渲染时 `terminal.read(cx).title()` 更简单,且 `cx.notify` 已保证及时刷新。

### D8:测试 accessor 适配

现有 `#[cfg(feature = "test-support")]` accessor 适配新模型:

| accessor | 旧语义 | 新语义 |
|---|---|---|
| `terminals_contains(path)` | 该路径有终端 | 该路径有 group 且 tabs 非空 |
| `terminal_at(path)` | 该路径的终端 Entity | 该路径 group 的 active tab 的终端 Entity |
| `active_path()` | active worktree 路径 | 不变 |
| `shutdown_all_terminals_for_test()` | 停所有终端 | 遍历所有 group 的所有 tab 停终端 |

新增:
| accessor | 用途 |
|---|---|
| `tab_count(path)` | 该路径 group 的 tab 数 |
| `active_tab_index()` | active worktree 的 active_tab 索引 |

`new_worktree_and_agent_for_test(agent_name)` 改为 `new_worktree_for_test()`(无 agent_name 参数,因为不再自动起 agent)。现有测试调用处需适配。

### D9:`spawn_shell_tab` 提取 + `send_agent_command`

**`spawn_shell_tab`**:现有 `open_worktree` 内联了「起默认 shell 终端」的逻辑(env 组装 + `cx.new(|cx| TerminalView::new(...))`)。提取为 `fn spawn_shell_tab(&self, wt_path: &Path, cx: &mut Context<Self>) -> TerminalTab`,供 `open_worktree`(无 group 时建首个 tab)、`new_terminal_tab`(`+` 按钮新建 tab)和 `new_worktree`(侧边栏 `+` 建 worktree 后开首个 tab)复用。返回 `TerminalTab { terminal, title: "Shell".into() }`。

**`send_agent_command(agent_name, cx)`**:tab 栏 agent 按钮的点击处理。构造命令字符串并写入当前 active tab 的 shell PTY:
1. 取 active worktree 的 active tab 的 `TerminalView` entity(无 active / 无 tab 则 no-op)
2. `AgentSpec::resolve(&self.config, agent_name, wt_path, &wt_env)` 取 command + args(走配置 / builtin 回退,与原 `new_worktree_and_agent` 同源)
3. 构造命令字符串:`command + " " + args.join(" ") + "\n"`,args 含空格时 shell-quote(用双引号包裹,转义 `\` 和 `"`)
4. `terminal.update(cx, |t, _| t.send_text(&cmd))` 写入 PTY

shell 已在 worktree 目录(cwd = wt_path)、已注入 `TERM` + worktree env(见 D5 的 `spawn_shell_tab`),agent 命令在 shell 里执行时自动继承。无需额外设 env 或 cwd。

**`TerminalView::send_text(&self, text: &str)`**:新增公开方法,`self.session.write_input(text.as_bytes().to_vec())`。供 `send_agent_command` 用,也可供未来其他场景(如快捷键发命令)。

**命令字符串构造**:
```rust
fn agent_command_string(spec: &AgentSpec) -> String {
    let mut s = spec.command.clone();
    for arg in &spec.args {
        s.push(' ');
        if arg.contains(' ') || arg.contains('"') || arg.contains('\'') || arg.is_empty() {
            s.push('"');
            s.push_str(&arg.replace('\\', "\\\\").replace('"', "\\\""));
            s.push('"');
        } else {
            s.push_str(arg);
        }
    }
    s.push('\n');
    s
}
```

builtin agent 的命令:`claude --dangerously-skip-permissions\n`、`codex --dangerously-bypass-approvals-and-sandbox\n`、`opencode --auto\n`(无空格参数,不需引号)。自定义 agent 配置(`.worktree.toml` 的 `[agents.*]`)的 command + args 也通过 `AgentSpec::resolve` 取到,正确引号。

### D10:删除侧边栏 agent 下拉菜单

`agent_menu_open: bool` 状态、`agent_menu()` 渲染方法、`open_agent_menu_for_test()` 测试 accessor 全部删除。侧边栏 AGENTS 标题行的 `+` 按钮改为直接调 `new_worktree(cx)`(不弹菜单)。

`agent_menu` 的 Esc 关闭逻辑(`render` 里的 `on_key_down` 检查 `agent_menu_open`)也删除。

**为什么不保留菜单**:原菜单的目的是「选 agent 后建 worktree + 起 agent」。新流程里 agent 选择移到 tab 栏按钮(发命令到 shell),侧边栏 `+` 只负责建 worktree,无需选择 agent。保留菜单会让用户困惑(菜单选 agent vs tab 栏按钮发命令,两条路径)。

## Risks / Trade-offs

- **[关闭最后一个 tab 后 worktree 无终端]** → group 被移除,终端区空态,agent 按钮隐藏。用户再点该 worktree 行 → `open_worktree` 发现无 group → 建新 shell tab,agent 按钮重新出现。行为自然,无数据丢失。
- **[tab 数量无上限]** → 用户可无限建 tab。MVP 不限制;后续可加最大 tab 数或滚动 tab 栏。tab 栏 `overflow_x_scroll` 处理溢出(横向滚动)。
- **[动态标题频繁变化导致 tab 宽度抖动]** → tab 标题用 `max_w` + `overflow_hidden` + `text_ellipsis` 截断,宽度上限固定,抖动只影响截断后的可见部分。shell 通常只在命令完成 / 目录切换时改标题,频率不高。
- **[关 tab 不弹未提交改动确认]** → tab 关闭只停终端进程,不删 worktree 目录,git 改动不丢。用户可继续在该 worktree 起新 tab 看到 git 状态。与「关 worktree」(删目录)不同,后者仍弹确认。
- **[切 worktree 时 tab 栏闪烁]** → 切到无终端的 worktree 时 tab 栏消失(高度 0),有终端时出现。高度变化可能引起 1 帧跳动,但 GPUI 原子重绘,视觉无感。
- **[`active_tab` 索引在关闭后越界]** → D6 的调整逻辑保证 `active_tab` 始终 `< tabs.len()`(空 group 直接移除)。需在 `close_tab` 和 `switch_tab` 里做边界检查。
- **[现有测试大面积适配]** → `terminals` 类型改了,所有用 `terminal_at` / `terminals_contains` 的测试都要检查。`new_worktree_and_agent_for_test` 改为 `new_worktree_for_test`(无 agent 参数)。`agent_menu` 测试(`tests/agent_menu.rs`)需删除或重写。
- **[agent 在 shell 内运行,退出后 shell 仍在]** → 这是设计目标(用户可在同终端跑多个 agent / 命令)。但 `Session.agent` 字段此前记 agent 名,现在建 worktree 时不知用户会用哪个 agent → `Session.agent` 记 `None`,session 注册表不追踪 agent(用户可能在一个 shell 里跑多个 agent)。
- **[agent 命令字符串引号]** → builtin agent 参数无空格,不需引号。自定义 agent 配置的参数可能含空格,`agent_command_string` 用双引号包裹 + 转义。极端情况(参数含 `$` / `` ` `` 等 shell 元字符)用户需自行处理——发的是 shell 命令,用户可在 shell 里 Ctrl+U 清行再手动输入。
- **[agent 按钮发命令时 shell 正在跑命令]** → 如果 shell 正在跑前一个命令(如 agent 未退出),发的新命令会排队等 shell 就绪(PTY 写入是字节流,shell readline 不在 prompt 时不处理输入)。实际场景:用户 Ctrl+C 退出当前 agent 后再点按钮,或在空 shell 里直接点。不做「检测 shell 是否空闲」(复杂且不可靠)。
