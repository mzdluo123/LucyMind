## Why

LucyMind 当前所有命令（git、hook、shell）都直接在本机执行（`std::process::Command::new("git")`、`tty::new` 本地 PTY），没有任何抽象层。在 Windows 上，`claude`/`codex` 等 agent CLI 需要 Linux 环境运行（它们是 Node CLI，依赖 Unix shell 生态），而 WSL 是 Windows 上的原生 Linux 环境。引入一个 **Host 抽象层**（trait + LocalHost/WslHost 实现），让命令执行、文件操作、路径规范化都通过 Host 间接调用，即可让 LucyMind 在 Windows 上连接 WSL 跑 worktree + agent；同时为未来 SSH 远程开发铺路。本期（Phase 1）聚焦：基础 shell、项目打开、worktree 管理。

## What Changes

- **新增 `Host` trait（core 层）**：抽象「命令在哪执行」。定义 `run_command`（替代 `std::process::Command::new` 直调）、`canonicalize`、`exists`、`read_to_string`、`copy` 等方法。这是 core 层首个跨后端抽象。
- **新增 `LocalHost` 实现**：封装现有行为（`std::process::Command`、`std::fs`），保持 100% 向后兼容。所有现有调用点改为走 `LocalHost`。
- **新增 `WslHost` 实现**：通过 `wsl.exe` 执行命令（`wsl.exe --cd <cwd> -- <program> <args>`），路径用 Linux 风格（`/home/...`），shell 直接 spawn `wsl.exe`。
- **改造 git/hook/config 走 Host**：`run_git`、hook `run_command`、config `load`、`canon()` 等全部改为接收 `&dyn Host`（或泛型 `H: Host`），不再直接调 `std::process::Command` / `std::fs`。
- **改造 app 层**：`WorkspaceView` 持有 `Host` 实例；`set_repo` 用 Host 加载 config + 列 worktree；`spawn_shell_tab` 用 Host 的 shell 命令 spawn PTY；`count_uncommitted` 走 Host。
- **WSL 项目打开**：启动时检测 WSL 可用性；repo picker 支持 WSL 路径输入（Phase 1 用文本输入，不做文件浏览）。
- **WSL shell 启动**：`ShellKind` 增加 `Wsl` 变体（或由 Host 提供 default shell），`TerminalSession::spawn` 收到 `wsl.exe` 作为 command 即可在 WSL 内起交互式 shell。
- **路径处理**：WSL 路径用 `/` 分隔、无盘符；`canon()` 委托 Host 在目标环境内规范化（WSL 用 `realpath`，不用 Windows 的 `canonicalize` + `strip_verbatim_prefix`）。
- **session 注册表仍存本地**：`sessions.json` 路径不变（Windows 本地），但其中记录的 `path` 字段是 WSL 路径（`/home/...`）。

## Capabilities

### New Capabilities
- `remote-host`: Host 抽象 trait 及 LocalHost 实现——定义命令执行、文件操作、路径规范化的统一接口，将所有 `std::process::Command` / `std::fs` 直调改为通过 Host 间接调用。这是支撑所有远程后端（WSL/SSH）的基础设施层。
- `wsl-host`: WSL Host 实现——通过 `wsl.exe` 执行命令、WSL 路径规范、WSL shell 启动、WSL 项目打开（路径输入 + config 加载 + worktree 列表）。Phase 1 覆盖基础 shell、项目打开、worktree 管理。

### Modified Capabilities
（无——`openspec/specs/` 当前为空，无既有 capability 的需求被改动。）

## Impact

- **`crates/core/src/host.rs`**（新增）：`Host` trait 定义 + `LocalHost` 实现 + `CommandOutput`/`HostError` 类型。
- **`crates/core/src/git/mod.rs`**：`run_git` 签名加 `&dyn Host` 参数（或改 `pub`）；所有 git 函数透传 Host。
- **`crates/core/src/hooks/engine.rs`**：`run_event` / `run_command` / `copy_file` 改走 Host；`shell_command` 的 `sh`/`cmd` 分平台逻辑移入 Host。
- **`crates/core/src/config/mod.rs`**：`load` 改走 Host 的 `read_to_string`（或保持本地读取 + 由调用方传入内容，视 design 决定）。
- **`crates/core/src/session/mod.rs`**：无改动（注册表仍存本地）；`path` 字段类型不变（`PathBuf`，存 WSL 路径字符串）。
- **`crates/terminal/src/session.rs`**：无 trait 改动（PTY 仍由 alacritty `tty::new` spawn）；`spawn` 的 `command` 参数传入 `wsl.exe` 即可。
- **`crates/app/src/workspace/mod.rs`**：`WorkspaceView` 持有 `Host`；`canon()` 委托 Host；`set_repo` / `new_worktree` / `spawn_shell_tab` / `count_uncommitted` / `do_close` 全部走 Host。
- **`crates/app/src/workspace/sidebar.rs`**：repo picker 增加 WSL 路径输入入口。
- **`crates/app/src/lib.rs`**：启动时检测 WSL 可用性（`wsl.exe --status`），构造 `WslHost` 或 `LocalHost`。
- **`crates/app/src/path_env.rs`**：Unix PATH fix 在 WSL shell 内不适用（WSL shell 自带 login PATH）；Host 抽象后由 WslHost 决定是否需要。
- **测试**：core 层 Host trait 用 mock 实现（`MockHost`）单测 git/hook/config；app 层 `#[gpui::test]` 用 `LocalHost` 保持现有测试不变。
