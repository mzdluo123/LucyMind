## Context

LucyMind 当前所有命令执行都是本机直调：`core/git/mod.rs:35` 的 `run_git` 用 `Command::new("git")`，`hooks/engine.rs:156` 的 `shell_command` 用 `Command::new("sh"|"cmd")`，`terminal/session.rs:180` 用 alacritty 的 `tty::new` 起本地 PTY，`workspace/mod.rs:78` 的 `count_uncommitted` 也是直调 `Command::new("git")`。config 读写（`config/mod.rs:32,84,106,155,191`）、路径规范化（`canon()` at `workspace/mod.rs:51`）、hook 文件复制（`hooks/engine.rs:107`）全部走 `std::fs`。**没有任何 trait 抽象「命令在哪执行」**——这是支撑远程后端的核心缺口。

在 Windows 上，`claude`/`codex`/`opencode` 等 agent CLI 依赖 Unix shell 生态（Node CLI、PATH 解析、sh 脚本），WSL 是 Windows 上的原生 Linux 环境。通过引入 Host 抽象层，让命令执行、文件操作、路径规范化都通过 Host 间接调用，即可让 LucyMind 连接 WSL 跑 worktree + agent；同时为未来 SSH 远程开发铺路（SshHost 实现同一 trait）。

**关键约束**：
- alacritty 的 `tty::new` 是本地 PTY spawner（Unix rustix-openpty / Windows ConPTY），不支持远程 PTY。但 `tty::new` 可以 spawn 任意二进制——包括 `wsl.exe`。所以终端层不需要 trait，只需把 `wsl.exe` 作为 shell command 传入。
- `WorkspaceView` 中的 git 操作有同步调用（`new_worktree`、`set_repo`）和后台调用（`do_close` 的 `git::unlock`+`git::remove` 在 `background_executor` 里跑）。Host 必须能跨线程使用（`Send + Sync`），后台任务需 clone Host。
- `config::set_alias` / `set_worktree_settings` 用 `toml_edit` 做格式保留改写——先读全文、改 DOM、写回。读写都需走 Host（文件在 WSL 内）。
- WSL 路径是 Linux 风格（`/home/user/project`），不含盘符、用 `/` 分隔。Windows 的 `Path::canonicalize` 返回 `\\?\` verbatim 前缀，WSL 路径不需要这个处理。

## Goals / Non-Goals

**Goals:**
- 在 core 层引入 `Host` trait，抽象命令执行 + 文件操作 + 路径规范化。所有 `std::process::Command` 直调和 `std::fs` 直调改为通过 Host。
- `LocalHost` 实现 100% 保持现有行为（回归零风险）。
- `WslHost` 实现：通过 `wsl.exe` 执行命令、用 `realpath` 规范化路径、用 `cat`/`tee` 读写文件、用 `cp` 复制文件。
- WSL shell 启动：`TerminalSession::spawn` 收到 `("wsl.exe", ["--cd", wt_path])` 作为 command 即可在 WSL 内起交互式 shell，终端层不改。
- WSL 项目打开：启动时检测 WSL 可用性，repo picker 支持文本输入 WSL 路径（如 `/home/user/project`）。
- WSL worktree 管理：`git add`/`remove`/`list`/`lock`/`unlock` 全部通过 WslHost 在 WSL 内执行。
- WSL hook 执行：`post_create`/`pre_remove` 命令通过 WslHost 在 WSL 内执行（`sh -c`）。
- 为未来 `SshHost` 铺路：trait 设计不 WSL-specific，方法语义与传输无关。

**Non-Goals:**
- 不实现 SSH 后端（Phase 2）。
- 不做 WSL 发行版选择器 UI（Phase 1 用默认发行版 `wsl.exe` 不带 `--distribution` flag）。
- 不做 WSL 文件浏览器（Phase 1 用文本输入 WSL 路径）。
- 不改终端层（`session.rs` / `terminal_view.rs`）——PTY 仍由 alacritty `tty::new` 本地 spawn，只是 spawn 的二进制是 `wsl.exe`。
- 不改 session 注册表持久化（`sessions.json` 仍存 Windows 本地，`path` 字段存 WSL 路径字符串）。
- 不改 `path_env.rs`（WSL shell 自带 login PATH；Windows 本地进程只需找 `wsl.exe`，它在 `System32` 始终在 PATH）。
- 不做 `reveal_in_file_manager` 的 WSL 版（Phase 1 该功能在 WSL 模式下禁用或 no-op）。
- 不改 agent 启动方式——agent 命令仍由 `send_agent_command` 写入 shell PTY（WSL shell 是 bash/zsh，命令字符串直接可用）。

## Decisions

### D1: `Host` trait 放 core 层，用 `&dyn Host` 线程化

在 `crates/core/src/host.rs` 新增 trait：

```rust
pub trait Host: Send + Sync {
    fn run(&self, cmd: HostCommand) -> Result<HostOutput, HostError>;
    fn run_shell(&self, cwd: &Path, cmd: &str, env: &[(String, String)]) -> Result<HostOutput, HostError>;
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, HostError>;
    fn exists(&self, path: &Path) -> bool;
    fn read_to_string(&self, path: &Path) -> Result<String, HostError>;
    fn write(&self, path: &Path, content: &str) -> Result<(), HostError>;
    fn copy(&self, from: &Path, to: &Path) -> Result<(), HostError>;
    fn create_dir_all(&self, path: &Path) -> Result<(), HostError>;
    fn default_shell(&self, cwd: &Path) -> Option<(String, Vec<String>)>;
    fn is_remote(&self) -> bool;
}
```

- `run`：执行程序 + 参数（含可选 cwd 和 env），返回 stdout/stderr/exit_code。替代 `run_git` 的 `Command::new("git")`、`count_uncommitted` 的 `Command::new("git")`。
- `run_shell`：执行 shell 命令字符串（`sh -c` / `cmd /C`），带 cwd 和 env。替代 hooks `engine.rs` 的 `run_command` + `shell_command`。
- `canonicalize`：替代 `canon()` 的 `Path::canonicalize` + `strip_verbatim_prefix`。
- `exists`/`read_to_string`/`write`/`copy`/`create_dir_all`：替代 `std::fs` 直调。
- `default_shell(cwd)`：返回终端 spawn 用的 `(program, args)`。LocalHost 返回 `None`（系统默认 shell）；WslHost 返回 `("wsl.exe", ["--cd", cwd])`。
- `is_remote()`：`false` for LocalHost，`true` for WslHost。app 层用来决定是否禁用 `reveal_in_file_manager` 等本地功能。

`HostCommand` / `HostOutput` 是普通 struct（`program: String, args: Vec<String>, cwd: Option<PathBuf>, env: Vec<(String,String)>` / `stdout: String, stderr: String, success: bool`）。

**线程化方式**：`WorkspaceView` 持有 `Arc<dyn Host>`。所有 `git::*` 函数签名加 `host: &dyn Host` 参数。hooks `run_event` 加 `host: &dyn Host`。config `load`/`set_alias`/`set_worktree_settings` 加 `host: &dyn Host`。后台任务（`do_close`）clone `Arc<dyn Host>` 移入 async block。

**备选（否决）**：泛型 `H: Host`——infects 所有签名（`git::add<H: Host>(...)`, `hooks::run_event<H: Host>(...)` 等），且 `WorkspaceView` 需要泛型化或用 `dyn`。trait object 更简洁，性能差异可忽略（git 操作是瓶颈，不是虚函数调用）。

**备选（否决）**：enum `Host { Local, Wsl }`——不可扩展（加 SSH 要改 enum + 所有 match）。

### D2: `LocalHost` 封装现有行为

`LocalHost` 是零大小 struct（ZST），`Clone`/`Copy` trivially。每个方法直接调 `std::process::Command` / `std::fs`：
- `run`：`Command::new(cmd.program).current_dir(cwd).args(args).envs(env).output()`
- `run_shell`：`#[cfg(unix)]` 用 `sh -c`、`#[cfg(windows)]` 用 `cmd /C`（移自 `hooks/engine.rs:154-167` 的 `shell_command`）
- `canonicalize`：`Path::canonicalize` + `strip_verbatim_prefix`（移自 `workspace/mod.rs:51-69` 的 `canon`）
- `exists`/`read_to_string`/`write`/`copy`/`create_dir_all`：直调 `std::fs`
- `default_shell`：返回 `None`（系统默认 shell，alacritty tty 层决定）
- `is_remote`：`false`

`LocalHost` 的 `run_shell` 继承现有 `shell_command` 的平台分叉逻辑（Unix `sh -c` / Windows `cmd /C`），保持 hook 执行行为不变。

### D3: `WslHost` 通过 `wsl.exe` 执行命令

`WslHost` 持有可选 distro 名（Phase 1 为 `None` = 默认发行版）。所有命令通过 `wsl.exe` 执行：
- `run(cmd)`：`wsl.exe [--cd <cwd>] -- <program> <args>`。env 通过 `env K=V` 前缀注入（`wsl.exe --cd <cwd> -- env K1=V1 K2=V2 <program> <args>`）。cwd 是 WSL 路径（`/home/...`），`--cd` 让 wsl.exe 先切到该目录再执行。
- `run_shell(cwd, cmd, env)`：`wsl.exe --cd <cwd> -- /bin/sh -c "<cmd>"`，env 通过在 sh -c 字符串前加 `export K=V;` 注入（单引号转义值）。
- `canonicalize(path)`：`wsl.exe -- realpath <path>`，取 stdout trim。
- `exists(path)`：`wsl.exe -- test -e <path>`，退出码 0 = 存在。
- `read_to_string(path)`：`wsl.exe -- cat <path>`，取 stdout。
- `write(path, content)`：`wsl.exe -- /bin/sh -c 'cat > <path>'`，stdin 喂 content。或 `wsl.exe -- tee <path>`，stdin 喂 content。
- `copy(from, to)`：`wsl.exe -- cp <from> <to>`。
- `create_dir_all(path)`：`wsl.exe -- mkdir -p <path>`。
- `default_shell(cwd)`：`Some(("wsl.exe", vec!["--cd".into(), cwd.to_string_lossy().into()]))`。
- `is_remote`：`true`。

**路径传递**：`wsl.exe --cd <linux_path>` 接受 Linux 路径。`wsl.exe` 的 `--` 后参数直接传给 WSL 进程，路径用 Linux 格式。`HostCommand.cwd` 是 `Option<PathBuf>`——对于 git 操作（`git -C <repo>`），cwd 为 None（`-C` flag 已处理目录）；对于 hook 执行，cwd 是 worktree 路径。

**env 注入**：`wsl.exe` 不转发 Windows 环境变量到 WSL（除非设 `WSLENV`）。`run_shell` 用 `export K=V;` 前缀注入 hook 环境变量（`WORKTREE_PATH` 等），值用单引号转义防注入。`run` 方法对 git 操作不需要 env（git 不依赖 `WORKTREE_*` 变量）。

**备选（否决）**：用 `\\wsl$\` UNC 路径从 Windows 直接读写 WSL 文件——性能差、权限问题、不可移植到 SSH，违背 remote-dev 设计。

**备选（否决）**：`wsl.exe --exec` 代替 `wsl.exe --`——`--exec` 不启动 shell（不读 profile），但 `--` 更通用且 hook 需要 shell 语义。

### D4: 终端 shell 启动不改终端层

`TerminalSession::spawn(dimensions, working_directory, command, env)` 已接受 `command: Option<(String, Vec<String>)>`。WslHost 的 `default_shell(cwd)` 返回 `("wsl.exe", ["--cd", cwd])`，app 层把它作为 `command` 传给 `TerminalView::new`，`working_directory` 传 None（cwd 由 `wsl.exe --cd` 在 WSL 内设置，不是 Windows cwd）。

alacritty 的 `tty::new` 在 Windows 上用 ConPTY spawn `wsl.exe`，`wsl.exe` 连接 WSL Linux 环境，交互式 shell 在 ConPTY 内运行。输入/输出通过 ConPTY 正常流转，terminal_view 的渲染/输入/IME/鼠标/复制全部不改。

**env 注入到终端**：`spawn_shell_tab` 当前注入 `TERM=xterm-256color` + `WORKTREE_*` 环境变量。对于 WSL，这些 env 需要在 WSL shell 内设置。方式：`wsl.exe` 支持 `--set-env VAR=VAL` flag（WSL 2.0+），或在 shell 启动后由 `send_text` 发送 `export VAR=VAL\r`。Phase 1 用 `wsl.exe --cd <cwd> -- env TERM=xterm-256color WORKTREE_PATH=... /bin/sh`（或 bash/zsh）。实际上 `wsl.exe` 默认启动 login shell 已设 `TERM`，只需注入 `WORKTREE_*`。

### D5: config 读写走 Host

`config::load(path)` 改为 `config::load(host: &dyn Host, path: &Path)`，内部用 `host.read_to_string(path)` 替代 `std::fs::read_to_string`。

`config::set_alias` / `set_worktree_settings` 改为接收 `host: &dyn Host`，用 `host.read_to_string` + `host.write` 替代 `std::fs::read_to_string` + `std::fs::write`。`toml_edit` 的 DOM 操作不变（纯内存）。

`create_dir_all` 调用（`set_alias:103`、`set_worktree_settings:188`）改走 `host.create_dir_all`。

### D6: session 注册表仍存本地

`session::Registry` 的 `default_path()` / `load` / `save` 不走 Host——注册表是 LucyMind 自身的运行时状态，存在 Windows 本地（`directories::ProjectDirs` 路径）。`Session.path` 字段存 WSL 路径字符串（`/home/user/project-worktrees/lucy-xxx`），类型仍是 `PathBuf`（`PathBuf` 不关心分隔符，只是字节容器）。

`registry.register(repo, session)` 的 `repo` 参数是 WSL 路径，作为 `BTreeMap` key 的字符串。这天然区分本地仓库和 WSL 仓库（本地仓库 key 是 `C:\...` 或 `/Users/...`，WSL 仓库 key 是 `/home/...`）。

### D7: `canon()` 委托 Host

`workspace/mod.rs:51-69` 的 `canon()` 改为 `host.canonicalize(p)`。`strip_verbatim_prefix` 移入 `LocalHost::canonicalize`（Windows-specific），WslHost 用 `realpath`（无 verbatim 前缀问题）。

`same_path(a, b)` 改为先 `host.canonicalize` 再比较。所有 `canon()` 调用点（`set_repo:275`、`new_worktree:475`、`do_close:591`、`spawn_shell_tab` 等）传 Host 引用。

### D8: hook 执行走 Host

`hooks/engine.rs` 的 `run_event` 加 `host: &dyn Host` 参数。`copy_file` 改用 `host.exists` + `host.create_dir_all` + `host.copy`。`run_command` 改用 `host.run_shell`（替代 `shell_command` + `Command::output`）。`HookContext` 不变（env vars 仍由 `env_vars()` 生成，传给 `host.run_shell`）。

### D9: WSL 检测与 Host 构造

`lib.rs::run()` 启动时检测 WSL 可用性：
1. `Command::new("wsl.exe").arg("--status").output()` —— 退出码 0 = WSL 已安装且有发行版。
2. WSL 可用 → 构造 `Arc<dyn Host> = Arc::new(WslHost::default())`。
3. WSL 不可用 → 构造 `Arc<dyn Host> = Arc::new(LocalHost)`。
4. `WorkspaceView::new(cx, candidate, host)` 接收 Host。

Phase 1 自动检测，不提供 UI 切换。用户通过 WSL 路径输入触发 WSL 模式（`is_remote()` 返回 true 后 UI 适配）。

**备选（否决）**：手动切换 Local/WSL 模式——Phase 1 自动检测更简单，且 WSL 不可用时回退 LocalHost 无副作用。

### D10: WSL 项目打开（文本路径输入）

`WorkspaceView::open_repo_picker` 当前用 `cx.prompt_for_paths`（原生目录选择器）。Phase 1 新增 WSL 路径输入入口：
- 侧边栏「Open Repository」按钮改为弹出一个小弹窗（复用 `ui/dialog.rs` 的 modal 机制），含一个文本输入框（placeholder `/home/user/project`）+ 「Open」按钮。
- 用户输入 WSL 路径 → `host.canonicalize` → `git::main_worktree_root(host, &path)` 验证 → `set_repo(host, path)`。
- 本地路径仍用原生目录选择器（「Browse...」按钮）。
- WSL 路径以 `/` 开头判定为 WSL 路径（`path.starts_with("/")`）。

**备选（否决）**：用 `wsl.exe -- ls` 做文件浏览器——Phase 1 不需要，文本输入够用。

### D11: `ShellKind` 在 WSL 模式下的行为

`ShellKind` enum 增加 `Default` 的语义变化：当 Host 是 WslHost 时，`Default` 的 `command()` 返回 `host.default_shell(cwd)`（即 `("wsl.exe", ["--cd", cwd])`）。具体实现：`spawn_shell_tab` 不再用 `ShellKind::command()`，改为 `host.default_shell(wt_path)`。Windows-specific 的 `Cmd`/`PowerShell`/`Pwsh` 变体在 WSL 模式下隐藏（tabs 菜单不显示）。

## Risks / Trade-offs

- **[每次 Host 操作 spawn 一个 wsl.exe 进程]** → Phase 1 可接受（git 操作本身是秒级，wsl.exe 启动 ~100ms）。未来可优化：`WslHost` 持有一个长期 `wsl.exe` 进程做 shell server（类似 SSH ControlMaster），但增加复杂度。
- **[env 注入用 `export K=V;` 前缀，值含单引号需转义]** → `run_shell` 实现单引号转义（`'` → `'\''`）。hook 环境变量值是路径和分支名，含单引号概率极低，但仍需正确处理。
- **[config 读写各 spawn 一次 wsl.exe（读 cat + 写 tee）]** → `set_alias`/`set_worktree_settings` 的读-改-写需两次 wsl.exe 调用。Phase 1 可接受（设置面板操作不频繁）。
- **[WSL 路径在 `PathBuf` 中混用分隔符]** → `PathBuf` 在 Windows 上用 `\` 分隔，WSL 路径用 `/`。`PathBuf::join("/home", "user")` 在 Windows 上可能产生异常路径。需确保 WSL 路径始终用 `PathBuf::from("/home/user/project")` 构造，不用 `join` 拼接（或用 `format!` 拼接字符串再 `PathBuf::from`）。
- **[`count_uncommitted` 当前在 app 层直调 git（bypass core）]** → 改为走 `host.run`（或移入 core 的 `git` 模块），统一 Host 线程化。
- **[WSL 默认发行版可能不是用户期望的]** → Phase 1 不做 distro 选择，用 `wsl.exe` 默认发行版。`wsl.exe --status` 检测可用性。未来加 `--distribution <name>` 支持。
- **[LocalHost 回归风险]** → LocalHost 方法逐个对应现有行为，用现有单测覆盖（`git_test`、`hooks_test`、`config_test` 全部用 LocalHost 跑）。新增 `MockHost`（内存实现）用于隔离单测。
