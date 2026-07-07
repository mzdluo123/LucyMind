## 1. Core: 纯逻辑函数（路径切分 + 过滤 + 补全）

- [ ] 1.1 新建 `crates/app/src/ui/path_picker.rs`，实现纯函数 `get_dir_and_suffix(query: &str, separator: char) -> (String, String)`：Posix（`/`）用 `rfind('/')` 切分，目录部分末尾补 `/`；Windows（`\`）用 `rfind('\\')` 或 `rfind('/')` 取最大，目录部分末尾补 `\`。无分隔符时返回 `("", query)`（相对路径，Phase 1 不处理但函数不 panic）
- [ ] 1.2 实现 `filter_entries(entries: &[DirEntry], suffix: &str) -> Vec<usize>`：后缀为空返回全部索引；非空用 `name.to_lowercase().contains(suffix.to_lowercase())` 过滤，保留 `Host::list_dir` 的排序（目录在前、同类按名称）
- [ ] 1.3 实现 `complete_path(dir: &str, name: &str, is_dir: bool, separator: char) -> String`：`format!("{dir}{name}{sep}")`（目录补 separator，文件不补）
- [ ] 1.4 `#[test]` 单测 `get_dir_and_suffix`：`/home/user/Doc` → `("/home/user/", "Doc")`；`/home/user/` → `("/home/user/", "")`；`/` → `("/", "")`；`Doc` → `("", "Doc")`；Windows `C:\Users\Doc` → `("C:\Users\", "Doc")`
- [ ] 1.5 `#[test]` 单测 `filter_entries`：后缀空 → 全部；后缀 `do` → 只含 `docs`；大小写不敏感（`Do` 匹配 `docs`）
- [ ] 1.6 `#[test]` 单测 `complete_path`：目录 `src` → `/home/user/src/`；文件 `README.md` → `/home/user/README.md`

## 2. App: PathPicker Entity 骨架

- [ ] 2.1 在 `crates/app/src/ui/path_picker.rs` 定义 `PathPicker` struct：`state: PathPickerState`、`host: Arc<dyn Host>`、`cancel_flag: Arc<AtomicBool>`、`on_confirm: Box<dyn Fn(PathBuf, &mut Window, &mut Context<Self>)>`、`input: Entity<InputState>`、`separator: char`。`PathPickerState` 含 `query: String`、`dir: String`、`entries: Vec<DirEntry>`、`filtered: Vec<usize>`、`selected_index: usize`、`loading: bool`、`error: Option<String>`
- [ ] 2.2 实现 `PathPicker::new(host: Arc<dyn Host>, initial_query: String, cx: &mut Context<Self>, on_confirm: Box<dyn Fn(PathBuf, &mut Window, &mut Context<Self>)>) -> Self`：创建 `InputState`（用 `initial_query` 设初始值）、`separator` 由 `host.is_remote()` 决定（`true` → `/`，`false` → `std::path::MAIN_SEPARATOR`）、初始 `state.query = initial_query`、触发初始 `update_matches`
- [ ] 2.3 实现 `PathPicker::update_matches(&mut self, query: String, cx: &mut Context<Self>)`：`get_dir_and_suffix` 切分 → `dir` 变化时翻转 `cancel_flag` + 后台 `host.list_dir` → 完成后检查 flag + 更新 `entries`/`dir`/`loading` → 调 `filter_entries` 更新 `filtered` → `selected_index = 0` → `cx.notify()`
- [ ] 2.4 实现 `PathPicker::select(&mut self, index: usize, cx: &mut Context<Self>)`：`selected_index = index`（clamp 到 `filtered.len()`），`cx.notify()`
- [ ] 2.5 实现 `PathPicker::confirm_completion(&mut self, cx: &mut Context<Self>)`：取 `filtered[selected_index]` 对应 entry，`complete_path` 生成新 query，写入 `InputState`（`input.update(cx, |s, cx| s.set_value(new_query, window, cx))`），触发 `update_matches`（目录补 `/` 后重新 `list_dir`）
- [ ] 2.6 实现 `PathPicker::confirm(&mut self, window: &mut Window, cx: &mut Context<Self>)`：取 `filtered[selected_index]` 对应 entry 的完整路径（`dir + name`）或输入的完整 query，调 `on_confirm(path, window, cx)`。`on_confirm` 内部验证 git 仓库 + `set_repo`；失败时 `self.set_error("所选目录不是 git 仓库")`（不关闭弹窗）
- [ ] 2.7 实现 `PathPicker::dismiss(&mut self, cx: &mut Context<Self>)`：cancel_flag 翻转（取消 in-flight 任务），`cx.emit(DismissEvent)` 或调 `on_dismiss` 回调（由 `WorkspaceView` 设 `path_picker = None`）
- [ ] 2.8 实现 `PathPicker::set_error(&mut self, msg: &str)`：`state.error = Some(msg.into())`，`cx.notify()`
- [ ] 2.9 在 `crates/app/src/ui/mod.rs` 加 `pub mod path_picker;` + `pub use path_picker::PathPicker;`

## 3. App: PathPicker 渲染

- [ ] 3.1 实现 `PathPicker::render`：复用 `ui::dialog::modal` 骨罩（遮罩 + 居中卡片，宽度 460px）。卡片内：`Input::new(&self.input)` 文本框 + 可滚动补全列表（`div().flex().flex_col().max_h(px(320.0)).overflow_y_scroll()`）+ 可选错误/loading 文本 + 底部按钮行（Browse + Cancel）
- [ ] 3.2 补全列表渲染：遍历 `state.filtered`，每条 `div().id("picker-entry-{i}").cursor_pointer().when(is_selected, |d| d.bg(BTN_BG_HOVER)).hover(|s| s.bg(BTN_BG_HOVER)).child("{icon}{name}")`。点击调 `select(i, cx)`；双击或 Enter 调 `confirm`
- [ ] 3.3 loading 状态：`state.loading == true` 时列表区显示 "加载中…"；`state.error` 有值时显示错误文字（`theme::TEXT_DIM`）；`filtered` 为空且非 loading 且无 error 时显示 "(无匹配)"
- [ ] 3.4 Browse 按钮：`!host.is_remote()` 时渲染 "Browse…" 按钮，点击调 `cx.prompt_for_paths`（原生目录选择器），选中后调 `on_confirm(path)`
- [ ] 3.5 Cancel 按钮：点击调 `dismiss(cx)`
- [ ] 3.6 遮罩点击关闭：`modal` 的遮罩 `on_mouse_down(Left)` 调 `dismiss(cx)`（或在 `modal` 骨罩加 `on_mouse_down` + `stop_propagation`）
- [ ] 3.7 `InputState` 值变化监听：`PathPicker::new` 中 `cx.subscribe(&input, |this, _, _, cx| { let q = this.input.read(cx).value().to_string(); this.update_matches(q, cx); })`（如果 `InputState` 有值变化事件）；或每次 `on_key_down` 时读 `input.read(cx).value()` 同步触发 `update_matches`

## 4. App: PathPicker 键盘交互

- [ ] 4.1 `PathPicker::render` 根 `div` 绑定 `on_key_down`：`Up` → `selected_index` 循环上移；`Down` → `selected_index` 循环下移；`Tab` → `confirm_completion(cx)` + `cx.stop_propagation()`；`Enter` → `confirm(window, cx)` + `cx.stop_propagation()`；`Escape` → `dismiss(cx)` + `cx.stop_propagation()`
- [ ] 4.2 `Up`/`Down` 在 `filtered` 为空时 no-op（不 panic）
- [ ] 4.3 `Tab` 在 `filtered` 为空时 no-op
- [ ] 4.4 `Enter` 在 `filtered` 为空时用输入的完整 query 调 `on_confirm`（用户可能输入了一个不在列表里的路径）

## 5. App: WorkspaceView 集成

- [ ] 5.1 `WorkspaceView` 新增字段 `path_picker: Option<Entity<PathPicker>>`（替代 `open_repo_choice_open: bool` + `wsl_browser: Option<WslBrowser>`）
- [ ] 5.2 `WorkspaceView::open_repo_picker` 改为：构造 `initial_query`（`self.repo` 或 host 默认 `/` / home）→ `cx.new(|cx| PathPicker::new(host, initial_query, cx, on_confirm_closure))` → `self.path_picker = Some(picker)` → `cx.notify()`。`on_confirm` 闭包捕获 `WeakEntity<WorkspaceView>`，内调 `git::main_worktree_root` + `set_repo` + `path_picker = None`
- [ ] 5.3 `WorkspaceView::render` 模态叠加区：移除 `if self.open_repo_choice_open` 和 `if self.wsl_browser.is_some()` 分支，改为 `if let Some(picker) = &self.path_picker { root.child(picker.clone()) }`
- [ ] 5.4 `WorkspaceView::on_key_down` Esc 处理：移除 `open_repo_choice_open` 和 `wsl_browser` 分支（`PathPicker` 自己处理 Esc，`WorkspaceView` 不需要管）
- [ ] 5.5 移除 `WslBrowser` struct、`open_repo_choice_open` 字段、`wsl_browser` 字段、`open_repo_choice_dialog` 方法、`wsl_browser_dialog` 方法、`open_wsl_browser` 方法、`load_wsl_dir` 方法、`navigate_wsl_dir` 方法、`commit_wsl_browser` 方法
- [ ] 5.6 移除 `dialogs.rs` 中的 `open_repo_choice_dialog` 和 `wsl_browser_dialog` 方法
- [ ] 5.7 `lib.rs` / `main.rs` 无改动（`PathPicker` 是 app 层组件，不影响启动流程）

## 6. App: 测试 — 单元测试（`#[test]`，无 GPUI / 无 Host）

- [ ] 6.1 `crates/app/src/ui/path_picker.rs` `#[cfg(test)] mod tests`：`get_dir_and_suffix` Posix 路径切分（`/home/user/Doc` → `("/home/user/", "Doc")`、`/home/user/` → `("/home/user/", "")`、`/` → `("/", "")`、`Doc` → `("", "Doc")`）
- [ ] 6.2 `get_dir_and_suffix` Windows 路径切分（`C:\Users\Doc` → `("C:\Users\", "Doc")`、`C:/Users/Doc` → `("C:/Users/", "Doc")`）
- [ ] 6.3 `filter_entries`：后缀空 → 全部索引；后缀 `do` → 只含 `docs` 的索引；大小写不敏感（`Do` 匹配 `docs`、`DOCS`）；无匹配 → 空 Vec
- [ ] 6.4 `complete_path`：目录 `src` → `/home/user/src/`（补 `/`）；文件 `README.md` → `/home/user/README.md`（不补）；Windows 分隔符 `\`

## 7. App: 测试 — UI 状态测试（`#[gpui::test]`，accessor 验证状态机）

- [ ] 7.1 `crates/app/tests/path_picker_test.rs` 新增：用 `MockHost` 预装目录条目（`/home/user/` 下有 `src`、`target`、`docs`），构造 `PathPicker::new(host, "/home/user/".into(), cx, on_confirm)`，验证 `path_picker_filtered_count() == 3`、`path_picker_entries() == ["src", "target", "docs"]`
- [ ] 7.2 过滤测试：`set_path_picker_query_for_test("/home/user/do")` → `path_picker_filtered_count() == 1`、`path_picker_entries() == ["docs"]`
- [ ] 7.3 选中导航测试：`path_picker_selected_index()` 初始为 0；模拟 Down 键 → `selected_index` 变 1；再 Down → 变 2；再 Down → 循环回 0；Up → 循环到 2
- [ ] 7.4 Tab 补全测试：选中 `src`（目录），模拟 Tab 键 → query 变为 `/home/user/src/`，`list_dir` 被调用（MockHost 记录），`path_picker_entries()` 变为 `src` 目录下的条目
- [ ] 7.5 Esc 关闭测试：模拟 Escape 键 → `path_picker_open()` 变 false（`WorkspaceView.path_picker` 变 None）
- [ ] 7.6 确认非 git 目录测试：`on_confirm` 回调验证 `git::main_worktree_root` 返回 None → `path_picker` 不关闭、`set_error` 被调用、错误文字显示
- [ ] 7.7 确认 git 目录测试：MockHost 预装 `git rev-parse --show-toplevel` 返回成功 → `on_confirm` 调 `set_repo` → `path_picker` 关闭、`WorkspaceView.repo` 被设置
- [ ] 7.8 WSL 模式路径分隔符测试：用 `WslHost::default()`（`is_remote() == true`）构造 `PathPicker`，验证 `separator == '/'`；用 `LocalHost` 构造，验证 `separator == std::path::MAIN_SEPARATOR`
- [ ] 7.9 Browse 按钮可见性测试：`LocalHost` 模式下 Browse 按钮渲染（通过 render snapshot 或 accessor）；`WslHost` 模式下不渲染（Phase 1: accessor 验证 `path_picker_has_browse_button() == !host.is_remote()`）

## 8. App: 测试 — 集成测试（`#[gpui::test]` + `wait_for`，端到端流程）

- [ ] 8.1 `crates/app/tests/path_picker_test.rs` 新增：用 `MockHost` 预装 `/home/user/project/.git` 目录（MockHost `list_dir` 返回 `.git` 目录 + `src` 目录），模拟输入 `/home/user/project/` + Enter → `wait_for` 验证 `WorkspaceView.repo` 被设置为 `/home/user/project`、`path_picker_open()` 变 false
- [ ] 8.2 WSL 模式集成测试（`#[ignore]`，需真实 WSL）：用真实 `WslHost`，在 WSL 内 `git init` 临时仓库，`PathPicker` 输入路径 + Enter → `wait_for` 验证 `set_repo` 成功、worktree 列表加载
- [ ] 8.3 cancel-flag 取消测试：`MockHost` 的 `list_dir` 延迟返回（用 `std::thread::sleep`），快速连续输入两个 query（第二个 dir 不同），`wait_for` 验证最终 `path_picker_entries()` 只反映第二个 query 的结果（第一个被取消）
- [ ] 8.4 本地模式回归测试：`LocalHost` 模式下 `PathPicker` 用 `tempfile::tempdir()` 创建临时目录 + 子目录，输入路径 + Enter → 验证 `set_repo` 成功（tempdir 不是 git 仓库 → 显示错误；`git init` 后重试 → 成功）

## 9. 质量门

- [ ] 9.1 `cargo fmt`（无 diff）
- [ ] 9.2 `cargo clippy --all-targets`（无 warning）
- [ ] 9.3 `cargo test -p lucy-app`（UI 状态测试 + 集成测试全绿；WSL 集成测试 `#[ignore]` 不跑）
- [ ] 9.4 `cargo run -p lucy-app` 在 Windows 上启动：WSL 已安装时打开 PathPicker，输入 `/home/` 看到补全列表，Tab 补全目录，Enter 打开 git 仓库
- [ ] 9.5 `cargo run -p lucy-app` 在无 WSL 的环境（或 macOS）启动：打开 PathPicker，输入 home 目录路径，看到补全列表，Browse 按钮可见且可点击
