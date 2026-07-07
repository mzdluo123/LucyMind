## 1. App: FileTreePanel Entity 骨架

- [ ] 1.1 新建 `crates/app/src/file_tree_panel.rs`，定义 `FileTreePanel` struct：`root: PathBuf`、`host: Arc<dyn Host>`、`expanded: HashSet<PathBuf>`、`dir_cache: HashMap<PathBuf, Vec<DirEntry>>`、`visible: Vec<VisibleEntry>`、`selected: Option<PathBuf>`、`width: f32`、`loading: Option<PathBuf>`、`context_menu: Option<ContextMenuState>`、`focus: FocusHandle`
- [ ] 1.2 定义 `VisibleEntry` struct：`path: PathBuf`、`name: String`、`is_dir: bool`、`depth: usize`、`is_expanded: bool`、`is_loaded: bool`
- [ ] 1.3 定义 `ContextMenuState` struct：`target: PathBuf`、`position: gpui::Point<Pixels>`
- [ ] 1.4 实现 `FileTreePanel::new(host: Arc<dyn Host>, root: PathBuf, cx: &mut Context<Self>) -> Self`：初始化字段，`expanded.insert(root.clone())`（自动展开根），触发 `load_dir(root, cx)`（后台 `list_dir`）
- [ ] 1.5 实现 `FileTreePanel::set_root(&mut self, root: PathBuf, cx: &mut Context<Self>)`：清空 `expanded` / `dir_cache` / `visible` / `selected`，设 `root = root`，`expanded.insert(root.clone())`，触发 `load_dir(root, cx)`
- [ ] 1.6 实现 `impl Render for FileTreePanel`：面板容器（`div().flex_none().w(px(width)).h_full().bg(SURFACE).border_r_1().border_color(BORDER).font_family(FONT_UI)`）+ 头部（"FILES" + 刷新按钮）+ 可滚动文件列表（`div().id("file-tree-scroll").size_full().overflow_y_scroll().p(space_sm)`）+ 空态（无 active worktree 时 "select a worktree to browse files"）+ 右键菜单 overlay
- [ ] 1.7 在 `crates/app/src/lib.rs` 加 `pub mod file_tree_panel;` + `pub use file_tree_panel::FileTreePanel;`

## 2. App: 目录加载与缓存

- [ ] 2.1 实现 `FileTreePanel::load_dir(&mut self, dir: PathBuf, cx: &mut Context<Self>)`：`loading = Some(dir.clone())` + `cx.notify()` → 后台 `cx.background_executor().spawn(async { host.list_dir(&dir) })` → 完成后 `dir_cache.insert(dir, entries)` + `loading = None` + `recompute_visible()` + `cx.notify()`
- [ ] 2.2 实现 `FileTreePanel::recompute_visible(&mut self)`：从 `root` DFS 遍历 `dir_cache` + `expanded`，计算 `Vec<VisibleEntry>`。对每个 `is_dir && is_expanded && !dir_cache.contains(path)` 的目录，插入一个 "加载中…" 占位条目（`is_loaded: false`）。保持 `dir_cache` 的排序（不重排）。`recompute_visible` 后修正 `selected`（在新 `visible` 中查找路径，找不到则 `None`）
- [ ] 2.3 实现 `FileTreePanel::toggle_expanded(&mut self, path: &Path, cx: &mut Context<Self>)`：`expanded.contains(path)` → 移除（折叠）+ `recompute_visible`；`!expanded.contains(path)` → 插入（展开）+ `dir_cache.contains(path)` ? `recompute_visible` : `load_dir(path, cx)`
- [ ] 2.4 实现 `FileTreePanel::refresh(&mut self, cx: &mut Context<Self>)`：`dir_cache.clear()` + 对 `expanded` 中每个目录调 `load_dir`（重新加载）
- [ ] 2.5 `#[test]` 单测 `recompute_visible`：用 `MockHost` 预装 `/root` → `[src (dir), README.md (file)]`、`/root/src` → `[main.rs, lib (dir)]`，展开 `/root` + `/root/src`，验证 `visible` = `[(src, 0, dir, expanded), (main.rs, 1, file), (lib, 1, dir), (README.md, 0, file)]`；折叠 `/root/src` → `visible` = `[(src, 0, dir, collapsed), (README.md, 0, file)]`
- [ ] 2.6 `#[test]` 单测 `toggle_expanded`：展开未缓存的目录 → `loading = Some(path)` + `load_dir` 被调用；展开已缓存的目录 → 不调 `load_dir`；折叠 → `expanded.remove` + `recompute_visible`

## 3. App: 选中与键盘导航

- [ ] 3.1 实现 `FileTreePanel::select(&mut self, path: PathBuf, cx: &mut Context<Self>)`：`selected = Some(path)` + `cx.notify()`（auto-scroll 在 render 时通过 scroll handle 实现）
- [ ] 3.2 实现 `FileTreePanel::navigate(&mut self, direction: NavDirection, cx: &mut Context<Self>)`：`NavDirection::Up` → `selected_index` 减 1（循环到末尾）；`NavDirection::Down` → `selected_index` 加 1（循环到第一条）；`NavDirection::Left` → 选中是目录且已展开 → 折叠；选中是目录且已折叠 或 文件 → 跳到父目录；`NavDirection::Right` → 选中是目录且已折叠 → 展开；选中是目录且已展开 → 跳到第一个子条目；选中是文件 → no-op
- [ ] 3.3 `selected_index` 计算：`selected` 路径在 `visible` 中的 `iter().position(|e| e.path == selected)`。`navigate` 用 `visible` 的索引做算术
- [ ] 3.4 实现 `FileTreePanel::on_key_down`：Up/Down/Left/Right → `navigate`；Enter → `selected` 是目录 → `toggle_expanded`；Escape → `context_menu` 有值 → 关闭菜单；无值 → no-op
- [ ] 3.5 `#[test]` 单测 `navigate`：`visible` = `[src, main.rs, lib, README.md]`，`selected = src`（index 0）→ Down → `main.rs`（index 1）→ Down → `lib`（index 2）→ Down → `README.md`（index 3）→ Down → 循环回 `src`（index 0）；Up → 循环到 `README.md`（index 3）
- [ ] 3.6 `#[test]` 单测 `navigate Left/Right`：`src` 已展开 → Left → 折叠 `src`；`src` 已折叠 → Left → 跳到父目录（root）；`src` 已折叠 → Right → 展开 `src`；`src` 已展开 → Right → 跳到第一个子条目（`main.rs`）

## 4. App: 渲染

- [ ] 4.1 实现 `FileTreePanel::render_entry(&self, entry: &VisibleEntry, cx: &mut Context<Self>) -> impl IntoElement`：`div().id(entry.path).flex().flex_row().items_center().gap(space_sm).pl(px(depth as f32 * 16.0)).cursor_pointer()`；选中 → `bg(SURFACE_RAISED).border_l_2().border_color(TEXT_BRIGHT)`；hover → `hover(|s| s.bg(BTN_BG_HOVER))`；图标 `📁`/`📂`/`📄`；文件名 `SharedString::from(entry.name)`；`on_click` → `select(path)` + 目录 `toggle_expanded(path)`；`on_mouse_down(Right)` → `context_menu = Some(ContextMenuState { target, position })`
- [ ] 4.2 实现 `FileTreePanel::render_header(&self, cx: &mut Context<Self>) -> impl IntoElement`：`div().flex().flex_row().justify_between().pb(space_md()).mb(space_sm()).border_b_1().border_color(BORDER_SUBTLE)`；左 "FILES"（`TEXT_DIM`）；右 刷新按钮（`refresh.svg` 图标，`TEXT_FAINT` + group-hover 染色，`on_click` → `refresh(cx)`）
- [ ] 4.3 实现 `FileTreePanel::render_empty_state(&self) -> impl IntoElement`：`div().flex_1().flex().items_center().justify_center().text_color(TEXT_FAINT).child("select a worktree to browse files")`（无 active worktree 时）
- [ ] 4.4 实现 `FileTreePanel::render_context_menu(&self, cx: &mut Context<Self>) -> impl IntoElement`：`modal` 或 `deferred` + `anchored` 定位在 `context_menu.position`；菜单项 "Reveal in File Manager"（`!host.is_remote()` 时显示）、"Copy Path"、"Copy Relative Path"；点击 → 执行动作 + `context_menu = None`
- [ ] 4.5 在 `crates/app/src/assets.rs` 登记 `file-tree.svg`（或 `folder-tree.svg`）和 `refresh.svg` 图标；新增 `crates/app/assets/icons/file-tree.svg` 和 `crates/app/assets/icons/refresh.svg`（Lucide 风格，`stroke="currentColor"`）

## 5. App: WorkspaceView 集成

- [ ] 5.1 `WorkspaceView` 新增字段：`file_tree_panel: Option<Entity<FileTreePanel>>`、`file_tree_panel_open: bool`、`file_tree_width: f32`（默认 240.0）、`dragging_file_tree_splitter: bool`
- [ ] 5.2 `WorkspaceView::construct` 初始化新字段：`file_tree_panel: None`、`file_tree_panel_open: false`、`file_tree_width: 240.0`、`dragging_file_tree_splitter: false`
- [ ] 5.3 `WorkspaceView::render` 在 sidebar + splitter 和 main 之间条件渲染：`if self.file_tree_panel_open { root.child(self.file_tree_splitter(cx)).child(self.file_tree_panel_view(cx)) }`；`file_tree_panel_view` 渲染 `self.file_tree_panel` Entity（如果 `None` 则创建或空态）
- [ ] 5.4 实现 `WorkspaceView::file_tree_splitter`：`div().id("file-tree-splitter").flex_none().w(px(4.0)).h_full().bg(BORDER).cursor_col_resize().hover(|s| s.bg(TEXT_FAINT)).on_mouse_down(Left, ...)`（与 sidebar splitter 风格一致）
- [ ] 5.5 `WorkspaceView::on_mouse_move` 新增 `dragging_file_tree_splitter` 分支：`this.file_tree_width = ev.position.x.clamp(FILE_TREE_MIN_W, FILE_TREE_MAX_W)`；`on_mouse_up` 新增 `dragging_file_tree_splitter = false`
- [ ] 5.6 `WorkspaceView::toggle_file_tree_panel(&mut self, cx: &mut Context<Self>)`：`file_tree_panel_open = !file_tree_panel_open`；首次打开时 `if file_tree_panel.is_none() { file_tree_panel = Some(cx.new(|cx| FileTreePanel::new(host, active, cx))) }`；`cx.notify()`
- [ ] 5.7 `WorkspaceView::open_worktree` / `new_worktree` / `set_repo`：active worktree 切换时通知面板 `if let Some(panel) = &self.file_tree_panel { panel.update(cx, |p, cx| p.set_root(active_path.clone(), cx)); }`
- [ ] 5.8 定义 `FILE_TREE_MIN_W: f32 = 180.0`、`FILE_TREE_MAX_W: f32 = 480.0`、`FILE_TREE_DEFAULT_W: f32 = 240.0`

## 6. App: 侧边栏切换按钮

- [ ] 6.1 `sidebar.rs` WORKTREES 标题行右侧新增文件树切换按钮（`file-tree.svg` 图标）：`div().id("toggle-file-tree").group("toggle-file-tree-btn").flex_none().px(space_xs).cursor_pointer().child(svg().size(px(14.0)).path("icons/file-tree.svg").text_color(if file_tree_panel_open { TEXT_BRIGHT } else { TEXT_FAINT }).group_hover("toggle-file-tree-btn", |s| s.text_color(TEXT)))`；`on_click` → `toggle_file_tree_panel(cx)` + `stop_propagation`
- [ ] 6.2 按钮放在齿轮按钮左侧（WORKTREES 标题行的 `justify_between` 右侧容器内，先文件树按钮再齿轮按钮）
- [ ] 6.3 `#[cfg(feature = "test-support")]` accessor `file_tree_panel_open() -> bool`、`set_file_tree_panel_open_for_test(open: bool)`

## 7. App: 右键菜单动作

- [ ] 7.1 实现 `FileTreePanel::reveal_in_file_manager(&self, path: &Path)`：`!host.is_remote()` 时 `std::process::Command::new("explorer"/"open"/"xdg-open").arg(path).spawn()`；`is_remote()` 时 no-op（菜单项不显示）
- [ ] 7.2 实现 `FileTreePanel::copy_path(&self, path: &Path, cx: &mut Context<Self>)`：`cx.write_to_clipboard(gpui::ClipboardItem::new_string(path.to_string_lossy()))`
- [ ] 7.3 实现 `FileTreePanel::copy_relative_path(&self, path: &Path, cx: &mut Context<Self>)`：`path.strip_prefix(&self.root)` → `cx.write_to_clipboard(relative)`
- [ ] 7.4 `#[cfg(feature = "test-support")]` accessor `context_menu_target() -> Option<PathBuf>`、`context_menu_is_open() -> bool`、`close_context_menu_for_test()`

## 8. App: 测试 — 单元测试（`#[test]`，无 GPUI / 无 Host）

- [ ] 8.1 `crates/app/src/file_tree_panel.rs` `#[cfg(test)] mod tests`：`recompute_visible` 测试（用 `MockHost` 预装 `/root` → `[src (dir), README.md]`、`/root/src` → `[main.rs, lib (dir)]`，展开 `/root` + `/root/src`，验证 `visible` 的 `name` / `depth` / `is_dir` / `is_expanded`）
- [ ] 8.2 `toggle_expanded` 测试：展开未缓存目录 → `loading = Some(path)` + `MockHost.list_dir` 被调用；展开已缓存目录 → 不调 `list_dir`；折叠 → `expanded.remove` + `visible` 缩减
- [ ] 8.3 `navigate` Up/Down 循环测试（`visible` 4 条目，Down 0→1→2→3→0，Up 0→3→2→1→0）
- [ ] 8.4 `navigate` Left/Right 测试（目录已展开 → Left 折叠；目录已折叠 → Left 跳父目录；目录已折叠 → Right 展开；目录已展开 → Right 跳第一个子条目）
- [ ] 8.5 `recompute_visible` 后 `selected` 修正测试：`selected = Some("/root/src/main.rs")`，折叠 `/root/src` → `recompute_visible` 后 `selected = None`（路径不在 `visible` 中）

## 9. App: 测试 — UI 状态测试（`#[gpui::test]`，accessor 验证状态机）

- [ ] 9.1 `crates/app/tests/file_tree_panel_test.rs` 新增：用 `MockHost` 预装 `/root` → `[src (dir), README.md (file)]`，构造 `FileTreePanel::new(host, "/root", cx)`，`wait_for` 验证 `file_tree_visible_count() == 2`、`file_tree_visible_entries()` == `[("src", 0, true, false), ("README.md", 0, false, false)]`
- [ ] 9.2 展开测试：`file_tree_toggle_expanded_for_test("/root/src")`（MockHost 预装 `/root/src` → `[main.rs, lib]`），`wait_for` 验证 `file_tree_expanded()` 含 `/root/src`、`file_tree_visible_count() == 4`（src + main.rs + lib + README.md）
- [ ] 9.3 折叠测试：展开 `/root/src` 后 `file_tree_toggle_expanded_for_test("/root/src")`，验证 `file_tree_expanded()` 不含 `/root/src`、`file_tree_visible_count() == 2`
- [ ] 9.4 选中测试：`file_tree_select_for_test("/root/src")` → `file_tree_selected() == Some("/root/src")`；Down → `file_tree_selected() == Some("/root/README.md")`（`/root` 的第二个条目）
- [ ] 9.5 切换按钮测试：`WorkspaceView` 构造后 `file_tree_panel_open() == false`；`set_file_tree_panel_open_for_test(true)` → `render` 包含 `FileTreePanel`；`set_file_tree_panel_open_for_test(false)` → `render` 不包含
- [ ] 9.6 active worktree 切换刷新测试：`FileTreePanel` 显示 worktree A 的文件树；`set_root(worktree_B_path)` → `file_tree_visible_count() == 0`（清空）→ `wait_for` 验证 `file_tree_root() == worktree_B_path` + `file_tree_visible_count()` 反映 worktree B 的条目
- [ ] 9.7 空态测试：`WorkspaceView` 无 active worktree（`active = None`）时打开面板 → `render` 显示 "select a worktree to browse files"
- [ ] 9.8 右键菜单测试：`file_tree_panel` 右键一个条目 → `context_menu_is_open() == true`、`context_menu_target() == Some(path)`；`close_context_menu_for_test()` → `context_menu_is_open() == false`
- [ ] 9.9 WSL 模式测试：用 `MockHost`（`is_remote() == true`）构造 `FileTreePanel`，右键菜单的 "Reveal in File Manager" 不显示（通过 render snapshot 或 accessor `context_menu_has_reveal() -> bool` 验证）

## 10. App: 测试 — 集成测试（`#[gpui::test]` + `wait_for`，端到端流程）

- [ ] 10.1 `crates/app/tests/file_tree_panel_test.rs` 新增：用 `LocalHost` + `tempfile::tempdir()` 创建临时目录 + 子文件，构造 `FileTreePanel`，`wait_for` 验证根目录条目加载；展开子目录 → `wait_for` 验证子条目加载；折叠 → 验证子条目消失
- [ ] 10.2 WSL 模式集成测试（`#[ignore]`，需真实 WSL）：用真实 `WslHost`，在 WSL 内 `mkdir -p /tmp/test-tree/src` + `touch /tmp/test-tree/README.md`，构造 `FileTreePanel`，`wait_for` 验证 `file_tree_visible_count() == 2`（src + README.md）；展开 `/tmp/test-tree/src` → `wait_for` 验证子条目
- [ ] 10.3 完整流程集成测试：`WorkspaceView` 打开 worktree → 切换文件树面板 → 验证面板显示 worktree 根目录条目 → 切换到另一个 worktree → 验证面板刷新到新 worktree 的条目
- [ ] 10.4 刷新按钮集成测试：`FileTreePanel` 加载目录 → `dir_cache` 非空 → 点刷新按钮 → `dir_cache` 清空 → `wait_for` 验证 `list_dir` 重新调用 + `dir_cache` 重新填充

## 11. 质量门

- [ ] 11.1 `cargo fmt`（无 diff）
- [ ] 11.2 `cargo clippy --all-targets`（无 warning）
- [ ] 11.3 `cargo test -p lucy-app`（UI 状态测试 + 集成测试全绿；WSL 集成测试 `#[ignore]` 不跑）
- [ ] 11.4 `cargo run -p lucy-app` 在 Windows 上启动：打开 worktree → 点文件树切换按钮 → 面板显示文件树 → 展开/折叠目录 → 键盘导航 → 右键 Copy Path
- [ ] 11.5 `cargo run -p lucy-app` 在 WSL 模式下启动：打开 WSL worktree → 面板用 `wsl.exe ls` 列目录 → 展开/折叠正常 → 右键菜单无 "Reveal in File Manager"
