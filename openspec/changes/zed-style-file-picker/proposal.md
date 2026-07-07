## Why

当前的 WSL 文件选择器（`wsl_browser_dialog`）是一个朴素的目录导航列表：点文件夹进入、点 `..` 返回、点「选择此目录」确认。每次导航都要等 `wsl.exe ls` 返回（1-2 秒），无法输入路径、无法模糊搜索、无法用键盘快速定位。本地仓库则走 `cx.prompt_for_paths`（系统原生选择器），两套入口割裂。用户反馈「不好用」。

Zed 编辑器的 `OpenPathPrompt`（`crates/open_path_prompt/src/open_path_prompt.rs`）给出了更好的范式：**一个文本输入框 + 下方实时补全列表**。用户输入路径时，系统自动切分出「目录部分」（用来 `list_dir`）和「后缀」（用来模糊过滤条目）；Tab 自动补全选中条目（目录补 `/`）；Enter 确认；上下键导航；Esc 关闭。目录列表用 **cancel-flag 模式**异步加载（新 keystroke 取消上一个 in-flight 任务），不阻塞 UI。这套范式对 LocalHost 和 WslHost 通用（都走 `Host::list_dir`），统一了本地 / WSL 两个入口。

## What Changes

- **新增 `PathPicker` 组件**（`crates/app/src/ui/path_picker.rs`）：一个 GPUI `Entity`，持有 `InputState`（文本输入）+ 补全列表状态。复用 `ui::dialog::modal` 骨罩（遮罩 + 居中卡片），卡片内：顶部文本输入框、下方可滚动补全列表。
- **路径切分**：`get_dir_and_suffix(query)` 把输入切分为目录部分（`list_dir` 的参数）和后缀（模糊过滤词）。Posix 路径用 `/` 分隔（WSL）；Windows 路径用 `\` 或 `/`（本地）。目录部分变化时才重新 `list_dir`。
- **异步目录列表 + cancel-flag**：`update_matches(query)` 在后台 executor 跑 `host.list_dir(dir)`，用 `Arc<AtomicBool>` 取消上一个任务（新 keystroke 到来时翻转旧 flag，旧任务完成后检查 flag 发现已取消则丢弃结果）。UI 先同步显示 loading，任务完成后回主线程更新条目。
- **模糊过滤**：后缀非空时，对 `list_dir` 返回的 `DirEntry` 列表做前缀匹配（`name.starts_with(suffix)` 或 `name.to_lowercase().contains(suffix)`）；后缀为空时显示全部条目。目录排前、文件排后，同类按名称排序（复用 `host.rs` 的 `sort_entries`）。
- **键盘交互**：Up/Down 移动 `selected_index`（循环到顶/底）；Tab 调用 `confirm_completion`（选中条目是目录 → 补全 `dir + name + /` 触发重新 `list_dir`；是文件 → 补全 `dir + name`）；Enter 调用 `confirm`（验证 `git::main_worktree_root` → `set_repo`，失败显示错误）；Esc 关闭。
- **Host 感知**：`PathPicker` 持有 `Arc<dyn Host>`，用它调 `list_dir`。路径风格由 Host 决定（`WslHost` → Posix `/`，`LocalHost` → OS 分隔符）。初始查询：WSL 从 `/` 开始，本地从 home 目录或当前 `self.repo` 开始。
- **统一入口**：`open_repo_picker` 不再弹「Local / WSL」二选一弹窗，而是直接打开 `PathPicker`。`PathPicker` 用当前 `self.host`（启动时已检测 LocalHost / WslHost）。移除 `open_repo_choice_dialog`、`wsl_browser_dialog`、`open_wsl_browser`、`load_wsl_dir`、`navigate_wsl_dir`、`commit_wsl_browser` 及相关状态（`open_repo_choice_open`、`wsl_browser`）。
- **本地模式仍可用原生选择器**：`PathPicker` 卡片底部保留「Browse…」按钮，点击调 `cx.prompt_for_paths`（原生目录选择器），作为本地模式的便捷入口（WSL 模式下该按钮隐藏）。

## Capabilities

### New Capabilities

- `path-picker`: Zed 风格的路径输入选择器——文本输入 + 实时目录补全 + 键盘导航（Up/Down/Tab/Enter/Esc）+ cancel-flag 异步目录列表 + 模糊过滤。替代当前的 WSL 目录导航列表和「Local / WSL」二选一弹窗，统一本地和 WSL 两个入口。

### Modified Capabilities

（无——`openspec/specs/` 当前为空，无既有 capability 的需求被改动。但本变更实质上替换了 `wsl-remote-host` 变更引入的 `wsl_browser_dialog` 目录导航列表和 `open_repo_choice_dialog` 二选一弹窗，改为单一 `PathPicker`。）

## Impact

- **`crates/app/src/ui/path_picker.rs`**（新增）：`PathPicker` Entity + `PathPickerState`（query / dir / suffix / entries / filtered / selected_index / loading / error / cancel_flag）。`render`：`modal` 骨罩 + `InputState` 文本框 + `uniform_list`（或手动 div 列表）补全列表。键盘：`on_key_down` 处理 Up/Down/Tab/Enter/Esc。
- **`crates/app/src/ui/mod.rs`**：`pub mod path_picker;` + `pub use path_picker::PathPicker;`。
- **`crates/app/src/workspace/mod.rs`**：`WorkspaceView` 新增 `path_picker: Option<Entity<PathPicker>>` 字段（替代 `open_repo_choice_open: bool` + `wsl_browser: Option<WslBrowser>`）；`open_repo_picker` 改为创建 `PathPicker` Entity（用 `self.host.clone()`）；移除 `open_repo_choice_dialog`、`wsl_browser_dialog`、`open_wsl_browser`、`load_wsl_dir`、`navigate_wsl_dir`、`commit_wsl_browser`；`render` 的模态叠加区改为 `if let Some(picker) = &self.path_picker { root.child(picker.clone()) }`；Esc 键处理改为转发给 `PathPicker`（或由 `PathPicker` 自己处理）。
- **`crates/app/src/workspace/dialogs.rs`**：移除 `open_repo_choice_dialog` 和 `wsl_browser_dialog`（逻辑移入 `PathPicker`）。
- **`crates/core/src/host.rs`**：`Host` trait 无改动（`list_dir` 已存在）。可选新增 `host.path_separator() -> char`（WSL 返回 `/`，本地返回 `std::path::MAIN_SEPARATOR`），供 `PathPicker` 切分路径。或由 `is_remote()` 推断（`true` → `/`，`false` → OS 分隔符）。
- **测试**：`PathPicker` 的纯逻辑函数（`get_dir_and_suffix`、过滤、补全）用 `#[test]` 单测；UI 状态（打开/关闭/选中/确认）用 `#[gpui::test]` + accessor 验证；`MockHost` 预装目录条目验证 `list_dir` 调用和补全列表。
