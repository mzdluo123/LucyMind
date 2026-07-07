## Why

LucyMind 当前只有侧边栏（worktree 列表）和终端区，没有文件树面板。用户在 worktree 里跑 agent / shell 时，想浏览文件结构、查看目录布局、快速定位文件路径，只能切到外部文件管理器。Zed 编辑器的 Project Panel（`crates/project_panel/src/project_panel.rs`）给出了成熟的文件树面板范式：可展开/折叠的树形视图、键盘导航、选中高亮、右键上下文菜单、Dock 面板可切换显示/隐藏。把这套范式抄过来，让用户在 LucyMind 内直接浏览 active worktree 的文件结构，无需切外部工具。

## What Changes

- **新增 `FileTreePanel` 组件**（`crates/app/src/file_tree_panel.rs`）：一个 GPUI `Entity`，渲染 active worktree 的文件树。用 `Host::list_dir` 懒加载目录（只列已展开目录的子条目），用 `HashSet<PathBuf>` 记录展开状态。
- **Dock 面板架构**：`FileTreePanel` 作为可切换的面板，嵌在侧边栏右侧、终端区左侧（或作为侧边栏的第二个 tab）。通过侧边栏的一个按钮（文件树图标）切换显示/隐藏。面板宽度可拖（复用 splitter 模式）。
- **展开/折叠**：点击目录行切换展开/折叠。展开时调 `Host::list_dir` 列子条目（缓存结果，折叠再展开不重新 `list_dir` 除非刷新）。折叠时从可见列表移除子条目。`HashSet<PathBuf>` 记录已展开目录路径。
- **可见条目扁平列表**：从树结构计算扁平的 `Vec<VisibleEntry>`（只含已展开目录的子条目），每个 `VisibleEntry` 含 `path`、`name`、`is_dir`、`depth`（缩进层级）、`is_expanded`、`is_loaded`（子条目是否已 `list_dir`）。`uniform_list` 或手动 `div` 列表渲染。
- **选中与键盘导航**：`Option<PathBuf>` 记录选中条目。Up/Down 移动选中（在扁平列表中上下），Left 折叠选中目录（或跳到父目录），Right 展开选中目录（或跳到第一个子条目）。点击选中，双击目录切换展开/折叠。
- **右键上下文菜单**：右键文件/目录弹出菜单（Reveal in File Manager / Copy Path / Copy Relative Path）。文件操作（New File / New Folder / Rename / Delete）作为 Phase 2，本期只做只读浏览 + 路径复制。
- **刷新**：active worktree 切换时清空树 + 重新列根目录。面板首次打开时列根目录。用户可点刷新按钮强制重新 `list_dir`（丢弃缓存）。
- **Host 感知**：`FileTreePanel` 持有 `Arc<dyn Host>`，用 `Host::list_dir` 列目录。LocalHost 用 `std::fs::read_dir`，WslHost 用 `wsl.exe ls`。路径风格由 Host 决定。

## Capabilities

### New Capabilities

- `file-tree-panel`: Zed 风格的文件树 Dock 面板——可展开/折叠的树形视图、键盘导航、选中高亮、右键上下文菜单（Reveal / Copy Path）、可切换显示/隐藏、宽度可拖。用 `Host::list_dir` 懒加载目录，LocalHost 和 WslHost 通用。

### Modified Capabilities

（无——`openspec/specs/` 当前为空，无既有 capability 的需求被改动。但本变更新增了 `WorkspaceView` 的 `file_tree_panel` 字段和 `file_tree_panel_open` 状态，与 `sidebar` / `tabs` / `dialogs` 并列。）

## Impact

- **`crates/app/src/file_tree_panel.rs`**（新增）：`FileTreePanel` Entity + `VisibleEntry` struct + `FileTreeState`（expanded dirs cache + visible entries flat list）。`render`：`uniform_list` 或手动 div 列表，每行按 `depth` 缩进 + 图标（📁/📄）+ 文件名 + 选中/展开状态。`on_key_down` 处理 Up/Down/Left/Right。右键菜单用 `deferred` + `anchored`（或简化为 `modal`）。
- **`crates/app/src/workspace/mod.rs`**：`WorkspaceView` 新增 `file_tree_panel: Option<Entity<FileTreePanel>>` 字段 + `file_tree_panel_open: bool` 状态；`render` 在 sidebar 和 main 之间条件渲染 `FileTreePanel`（`if self.file_tree_panel_open { root.child(splitter2).child(file_tree_panel) }`）；active worktree 切换时通知 `FileTreePanel` 刷新（`panel.update(cx, |p, cx| p.set_root(active_path, cx))`）。
- **`crates/app/src/workspace/sidebar.rs`**：侧边栏新增文件树切换按钮（文件树图标，类似齿轮按钮的 group-hover 风格），点击切换 `file_tree_panel_open`。
- **`crates/core/src/host.rs`**：`Host` trait 无改动（`list_dir` 已存在）。可选新增 `Host::remove(&self, path: &Path) -> Result<(), HostError>` 和 `Host::rename(&self, from: &Path, to: &Path) -> Result<(), HostError>`（Phase 2 文件操作用，本期不实现）。
- **`crates/app/src/theme.rs`**：可选新增 `PANEL_BG` / `PANEL_BORDER` 语义色 token（或复用 `SURFACE` / `BORDER`）。
- **`crates/app/src/assets.rs`**：新增 `file-tree.svg`（或 `folder-tree.svg`）图标（Lucide 风格），登记路径。
- **测试**：`FileTreePanel` 的纯逻辑函数（`compute_visible_entries`、`toggle_expanded`、`navigate`）用 `#[test]` 单测；UI 状态（打开/关闭/选中/展开/折叠）用 `#[gpui::test]` + accessor 验证；`MockHost` 预装目录条目验证 `list_dir` 调用和树渲染。
