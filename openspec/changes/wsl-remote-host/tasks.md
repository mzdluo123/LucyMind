## 1. Core: Host trait 与类型定义

- [x] 1.1 新建 `crates/core/src/host.rs`，定义 `Host` trait（`Send + Sync`）：`run(&self, cmd: HostCommand) -> Result<HostOutput, HostError>`、`run_shell(&self, cwd: &Path, cmd: &str, env: &[(String, String)]) -> Result<HostOutput, HostError>`、`canonicalize(&self, path: &Path) -> Result<PathBuf, HostError>`、`exists(&self, path: &Path) -> bool`、`read_to_string(&self, path: &Path) -> Result<String, HostError>`、`write(&self, path: &Path, content: &str) -> Result<(), HostError>`、`copy(&self, from: &Path, to: &Path) -> Result<(), HostError>`、`create_dir_all(&self, path: &Path) -> Result<(), HostError>`、`default_shell(&self, cwd: &Path) -> Option<(String, Vec<String>)>`、`is_remote(&self) -> bool`
- [x] 1.2 定义 `HostCommand` struct（`program: String, args: Vec<String>, cwd: Option<PathBuf>, env: Vec<(String, String)>`）和 `HostOutput` struct（`stdout: String, stderr: String, success: bool, exit_code: Option<i32>`）
- [x] 1.3 定义 `HostError` 枚举（`Io(io::Error)`、`Command { cmd, stderr }`、`NotFound`、`other`），实现 `std::error::Error`
- [x] 1.4 在 `crates/core/src/lib.rs` 加 `pub mod host;` 并 `pub use host::*;`
- [x] 1.5 `cargo build -p lucy-core` 通过（trait 定义无实现，编译通过即可）

## 2. Core: LocalHost 实现

- [x] 2.1 在 `host.rs` 实现 `LocalHost`（ZST，`Clone + Copy`）：`run` 用 `Command::new(program).current_dir(cwd).args(args).envs(env).output()`；`run_shell` 移入 `hooks/engine.rs:154-167` 的 `shell_command` 逻辑（Unix `sh -c` / Windows `cmd /C`）+ `.current_dir(cwd).envs(env).output()`；`canonicalize` 移入 `workspace/mod.rs:51-69` 的 `canon` 逻辑（`Path::canonicalize` + `strip_verbatim_prefix`）；`exists`/`read_to_string`/`write`/`copy`/`create_dir_all` 直调 `std::fs`；`default_shell` 返回 `None`；`is_remote` 返回 `false`
- [x] 2.2 `strip_verbatim_prefix` 函数从 `workspace/mod.rs` 移到 `host.rs`（LocalHost 内部用，app 层不再直接调用）
- [x] 2.3 `cargo build -p lucy-core` 通过

## 3. Core: MockHost 测试替身

- [x] 3.1 在 `host.rs` 的 `#[cfg(test)]` mod（或 `#[cfg(feature = "test-support")]`）实现 `MockHost`：内存记录所有 `run`/`run_shell` 调用（`commands: Vec<HostCommand>`、`shell_commands: Vec<(PathBuf, String, Vec<(String,String)>)>`），可配置返回值（`push_output(stdout, success)`），文件操作用内存 `HashMap<PathBuf, String>`
- [x] 3.2 `MockHost` 实现 `canonicalize`（直接返回输入，或查内存 map）、`exists`（查内存 map）、`read_to_string`（查内存 map）、`write`（写内存 map）、`copy`（内存 map 复制）、`create_dir_all`（no-op）、`default_shell`（返回 `None`）、`is_remote`（返回 `false`）

## 4. Core: git 模块线程化 Host

- [x] 4.1 `git/mod.rs::run_git` 签名改为 `pub(crate) fn run_git(host: &dyn Host, repo: &Path, args: &[&str]) -> Result<String, GitError>`，内部用 `host.run(HostCommand { program: "git", args: ["-C", repo, ...args], cwd: None, env: [] })` 替代 `Command::new("git")`
- [x] 4.2 `git/mod.rs` 所有 pub 函数加 `host: &dyn Host` 参数并透传给 `run_git`：`add`、`remove`、`list_worktrees`、`lock`、`unlock`、`prune`、`toplevel`、`main_worktree_root`、`branch_exists`、`branch_checked_out_at`、`has_uncommitted_changes`
- [x] 4.3 `git/status.rs` 和 `git/worktree.rs` 内部 `run_git` 调用全部加 `host` 参数透传
- [x] 4.4 `git/worktree.rs::uses_submodules`（`.gitmodules` is_file 检查）改用 `host.exists`
- [x] 4.5 更新 `git_test` 所有测试：用 `LocalHost`（或 `MockHost`）替换无 Host 参数的调用
- [x] 4.6 `cargo test -p lucy-core --test git_test` 通过

## 5. Core: hooks 模块线程化 Host

- [x] 5.1 `hooks/engine.rs::run_event` 签名加 `host: &dyn Host` 参数，透传给 `copy_file` 和 `run_command`
- [x] 5.2 `copy_file` 改用 `host.exists(src)` + `host.create_dir_all(parent)` + `host.copy(src, dst)` 替代 `std::fs`
- [x] 5.3 `run_command` 改用 `host.run_shell(ctx.worktree_path, cmd, &ctx.env_vars())` 替代 `shell_command(cmd) + Command::output()`；删除 `shell_command` 函数（逻辑已移入 LocalHost::run_shell）
- [x] 5.4 更新 `hooks_test`：用 `LocalHost` 调用 `run_event`
- [x] 5.5 `cargo test -p lucy-core --test hooks_test` 通过

## 6. Core: config 模块线程化 Host

- [x] 6.1 `config/mod.rs::load` 签名加 `host: &dyn Host`，内部用 `host.read_to_string(path)` 替代 `std::fs::read_to_string`
- [x] 6.2 `config/mod.rs::set_alias` 签名加 `host: &dyn Host`，读用 `host.read_to_string`、写用 `host.write`、`create_dir_all` 用 `host.create_dir_all`
- [x] 6.3 `config/mod.rs::set_worktree_settings` 同上改造
- [x] 6.4 `config::parse`（纯字符串解析）不加 Host（无 I/O）
- [x] 6.5 更新 `config_test`：用 `LocalHost` 或 `MockHost`（MockHost 预装 `.worktree.toml` 内容）
- [x] 6.6 `cargo test -p lucy-core --test config_test` 通过

## 7. Core: WslHost 实现

- [x] 7.1 在 `host.rs` 实现 `WslHost`：持有可选 `distro: Option<String>`（Phase 1 为 None = 默认发行版）
- [x] 7.2 `WslHost::run`：构造 `Command::new("wsl.exe")`，如有 cwd 加 `--cd <cwd>`，加 `--`，加 program + args；env 用 `env K=V` 前缀注入（在 `--` 后、program 前）；`.output()` 取 stdout/stderr/exit code
- [x] 7.3 `WslHost::run_shell`：构造 `wsl.exe --cd <cwd> -- /bin/sh -c "<env_exports> <cmd>"`，env 用 `export K='V';` 前缀（单引号转义：`'` → `'\''`）
- [x] 7.4 `WslHost::canonicalize`：`wsl.exe -- realpath <path>`，取 stdout trim
- [x] 7.5 `WslHost::exists`：`wsl.exe -- test -e <path>`，退出码 0 = true
- [x] 7.6 `WslHost::read_to_string`：`wsl.exe -- cat <path>`，取 stdout
- [x] 7.7 `WslHost::write`：`wsl.exe -- tee <path>`，stdin 喂 content
- [x] 7.8 `WslHost::copy`：`wsl.exe -- cp <from> <to>`
- [x] 7.9 `WslHost::create_dir_all`：`wsl.exe -- mkdir -p <path>`
- [x] 7.10 `WslHost::default_shell`：返回 `Some(("wsl.exe", vec!["--cd".into(), cwd.to_string_lossy().into()]))`
- [x] 7.11 `WslHost::is_remote`：返回 `true`
- [x] 7.12 新增 `wsl_host_test`（`#[test]`）：用 `MockHost` 无法测 WslHost（需真实 WSL）；改为单测 `WslHost` 的命令构造逻辑（提取 `build_wsl_args` / `build_shell_command` 为可测函数，断言生成的参数/命令字符串正确，不实际 spawn `wsl.exe`）

## 8. App: WorkspaceView 持有 Host

- [x] 8.1 `WorkspaceView` 加字段 `host: Arc<dyn Host>`，`new` / `new_for_test` / `construct` 接收 `Arc<dyn Host>` 参数
- [x] 8.2 `canon(p)` 改为 `canon(host: &dyn Host, p: &Path)` → `host.canonicalize(p)`；`same_path` 同理加 host 参数
- [x] 8.3 `count_uncommitted` 改为 `count_uncommitted(host: &dyn Host, worktree: &Path)` → `host.run(HostCommand { program: "git", args: ["-C", worktree, "status", "--porcelain"], .. })`（不再直调 `std::process::Command`）
- [x] 8.4 `set_repo`：`canon` → `host.canonicalize`；`config::load` → `config::load(host, ...)`；`git::list` → `git::list(host, ...)`
- [x] 8.5 `new_worktree`：`git::add` → `git::add(host, ...)`；`hooks::run_event` → `hooks::run_event(host, ...)`；`canon` → `host.canonicalize`；`git::lock` → `git::lock(host, ...)`
- [x] 8.6 `do_close`：`hooks::run_event` → `hooks::run_event(host, ...)`；`canon` → `host.canonicalize`；后台 `git::unlock`/`git::remove` clone `Arc<dyn Host>` 传入
- [x] 8.7 `request_close`：`git::has_uncommitted_changes` → `git::has_uncommitted_changes(host, ...)`；`count_uncommitted` → `count_uncommitted(host, ...)`
- [x] 8.8 `refresh_worktrees`：`git::list` → `git::list(host, ...)`
- [x] 8.9 `open_repo_picker`：`git::main_worktree_root` → `git::main_worktree_root(host, ...)`
- [x] 8.10 `spawn_shell_tab`：`shell.command()` 改为 `self.host.default_shell(wt_path)`（LocalHost 返回 None = 系统默认 shell；WslHost 返回 `("wsl.exe", ["--cd", wt_path])`）；`ShellKind` 仍用于 tab 标题回退和菜单显示，但 command 来源改为 Host
- [x] 8.11 所有 `self.repo` 的使用处检查路径来源——repo 是 Host 规范化后的路径（LocalHost = Windows 路径，WslHost = Linux 路径），`PathBuf::join` 拼接 `.worktree.toml` 等子路径在 WSL 模式下用 `/` 分隔（`PathBuf::join` 在 Windows 上用 `\`，但 `wsl.exe` 接受 `/` 和 `\`——需验证或手动用 `format!("{}/{}", dir, file)` 拼接）

## 9. App: WSL 检测与启动

- [x] 9.1 `lib.rs::run()`：在构造 `WorkspaceView` 前检测 WSL——`Command::new("wsl.exe").arg("--status").output()`，退出码 0 = WSL 可用；失败/不存在 = 回退 `LocalHost`
- [x] 9.2 检测结果构造 `Arc<dyn Host>`（`Arc::new(WslHost::default())` 或 `Arc::new(LocalHost)`），传给 `WorkspaceView::new(cx, candidate, host)`
- [x] 9.3 WSL 检测失败时 `log::warn!("WSL 检测失败，回退本地模式: {e}")`，不 panic
- [x] 9.4 `path_env::fix_path_from_login_shell()` 保持原样（Unix only，Windows no-op；WSL shell 自带 login PATH）

## 10. App: WSL 项目打开（路径输入）

- [x] 10.1 新增 WSL 路径输入弹窗：当 `host.is_remote()` 为 true 时，`open_repo_picker` 弹出一个 modal（复用 `ui/dialog.rs` 的 modal 机制），含一个 `gpui-component::InputState` 文本框（placeholder `/home/user/project`）+ 「Open」按钮
- [x] 10.2 用户输入路径点 Open → `host.canonicalize(path)` → `git::main_worktree_root(host, &path)` 验证 → 成功 `set_repo`，失败 `set_status("所选目录不是 git 仓库", true)`
- [x] 10.3 当 `host.is_remote()` 为 false 时，`open_repo_picker` 保持原有 `cx.prompt_for_paths` 原生目录选择器
- [x] 10.4 WSL 模式下也提供「Browse...」入口（用 `cx.prompt_for_paths` 选本地目录），但选本地目录后提示「WSL 模式下请输入 WSL 路径」或切换到 LocalHost（Phase 1: 仅提示，不切换）

## 11. App: WSL shell 启动与 ShellKind 适配

- [x] 11.1 `spawn_shell_tab` 的 env 注入：WSL 模式下 `TERM=xterm-256color` + `WORKTREE_*` 环境变量需在 WSL shell 内生效。方式：`wsl.exe --cd <cwd> -- env TERM=xterm-256color WORKTREE_PATH=... /bin/sh`（或用 `--set-env` flag，需验证 WSL 版本支持）
- [x] 11.2 `ShellKind` 的 `command()` 方法改为接收 `host: &dyn Host`（或 `is_remote: bool`）：`Default` 在 WSL 模式返回 `host.default_shell(cwd)`，在本地模式返回 `None`；`Cmd`/`PowerShell`/`Pwsh` 仅在 `!is_remote && cfg(windows)` 时可用
- [x] 11.3 `workspace/tabs.rs` 的 New Tab 菜单：WSL 模式下（`host.is_remote()`）只显示 `Default`，不显示 `Cmd`/`PowerShell`/`Pwsh`
- [x] 11.4 `send_agent_command` / `agent_command_string`：WSL 模式下 shell quoting 用 Unix 风格（单引号），LocalHost Windows 模式保持现状（当前实现已用单引号，需确认）

## 12. App: 测试 — 单元测试（`#[test]`，无 GPUI / 无 PTY）

- [x] 12.1 `crates/core/src/host.rs` `#[cfg(test)] mod tests`：`LocalHost::run` 执行 `git --version` 成功返回（真实进程，标记 `#[ignore]` 或用 MockHost 代替）；`LocalHost::run_shell` 执行 `echo hello` 返回 stdout 含 `hello`；`LocalHost::canonicalize` 规范化 `.` 为绝对路径；`LocalHost::exists` 对存在的文件返回 true、不存在返回 false
- [x] 12.2 `MockHost` 单测：`run` 记录命令参数、返回配置的 stdout；`read_to_string`/`write` 内存读写正确；`copy` 内存复制正确；`canonicalize` 返回输入路径
- [x] 12.3 `WslHost` 命令构造单测（不 spawn `wsl.exe`）：`build_run_args(HostCommand { program: "git", args: ["status"], cwd: Some("/home"), env: [] })` 断言生成 `["--cd", "/home", "--", "git", "status"]`；`build_shell_command("/home/wt", "npm install", &[("WORKTREE_PATH", "/home/wt")])` 断言生成 `--cd /home/wt -- /bin/sh -c "export WORKTREE_PATH='/home/wt'; npm install"`；单引号转义测试：值含 `'` 时正确转义为 `'\''`
- [x] 12.4 `git` 模块用 `MockHost` 单测：`MockHost` 预装 `git worktree list --porcelain` 输出，`list_worktrees` 正确解析；`MockHost` 预装 `git status --porcelain` 空输出，`has_uncommitted_changes` 返回 false
- [x] 12.5 `hooks` 模块用 `MockHost` 单测：`run_event` 调用 `MockHost::run_shell` 记录了正确的 cwd / cmd / env；`copy_file` 调用 `MockHost::copy` 记录了 src / dst
- [x] 12.6 `config` 模块用 `MockHost` 单测：`MockHost` 预装 `.worktree.toml` 内容，`load` 正确解析；`set_alias` 调用 `MockHost::write` 记录了修改后的内容

## 13. App: 测试 — UI 状态测试（`#[gpui::test]`，accessor 验证状态机）

- [x] 13.1 `crates/app/tests/` 新增 `wsl_host_test.rs`：用 `LocalHost` 构造 `WorkspaceView::new_for_test(cx, candidate, Arc::new(LocalHost))`，验证 `host` 字段为 `LocalHost`（通过 `is_remote()` accessor 返回 false）
- [x] 13.2 WSL 模式 UI 状态测试：构造 `WorkspaceView` with `MockHost`（`is_remote` 返回 true），验证 launcher menu 的 New Tab 只显示 `Default`（通过 `available_shell_kinds()` accessor 或类似）
- [x] 13.3 `open_repo_picker` 在 `is_remote` 为 true 时弹出路径输入 modal（通过 `wsl_path_input_open` accessor 验证状态）；`is_remote` 为 false 时走原生 picker（`wsl_path_input_open` 为 false）
- [x] 13.4 现有 `#[gpui::test]` 测试全部改为传 `Arc::new(LocalHost)`（或 `Arc::new(MockHost)`）给 `WorkspaceView::new_for_test`，确保 LocalHost 回归无破坏

## 14. App: 测试 — 集成测试（`#[gpui::test]` + `wait_for`，端到端流程）

- [x] 14.1 `crates/app/tests/` 新增 `wsl_integration_test.rs`（标注 `#[ignore]`，需真实 WSL 环境）：用真实 `WslHost`，在 WSL 内 `git init` 临时仓库，`WorkspaceView::new_for_test` 打开该仓库，`wait_for` 验证 worktree 列表正确加载
- [x] 14.2 WSL worktree 创建集成测试（`#[ignore]`）：`new_worktree` 在 WSL 内创建 worktree，`wait_for` 验证 `git worktree list` 包含新 worktree，终端 tab 启动（验证 `terminals` map 有条目）
- [x] 14.3 WSL shell 启动集成测试（`#[ignore]`）：`spawn_shell_tab` 在 WSL worktree 内启动 shell，`wait_for` 轮询终端 snapshot 包含 shell prompt（`$` 或 `%` 或用户名）
- [x] 14.4 WSL hook 执行集成测试（`#[ignore]`）：配置 `.worktree.toml` 的 `post_create = ["echo HELLO > hook_test.txt"]`，`new_worktree` 后 `wait_for` 验证 `host.exists("worktree/hook_test.txt")` 为 true 且 `host.read_to_string` 内容含 `HELLO`
- [x] 14.5 LocalHost 回归集成测试（不 ignore）：现有 `smoke` / `new_worktree` / `tab_crud` 等测试全部用 `LocalHost` 跑通，验证 Host 抽象未破坏本地模式

## 15. 质量门

- [x] 15.1 `cargo fmt`（无 diff）
- [x] 15.2 `cargo clippy --all-targets`（无 warning）
- [x] 15.3 `cargo test -p lucy-core`（host/git/hooks/config 单测全绿）
- [x] 15.4 `cargo test -p lucy-app`（UI 状态测试 + LocalHost 回归集成测试全绿；WSL 集成测试 `#[ignore]` 不跑）
- [ ] 15.5 `cargo run -p lucy-app` 在 Windows 上启动：WSL 已安装时自动检测为 WslHost，输入 WSL 仓库路径（如 `/home/user/project`）能打开仓库、列出 worktree、新建 worktree、起 WSL shell 终端
- [ ] 15.6 `cargo run -p lucy-app` 在无 WSL 的环境（或 macOS）启动：回退 LocalHost，行为与当前完全一致（回归零风险）
