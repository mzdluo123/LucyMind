## ADDED Requirements

### Requirement: WslHost executes commands via wsl.exe
A `WslHost` struct SHALL implement the `Host` trait by delegating all command execution and filesystem operations to `wsl.exe`. `run(cmd)` SHALL execute `wsl.exe [--cd <cwd>] -- [<env_prefix>] <program> <args>` where env vars are injected via `env K=V` prefix before the program. `run_shell(cwd, cmd, env)` SHALL execute `wsl.exe --cd <cwd> -- /bin/sh -c "<env_exports> <cmd>"` where env vars are injected as `export K='V';` prefix with single-quote escaping. All paths passed to WslHost SHALL be Linux-style (`/home/...`, `/` separator).

#### Scenario: WslHost runs git command
- **WHEN** `WslHost::run(HostCommand { program: "git", args: ["-C", "/home/user/repo", "worktree", "list", "--porcelain"], cwd: None, env: [] })` is called
- **THEN** it executes `wsl.exe -- git -C /home/user/repo worktree list --porcelain` and returns stdout/stderr/exit code

#### Scenario: WslHost runs shell command with env vars
- **WHEN** `WslHost::run_shell("/home/user/wt", "npm install", &[("WORKTREE_PATH", "/home/user/wt")])` is called
- **THEN** it executes `wsl.exe --cd /home/user/wt -- /bin/sh -c "export WORKTREE_PATH='/home/user/wt'; npm install"`

#### Scenario: WslHost injects env via export prefix
- **WHEN** `WslHost::run_shell` is called with env containing `("WORKTREE_BRANCH", "feature/x")`
- **THEN** the generated shell command string contains `export WORKTREE_BRANCH='feature/x';` before the user command

#### Scenario: WslHost escapes single quotes in env values
- **WHEN** `WslHost::run_shell` is called with an env value containing a single quote (e.g. `it's a path`)
- **THEN** the value is single-quote-escaped as `'it'\''s a path'` in the export prefix

### Requirement: WslHost filesystem operations via wsl.exe
WslHost SHALL implement filesystem operations by spawning `wsl.exe` with the corresponding Linux utility: `canonicalize` uses `realpath`, `exists` uses `test -e`, `read_to_string` uses `cat`, `write` uses `tee` or `cat >`, `copy` uses `cp`, `create_dir_all` uses `mkdir -p`. Each operation SHALL use the WSL path directly (no path translation).

#### Scenario: WslHost canonicalize uses realpath
- **WHEN** `WslHost::canonicalize("/home/user/repo/.")` is called
- **THEN** it executes `wsl.exe -- realpath /home/user/repo/.` and returns the resolved path (e.g. `/home/user/repo`)

#### Scenario: WslHost exists uses test
- **WHEN** `WslHost::exists("/home/user/repo/.gitmodules")` is called
- **THEN** it executes `wsl.exe -- test -e /home/user/repo/.gitmodules` and returns `true` if exit code is 0

#### Scenario: WslHost read_to_string uses cat
- **WHEN** `WslHost::read_to_string("/home/user/repo/.worktree.toml")` is called
- **THEN** it executes `wsl.exe -- cat /home/user/repo/.worktree.toml` and returns the file content as stdout

#### Scenario: WslHost write uses tee
- **WHEN** `WslHost::write("/home/user/repo/.worktree.toml", "content")` is called
- **THEN** it executes `wsl.exe -- tee /home/user/repo/.worktree.toml` with `content` piped to stdin

#### Scenario: WslHost copy uses cp
- **WHEN** `WslHost::copy("/home/user/repo/.env", "/home/user/wt/.env")` is called
- **THEN** it executes `wsl.exe -- cp /home/user/repo/.env /home/user/wt/.env`

### Requirement: WslHost default_shell returns wsl.exe
`WslHost::default_shell(cwd)` SHALL return `Some(("wsl.exe", ["--cd", cwd]))` so that `TerminalSession::spawn` starts an interactive WSL shell in the worktree directory. The terminal layer (alacritty `tty::new`) spawns `wsl.exe` as a local process via ConPTY, which connects to the WSL Linux environment.

#### Scenario: WslHost default_shell returns wsl.exe with cd
- **WHEN** `WslHost::default_shell("/home/user/wt")` is called
- **THEN** it returns `Some(("wsl.exe", vec!["--cd".to_string(), "/home/user/wt".to_string()]))`

#### Scenario: WslHost is_remote returns true
- **WHEN** `WslHost::is_remote()` is called
- **THEN** it returns `true`

### Requirement: WSL availability detection at startup
The app SHALL detect WSL availability at startup by running `wsl.exe --status`. If WSL is available (exit code 0), the app SHALL construct a `WslHost` as the default `Host`. If WSL is not available (exit code non-zero or `wsl.exe` not found), the app SHALL fall back to `LocalHost`. The detection SHALL be non-fatal — failure to detect WSL does not prevent the app from starting with `LocalHost`.

#### Scenario: WSL available constructs WslHost
- **WHEN** the app starts and `wsl.exe --status` exits with code 0
- **THEN** `WorkspaceView` is constructed with `Arc<dyn Host> = Arc::new(WslHost::default())`

#### Scenario: WSL unavailable falls back to LocalHost
- **WHEN** the app starts and `wsl.exe` is not found (or `wsl.exe --status` exits non-zero)
- **THEN** `WorkspaceView` is constructed with `Arc<dyn Host> = Arc::new(LocalHost)`

#### Scenario: Detection failure does not crash
- **WHEN** `wsl.exe` execution fails with an I/O error
- **THEN** the app logs a warning and falls back to `LocalHost`

### Requirement: WSL project opening via path input
When the Host is `WslHost`, the repo picker SHALL accept a WSL path entered as text (e.g. `/home/user/project`) in addition to the native directory picker for local paths. A path starting with `/` SHALL be treated as a WSL path and validated via `host.canonicalize` + `git::main_worktree_root(host, &path)`. The native directory picker (`cx.prompt_for_paths`) SHALL still be available for local paths via a "Browse..." button.

#### Scenario: WSL path input opens repo
- **WHEN** the Host is WslHost and the user enters `/home/user/myproject` in the path input and clicks "Open"
- **THEN** the app calls `host.canonicalize` on the path, then `git::main_worktree_root(host, &path)` to verify it is a git repo, and on success calls `set_repo` with the canonicalized path

#### Scenario: Invalid WSL path shows error
- **WHEN** the user enters `/home/user/not-a-repo` and clicks "Open"
- **THEN** `git::main_worktree_root` returns None and the status bar shows "所选目录不是 git 仓库"

#### Scenario: Native picker still works for local paths
- **WHEN** the Host is WslHost and the user clicks "Browse..." and selects a local Windows directory
- **THEN** the app treats it as a local path (not a WSL path) and opens it with the LocalHost, or shows an error indicating local paths are not supported in WSL mode

### Requirement: WSL shell spawning
When the Host is `WslHost`, `spawn_shell_tab` SHALL use `host.default_shell(wt_path)` as the terminal command instead of `ShellKind::command()`. The resulting `("wsl.exe", ["--cd", wt_path])` is passed to `TerminalView::new` as the `command` parameter, with `working_directory` set to `None` (the cwd is set by `wsl.exe --cd` inside WSL, not by the local PTY). The `TERM=xterm-256color` and `WORKTREE_*` env vars SHALL be injected into the WSL shell environment.

#### Scenario: WSL shell spawns with correct command
- **WHEN** `spawn_shell_tab` is called with a WslHost and worktree path `/home/user/wt`
- **THEN** `TerminalView::new` receives `command = Some(("wsl.exe", ["--cd", "/home/user/wt"]))` and `working_directory = None`

#### Scenario: WSL shell env vars are injected
- **WHEN** a shell tab is spawned in a WSL worktree `/home/user/wt` on branch `lucy/feature`
- **THEN** the WSL shell environment includes `TERM=xterm-256color`, `WORKTREE_PATH=/home/user/wt`, `WORKTREE_BRANCH=lucy/feature`, `WORKTREE_NAME=lucy-feature`, `REPO_ROOT=<repo_root>`

### Requirement: WSL worktree management
When the Host is `WslHost`, all git worktree operations (`add`, `remove`, `list`, `lock`, `unlock`) SHALL execute via `host.run` (i.e. `wsl.exe -- git -C <repo> <args>`), with paths in Linux format. Worktree paths created by `git::sibling_worktree_path` SHALL use Linux path conventions (parent dir from config, `/` separator). The `canon()` function SHALL delegate to `host.canonicalize` (WSL `realpath`) instead of `Path::canonicalize`.

#### Scenario: WSL worktree creation
- **WHEN** `new_worktree` is called with a WslHost and repo `/home/user/project`
- **THEN** `git::add(host, "/home/user/project", "/home/user/project-worktrees/lucy-xxx", mode)` executes `wsl.exe -- git -C /home/user/project worktree add -b lucy/xxx /home/user/project-worktrees/lucy-xxx`

#### Scenario: WSL worktree listing
- **WHEN** `set_repo` is called with a WslHost and repo `/home/user/project`
- **THEN** `git::list(host, "/home/user/project")` executes `wsl.exe -- git -C /home/user/project worktree list --porcelain` and parses the output into `WorktreeEntry` list

#### Scenario: WSL path canonicalization
- **WHEN** `canon("/home/user/project/.")` is called with a WslHost
- **THEN** it calls `host.canonicalize("/home/user/project/.")` which executes `wsl.exe -- realpath /home/user/project/.` and returns `/home/user/project`

### Requirement: WSL hook execution
When the Host is `WslHost`, hook commands (`post_create`, `pre_remove`) SHALL execute via `host.run_shell` (i.e. `wsl.exe --cd <worktree> -- /bin/sh -c "export K='V'; ...; <cmd>"`). The `[copy]` file copies SHALL execute via `host.copy` (i.e. `wsl.exe -- cp <src> <dst>`). Hook context env vars (`WORKTREE_PATH`, `WORKTREE_BRANCH`, `WORKTREE_NAME`, `REPO_ROOT`) SHALL be injected into the WSL shell environment via `export` prefix.

#### Scenario: WSL post_create hook runs shell command
- **WHEN** a `post_create` hook command `npm install` is run with a WslHost in worktree `/home/user/wt`
- **THEN** it executes `wsl.exe --cd /home/user/wt -- /bin/sh -c "export WORKTREE_PATH='/home/user/wt'; export WORKTREE_BRANCH='lucy/feature'; export WORKTREE_NAME='lucy-feature'; export REPO_ROOT='/home/user/project'; npm install"`

#### Scenario: WSL hook copy uses cp
- **WHEN** a `[copy]` entry `.env` is processed with repo_root `/home/user/project` and worktree `/home/user/wt`
- **THEN** it executes `wsl.exe -- cp /home/user/project/.env /home/user/wt/.env` (after checking `host.exists` on the source)

### Requirement: WSL config loading and writing
When the Host is `WslHost`, `config::load(host, path)` SHALL read `.worktree.toml` via `host.read_to_string` (i.e. `wsl.exe -- cat <path>`). `config::set_alias` and `config::set_worktree_settings` SHALL read via `host.read_to_string`, modify with `toml_edit`, and write back via `host.write` (i.e. `wsl.exe -- tee <path>` with content piped to stdin).

#### Scenario: WSL config load
- **WHEN** `config::load(wsl_host, "/home/user/project/.worktree.toml")` is called
- **THEN** it calls `host.read_to_string` which executes `wsl.exe -- cat /home/user/project/.worktree.toml` and parses the returned content as TOML

#### Scenario: WSL config write
- **WHEN** `config::set_alias(wsl_host, "/home/user/project/.worktree.toml", "lucy/feature", "my-alias")` is called
- **THEN** it reads the file via `host.read_to_string`, modifies the `[alias]` table with `toml_edit`, and writes back via `host.write` which pipes the content to `wsl.exe -- tee /home/user/project/.worktree.toml`

### Requirement: Session registry stores WSL paths locally
The session registry (`sessions.json`) SHALL continue to persist on the local machine (via `directories::ProjectDirs`), not via the Host. The `Session.path` field SHALL store the WSL path as a string (e.g. `/home/user/project-worktrees/lucy-xxx`). The `Registry` repo key SHALL be the WSL repo root path string, naturally distinguishing WSL repos from local repos.

#### Scenario: WSL session registered with WSL path
- **WHEN** a worktree is created at `/home/user/project-worktrees/lucy-xxx` in a WSL repo `/home/user/project`
- **THEN** `registry.register("/home/user/project", Session { path: "/home/user/project-worktrees/lucy-xxx", .. })` stores the entry under key `/home/user/project`

#### Scenario: WSL session persists locally
- **WHEN** `registry.save_default()` is called on Windows
- **THEN** the JSON file is written to `%APPDATA%\rainchan\LucyMind\sessions.json` (local filesystem, not via Host)

### Requirement: Windows-specific shell variants hidden in WSL mode
When the Host is `WslHost`, the `ShellKind::Cmd`, `ShellKind::PowerShell`, and `ShellKind::Pwsh` variants SHALL NOT appear in the launcher menu's New Tab options. Only `ShellKind::Default` (which spawns `wsl.exe`) SHALL be available. When the Host is `LocalHost` on Windows, all variants SHALL appear as before.

#### Scenario: WSL mode shows only Default shell
- **WHEN** the launcher menu is rendered with a WslHost
- **THEN** the New Tab menu shows only "Shell" (Default), not "Command Prompt"/"PowerShell"/"PowerShell 7"

#### Scenario: Local mode on Windows shows all shells
- **WHEN** the launcher menu is rendered with a LocalHost on Windows
- **THEN** the New Tab menu shows "Shell", "Command Prompt", "PowerShell", and "PowerShell 7" as before
