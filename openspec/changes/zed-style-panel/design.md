## Context

LucyMind 当前的布局（`workspace/mod.rs::render`）是：`sidebar`（worktree 列表）| `splitter` | `main`（tab 栏 + 终端区 + 状态栏）。没有文件树面板——用户想看 worktree 的文件结构只能切外部文件管理器。

Zed 的 Project Panel（`crates/project_panel/src/project_panel.rs`，~7700 行）是成熟的文件树面板实现：

- **Dock Panel**：`impl Panel`，可 dock 左/右，`toggle_panel_focus` 切换显示/隐藏，`DockPosition` / `default_size` / `icon` / `toggle_action`。
- **可见条目扁平列表**：不维护树结构，而是从 worktree snapshot 每次重新计算 `Vec<VisibleEntriesForWorktree>`（只含已展开目录的子条目）。`expanded_dir_ids: HashMap<WorktreeId, Vec<EntryId>>`（sorted vec，binary search）记录展开状态。
- **`update_visible_entries`**：后台 `background_spawn` 遍历树，跳过折叠目录的子树（`advance_to_sibling` vs `advance`），应用 `hide_gitignore` / `hide_hidden` / `auto_fold_dirs` 过滤，计算 `depth`（缩进层级），排序。
- **选中**：`selection: Option<SelectedEntry>` + `marked_entries: Vec<SelectedEntry>`（多选）。`select_next` / `select_previous` 在扁平列表上做索引算术。
- **渲染**：`uniform_list("entries", item_count, cx.processor(|this, range, ...| render_entry))`。每行 `ListItem::new(id).indent_level(depth).indent_step_size(px(indent_size))` + 图标 + 文件名 + git status 指示。
- **右键菜单**：`ContextMenu::build` + `deferred(anchored().position(pos))`。
- **自动滚动**：`scroll_handle.scroll_to_item_with_offset(index, Center, sticky_count)`。
- **设置**：`ProjectPanelSettings`（`auto_fold_dirs` / `file_icons` / `folder_icons` / `git_status` / `indent_size` / `indent_guides` / `sticky_scroll` / `hide_hidden` / `hide_gitignore` / `sort_mode` / `drag_and_drop`）。

**关键约束**：
- LucyMind 用 `gpui 0.2.2`（crates.io 发布版），没有 Zed 的 `Panel` trait / `DockPosition` / `workspace.toggle_panel_focus` / `ContextMenu` / `ListItem` / `FileIcons` / `GitEntry` / `Worktree` snapshot。需要在 `crates/app/` 自建简化版。
- LucyMind 没有 worktree snapshot（Zed 的 `Project` entity 提供 `worktree.entries(true, 0)` 快照）。LucyMind 用 `Host::list_dir` 按需懒加载目录，每次 `list_dir` 是一次 `wsl.exe ls` 或 `std::fs::read_dir` 调用。
- LucyMind 的 worktree 路径是 `PathBuf`（不是 Zed 的 `ProjectEntryId` / `WorktreeId`）。展开状态用 `HashSet<PathBuf>`（已展开目录的规范化路径）。
- LucyMind 已有 `Host::list_dir` 返回 `Vec<DirEntry>`（`name: String, is_dir: bool`，隐藏文件已过滤，目录排前、同类按名称排序）。`FileTreePanel` 直接复用。
- LucyMind 的 sidebar splitter 模式（`sidebar_width` + `dragging_splitter` + `on_mouse_move`）可复用为面板宽度拖拽。
- GPUI 0.2.2 有 `uniform_list`（虚拟化列表）和 `ListState` / `UniformListScrollHandle`（滚动控制），可用于文件树渲染。

## Goals / Non-Goals

**Goals:**
- 新增 `FileTreePanel` 组件（`crates/app/src/file_tree_panel.rs`），渲染 active worktree 的文件树。
- 懒加载目录：只列已展开目录的子条目（`Host::list_dir`），缓存结果（折叠再展开不重新 `list_dir`，除非刷新）。`HashSet<PathBuf>` 记录展开状态。
- 展开折叠：点击目录行切换展开/折叠。展开时后台 `list_dir`，折叠时从可见列表移除子条目。
- 选中与键盘导航：`Option<PathBuf>` 选中条目。Up/Down 移动选中，Left 折叠选中目录（或跳到父目录），Right 展开选中目录（或跳到第一个子条目）。点击选中，双击目录切换展开/折叠。
- Dock 面板切换：侧边栏新增文件树图标按钮，点击切换 `file_tree_panel_open`。面板嵌在 sidebar 右侧、main 左侧，宽度可拖（复用 splitter 模式）。
- 右键上下文菜单：Reveal in File Manager（本地模式）、Copy Path、Copy Relative Path。文件操作（New File / New Folder / Rename / Delete）Phase 2，本期只做只读浏览。
- active worktree 切换时刷新面板（清空树 + 重新列根目录）。
- `Host` 感知：LocalHost 用 `std::fs::read_dir`，WslHost 用 `wsl.exe ls`，路径风格由 Host 决定。

**Non-Goals:**
- 不做 git status 指示（Zed 的 `GitSummary` / `git_status_indicator`）——LucyMind 不嵌入 git status 检查到文件树（`git status` 慢，且 worktree 的 git status 在终端里跑更合适）。
- 不做 diagnostic 指示（Zed 的 LSP diagnostic badges）——LucyMind 不嵌入 LSP。
- 不做 drag-and-drop（Zed 的 `ExternalPaths` / `DraggedSelection`）——Phase 1 只读浏览。
- 不做 inline rename / new file / new folder 编辑器（Zed 的 `filename_editor: Entity<Editor>` inline 编辑）——Phase 2 文件操作。
- 不做 auto-fold dirs（Zed 的单子目录自动折叠）——Phase 1 手动展开/折叠。
- 不做 sticky scroll / indent guides（Zed 的 `sticky_items_count` / `IndentGuidesSettings`）——Phase 1 简单缩进。
- 不做多 worktree 支持（Zed 的 `Vec<VisibleEntriesForWorktree>`）——Phase 1 只显示 active worktree 的文件树。
- 不做 `Panel` trait / `DockPosition` / `toggle_panel_focus`（Zed 的 dock 系统）——Phase 1 用简单的 `file_tree_panel_open: bool` + 条件渲染。
- 不做 `ProjectPanelSettings`（Zed 的设置 struct）——Phase 1 硬编码缩进、图标、排序。
- 不做文件图标系统（Zed 的 `FileIcons`）——Phase 1 用 `📁` / `📄` emoji（与 PathPicker 一致）。
- 不做 `ContextMenu` 组件（Zed 的 `gpui-component` context menu）——Phase 1 右键菜单用简化版（`modal` 或 `deferred` + `anchored`，或直接用按钮行）。

## Decisions

### D1: `FileTreePanel` 是独立 Entity，持有 `Arc<dyn Host>` + active worktree 路径

```rust
pub struct FileTreePanel {
    /// 当前显示的 worktree 根路径（active worktree 的 canon 路径）。
    root: PathBuf,
    /// Host 抽象（LocalHost / WslHost），用于 list_dir。
    host: Arc<dyn Host>,
    /// 已展开的目录路径集合（规范化路径）。
    expanded: HashSet<PathBuf>,
    /// 目录缓存：path → entries（list_dir 结果）。折叠再展开不重新 list_dir（除非刷新）。
    dir_cache: HashMap<PathBuf, Vec<DirEntry>>,
    /// 可见条目扁平列表（从树计算，只含已展开目录的子条目）。
    visible: Vec<VisibleEntry>,
    /// 选中条目路径（None = 无选中）。
    selected: Option<PathBuf>,
    /// 面板宽度（可拖 splitter 调整）。
    width: f32,
    /// 滚动状态（uniform_list 的 scroll handle）。
    scroll: UniformListScrollHandle,
    /// 后台 list_dir 进行中（loading 指示）。
    loading: Option<PathBuf>,
    /// 右键菜单状态（None = 无菜单；Some = 菜单打开，记录目标路径 + 位置）。
    context_menu: Option<ContextMenuState>,
    focus: FocusHandle,
}

struct VisibleEntry {
    /// 完整路径（root + 相对路径）。
    path: PathBuf,
    /// 条目名（文件名 / 目录名）。
    name: String,
    /// 是否目录。
    is_dir: bool,
    /// 缩进层级（root 的直接子条目 depth=0，孙子 depth=1，…）。
    depth: usize,
    /// 是否已展开（仅目录有意义；文件始终 false）。
    is_expanded: bool,
    /// 子条目是否已加载（list_dir 完成）。未加载的目录展开时显示 loading。
    is_loaded: bool,
}
```

**理由**：
- 独立 Entity 把文件树状态隔离到 `file_tree_panel.rs`，不污染 `WorkspaceView`。
- `dir_cache` 避免折叠再展开时重复 `list_dir`（WSL `ls` 有延迟）。刷新按钮清空 `dir_cache` 强制重新加载。
- `visible` 是从 `expanded` + `dir_cache` 计算的扁平列表（类似 Zed 的 `update_visible_entries`，但同步计算而非后台，因为 LucyMind 的目录结构通常不深）。
- `loading: Option<PathBuf>` 记录正在 `list_dir` 的目录（展开目录时先显示 loading，`list_dir` 完成后更新 `dir_cache` + `recompute_visible`）。

**备选（否决）**：每次 render 时递归 `list_dir`——太慢（WSL `ls` 每层 100ms+，深目录树要几秒）。`dir_cache` 是必须的。

**备选（否决）**：后台 `background_spawn` 计算 `visible`（像 Zed 的 `update_visible_entries`）——LucyMind 的目录树通常不深（< 10 层），同步计算 < 1ms，不需要后台。WSL 的 `list_dir` 是慢的部分（已在 `expand` 时异步）。

### D2: 展开折叠 + 懒加载

`toggle_expanded(path)`:
1. `path` 在 `expanded` 中 → 移除（折叠）：`expanded.remove(&path)` + `recompute_visible()`。
2. `path` 不在 `expanded` 中 → 展开：`expanded.insert(path.clone())`。如果 `dir_cache` 无 `path` → `loading = Some(path)` + 后台 `host.list_dir(path)` → 完成后 `dir_cache.insert(path, entries)` + `loading = None` + `recompute_visible()`。如果 `dir_cache` 有 `path` → 直接 `recompute_visible()`。

`recompute_visible()`:
1. 从 `root` 开始 DFS：`dir_cache[root]` 的条目 → `VisibleEntry { depth: 0, ... }`。
2. 对每个 `is_dir && is_expanded` 的条目：递归 `dir_cache[entry.path]`（如有）→ `VisibleEntry { depth: depth+1, ... }`。
3. 未加载的展开目录（`is_dir && is_expanded && !dir_cache.contains(path)`）→ 显示一个 `VisibleEntry { name: "加载中…", is_dir: false, depth: depth+1 }` 占位条目（或在该目录行显示 loading 图标）。
4. 排序：`Host::list_dir` 已排序（目录在前、同类按名称），`recompute_visible` 保持 `dir_cache` 的顺序（不再重排）。

**备选（否决）**：`expanded` 用 `HashSet<PathBuf>` 而非 `HashMap<WorktreeId, Vec<PathBuf>>`（Zed 的 sorted vec + binary search）——LucyMind 只有一个 worktree 的文件树，`HashSet` 查找/插入是 O(1)，不需要 sorted vec。

### D3: 可见条目扁平列表 + 渲染

`render`:
1. `uniform_list("file-tree", visible.len(), cx.processor(|this, range, window, cx| { render_entry(visible[i]) }))` 或手动 `div` 列表（`overflow_y_scroll`）。Phase 1 用手动 `div` 列表（与 sidebar / PathPicker 风格一致，条目数通常 < 1000）。Phase 2 可改 `uniform_list` 虚拟化。
2. 每行 `render_entry(entry)`:
   - `div().id(entry.path).flex().flex_row().items_center().gap(space_sm).pl(px(depth as f32 * 16.0))`（缩进 `depth * 16px`）。
   - 图标：`is_dir` → `📁`（展开时 `📂`）或 SVG `folder.svg` / `folder-open.svg`；`!is_dir` → `📄` 或 SVG `file.svg`。
   - 文件名：`SharedString::from(entry.name)`。
   - 选中：`is_selected` → `bg(SURFACE_RAISED)` + 左边框标记（`border_l_2 border_color(TEXT_BRIGHT)`，与 sidebar worktree 行风格一致）。
   - hover：`hover(|s| s.bg(BTN_BG_HOVER))`。
   - `cursor_pointer()`。
   - 点击：`on_click` → `select(path)`（选中）。目录双击：`on_click` 检测双击 → `toggle_expanded(path)`。
3. 面板容器：`div().flex_none().w(px(width)).h_full().bg(SURFACE).border_r_1().border_color(BORDER).font_family(FONT_UI).child(scrollable_list)`。右侧描边（与 sidebar 一致）。

**点击展开 vs 双击展开**：Phase 1 用单击目录行切换展开/折叠（与 sidebar worktree 行的单击切换一致），更简单。Zed 是单击选中 + 双击展开（或单击目录的展开箭头）。LucyMind Phase 1 单击目录行 = `select(path)` + `toggle_expanded(path)`（选中并展开/折叠）。文件行单击 = `select(path)`（只选中）。

### D4: 键盘导航

`on_key_down`:
| 键 | 行为 |
|---|---|
| `Up` | `selected_index` 减 1（循环到末尾），`scroll_to(selected_index)` |
| `Down` | `selected_index` 加 1（循环到第一条），`scroll_to(selected_index)` |
| `Left` | 选中是目录且已展开 → 折叠；选中是目录且已折叠 → 跳到父目录；选中是文件 → 跳到父目录 |
| `Right` | 选中是目录且已折叠 → 展开；选中是目录且已展开 → 跳到第一个子条目；选中是文件 → no-op |
| `Enter` | 选中是目录 → `toggle_expanded`；选中是文件 → no-op（Phase 1 不打开文件） |
| `Escape` | 如果右键菜单打开 → 关闭菜单；否则 no-op（不关闭面板） |

`selected_index` 是 `visible` 列表中的索引。`select(index)` 更新 `selected = Some(visible[index].path)` + `cx.notify()`。

**备选（否决）**：用 `selection: Option<PathBuf>` 而非 `selected_index: usize`——`PathBuf` 在 `recompute_visible` 后可能失效（路径仍在但索引变了）。用 `selected_index` 更直接（导航在扁平列表上做索引算术），但 `recompute_visible` 后需修正 `selected_index`（找到 `selected` 路径在新 `visible` 中的索引）。Phase 1 用 `selected: Option<PathBuf>` + `recompute_visible` 后重新查找索引（`visible.iter().position(|e| e.path == selected)`）。

### D5: Dock 面板切换 + 宽度拖拽

`WorkspaceView` 新增：
- `file_tree_panel: Option<Entity<FileTreePanel>>`（None = 面板未创建；Some = 面板已创建，可能隐藏）。
- `file_tree_panel_open: bool`（面板是否显示）。
- `file_tree_width: f32`（面板宽度，默认 240px，可拖调整，范围 180-480px，与 sidebar 一致）。
- `dragging_file_tree_splitter: bool`（拖文件树分隔条）。

`render`:
1. `sidebar` | `splitter` | `file_tree_panel`（条件渲染）| `splitter2`（条件渲染）| `main`。
2. `if self.file_tree_panel_open { root.child(splitter2).child(file_tree_panel) }`。
3. `splitter2` 与 `splitter` 风格一致（`div().id("file-tree-splitter").w(px(4.0)).h_full().bg(BORDER).cursor_col_resize().hover(|s| s.bg(TEXT_FAINT)).on_mouse_down(Left, |this, _, _, cx| { this.dragging_file_tree_splitter = true; cx.notify(); })`）。
4. `on_mouse_move` 已处理 `dragging_splitter`，新增 `dragging_file_tree_splitter` 分支：`this.file_tree_width = ev.position.x.clamp(FILE_TREE_MIN_W, FILE_TREE_MAX_W)`。

侧边栏新增文件树切换按钮（`sidebar.rs`）：
- 在 WORKTREES 标题行右侧（齿轮按钮旁）加一个文件树图标按钮（`file-tree.svg`），点击切换 `file_tree_panel_open`。
- `file_tree_panel_open` 为 true 时按钮高亮（`TEXT_BRIGHT`）；false 时 `TEXT_FAINT` + group-hover 染色。

active worktree 切换时（`open_worktree` / `new_worktree` / `set_repo`）：
- `if let Some(panel) = &self.file_tree_panel { panel.update(cx, |p, cx| p.set_root(active_path.clone(), cx)); }`。
- `set_root` 清空 `expanded` / `dir_cache` / `visible` / `selected`，设 `root = active_path`，自动展开 `root`（`expanded.insert(root)` + `list_dir(root)`）。

### D6: 右键上下文菜单（简化版）

`context_menu: Option<ContextMenuState>`:
```rust
struct ContextMenuState {
    target: PathBuf,      // 右键的文件/目录路径
    position: Point<Pixels>,  // 鼠标位置（菜单定位）
}
```

右键文件/目录行 → `on_mouse_down(Right)` → `context_menu = Some(ContextMenuState { target, position })` → `cx.notify()`。

菜单渲染（简化版，不用 `ContextMenu` 组件）：
- `modal` 或 `deferred` + `anchored` 定位在 `position`。
- 菜单项：
  - "Reveal in File Manager"（`!host.is_remote()` 时显示）→ `host` 本地模式调 `std::process::Command::new("explorer"/"open"/"xdg-open").arg(target)`；WSL 模式隐藏。
  - "Copy Path" → `cx.write_to_clipboard(target.to_string_lossy())`。
  - "Copy Relative Path" → `target.strip_prefix(root)` → `cx.write_to_clipboard(relative)`。
- 菜单项点击后 `context_menu = None` + `cx.notify()`。
- 点击菜单外区域 → `context_menu = None`（根 `div` 的 `on_mouse_down(Left)` 清除）。
- Esc → `context_menu = None`。

**备选（否决）**：用 `gpui-component` 的 `ContextMenu`——LucyMind 的 `gpui-component 0.5.1` 可能有 `ContextMenu`，但 API 不确定。Phase 1 用简化版（`modal` 骨罩 + 按钮行），Phase 2 可改 `ContextMenu`。

### D7: 刷新按钮

面板标题区（顶部）：
- "FILES" 标题 + 刷新按钮（`refresh.svg` 图标，group-hover 染色）。
- 刷新按钮点击 → `dir_cache.clear()` + `recompute_visible()` + 重新 `list_dir` 所有已展开目录（或只 `list_dir(root)` + 递归展开的目录）。
- 简化：刷新 = 清空 `dir_cache` + 对 `expanded` 中的每个目录重新 `list_dir`（后台并行或顺序）。

### D8: Host 感知与路径处理

- `FileTreePanel` 持有 `Arc<dyn Host>`，`list_dir` 走 Host。
- `root` 是 active worktree 的规范化路径（`canon(host, &active_path)`），与 `WorkspaceView::active` 一致。
- `expanded` / `dir_cache` 的 key 用 Host 规范化路径（`host.canonicalize` 或 `canon` wrapper）。
- `is_remote()` 为 true 时隐藏 "Reveal in File Manager" 菜单项。
- WSL 模式下 `list_dir` 用 `wsl.exe ls`（已有 `WslHost::list_dir`），路径是 Linux 风格（`/home/...`）。

## Risks / Trade-offs

- **[WSL `list_dir` 延迟]** → 每次 `list_dir` 是一次 `wsl.exe ls` 调用（~100ms）。展开一个深目录树（10 层 × 10 子目录 = 100 次 `list_dir`）要 10 秒。`dir_cache` 缓存避免重复加载，但首次展开仍慢。未来可优化：`WslHost` 持有长连接 shell server（`wsl.exe` 进程常驻，批量 `ls`）。
- **[同步 `recompute_visible`]** → 目录树深时（1000+ 条目）同步计算可能卡 UI（< 1ms 通常，但极端情况可能 10ms+）。Phase 1 可接受（worktree 通常 < 500 文件）。未来可改后台 `background_spawn`。
- **[手动 div 列表 vs `uniform_list`]** → 手动 div 在 1000+ 条目时渲染慢（每条都渲染 div）。Phase 1 可接受（worktree 通常 < 500 文件）。Phase 2 改 `uniform_list` 虚拟化。
- **[单击目录 = 选中 + 展开/折叠]** → 与 Zed 的单击选中 + 双击展开不同。LucyMind Phase 1 用单击展开（更简单，与 sidebar worktree 行的单击切换一致）。未来可改为单击选中 + 展开箭头单独点击。
- **[右键菜单简化版]** → 不用 `ContextMenu` 组件，用 `modal` 骨罩 + 按钮行。定位用 `deferred` + `anchored`（如果 GPUI 0.2.2 有 `anchored`）或固定在面板中间。Phase 1 可接受（菜单项少，3 个）。Phase 2 可改 `ContextMenu`。
- **[文件操作 Phase 2]** → 本期不做 New File / New Folder / Rename / Delete。`Host` trait 需新增 `remove` / `rename` 方法（Phase 2）。本期只读浏览 + 路径复制。
- **[面板宽度持久化]** → Phase 1 `file_tree_width` 不持久化（重启恢复默认 240px）。未来可存 `~/.config/LucyMind/` 或 `Session` 注册表。
- **[active worktree 切换刷新]** → `set_root` 清空树 + 重新列根目录。如果用户展开了深目录树后切 worktree 再切回，展开状态丢失。未来可按 worktree 路径缓存展开状态（`HashMap<PathBuf, HashSet<PathBuf>>`）。
