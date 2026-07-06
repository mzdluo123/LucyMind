## ADDED Requirements

### Requirement: Host trait abstracts command execution and filesystem operations
The core layer SHALL define a `Host` trait that abstracts where commands run and where filesystem operations occur. The trait SHALL include methods for: running a program with arguments (`run`), running a shell command string with environment variables (`run_shell`), canonicalizing a path (`canonicalize`), checking path existence (`exists`), reading a file as string (`read_to_string`), writing a file (`write`), copying a file (`copy`), creating directories recursively (`create_dir_all`), providing the default shell command for terminal spawning (`default_shell`), and reporting whether the host is remote (`is_remote`). The trait SHALL require `Send + Sync` so it can be used across threads (background git operations). All `std::process::Command::new` direct calls in `core/git`, `core/hooks`, and `app/workspace` SHALL be replaced with `host.run()` or `host.run_shell()`. All `std::fs` direct calls in `core/config`, `core/hooks` (copy), and `app/workspace` (`canon`, `count_uncommitted`) SHALL be replaced with the corresponding `Host` method.

#### Scenario: git operations go through Host
- **WHEN** `git::add` is called with a `LocalHost`
- **THEN** it invokes `host.run(HostCommand { program: "git", args: ["-C", repo, "worktree", "add", ...], .. })` and `LocalHost` executes `Command::new("git").arg("-C").arg(repo).args(args).output()`

#### Scenario: hook commands go through Host
- **WHEN** `hooks::run_event` is called with a `LocalHost`
- **THEN** each hook command string is executed via `host.run_shell(cwd, cmd, env)` and `LocalHost` wraps it in `sh -c` (Unix) or `cmd /C` (Windows) with env vars set via `Command::env()`

#### Scenario: config read goes through Host
- **WHEN** `config::load` is called with a `LocalHost`
- **THEN** it invokes `host.read_to_string(path)` and `LocalHost` calls `std::fs::read_to_string(path)`

#### Scenario: path canonicalization goes through Host
- **WHEN** `canon(path)` is called with a `LocalHost` on Windows
- **THEN** it invokes `host.canonicalize(path)` which calls `Path::canonicalize` and strips the `\\?\` verbatim prefix

#### Scenario: Host is Send + Sync for background tasks
- **WHEN** `do_close` spawns a background task to run `git::unlock` and `git::remove`
- **THEN** the `Host` (wrapped in `Arc`) is cloned and moved into the async block, and `git::unlock`/`git::remove` receive `&*arc` as `&dyn Host`

### Requirement: LocalHost preserves existing behavior
A `LocalHost` struct SHALL implement the `Host` trait by delegating to `std::process::Command` and `std::fs`, matching the exact behavior of the current direct calls. `run` SHALL use `Command::new(program).current_dir(cwd).args(args).envs(env).output()`. `run_shell` SHALL use `sh -c` on Unix and `cmd /C` on Windows (moved from `hooks/engine.rs::shell_command`). `canonicalize` SHALL use `Path::canonicalize` + `strip_verbatim_prefix` (moved from `workspace/mod.rs::canon`). `default_shell` SHALL return `None` (system default shell). `is_remote` SHALL return `false`.

#### Scenario: LocalHost run matches current git execution
- **WHEN** `LocalHost::run(HostCommand { program: "git", args: ["-C", "/repo", "status", "--porcelain"], cwd: None, env: [] })` is called
- **THEN** it executes `Command::new("git").arg("-C").arg("/repo").args(["status", "--porcelain"]).output()` and returns stdout/stderr/exit code

#### Scenario: LocalHost run_shell matches current hook execution on Unix
- **WHEN** `LocalHost::run_shell("/worktree", "echo hi", &[("WORKTREE_PATH", "/worktree")])` is called on Unix
- **THEN** it executes `Command::new("sh").arg("-c").arg("echo hi").current_dir("/worktree").env("WORKTREE_PATH", "/worktree").output()`

#### Scenario: LocalHost default_shell returns None
- **WHEN** `LocalHost.default_shell("/some/path")` is called
- **THEN** it returns `None`, meaning the terminal layer uses the system default shell

#### Scenario: LocalHost is_remote returns false
- **WHEN** `LocalHost.is_remote()` is called
- **THEN** it returns `false`

### Requirement: Host threaded through all command execution sites
The `Host` SHALL be threaded through all functions that currently call `std::process::Command` or `std::fs` directly. In `core/git`: `run_git`, `add`, `remove`, `list_worktrees`, `lock`, `unlock`, `prune`, `toplevel`, `main_worktree_root`, `branch_exists`, `branch_checked_out_at`, `has_uncommitted_changes` SHALL accept `&dyn Host`. In `core/hooks`: `run_event`, `copy_file`, `run_command` SHALL accept `&dyn Host`. In `core/config`: `load`, `set_alias`, `set_worktree_settings` SHALL accept `&dyn Host`. In `app/workspace`: `canon`, `count_uncommitted`, `set_repo`, `new_worktree`, `spawn_shell_tab`, `do_close`, `request_close` SHALL use the `WorkspaceView`'s `Host` instance.

#### Scenario: run_git accepts Host parameter
- **WHEN** `git::list_worktrees` is called
- **THEN** its signature includes `host: &dyn Host` and it calls `host.run(...)` instead of `Command::new("git")`

#### Scenario: hooks run_event accepts Host parameter
- **WHEN** `hooks::run_event` is called
- **THEN** its signature includes `host: &dyn Host` and `copy_file`/`run_command` use `host.copy`/`host.run_shell`

#### Scenario: WorkspaceView holds a Host instance
- **WHEN** `WorkspaceView::new` is called
- **THEN** it receives and stores an `Arc<dyn Host>`, and all subsequent git/hook/config operations use this Host instance

### Requirement: MockHost for unit testing
A `MockHost` (or test-only Host implementation) SHALL be available in `core` tests to verify git/hook/config logic without spawning real processes. It SHALL record commands executed and allow asserting on program/args/cwd/env. It SHALL return configurable outputs (stdout/stderr/exit code) and simulate filesystem operations in memory.

#### Scenario: MockHost records git commands
- **WHEN** `git::add(mock_host, repo, path, mode)` is called
- **THEN** `mock_host.commands()` contains an entry with `program == "git"` and the expected args

#### Scenario: MockHost returns configurable output
- **WHEN** `MockHost` is configured to return stdout `"worktree /repo\nHEAD 0000\n"` for `git worktree list --porcelain`
- **THEN** `git::list_worktrees(mock_host, repo)` returns a `Vec<WorktreeEntry>` parsed from that stdout
