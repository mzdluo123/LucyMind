## 1. 图标资产:folder-open.svg

- [x] 1.1 新增 `crates/app/assets/icons/folder-open.svg`(Lucide 风格,`stroke="currentColor"`,24×24 viewBox,打开的文件夹图形)
- [x] 1.2 `crates/app/src/assets.rs`:`AssetSource::load` match 加 `"icons/folder-open.svg"`;`list()` 补一条

## 2. App: Open 按钮换图标

- [x] 2.1 `crates/app/src/workspace/sidebar.rs`:仓库行的 `button("open-repo", "Open…")` 替换为 `div` + `gpui::svg().path("icons/folder-open.svg")` 图标按钮(与齿轮按钮同构:无背景无描边、`group_hover` 染色 `TEXT_FAINT` → `TEXT`、`cursor_pointer`、`on_click` 调 `open_repo_picker`)
- [x] 2.2 `crates/app/src/workspace/sidebar.rs`:移除该行对 `crate::ui::button` 的依赖引用(若该行是 `button` 的唯一使用点则清理 import;否则保留)
- [x] 2.3 `cargo build -p lucy-app` 通过

## 3. App: 数据模型改造(TerminalGroup / TerminalTab)

- [x] 3.1 `crates/app/src/workspace/mod.rs`:新增 `struct TerminalTab { terminal: Entity<TerminalView>, title: String }` 和 `struct TerminalGroup { tabs: Vec<TerminalTab>, active_tab: usize }`(`title` 为静态回退标题,所有 tab 都是 "Shell")
- [x] 3.2 `WorkspaceView.terminals` 字段类型改 `HashMap<PathBuf, TerminalGroup>`;`active: Option<PathBuf>` 不变
- [x] 3.3 `construct()` 初始化 `terminals: HashMap::new()`(类型变了,初始化不变)
- [x] 3.4 提取 `fn spawn_shell_tab(&self, wt_path: &Path, cx: &mut Context<Self>) -> TerminalTab`:封装 env 组装(`TERM` + worktree env)+ `TerminalView::new(cx, Some(cwd), None, env)`(command=None 起 shell),title = `"Shell"`。供 `open_worktree` / `new_worktree` / `new_terminal_tab` 复用
- [x] 3.5 `cargo build -p lucy-app` 通过(此时 `new_worktree` / `open_worktree` / `request_close` / `do_close` / `render` 尚未适配,编译错误预期)

## 3b. App: TerminalView 存储终端标题(OSC 0/2)+ send_text

- [x] 3b.1 `crates/app/src/terminal_view.rs`:新增字段 `title: Option<String>`(动态标题,None = 未收到 OSC 0/2)
- [x] 3b.2 `TerminalView::new` 初始化 `title: None`
- [x] 3b.3 事件循环 `TermEvent::Title(t)` 分支改为 `view.title = Some(t); dirty = true;`(当前只标记 dirty 丢弃标题字符串)
- [x] 3b.4 新增 `pub fn title(&self) -> Option<&str>`(非 test-support gate,tab 栏渲染要用)
- [x] 3b.5 新增 `pub fn send_text(&self, text: &str)`:调 `self.session.write_input(text.as_bytes().to_vec())`,供 agent 按钮发命令
- [x] 3b.6 `cargo build -p lucy-app` 通过

## 4. App: new_worktree(原 new_worktree_and_agent)改为开 shell

- [x] 4.1 `new_worktree_and_agent` 重构为 `new_worktree(&mut self, cx: &mut Context<Self>)`(无 `agent_name` 参数):建 worktree(git add + postCreate hook,不变)→ `spawn_shell_tab` → 建新 group(`tabs: vec![tab]`, `active_tab: 0`)→ `self.active = Some(wt_key)`。git lock + session 注册(`Session.agent = None`)+ persist 不变
- [x] 4.2 `open_worktree`:无 group → `spawn_shell_tab` 建首个 tab + group;有 group → 只切 active(不新建 tab)。`self.active = Some(wt_path)`
- [x] 4.3 `render` 主区:`term_area` 从 `self.active` 取 group → 取 `tabs[active_tab].terminal` 渲染;无 group 或空 tabs → 空态文字
- [x] 4.4 删除 `agent_menu_open: bool` 字段、`agent_menu()` 渲染方法、`open_agent_menu_for_test()` accessor、`render` 里的 `agent_menu_open` overlay 和 Esc 关闭逻辑
- [x] 4.5 侧边栏 `+` 按钮(`sidebar.rs`)改为直接调 `new_worktree(cx)`(不弹菜单)
- [x] 4.6 `cargo build -p lucy-app` 通过

## 5. App: tab 栏 UI + agent 按钮(workspace/tabs.rs)

- [x] 5.1 新建 `crates/app/src/workspace/tabs.rs`,`mod.rs` 加 `mod tabs;`
- [x] 5.2 `impl WorkspaceView` 新增 `pub(super) fn tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement`:active worktree 无 group / 空 tabs → 返回 `div().h_0()`(不占空间);有 tabs → 渲染水平 tab 栏 + agent 按钮行
- [x] 5.3 tab 区(左侧):每个 tab = `div` + flex_row + 标题(`SharedString`,单行省略,取 `tab.terminal.read(cx).title().unwrap_or(&tab.title)` 动态优先) + `✕` 关闭按钮(`TEXT_FAINT` → hover `STATE_ERROR`)。active tab 顶部 2px `TEXT_BRIGHT` 标记线 + `SURFACE_RAISED` 底;inactive `SURFACE` 底 + `TEXT_DIM` 字,hover `BTN_BG_HOVER`。`min_w` / `max_w` / `overflow_hidden` / `text_ellipsis`。tab 区 `overflow_x_scroll`
- [x] 5.4 tab 点击(`on_click`)→ `switch_tab(index)`(不触发关闭);`✕` 点击(`on_click` + `cx.stop_propagation()`)→ `close_tab(index)`
- [x] 5.5 `+` 按钮:复用 `icons/plus.svg`,`TEXT_FAINT` → group-hover `TEXT`,`on_click` → `new_terminal_tab(cx)`
- [x] 5.6 agent 按钮区(右侧,`flex_none` + `flex_row` + `gap(space_xs)` + `pr(space_sm)`):迭代 `builtin_agents()`,每个按钮 = agent 图标(`crate::assets::agent_icon`) + display 名,`BTN_BG` 底 + `BORDER` 描边 + `TEXT` 字,hover `BTN_BG_HOVER`,`on_click` → `send_agent_command(name, cx)`
- [x] 5.7 tab 栏容器:`flex_row` + `bg(SURFACE)` + `border_b_1(BORDER)` + 高度 ~32px;左侧 tab 区 `flex_1` + `overflow_x_scroll`,右侧 agent 按钮区 `flex_none`
- [x] 5.8 `mod.rs::render` 主区改为 `.child(self.tab_bar(cx)).child(term_area).child(self.status_bar())`
- [x] 5.9 `cargo build -p lucy-app` 通过

## 6. App: tab 操作方法 + agent 命令发送

- [x] 6.1 `fn switch_tab(&mut self, index: usize, cx: &mut Context<Self>)`:取 active group,设 `active_tab = index`(边界 clamp),`cx.notify()`
- [x] 6.2 `fn new_terminal_tab(&mut self, cx: &mut Context<Self>)`:取 active worktree 路径(无则 no-op);`spawn_shell_tab` → append 到 group(无 group 则先建)→ `active_tab = tabs.len() - 1`;`cx.notify()`
- [x] 6.3 `fn close_tab(&mut self, index: usize, cx: &mut Context<Self>)`:取 active group → `tabs[index].terminal.shutdown()` → `tabs.remove(index)` → 调整 `active_tab`(删 active 则回退 `min(index, tabs.len()-1)`;删 active 之前则 `active_tab -= 1`)→ `tabs.is_empty()` 则移除 group → `cx.notify()`
- [x] 6.4 `fn send_agent_command(&mut self, agent_name: &str, cx: &mut Context<Self>)`:取 active group 的 active tab 的 terminal entity → `AgentSpec::resolve(&self.config, agent_name, wt_path, &wt_env)` → 构造命令字符串(`command args\n`,args 含空格时 shell-quote)→ `terminal.update(cx, |t, _| t.send_text(&cmd))`。无 active / 无 tab 则 no-op
- [x] 6.5 `fn agent_command_string(spec: &AgentSpec) -> String`:`command + " " + args.join(" ") + "\n"`,args 含空格/引号/空时用双引号包裹 + 转义
- [x] 6.6 `request_close` / `do_close`:把「停单个终端」改为「遍历 group 内所有 tab 调 `shutdown()`」再移除 group
- [x] 6.7 `cargo build -p lucy-app` 通过

## 7. App: 测试 accessor 适配

- [x] 7.1 `terminals_contains(path)`:改为 `group.tabs.is_empty()` 判断(有 group 且 tabs 非空 → true)
- [x] 7.2 `terminal_at(path)`:改为返回 `group.tabs[active_tab].terminal`(Option<&Entity<TerminalView>>)
- [x] 7.3 `shutdown_all_terminals_for_test()`:改为遍历所有 group 的所有 tab 调 `shutdown()`
- [x] 7.4 新增 `tab_count(path: &Path) -> usize`:返回 group 的 tabs 数(无 group → 0)
- [x] 7.5 新增 `active_tab_index() -> Option<usize>`:返回 active worktree 的 `active_tab`(无 active / 无 group → None)
- [x] 7.6 `new_worktree_and_agent_for_test(agent_name)` 改为 `new_worktree_for_test()`(无 agent_name 参数)
- [x] 7.7 删除 `open_agent_menu_for_test()`(agent 菜单已移除)
- [x] 7.8 `cargo build -p lucy-app --features test-support` 通过

## 8. App: 现有测试适配

- [x] 8.1 `tests/new_worktree.rs`:`new_worktree_creates_terminal_and_switches_active` 改用 `new_worktree_for_test()`(无 agent 参数);断言 `tab_count == 1` + `terminal_at` 取 active tab;`new_worktree_terminal_renders_pty_output` 改用 fake shell 命令(不依赖 agent config)或保留 `.worktree.toml` 但 `new_worktree_for_test` 不走 agent config
- [x] 8.2 `tests/agent_menu.rs`:删除或重写(agent 菜单已移除;agent 按钮发命令的测试在 `tests/multi_tab.rs`)
- [x] 8.3 `tests/close.rs`:`clean_worktree_closes_without_confirmation` / `dirty_worktree_prompts_confirmation` / `cancel_close_keeps_worktree` 用 `terminals_contains` 判断(语义不变);`close_tab` 关 tab 不触发 worktree close,现有 close 测试不受影响
- [x] 8.4 `tests/worktree_list.rs` / `tests/smoke.rs` / `tests/startup.rs`:检查 `terminals_contains` / `terminal_at` / `new_worktree_and_agent_for_test` 用法,适配新模型
- [x] 8.5 `cargo test -p lucy-app` 全绿(含 `#[gpui::test]`)

## 9. App: 多 tab + agent 按钮新测试

- [x] 9.1 `tests/multi_tab.rs`(新文件):`new_worktree_creates_shell_tab` —— `new_worktree_for_test()` → `tab_count == 1`,`active_tab_index == 0`,`terminal_at` 返回 shell 终端(非 agent 子进程)
- [x] 9.2 `new_tab_increments_tab_count` —— 建 worktree(1 tab)→ `new_terminal_tab` → `tab_count == 2`,`active_tab_index == 1`
- [x] 9.3 `switch_tab_preserves_terminal` —— 建 2 tab → `switch_tab(0)` → `active_tab_index == 0` → `terminal_at` 返回第一个 tab 的终端
- [x] 9.4 `close_non_active_tab` —— 建 2 tab(active=1)→ `close_tab(0)` → `tab_count == 1`,active tab 仍是原第二个(现在 index 0)
- [x] 9.5 `close_active_tab_falls_back` —— 建 2 tab(active=1)→ `close_tab(1)` → `tab_count == 1`,`active_tab_index == 0`
- [x] 9.6 `close_last_tab_empties_group` —— 建 1 tab → `close_tab(0)` → `tab_count == 0`,`terminals_contains == false`,worktree 仍在(`worktree_count` 不变)
- [x] 9.7 `close_tab_does_not_delete_worktree` —— 建 1 tab → `close_tab(0)` → worktree 仍在列表,`has_pending_close == false`(不走 git remove)
- [x] 9.8 `switch_worktree_preserves_active_tab` —— 建 worktree A(2 tab, active=1)+ worktree B(1 tab)→ 切 A → 切 B → 切 A → `active_tab_index == 1`(A 的 active tab 恢复)
- [x] 9.9 `terminal_title_updates_from_osc` —— 用 fake shell 发 `printf '\033]0;MARKER_TITLE\007'` → `wait_for` 轮询 `terminal_at(path).read(cx).title()` 返回 `Some("MARKER_TITLE")`
- [x] 9.10 `tab_title_falls_back_to_shell` —— 建 shell tab 但 shell 不发 OSC 0/2 → `terminal.title()` 返回 `None`,tab 栏用静态 "Shell"
- [x] 9.11 `send_agent_command_writes_to_shell` —— 建 worktree + shell tab → `send_agent_command("test")` → `wait_for` 轮询 `terminal_at(path).snapshot_text()` 包含 fake agent 命令的输出(复用 `temp_repo_with_agent` 的 marker 模式,但 fake agent 配置改为 shell 命令如 `echo MARKER_READY`)
- [x] 9.12 `send_agent_command_noop_without_terminal` —— 无 active worktree 时 `send_agent_command("claude")` 不 panic(no-op)
- [x] 9.13 `shutdown_workspace` 停所有 tab(避免 leak-detection):`shutdown_all_terminals_for_test` 遍历所有 group 所有 tab

## 10. 验证

- [x] 10.1 `cargo fmt && cargo clippy --all-targets` 无 warning
- [x] 10.2 `cargo test` 全绿(core 测试不受影响;app 测试含新增多 tab + agent 按钮测试)
- [ ] 10.3 `cargo run -p lucy-app`:手动验证——
  - 仓库行「Open」是图标按钮(无背景/描边),点击弹目录选择器
  - 侧边栏 `+` 直接建 worktree + 开 shell(不弹菜单)
  - 终端区顶部出现 tab 栏(1 tab "Shell") + 右侧 agent 按钮(Claude / Codex / OpenCode)
  - 点 agent 按钮 → shell 里出现 agent 命令并开始执行;tab 标题随 agent OSC 0/2 标题更新
  - 点 tab 栏 `+` → 新建 "Shell" tab,active 切到新 tab
  - 点 tab 切换 → 终端区切换到对应终端
  - 点 tab `✕` → 只关该终端,worktree 仍在;关最后一个 tab → 终端区空态 + agent 按钮隐藏,worktree 仍在
  - 切到另一 worktree → tab 栏显示该 worktree 的 tabs;切回 → active tab 恢复
  - 关 worktree(侧边栏 `✕`)→ 该 worktree 所有 tab 停止 + worktree 删除
  - 在 shell tab 里跑 `printf '\033]0;my-title\007'` → tab 标题从 "Shell" 变成 "my-title"
