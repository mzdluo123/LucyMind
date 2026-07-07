## Context

当前仓库打开流程（`crates/app/src/workspace/mod.rs` + `dialogs.rs`）有两套入口：

1. **本地模式**（`LocalHost`）：`open_repo_picker` → `open_repo_choice_open = true` → `open_repo_choice_dialog` 渲染「本地文件夹…」+「WSL 路径…」二选一弹窗。选「本地文件夹…」→ `open_local_picker` → `cx.prompt_for_paths`（系统原生目录选择器，异步返回路径）→ `git::main_worktree_root` → `set_repo`。
2. **WSL 模式**（`WslHost`）：`open_repo_picker` → 同一个二选一弹窗 → 选「WSL 路径…」→ `open_wsl_browser` → `WslBrowser` 状态（`current_dir` + `entries` + `loading` + `error`）→ `load_wsl_dir`（后台 `host.list_dir`）→ `wsl_browser_dialog` 渲染目录导航列表（点文件夹进入、点 `..` 返回、点「选择此目录」确认）→ `commit_wsl_browser` → `git::main_worktree_root` → `set_repo`。

问题：
- WSL 浏览器每次导航都等 `wsl.exe ls` 返回（1-2 秒），无法输入路径快速跳转。
- 无模糊搜索——目录里 100 个条目要肉眼找。
- 无键盘导航（只能鼠标点）。
- 两套入口割裂（本地用原生选择器，WSL 用自建浏览器），体验不一致。
- `WslBrowser` 状态（`current_dir` / `entries` / `loading` / `error`）和 `load_wsl_dir` / `navigate_wsl_dir` / `commit_wsl_browser` 方法占用 `WorkspaceView` 大量空间。

Zed 的 `OpenPathPrompt`（`crates/open_path_prompt/src/open_path_prompt.rs`）给出了更好的范式：

- **文本输入 + 实时补全**：用户输入路径，系统切分出「目录部分」（`list_dir` 的参数）和「后缀」（模糊过滤词），后台异步列目录，前台显示过滤后的条目。
- **cancel-flag 模式**：每次 keystroke 翻转 `Arc<AtomicBool>`，旧任务完成后检查 flag 发现已取消则丢弃结果——无显式 debounce timer，但效果等同（快速打字时只有最后一个 keystroke 的结果生效）。
- **路径切分**：`get_dir_and_suffix(query)` 用最后一个路径分隔符切分。Posix：`rfind('/')`；Windows：`rfind('\\')` 或 `rfind('/')` 取最大。
- **Tab 补全**：选中条目是目录 → 补全 `dir + name + separator`（触发重新 `list_dir`）；是文件 → 补全 `dir + name`。
- **Enter 确认**：取选中条目（或输入的完整路径），调 `git::main_worktree_root` 验证，成功 → `set_repo` + 关闭弹窗，失败 → 显示错误。
- **键盘导航**：Up/Down 移动 `selected_index`（循环），Esc 关闭。

**关键约束**：
- LucyMind 用 `gpui 0.2.2`（crates.io 发布版，非 Zed 的 git fork）+ `gpui-component 0.5.1`。没有 Zed 的 `picker` crate / `ModalLayer` / `ui::ListItem` / `HighlightedLabel` / `fuzzy` crate。需要在 `crates/app/src/ui/` 自建简化版 `PathPicker`。
- `Host::list_dir` 已存在（`crates/core/src/host.rs`），返回 `Vec<DirEntry>`（`name: String, is_dir: bool`），隐藏文件不返回，目录排前、同类按名称排序。`PathPicker` 直接复用。
- `gpui-component` 的 `InputState` 已用于别名编辑器（`dialogs.rs` 的 `alias_dialog`），支持 IME + 选择 + 复制粘贴。`PathPicker` 复用 `InputState` 作文本输入。
- GPUI 0.2.2 有 `uniform_list`（虚拟化列表）和 `list`（变高列表），可用于补全列表渲染。但补全列表通常不超过几十条（一个目录的条目数），用手动 `div` 列表 + `overflow_y_scroll` 也可（当前 `wsl_browser_dialog` 就是手动 div）。
- `WorkspaceView` 已有 `host: Arc<dyn Host>` 字段，`PathPicker` 接收 `host` clone 来调 `list_dir`。
- 路径风格：`WslHost` 用 Posix `/`；`LocalHost` 在 Windows 上用 `\`（但 `/` 也被 Windows 接受）。`PathPicker` 用 `host.is_remote()` 判断：`true` → Posix `/`，`false` → OS 分隔符（`std::path::MAIN_SEPARATOR`）。或新增 `Host::path_separator()` trait 方法。

## Goals / Non-Goals

**Goals:**
- 新增 `PathPicker` 组件（`crates/app/src/ui/path_picker.rs`），复用 Zed `OpenPathPrompt` 的核心范式：文本输入 + 实时补全 + cancel-flag 异步 + 键盘导航 + Tab 补全 + Enter 确认。
- 统一本地 / WSL 两个入口：`open_repo_picker` 直接打开 `PathPicker`（用当前 `self.host`），不再弹「Local / WSL」二选一弹窗。
- 移除 `WslBrowser` 状态、`open_repo_choice_dialog`、`wsl_browser_dialog`、`load_wsl_dir`、`navigate_wsl_dir`、`commit_wsl_browser` 及 `open_repo_choice_open` / `wsl_browser` 字段。
- 本地模式底部保留「Browse…」按钮（`cx.prompt_for_paths`），WSL 模式隐藏。
- 路径切分、模糊过滤、补全逻辑用纯函数单测（`#[test]`，无 GPUI / 无 Host）。
- UI 状态（打开/关闭/选中/确认/错误）用 `#[gpui::test]` + accessor 验证。

**Non-Goals:**
- 不做 fuzzy 匹配的高亮（字符级高亮匹配位置）——Phase 1 用简单 `contains` / `starts_with` 过滤，不高亮匹配字符。未来可加 `HighlightedLabel`。
- 不做路径历史 / 最近打开的仓库列表——Phase 1 每次从 `/`（WSL）或 home（本地）开始。
- 不做 `~` home 展开——Phase 1 用户手动输入完整路径。未来可加 `host.resolve_home()` 展开。
- 不做「创建新路径」（Zed 的 `creating_path: true` 模式）——Phase 1 只打开已有目录。
- 不做文件图标系统（Zed 的 `FileIcons`）——Phase 1 用 `📁` / `📄` emoji（与当前 `wsl_browser_dialog` 一致）。
- 不做多工作区（Zed 的 `WorktreeId`）——Phase 1 单仓库。
- 不依赖 Zed 的 `picker` / `workspace` / `ui` crate——在 `crates/app/src/ui/` 自建简化版。

## Decisions

### D1: `PathPicker` 是独立 Entity，不是 `WorkspaceView` 的 impl 方法

`PathPicker` 是 `gpui::Entity<PathPicker>`（类似 `TerminalView`），持有自己的状态（query / entries / selected_index / cancel_flag / host / on_confirm 回调）。`WorkspaceView` 持有 `Option<Entity<PathPicker>>`，打开时 `cx.new(|cx| PathPicker::new(host, initial_query, on_confirm, cx))`，关闭时 `self.path_picker = None`。

**理由**：
- 当前 `wsl_browser` 状态直接放在 `WorkspaceView` 上（`current_dir` / `entries` / `loading` / `error`），导致 `WorkspaceView` 字段膨胀、`load_wsl_dir` / `navigate_wsl_dir` / `commit_wsl_browser` 方法占用 `mod.rs` 空间。独立 Entity 把这些状态隔离到 `path_picker.rs`。
- `PathPicker` 可复用（未来可用于「选择 hook 的 copy 源文件」等其他路径选择场景）。
- `PathPicker::new` 接收 `on_confirm: Box<dyn Fn(PathBuf, &mut Window, &mut App)>` 回调，确认时调用（解耦 `PathPicker` 与 `WorkspaceView`）。

**备选（否决）**：把 `PathPicker` 状态直接放 `WorkspaceView`（像当前 `WslBrowser`）——字段膨胀、`mod.rs` 变大、不可复用。

### D2: 路径切分用纯函数 `get_dir_and_suffix`

```rust
/// 把查询字符串切分为 (目录部分, 后缀)。
/// Posix: `rfind('/')` → `(dir + "/", suffix)`。
/// Windows: `rfind('\\')` 或 `rfind('/')` 取最大 → `(dir + "\\", suffix)`。
/// 目录部分末尾补分隔符（方便拼接条目名）。
fn get_dir_and_suffix(query: &str, separator: char) -> (String, String)
```

例：
- `get_dir_and_suffix("/home/user/Doc", '/')` → `("/home/user/", "Doc")`
- `get_dir_and_suffix("/home/user/", '/')` → `("/home/user/", "")`（后缀空 = 显示全部）
- `get_dir_and_suffix("/", '/')` → `("/", "")`（根目录）
- `get_dir_and_suffix("Doc", '/')` → `("", "Doc")`（无分隔符 = 相对路径，Phase 1 不处理相对路径，初始查询总是绝对路径）

目录部分变化时才重新 `list_dir`（对比 `PathPickerState.dir` 与新 `dir`）。

### D3: cancel-flag 异步目录列表

```rust
struct PathPicker {
    state: PathPickerState,
    host: Arc<dyn Host>,
    cancel_flag: Arc<AtomicBool>,
    on_confirm: Box<dyn Fn(PathBuf, &mut Window, &mut Context<Self>)>,
    input: Entity<InputState>,
    separator: char,
}

struct PathPickerState {
    /// 当前查询文本（与 InputState 同步）。
    query: String,
    /// 当前列出的目录部分（list_dir 的参数）。
    dir: String,
    /// 该目录的条目列表（list_dir 返回，未过滤）。
    entries: Vec<DirEntry>,
    /// 过滤后的条目索引（按 suffix 过滤）。
    filtered: Vec<usize>,
    /// 当前选中条目在 filtered 中的索引。
    selected_index: usize,
    /// 后台 list_dir 进行中。
    loading: bool,
    /// 加载失败信息。
    error: Option<String>,
}
```

`update_matches(query)` 流程：
1. `get_dir_and_suffix(query, separator)` → `(new_dir, suffix)`。
2. `new_dir != state.dir` → 翻转 `cancel_flag`（取消旧任务），新建 `cancel_flag`，`state.loading = true`，`cx.notify()`。后台 `cx.background_executor().spawn` 跑 `host.list_dir(&new_dir)`，完成后回主线程检查 `cancel_flag`（已取消则丢弃），更新 `state.entries` + `state.dir` + `state.loading = false`。
3. `new_dir == state.dir` → 只更新 `suffix`（过滤不变，重新过滤 `entries`）。
4. 过滤：`state.filtered = entries.iter().enumerate().filter(|(_, e)| e.name.to_lowercase().contains(&suffix.to_lowercase())).map(|(i, _)| i).collect()`。后缀为空时 `filtered = all indices`。
5. `state.selected_index = 0`（重置选中到第一条）。
6. `state.query = query`。

**备选（否决）**：显式 debounce timer（`tokio::time::sleep` 100ms）——cancel-flag 模式更简单，且 GPUI 的 `background_executor` 已是异步的，不需要额外 timer。

### D4: 键盘交互

`PathPicker::render` 的根 `div` 绑定 `on_key_down`：

| 键 | 行为 |
|---|---|
| `Up` | `selected_index = (selected_index + filtered.len() - 1) % filtered.len()`（循环到末尾） |
| `Down` | `selected_index = (selected_index + 1) % filtered.len()`（循环到第一条） |
| `Tab` | `confirm_completion()`：取 `filtered[selected_index]` 对应的 `entry`，补全 `dir + entry.name + (entry.is_dir ? separator : "")`，写入 `InputState`，触发 `update_matches`（目录补 `/` 后重新 `list_dir`）。`cx.stop_propagation()`（防 Tab 切焦点） |
| `Enter` | `confirm()`：取 `filtered[selected_index]`（或输入的完整路径），调 `on_confirm(path)` 回调（`WorkspaceView` 侧验证 `git::main_worktree_root` + `set_repo`），成功关闭弹窗 |
| `Escape` | `on_dismiss()` 回调（`WorkspaceView` 侧 `self.path_picker = None`） |

`InputState` 的文本变化监听：`InputState` 值变化时触发 `update_matches`。具体实现：`PathPicker` 在 `new` 时 `cx.subscribe(&input, |this, _, _, cx| { this.update_matches(input.read(cx).value().to_string(), cx); })`（`InputState` emit 值变化事件）。或每次 `on_key_down` 时同步读 `input.read(cx).value()`（更简单，但 IME 组合中可能不同步）。

**备选（否决）**：用 GPUI 的 `EntityInputHandler` 直接处理字符输入（不经过 `InputState`）——`InputState` 已提供 IME + 选择 + 复制粘贴，重造不划算。

### D5: 补全列表渲染

补全列表用手动 `div` 列表（不用 `uniform_list`，因为条目数通常 < 100，虚拟化收益不大，手动 div 更简单且与当前 `wsl_browser_dialog` 风格一致）：

```rust
// 补全列表容器（可滚动，max_h 限制高度）。
let mut list = div().flex().flex_col().gap_0().max_h(px(320.0)).overflow_y_scroll();
for (i, &entry_idx) in state.filtered.iter().enumerate() {
    let entry = &state.entries[entry_idx];
    let is_selected = i == state.selected_index;
    let icon = if entry.is_dir { "📁 " } else { "📄 " };
    list = list.child(
        div()
            .id(SharedString::from(format!("picker-entry-{i}")))
            .flex().flex_row().items_center().gap(space_sm)
            .px(space_sm).py(space_xs)
            .cursor_pointer()
            .when(is_selected, |d| d.bg(theme::BTN_BG_HOVER))
            .hover(|s| s.bg(theme::BTN_BG_HOVER))
            .text_color(if entry.is_dir { theme::TEXT } else { theme::TEXT_FAINT })
            .child(SharedString::from(format!("{icon}{}", entry.name)))
            .on_click(cx.listener(move |this, _ev, _w, cx| {
                this.select(i, cx);  // 选中 + 触发更新
            })),
    );
}
```

点击条目：选中该条目（`selected_index = i`）。双击或点击后按 Enter 确认（Phase 1 单击只选中，Enter 确认；未来可双击直接确认）。

### D6: `on_confirm` 回调与 `WorkspaceView` 集成

`PathPicker::new(host, initial_query, on_confirm, cx)` 的 `on_confirm` 是 `Box<dyn Fn(PathBuf, &mut Window, &mut Context<WorkspaceView>)>`（但 GPUI 的 Entity 回调签名限制，实际用 `Box<dyn Fn(PathBuf, &mut Window, &mut App)>` + `WeakEntity<WorkspaceView>` 在闭包内 `update`）。

更简洁的方式：`PathPicker` emit 一个 `PathPicked(PathBuf)` 事件，`WorkspaceView` `cx.subscribe(&picker, |this, _, event, cx| { ... })` 处理。但 GPUI 0.2.2 的 `EventEmitter` 需要明确事件类型。用回调更直接：

```rust
impl PathPicker {
    pub fn new(
        host: Arc<dyn Host>,
        initial_query: String,
        cx: &mut Context<Self>,
        on_confirm: Box<dyn Fn(PathBuf, &mut Window, &mut Context<Self>)>,
    ) -> Self { ... }
}
```

`on_confirm` 内部：`this.on_confirm` 存 `Box<dyn Fn(PathBuf, &mut Window, &mut Context<PathPicker>)>`，确认时调用。`PathPicker` 的 `on_confirm` 闭包内调 `WeakEntity<WorkspaceView>` 的 `update` 来执行 `set_repo` + 关闭弹窗。

实际实现时 `WorkspaceView::open_repo_picker`：
```rust
fn open_repo_picker(&mut self, cx: &mut Context<Self>) {
    let host = self.host.clone();
    let initial = self.repo.clone()
        .map(|r| r.to_string_lossy().into_owned())
        .unwrap_or_else(|| if host.is_remote() { "/".into() } else { /* home dir */ });
    let weak = cx.weak_entity();
    let picker = cx.new(|cx| PathPicker::new(host, initial, cx, Box::new(move |path, _w, cx| {
        let _ = weak.update(cx, |view, cx| {
            match git::main_worktree_root(view.host.as_ref(), &path) {
                Some(root) => {
                    view.set_repo(root);
                    view.path_picker = None;
                    view.set_status("已打开仓库", false);
                }
                None => {
                    // 在 PathPicker 内显示错误（而非 status_bar）
                    cx.update_entity(.., |picker: &mut PathPicker, cx| {
                        picker.set_error("所选目录不是 git 仓库");
                    });
                }
            }
            cx.notify();
        });
    })));
    self.path_picker = Some(picker);
    cx.notify();
}
```

### D7: Host 路径风格

`PathPicker` 用 `host.is_remote()` 判断路径分隔符：
- `true`（WslHost）→ `separator = '/'`，初始查询 `"/"`。
- `false`（LocalHost）→ `separator = std::path::MAIN_SEPARATOR`（Windows `\`，macOS/Linux `/`），初始查询为 home 目录（`dirs::home_dir()` 或 `std::env::var("HOME")`）或当前 `self.repo`。

**备选（否决）**：新增 `Host::path_separator() -> char` trait 方法——`is_remote()` 已够用，且 `LocalHost` 在 Windows 上 `/` 和 `\` 都接受，不必精确。

### D8: 移除旧代码

移除以下 `WorkspaceView` 字段和方法：
- 字段：`open_repo_choice_open: bool`、`wsl_browser: Option<WslBrowser>`、`WslBrowser` struct。
- 方法：`open_repo_choice_dialog`、`wsl_browser_dialog`、`open_wsl_browser`、`load_wsl_dir`、`navigate_wsl_dir`、`commit_wsl_browser`。
- `render` 的模态叠加区：移除 `if self.open_repo_choice_open` 和 `if self.wsl_browser.is_some()` 两个分支，改为 `if let Some(picker) = &self.path_picker { root.child(picker.clone()) }`。
- `on_key_down` 的 Esc 处理：移除 `open_repo_choice_open` 和 `wsl_browser` 分支（`PathPicker` 自己处理 Esc）。
- 测试 accessor：移除 `open_repo_choice_open`、`wsl_browser_open`、`open_wsl_browser_for_test`、`commit_wsl_browser_for_test`、`set_wsl_browser_dir_for_test`、`open_repo_picker_for_test`、`open_local_picker_for_test`。新增 `path_picker_open`、`path_picker_query`、`path_picker_selected_entry`、`path_picker_entries`、`set_path_picker_query_for_test`、`confirm_path_picker_for_test` 等 accessor。

### D9: 错误处理

`PathPicker` 内部显示错误（不经过 `WorkspaceView::set_status`）：
- `list_dir` 失败（如权限不足、路径不存在）→ `state.error = Some(format!("{e}"))`，列表区显示错误文字。
- `confirm` 失败（不是 git 仓库）→ `state.error = Some("所选目录不是 git 仓库")`，列表区显示错误文字，弹窗不关闭。
- 错误在下次 `update_matches`（用户继续输入）时清除（`state.error = None`）。

## Risks / Trade-offs

- **[InputState 值变化监听]** → `gpui-component 0.5.1` 的 `InputState` 可能没有 `on_change` 事件。备选：每次 `on_key_down` 时读 `input.read(cx).value()` 同步触发 `update_matches`（IME 组合中可能不同步，但路径输入基本是 ASCII，影响小）。或用 `cx.subscribe(&input, ...)` 监听 `InputEvent`（如果 `InputState` emit 事件）。实现时需验证 `InputState` 的事件 API。
- **[cancel-flag 不取消 wsl.exe 进程]** → `Arc<AtomicBool>` 只丢弃结果（不杀进程），`wsl.exe ls` 仍会跑完。快速打字时可能有多个 `wsl.exe` 进程并行（每个 ~100ms），但结果被 cancel-flag 丢弃，UI 只显示最后一个。Phase 1 可接受（WSL `ls` 快，不会堆积太多）。未来可优化为长连接 shell server。
- **[手动 div 列表 vs uniform_list]** → 手动 div 列表在条目多时（如 500+ 文件的目录）渲染性能差（每条都渲染 div）。Phase 1 可接受（仓库根目录通常 < 100 条目）。未来可改 `uniform_list` 虚拟化。
- **[模糊过滤用 contains 而非 fuzzy match]** → Zed 用 `fuzzy` crate 做字符级模糊匹配 + 高亮匹配位置。Phase 1 用 `to_lowercase().contains()` 简化（足够好用，且无新依赖）。未来可加 fuzzy match + 高亮。
- **[本地模式初始查询用 home 目录]** → `dirs::home_dir()` 需新增 `dirs` 依赖（或用 `std::env::var("HOME" / "USERPROFILE")`）。备选：初始查询为空（用户从根开始输入），或用当前 `self.repo`（已有仓库时切换仓库的场景）。
- **[PathPicker 是 Entity 不是 WorkspaceView 方法]** → `WorkspaceView` 需 `cx.subscribe` 或用 `WeakEntity` 回调与 `PathPicker` 通信。回调闭包捕获 `WeakEntity<WorkspaceView>`，在 `on_confirm` 内 `weak.update(cx, |view, cx| { ... })`。这是 GPUI 的标准模式（`TerminalView` 的事件也是这么处理的）。
