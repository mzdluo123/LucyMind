## 1. lib+bin 分离 + dev-dependencies

- [x] 1.1 `crates/app/Cargo.toml`：新增 `[lib] name = "lucy_app" path = "src/lib.rs"`；新增 `[dev-dependencies]`：`gpui = { version = "0.2.2", features = ["test-support"] }`、`tempfile = "3"`
- [x] 1.2 新建 `crates/app/src/lib.rs`：`pub mod assets/terminal_view/theme/ui/workspace`；`pub fn run()` 搬入 `main.rs` 的 `Application::new().with_assets(Assets).run(...)` 逻辑（含 `env_logger` init、`gpui_component::init`、`Root` 包裹、窗口创建）
- [x] 1.3 `crates/app/src/main.rs` 瘦身为 `fn main() { lucy_app::run() }`
- [x] 1.4 `cargo build -p lucy-app` + `cargo run -p lucy-app` 验证 lib+bin 分离不破坏现有行为

## 2. 测试 accessor

- [x] 2.1 `WorkspaceView` 加 `#[cfg(test)] pub fn` accessor：`active_path`、`worktree_count`、`worktree_paths`、`is_agent_menu_open`、`current_status`、`has_pending_close`、`pending_close_branch`、`terminals_contains`、`settings_open`、`editing_alias`、`is_ours_path`
- [x] 2.2 `TerminalView` 加 `#[cfg(test)] pub fn` accessor：`snapshot_text`、`is_exited`、`selection_text`、`has_selection`、`dimensions`
- [x] 2.3 `cargo build -p lucy-app --tests` 通过（accessor 编译）

## 3. 测试 harness

- [x] 3.1 `crates/app/tests/common/mod.rs`：`temp_repo()`（`tempfile::tempdir` + `git init` + `git commit --allow-empty -m init`）、`temp_registry_dir()`、`build_workspace(cx, repo)`（`gpui_component::init(cx)` + `Root::new` 包裹 + `WorkspaceView::new`，返回 `Entity<WorkspaceView>` + `WindowHandle`）
- [x] 3.2 `crates/app/tests/common/mod.rs`：`wait_for(cx, predicate_fn, timeout)`（循环 `run_until_parked` + 检查谓词，超时 panic）、`shutdown_workspace(cx, workspace)`（drop 所有 terminal + `run_until_parked` 排空，避免 leak-detection 误报）
- [x] 3.3 `crates/app/tests/common/mod.rs`：`fake_agent_command()`（返回 `/bin/sh` 或 Windows `cmd.exe`，供 `new_worktree_and_agent` 测试用，避免真实 claude 依赖）
- [x] 3.4 smoke 测试 `tests/smoke.rs`：`#[gpui::test]` 构造 `build_workspace` + 断言 `worktree_count() >= 1` + `shutdown_workspace`，验证 harness 跑通

## 4. 启动 / 空态

- [ ] 4.1 `tests/startup.rs`：有 repo 启动（`build_workspace(cx, Some(temp_repo))`）→ `worktree_count() >= 1`（main 行）、`active_path` 指向 main、`current_status` 无错误
- [ ] 4.2 空态启动（candidate=None）→ `has_pending_prompt` true（`open_repo_picker` 触发原生选择器）；`simulate_new_path_selection(temp_repo)` → `worktree_count() >= 1`
- [ ] 4.3 非 git 目录 candidate → 空态 + prompt（`has_pending_prompt` true）

## 5. worktree 列表 / 切换

- [x] 4.1 `tests/startup.rs`：有 repo 启动（`build_workspace(cx, Some(temp_repo))`）→ `worktree_count() >= 1`（main 行）、`active_path` 指向 main、`current_status` 无错误
- [x] 4.2 空态启动（candidate=None）→ `has_pending_prompt` true（`open_repo_picker` 触发原生选择器）；`simulate_new_path_selection(temp_repo)` → `worktree_count() >= 1`
- [x] 4.3 非 git 目录 candidate → 空态 + prompt（`has_pending_prompt` true）

## 5. worktree 列表 / 切换

- [x] 5.1 `tests/worktree_list.rs`：多 worktree 仓库（`git worktree add` 造 2-3 个）→ `worktree_count` == 预期；main 行 `is_main_repo` true（不可关闭守卫）
- [ ] 5.2 `tests/switch.rs`：`simulate_click` 点 worktree 行 → `active_path` 切换；已开终端切换不重建（`terminals_contains` 不变、terminal session 复用）

## 6. 新建 worktree + agent

- [x] 6.1 `tests/new_worktree.rs`：`new_worktree_and_agent("sh")`（用 `fake_agent_command`）→ `git::list` 含新分支、PostCreate hook 跑、`terminals_contains(new_path)` true、`active_path` == new_path、registry `is_ours` true
- [x] 6.2 新 worktree 的 TerminalView 渲染 PTY 输出：`wait_for(cx, |c| c.snapshot_text(path).contains("SHELL_READY"))` 断言（sh 打印 marker）
- [x] 6.3 新建失败路径：分支已存在 / hook 失败 → `current_status` 是错误、`terminals` 未新增

## 7. 关闭 worktree

- [x] 7.1 `tests/close.rs`：干净 worktree → `request_close` → `do_close` → `terminals_contains` false、registry 注销、`git::list` 不含该分支（或 `git lock` 清理）
- [x] 7.2 脏 worktree（`git touch` 未提交）→ `request_close` → `has_pending_close` true + `pending_close_branch` 正确；`confirm_close` → 执行关闭；`cancel_close` → `has_pending_close` false、terminal 未销毁
- [x] 7.3 主仓 close 守卫：对 main 行 `request_close` → 不删主仓（`is_main_repo` 守卫，`terminals`/`git::list` 不变）

## 8. agent 菜单

- [x] 8.1 `tests/agent_menu.rs`：`simulate_click` 点 `+` 按钮 → `is_agent_menu_open` true
- [x] 8.2 菜单项数 == `lucy_core::agent::builtin_agents().len()`；`VisualTestContext::draw` 断言菜单元素渲染
- [ ] 8.3 点菜单项 → `is_agent_menu_open` false + 触发 `new_worktree_and_agent`（同 6.1 断言）
- [ ] 8.4 点遮罩 / `simulate_keystrokes("escape")` → `is_agent_menu_open` false

## 9. 别名 / 设置

- [ ] 9.1 `tests/alias.rs`：打开别名编辑器 → `editing_alias` 正确；`simulate_input` 输入别名 + 提交 → `.worktree.toml` 更新（`lucy_core::config::load` 验证 alias）
- [ ] 9.2 `tests/settings.rs`：打开设置 → `settings_open` true；修改 fail_fast / location + `commit_settings` → `EditableSettings` 落盘（`config::load` 验证）

## 10. 终端渲染 / 输入

- [x] 10.1 `tests/terminal_render.rs`：`/bin/sh -c 'printf HELLO_LUCY'` → `wait_for` 断言 `snapshot_text` 含 `HELLO_LUCY`；`VisualTestContext::draw` 断言 TerminalElement 渲染不 panic
- [ ] 10.2 `tests/terminal_input.rs`：`/bin/cat` + `simulate_input("abc")` → `snapshot_text` 含 `abc`（回显）；`simulate_keystrokes` 功能键（如 Ctrl+C）编码到 PTY
- [ ] 10.3 resize：`simulate_resize(new_size)` → `TerminalView` `maybe_resize` → `dimensions()` 更新（columns/rows 按新像素算）

## 11. 终端复制交互（回归 terminal-copy）

- [ ] 11.1 `tests/terminal_copy.rs`：双击选词 → `selection_text` == 词；`read_from_clipboard` == 词
- [ ] 11.2 三击选行 → 选整行（尾随空格修剪）；`read_from_clipboard` 含修剪后文本
- [ ] 11.3 拖选释放 → copy-on-select（`read_from_clipboard` == 选区文本）
- [ ] 11.4 `simulate_keystrokes("cmd+a")` 全选；`simulate_keystrokes("cmd+c")` 复制（`read_from_clipboard` 非空）
- [ ] 11.5 右键菜单（`simulate_click` 右键）→ 菜单渲染；点 Copy/Paste/Select All 执行对应操作；无选区时 Copy 灰显（`VisualTestContext::draw` 断言 disabled 状态）
- [ ] 11.6 Shift+点击扩展选区 → `selection_text` 覆盖扩展范围
- [ ] 11.7 复制视觉反馈：copy 后 `copy_flash` 状态变化 + `notifications` 断言 `cx.notify()` 触发

## 12. 闭环门禁

- [x] 12.1 `cargo test -p lucy-app` 全绿（含所有 `#[gpui::test]`）
- [x] 12.2 `cargo fmt && cargo clippy --all-targets` 无 warning
- [x] 12.3 `CLAUDE.md` 测试段补 app 层 `#[gpui::test]` 说明（`cargo test -p lucy-app`、harness 位置 `tests/common/`、新增测试约定：UI 功能改动必须伴随 `#[gpui::test]`）
- [x] 12.4 门禁有效性验证：手动改一个 UI 行为（如注释掉 agent 菜单 `agent_menu_open = true` 的 toggle）→ 对应 `agent_menu.rs` 测试失败（证明门禁能抓回归）
