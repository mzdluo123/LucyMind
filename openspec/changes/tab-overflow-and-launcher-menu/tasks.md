## 1. ShellKind 枚举 + spawn_shell_tab 适配

- [x] 1.1 `crates/app/src/workspace/mod.rs`:新增 `enum ShellKind { Default, #[cfg(windows)] Cmd, #[cfg(windows)] PowerShell, #[cfg(windows)] Pwsh }` + `impl ShellKind { fn command(&self) -> Option<(String, Vec<String>)>; fn label(&self) -> &'static str }`
- [x] 1.2 `spawn_shell_tab` 签名改为 `fn spawn_shell_tab(&self, wt_path: &Path, shell: ShellKind, cx: &mut Context<Self>) -> TerminalTab`:把 `shell.command()` 传给 `TerminalView::new` 的 `command` 参数;`TerminalTab.title` 用 `shell.label()` 而非硬编码 "Shell"
- [x] 1.3 `new_terminal_tab` 签名改为 `fn new_terminal_tab(&mut self, shell: ShellKind, cx: &mut Context<Self>)`:内部调 `spawn_shell_tab(&key, shell, cx)`
- [x] 1.4 所有 `spawn_shell_tab` / `new_terminal_tab` 调用处适配:`open_worktree`(无 group 建首个 tab → `ShellKind::Default`)、`new_worktree`(建 worktree 后建首个 tab → `ShellKind::Default`)
- [x] 1.5 `cargo build -p lucy-app` 通过

## 2. launch_agent 方法

- [x] 2.1 `crates/app/src/workspace/mod.rs`:新增 `fn launch_agent(&mut self, agent_name: &str, cx: &mut Context<Self>)`:`self.new_terminal_tab(ShellKind::Default, cx)` + `self.send_agent_command(agent_name, cx)`(创建新 tab + 立即发命令到新 active tab)
- [x] 2.2 `cargo build -p lucy-app` 通过(`launch_agent` 暂无调用处,下一步 UI 接)

## 3. launcher_menu_open 状态 + render overlay

- [x] 3.1 `WorkspaceView` 新增字段 `launcher_menu_open: bool`(默认 `false`),在 `new` / 构造处初始化
- [x] 3.2 `render` 末尾叠加:`if self.launcher_menu_open { root = root.child(self.launcher_menu(cx)); }`(与 `confirm_dialog` / `settings_dialog` 同模式)
- [x] 3.3 `render` 的 `on_key_down`(或 root 的事件):Esc 时若 `launcher_menu_open` 则关闭菜单 + `cx.notify()` + `cx.stop_propagation()`
- [x] 3.4 `cargo build -p lucy-app` 通过(`launcher_menu` 方法暂用 `todo!()`,下一步实现)

## 4. Launcher menu 渲染(tabs.rs)

- [x] 4.1 `crates/app/src/workspace/tabs.rs`:新增 `fn launcher_menu(&self, cx: &mut Context<Self>) -> impl IntoElement`。结构:backdrop(`absolute().inset_0()` + `on_mouse_down(Left)` 关闭)+ card(`absolute().top(px(32.0)).right_0()` + `SURFACE` 底 + `BORDER` 描边 + `radius()` + `min_w(px(200.0))` + `flex_col`)。card 的 `on_mouse_down(Left)` `stop_propagation`(点菜单内不冒泡到 backdrop)
- [x] 4.2 card 内 "New Tab" 分组:标题(`TEXT_DIM` + `text_xs()` + `px(space_sm)` + `py(space_xs)`)+ 菜单项。项:`Default Shell`(所有平台);`#[cfg(windows)]` 追加 `Command Prompt`(cmd.exe)、`PowerShell`(powershell.exe)、`PowerShell 7`(pwsh.exe)。每项 `px(space_md)` + `py(space_xs)` + `cursor_pointer` + `hover(BTN_BG_HOVER)` + `on_click` → `new_terminal_tab(ShellKind::Xxx, cx)` + `launcher_menu_open = false` + `cx.notify()`
- [x] 4.3 分隔线:`h_1()` + `bg(BORDER)` + `my(space_xs)`(New Tab 组与 Launch Agent 组之间)
- [x] 4.4 "Launch Agent" 分组:标题同上 + 迭代 `builtin_agents()`,每项 = agent 图标(`crate::assets::agent_icon`)+ display 名,`on_click` → `launch_agent(name, cx)` + `launcher_menu_open = false` + `cx.notify()`
- [x] 4.5 `cargo build -p lucy-app` 通过

## 5. `+` 按钮移出 tab_list + 改为菜单触发

- [x] 5.1 `tabs.rs::tab_list`:删除末尾的 `+` 按钮 child(`tabs.rs:57-76` 的 `div().id("new-tab")...`)
- [x] 5.2 `tabs.rs::tab_bar`:在 `tab_list` 之后追加 `+` 按钮(`flex_none` + `px(space_sm)` + `h_full()` + `cursor_pointer` + `svg("icons/plus.svg")` + `hover(BTN_BG_HOVER)` + `on_click` → `launcher_menu_open = !launcher_menu_open` + `cx.notify()`)。删除 `.child(self.agent_buttons(cx))`
- [x] 5.3 `tabs.rs`:删除 `fn agent_buttons(&self, cx: &mut Context<Self>) -> impl IntoElement` 方法整个
- [x] 5.4 `cargo build -p lucy-app` 通过

## 6. Tab 自适应宽度 + 横向滚动

> **修订**:tab 用 `flex_1` + `min_w(80px)` + `max_w(200px)`,少时宽(填满可用宽度 ≤200px)、
> 多时缩窄(≥80px,可读)、超出 80px 下限后 `overflow_x_scroll` 横向滚动。
> GPUI `overflow_x_scroll`(`overflow.x = Scroll` + `overflow.y != Scroll`)自动把垂直鼠标滚轮
> 转为横向滚动(div.rs:2424-2428)。`min_w(80px)` 是 CSS flexbox 硬下限,tab 不会缩到 80px 以下。

- [x] 6.1 `tabs.rs::tab_item`:tab 用 `flex_1()` + `min_w(px(80.0))` + `max_w(px(200.0))`(自适应宽度,80px 下限后滚动)
- [x] 6.2 验证:tab 少时宽(≤200px,填满可用宽度)、tab 多时缩窄(≥80px)、超出 80px 下限后 `overflow_x_scroll` 横向滚动(鼠标滚轮悬停 tab 区自动转横向)。目视检查(cargo run)
- [x] 6.3 `cargo build -p lucy-app` 通过

## 7. 测试 accessor 适配

- [x] 7.1 `new_terminal_tab_for_test` 签名改为 `pub fn new_terminal_tab_for_test(&mut self, shell: ShellKind, cx: &mut Context<Self>)`(加 `shell` 参数)
- [x] 7.2 新增 `pub fn launcher_menu_open_for_test(&self) -> bool`(读 `launcher_menu_open`)
- [x] 7.3 新增 `pub fn set_launcher_menu_open_for_test(&mut self, open: bool)`(写 `launcher_menu_open`,供测试直接打开菜单)
- [x] 7.4 新增 `pub fn launch_agent_for_test(&mut self, agent_name: &str, cx: &mut Context<Self>)`(调 `launch_agent`)
- [x] 7.5 新增 `pub fn tab_title_for_test(&self, path: &Path) -> Option<String>`(返回 active tab 的静态回退标题 `TerminalTab.title`,供测试验证 `ShellKind::label()` 生效)
- [x] 7.6 `ShellKind` 在 `#[cfg(feature = "test-support")]` 下 `pub` 导出(测试需构造 `ShellKind::Default` / `ShellKind::Cmd` 等)
- [x] 7.7 `cargo build -p lucy-app --features test-support` 通过

## 8. 现有测试适配

- [x] 8.1 `tests/multi_tab.rs`:所有 `new_terminal_tab_for_test()` 调用改为 `new_terminal_tab_for_test(ShellKind::Default, cx)`(需 `use crate::workspace::ShellKind` 或 `use super::*`)
- [x] 8.2 `tests/multi_tab.rs`:`send_agent_command_for_test` 调用不变(仍发命令到 active tab);新增 `launch_agent_for_test` 用法的测试(见第 9 组)
- [x] 8.3 `tests/common/mod.rs`:若有 `new_terminal_tab_for_test` 调用则适配
- [x] 8.4 `tests/terminal_render.rs` / `tests/close.rs` / `tests/new_worktree.rs` / `tests/smoke.rs` / `tests/worktree_list.rs`:检查 `new_terminal_tab_for_test` 调用,适配新签名
- [x] 8.5 `cargo test -p lucy-app` 全绿(现有测试)

## 9. 单元测试:ShellKind + launcher 状态(`crates/app/src/workspace/mod.rs` `mod tests`)

> 纯逻辑测试(无 PTY / 无 GPUI context),验证 `ShellKind` 枚举的 `command()` / `label()` 映射、
> `launch_agent` 的命令构造逻辑(复用 `agent_command_string`)。与现有 `agent_command_string_*`
> 单元测试同处 `mod tests`,用 `#[test]` 不是 `#[gpui::test]`。

- [x] 9.1 `shell_kind_default_command_is_none` — `ShellKind::Default.command()` 返回 `None`(系统默认 shell,交由 alacritty tty 层决定)
- [x] 9.2 `shell_kind_default_label` — `ShellKind::Default.label()` 返回 `"Shell"`
- [x] 9.3 `shell_kind_cmd_command`(`#[cfg(windows)]`)— `ShellKind::Cmd.command()` 返回 `Some(("cmd.exe", vec![]))`
- [x] 9.4 `shell_kind_cmd_label`(`#[cfg(windows)]`)— `ShellKind::Cmd.label()` 返回 `"cmd"`
- [x] 9.5 `shell_kind_powershell_command`(`#[cfg(windows)]`)— `ShellKind::PowerShell.command()` 返回 `Some(("powershell.exe", vec![]))`
- [x] 9.6 `shell_kind_powershell_label`(`#[cfg(windows)]`)— `ShellKind::PowerShell.label()` 返回 `"PowerShell"`
- [x] 9.7 `shell_kind_pwsh_command`(`#[cfg(windows)]`)— `ShellKind::Pwsh.command()` 返回 `Some(("pwsh.exe", vec![]))`
- [x] 9.8 `shell_kind_pwsh_label`(`#[cfg(windows)]`)— `ShellKind::Pwsh.label()` 返回 `"pwsh"`
- [x] 9.9 `shell_kind_all_variants_covered` — `ShellKind` 所有变体的 `command()` 返回值与 `label()` 返回值一一对应,无 panic(穷举匹配测试,防新增变体漏实现)

## 10. UI 状态测试:launcher menu 状态机(`tests/multi_tab.rs`)

> 覆盖 `launcher_menu_open` 状态 + `ShellKind` + `launch_agent` 的状态机行为。
> 用 `#[gpui::test]` + `build_workspace` + accessor 验证,不依赖像素渲染。

- [x] 10.1 `launcher_menu_closed_by_default` — 新建 workspace 后 `launcher_menu_open_for_test()` 返回 false
- [x] 10.2 `launcher_menu_open_close` — `set_launcher_menu_open_for_test(true)` → `launcher_menu_open_for_test()` 为 true;`set_launcher_menu_open_for_test(false)` → false
- [x] 10.3 `new_terminal_tab_default_shell` — `new_terminal_tab_for_test(ShellKind::Default, cx)` 创建 tab,`tab_count` +1,`active_tab_index` 指向新 tab;tab 静态标题回退为 "Shell"(`terminal_at` 的 `title()` 为 None 时 tab 栏显示 "Shell")
- [x] 10.4 `new_terminal_tab_cmd`(`#[cfg(windows)]`)— `new_terminal_tab_for_test(ShellKind::Cmd, cx)` 创建 tab,`tab_count` +1,`terminal_at` 非 None(PTY spawn 成功)。非 Windows 跳过(条件编译)
- [x] 10.5 `new_terminal_tab_powershell`(`#[cfg(windows)]`)— 同上,`ShellKind::PowerShell`
- [x] 10.6 `new_terminal_tab_pwsh`(`#[cfg(windows)]`)— 同上,`ShellKind::Pwsh`(若系统未装 pwsh,PTY spawn 失败 → `TerminalView::new` 返回 Err → `spawn_shell_tab` panic;此测试用 `#[ignore]` 标记或验证 panic 行为)
- [x] 10.7 `new_terminal_tab_multiple_shells` — 同一 worktree 连续建 Default + Cmd(Windows) / Default + Default(非 Windows)多个 tab,`tab_count` 递增,`active_tab_index` 每次指向最新 tab,每个 `terminal_at` entity_id 不同
- [x] 10.8 `new_terminal_tab_noop_without_active` — 无 active worktree 时 `new_terminal_tab_for_test(ShellKind::Default, cx)` 不 panic,`active_tab_index()` 仍为 None
- [x] 10.9 `launch_agent_creates_new_tab` — `launch_agent_for_test("test", cx)` 后 `tab_count` +1,`active_tab_index` 指向新 tab。用 `temp_repo_with_agent`(fake agent config)
- [x] 10.10 `launch_agent_sends_command_to_new_tab` — `launch_agent_for_test("test", cx)` 后 `wait_for` 轮询新 tab 的 `snapshot_text()` 是否包含 agent 命令的 marker(如 MARKER_READY)。用 `temp_repo_with_agent` + 跨平台 fake agent 命令
- [x] 10.11 `launch_agent_unknown_sets_error` — `launch_agent_for_test("nonexistent", cx)` 后 `status_is_error()` 为 true 且 `current_status()` 包含 "nonexistent"。注意:`launch_agent` 先建 tab 再发命令,unknown agent 时 tab 已建但命令未发(`send_agent_command` no-op + set error)
- [x] 10.12 `launch_agent_noop_without_active` — 无 active worktree 时 `launch_agent_for_test("test", cx)` 不 panic。`new_terminal_tab(Default)` no-op(active 为 None),`send_agent_command` 也 no-op
- [x] 10.13 `multiple_launch_agent_creates_separate_tabs` — 连续 `launch_agent_for_test("test", cx)` 两次,`tab_count` +2,两个 tab 的 `terminal_at` entity_id 不同(各自独立终端)
- [x] 10.14 `close_tab_after_launch_agent` — `launch_agent_for_test("test", cx)` 建新 tab → `close_tab_for_test(active, cx)` 关掉它 → `tab_count` 减 1,`active_tab_index` 回退
- [x] 10.15 `send_agent_command_targets_new_tab_after_launch` — `launch_agent_for_test("test", cx)` 建新 tab 后,`send_agent_command_for_test("test", cx)` 发命令到新 tab(不是旧 tab)。验证:记录旧 tab entity_id → launch_agent → 新 tab entity_id 不同 → send_agent_command 命令出现在新 tab snapshot
- [x] 10.16 `switch_worktree_does_not_affect_launcher_menu` — 开两个 worktree,`set_launcher_menu_open_for_test(true)` → 切到另一个 worktree → `launcher_menu_open_for_test()` 仍为 true(menu 状态是 workspace 级,不随 worktree 切换)
- [x] 10.17 `close_last_tab_does_not_crash_launcher` — 关掉最后一个 tab(group 移除)后 `launcher_menu_open_for_test()` 状态不变(不 panic),`new_terminal_tab_for_test(Default)` 能重建 group

## 11. 集成测试:ShellKind + launch_agent 端到端(`tests/multi_tab.rs`)

> 覆盖 ShellKind → PTY spawn → 终端可交互的端到端流程。
> 用 `wait_for` 轮询 PTY 输出,验证不同 shell 类型确实启动了对应进程。

- [x] 11.1 `shell_kind_default_spawns_shell` — `new_terminal_tab_for_test(ShellKind::Default, cx)` → 等 PTY 就绪 → `send_text("echo MARKER_SHELL\r")` → `wait_for` snapshot 包含 "MARKER_SHELL"(验证 Default shell 可交互)
- [x] 11.2 `shell_kind_cmd_echo`(`#[cfg(windows)]`)— `ShellKind::Cmd` → `send_text("echo MARKER_CMD\r")` → snapshot 包含 "MARKER_CMD"(验证 cmd.exe 启动 + 可交互)
- [x] 11.3 `shell_kind_powershell_echo`(`#[cfg(windows)]`)— `ShellKind::PowerShell` → `send_text("echo MARKER_PS\r")` → snapshot 包含 "MARKER_PS"
- [x] 11.4 `launch_agent_default_shell_tab` — `launch_agent_for_test("test", cx)` 创建的新 tab 是 Default shell(不是 agent 子进程)。验证:新 tab `terminal_at` 的 `title()` 初始为 None(shell 未发 OSC),发命令后 snapshot 出现 agent 输出 marker
- [x] 11.5 `launch_agent_command_in_new_tab_not_old` — 建两个 tab(tab 0 + tab 1),切到 tab 0,`launch_agent_for_test("test", cx)` 创建 tab 2(active=2),agent 命令出现在 tab 2 的 snapshot,不出现在 tab 0 的 snapshot。验证 launch_agent 总是发到新 tab,不是当前 tab
- [x] 11.6 `shell_kind_label_as_fallback_title` — `ShellKind::Default` 的 tab 静态标题是 "Shell";`ShellKind::Cmd`(Windows)的 tab 静态标题是 "cmd"。验证:tab 栏回退标题(`TerminalTab.title` 字段)与 `ShellKind::label()` 一致(用 `tab_title_for_test` accessor)
- [x] 11.7 `tab_flex_shrink_many_tabs` — 建 10 个 tab(Default shell),`tab_count` == 10,不 panic,所有 tab 的 `terminal_at` entity_id 互不相同。验证 tab 多时不崩溃(无法在 headless 测像素宽度,但验证状态机不崩)

## 12. 集成测试:launcher menu 交互(`tests/multi_tab.rs`)

> 覆盖 launcher menu 打开 / 关闭 / 选择项后的端到端行为。
> menu 渲染是 GPUI div,无法在 headless 测点击;用 `set_launcher_menu_open_for_test` + accessor 验证状态机。

- [x] 12.1 `launcher_menu_open_after_toggle` — `set_launcher_menu_open_for_test(true)` → `launcher_menu_open_for_test()` 为 true;`new_terminal_tab_for_test(Default, cx)` 后(模拟选中菜单项)`launcher_menu_open_for_test()` 仍为 true(菜单项逻辑需手动设 false;或 accessor 不验证菜单关闭,改在实现里 `launcher_menu_open = false` + 测试验证)
- [x] 12.2 `launcher_menu_closes_after_new_terminal_tab` — 验证 `new_terminal_tab` 在 menu 选中场景下关闭菜单(实现:`+` 按钮的 `on_click` 先 `set_launcher_menu_open(false)` 再调 `new_terminal_tab`;或 `new_terminal_tab` 不关菜单,由 menu item 的 `on_click` 关)。按 design D4,menu item `on_click` 设 `launcher_menu_open = false` + 调 `new_terminal_tab`。测试:open menu → 模拟 menu item 选中(直接调 `new_terminal_tab_for_test`)→ 验证实现侧关菜单(需 `new_terminal_tab` 方法内部不关菜单,由调用方关;或加 `new_terminal_tab_from_menu_for_test` 包装)
- [x] 12.3 `launcher_menu_closes_after_launch_agent` — 同上,`launch_agent` 选中后菜单关闭。`launch_agent_for_test` 包装:设 `launcher_menu_open = false` + 调 `launch_agent`
- [x] 12.4 `launcher_menu_state_is_workspace_level` — 开两个 worktree,worktree A 打开 menu → 切到 worktree B → menu 仍打开 → 切回 A → menu 仍打开(menu 是 workspace 级状态,非 worktree 级)

## 13. 质量门

- [x] 13.1 `cargo fmt`
- [x] 13.2 `cargo clippy --all-targets` 0 warnings
- [x] 13.3 `cargo test` 全绿(core + terminal + app,含所有新增 `#[test]` 单元测试 + `#[gpui::test]` UI 状态 / 集成测试)

## 14. 在文件管理器中打开 worktree 目录(reveal button)

> `tab_bar` 的 `+` 按钮右边新增「在文件管理器中打开」按钮(folder-open 图标)。
> 点击调用系统命令(macOS: `open`、Windows: `explorer`、Linux: `xdg-open`)打开 active worktree 目录。
> 按钮在 `tab_list` 外(`tab_bar` 直接 child),`flex_none` 固定,不受 tab 滚动影响。

- [x] 14.1 `crates/app/src/workspace/mod.rs`:新增 `fn reveal_in_file_manager(&self, cx: &mut Context<Self>)`:取 `self.active` 路径,用 `std::process::Command::new("open"/"explorer"/"xdg-open")` + `.arg(path)` + `.spawn()`(不阻塞 UI 线程)。无 active 时 no-op
- [x] 14.2 `crates/app/src/workspace/tabs.rs`:`tab_bar` 在 `+` 按钮(`plus_button`)之后追加 `reveal_button`(`flex_none` + `px(space_sm)` + `h_full()` + `cursor_pointer` + `svg("icons/folder-open.svg")` + `hover(BTN_BG_HOVER)` + `on_click` → `reveal_in_file_manager(cx)`)。`tab_bar` 结构改为 `[tab_list] [+] [reveal]`
- [x] 14.3 `crates/app/src/workspace/mod.rs`:新增 `#[cfg(feature = "test-support")] pub fn reveal_in_file_manager_for_test(&self, cx: &mut Context<Self>)`(调 `reveal_in_file_manager`,测试用)
- [x] 14.4 `cargo build -p lucy-app` 通过

## 15. 测试:reveal_in_file_manager

> 覆盖 `reveal_in_file_manager` 的状态机行为(无 active no-op、有 active 调系统命令)。
> `std::process::Command::spawn` 在测试环境可执行(`open`/`explorer`/`xdg-open` 是系统命令),
> 但测试不验证文件管理器是否真打开(无法在 headless 验证),只验证不 panic + 无 active 时 no-op。

- [x] 15.1 `reveal_in_file_manager_noop_without_active` — 无 active worktree 时 `reveal_in_file_manager_for_test(cx)` 不 panic(无 active → no-op)
- [x] 15.2 `reveal_in_file_manager_with_active` — 有 active worktree(建 worktree + open)时 `reveal_in_file_manager_for_test(cx)` 不 panic(spawn 系统命令,不阻塞)
- [x] 15.3 `reveal_in_file_manager_after_close` — 关掉最后一个 tab(group 移除,active 仍在但 terminals 无 entry)后 `reveal_in_file_manager_for_test(cx)` 不 panic
