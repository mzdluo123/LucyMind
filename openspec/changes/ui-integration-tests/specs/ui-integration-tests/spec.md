## ADDED Requirements

### Requirement: App crate lib+bin separation for testability
The app crate SHALL expose a library target (`lucy_app`) alongside the existing binary, so that integration tests under `crates/app/tests/` can import the app's public types (`WorkspaceView`, `TerminalView`, modules). `src/lib.rs` SHALL re-export the existing modules (`workspace`, `terminal_view`, `theme`, `ui`, `assets`) and a `pub fn run()` that encapsulates the `Application::new().with_assets(Assets).run(...)` startup logic. `src/main.rs` SHALL be reduced to `fn main() { lucy_app::run() }`. The production binary behavior SHALL remain unchanged.

#### Scenario: Library target compiles and is importable
- **WHEN** `cargo build -p lucy-app` is run
- **THEN** both the `lucy` binary and the `lucy_app` library target compile successfully

#### Scenario: Binary behavior unchanged
- **WHEN** `cargo run -p lucy-app` is run from a git repository
- **THEN** the window opens and `WorkspaceView` initializes identically to before the separation (same window title, same startup flow, same `gpui_component::init` + `Root` wrapping)

### Requirement: Test dependencies and harness
The app crate SHALL declare `[dev-dependencies]` with `gpui = { version = "0.2.2", features = ["test-support"] }` (unlocking `TestAppContext`, `#[gpui::test]`, `VisualTestContext`) and `tempfile = "3"`. A shared test harness SHALL live at `crates/app/tests/common/mod.rs` providing: `temp_repo()` (tempdir + `git init` + empty commit), `build_workspace(cx, repo)` (`gpui_component::init` + `Root` wrapping + `WorkspaceView::new`), `wait_for(cx, predicate, timeout)` (loop `run_until_parked` + check), and `shutdown_workspace` (drop terminals + drain to avoid leak-detection false positives).

#### Scenario: Harness constructs a workspace headlessly
- **WHEN** a `#[gpui::test]` test calls `build_workspace(&mut cx, Some(temp_repo))`
- **THEN** it returns an `Entity<WorkspaceView>` inside a `Root`-wrapped window, with `gpui_component::init` already called, without opening a real OS window or requiring a GPU

#### Scenario: Temp repo is isolated and git-initialized
- **WHEN** `temp_repo()` is called
- **THEN** it returns a `TempDir` whose path is a valid git repository (`.git` exists, `git rev-parse --show-toplevel` succeeds) with at least one commit, and is distinct from the host repository

#### Scenario: Async tasks are drained before assertion
- **WHEN** a test triggers an action that spawns a `cx.spawn` task (e.g. `new_worktree_and_agent`)
- **THEN** `wait_for(cx, predicate, timeout)` calls `run_until_parked` in a loop until the predicate holds or the timeout elapses, so background git/PTY operations complete before state assertions

### Requirement: Test accessors are cfg-gated
`WorkspaceView` and `TerminalView` SHALL expose `#[cfg(test)] pub fn` accessors for the state that integration tests need to observe. These accessors SHALL NOT be present in release builds. Accessors SHALL include at minimum: `active_path`, `worktree_count`, `worktree_paths`, `is_agent_menu_open`, `current_status`, `has_pending_close`, `pending_close_branch`, `terminals_contains`, `settings_open`, `editing_alias` (on `WorkspaceView`); `snapshot_text`, `is_exited`, `selection_text`, `has_selection`, `dimensions` (on `TerminalView`).

#### Scenario: Accessors compile only in test builds
- **WHEN** `cargo build -p lucy-app` (non-test) is run
- **THEN** the `#[cfg(test)]` accessors are absent from the binary (no symbol bloat, no internal exposure)

#### Scenario: Accessors available in tests
- **WHEN** `cargo test -p lucy-app` is run
- **THEN** the accessors are callable from `crates/app/tests/` integration tests

### Requirement: App startup state machine coverage
The test suite SHALL cover `WorkspaceView::new` for three startup paths: (1) candidate is a git repository → worktree list loaded with main row, `active_path` points to main, no error status; (2) candidate is `None` → empty state, `open_repo_picker` triggers a native path prompt (`has_pending_prompt` true); (3) candidate is a non-git directory → empty state with prompt.

#### Scenario: Startup with a git repository
- **WHEN** `build_workspace(cx, Some(temp_repo))` constructs a `WorkspaceView` with a valid git repo
- **THEN** `worktree_count() >= 1`, `active_path()` points to the main worktree root, and `current_status()` is not an error

#### Scenario: Startup with no candidate (empty state)
- **WHEN** `WorkspaceView::new(cx, None)` is constructed
- **THEN** `cx.has_pending_prompt()` is true (the repo picker was invoked), and `worktree_count() == 0`

#### Scenario: Startup with a non-git directory
- **WHEN** `WorkspaceView::new(cx, Some(non_git_tempdir))` is constructed
- **THEN** the workspace enters the empty state with a pending path prompt, and no terminal is spawned

#### Scenario: Path selection resolves the repo
- **WHEN** the empty-state workspace has a pending prompt and `cx.simulate_new_path_selection(temp_repo)` selects a git repo
- **THEN** `worktree_count() >= 1` and `active_path()` points to the selected repo's main worktree

### Requirement: Worktree list rendering and main-row guard
The test suite SHALL cover that the sidebar worktree list reflects `git::list` output and that the main repository row is protected from closure.

#### Scenario: List count matches git worktrees
- **WHEN** a repo has N worktrees (main + N-1 extras created via `git worktree add`)
- **THEN** `worktree_count() == N` and `worktree_paths()` contains each worktree's canonical path

#### Scenario: Main row is not closable
- **WHEN** `request_close` is invoked on the main repository path
- **THEN** no terminal is removed, `git::list` still contains the main worktree, and the main row remains (the `is_main_repo` guard prevents deletion)

### Requirement: New worktree + agent launch flow coverage
The test suite SHALL cover `new_worktree_and_agent`: git worktree creation, PostCreate hook execution, terminal session spawn, active-path switching, and registry registration. Tests SHALL use a fake agent command (`/bin/sh` on Unix, `cmd.exe` on Windows) rather than real `claude`/`codex`.

#### Scenario: New worktree creates a terminal and switches active
- **WHEN** `new_worktree_and_agent(fake_agent)` is invoked on a repo
- **THEN** `git::list` contains the new branch, `terminals_contains(new_path)` is true, `active_path()` equals the new worktree path, and `is_ours_path(new_path)` is true (registry)

#### Scenario: Terminal renders PTY output
- **WHEN** the fake agent command is `/bin/sh -c 'printf MARKER'`
- **THEN** `wait_for(cx, |c| c.snapshot_text(new_path).contains("MARKER"))` succeeds within the timeout

#### Scenario: Hook failure surfaces an error status
- **WHEN** `new_worktree_and_agent` is invoked on a repo whose `.worktree.toml` has a `post_create` command that exits non-zero
- **THEN** `current_status()` is an error, and `terminals_contains(new_path)` is false (no terminal spawned on hook failure)

### Requirement: Active terminal switching coverage
The test suite SHALL cover clicking a worktree row to switch the active terminal, and that switching to an already-open terminal reuses it (does not spawn a new session).

#### Scenario: Click switches active terminal
- **WHEN** two worktrees have open terminals and the user clicks the inactive worktree's row
- **THEN** `active_path()` switches to the clicked worktree, and the previously active terminal remains in `terminals`

#### Scenario: Switching reuses existing terminal
- **WHEN** a worktree already has an open terminal and the user clicks its row
- **THEN** no new `TerminalSession` is spawned (the existing `Entity<TerminalView>` is reused, identifiable by stable identity)

### Requirement: Close worktree flow coverage
The test suite SHALL cover the close flow: clean worktree (no confirmation), dirty worktree (confirmation dialog with dirty count), confirmation/cancellation, and PreRemove hook + git removal.

#### Scenario: Clean worktree closes without confirmation
- **WHEN** `request_close` is invoked on a worktree with no uncommitted changes
- **THEN** the terminal is shut down, `terminals_contains(path)` becomes false, the worktree is removed from `git::list`, and the registry unregisters it

#### Scenario: Dirty worktree prompts confirmation
- **WHEN** `request_close` is invoked on a worktree with uncommitted changes (a new file written but not committed)
- **THEN** `has_pending_close()` is true, `pending_close_branch()` matches the worktree's branch, and the terminal is NOT yet shut down

#### Scenario: Confirm closes a dirty worktree
- **WHEN** a dirty worktree has a pending close and `confirm_close` is invoked
- **THEN** the terminal shuts down, the worktree is removed, and `has_pending_close()` becomes false

#### Scenario: Cancel keeps a dirty worktree open
- **WHEN** a dirty worktree has a pending close and `cancel_close` is invoked
- **THEN** `has_pending_close()` becomes false, the terminal remains alive (`terminals_contains` still true), and the worktree is not removed

### Requirement: Agent launcher menu coverage
The test suite SHALL cover the `+` button agent launcher menu: opening, item count driven by `builtin_agents()`, selecting an item triggers `new_worktree_and_agent`, and dismissal via outside-click or Esc.

#### Scenario: Plus button opens the menu
- **WHEN** the `+` (agent launcher) button is clicked
- **THEN** `is_agent_menu_open()` is true

#### Scenario: Menu items match builtin agents
- **WHEN** the agent menu is open
- **THEN** the number of menu items equals `lucy_core::agent::builtin_agents().len()`, and each item's label matches a builtin agent's display name

#### Scenario: Selecting an item launches an agent and closes the menu
- **WHEN** a menu item is clicked
- **THEN** `is_agent_menu_open()` becomes false, and `new_worktree_and_agent` is triggered (asserted via `terminals_contains(new_path)` and `active_path()` switch, as in the new-worktree flow)

#### Scenario: Outside click dismisses the menu
- **WHEN** the menu is open and a click lands on the scrim/overlay outside the menu card
- **THEN** `is_agent_menu_open()` becomes false and no agent is launched

#### Scenario: Esc dismisses the menu
- **WHEN** the menu is open and `Esc` is pressed
- **THEN** `is_agent_menu_open()` becomes false and no agent is launched

### Requirement: Alias editing coverage
The test suite SHALL cover opening the alias editor, entering text, and committing — verifying the alias persists to `.worktree.toml`.

#### Scenario: Alias editor opens with current alias
- **WHEN** the alias edit action is invoked on a worktree
- **THEN** `editing_alias()` reflects the worktree's branch, and the input field contains the current alias (if any)

#### Scenario: Committing an alias persists to config
- **WHEN** an alias is entered via `simulate_input` and committed
- **THEN** `lucy_core::config::load(repo_root)` returns the updated alias for that branch, and `editing_alias()` becomes None (editor closed)

### Requirement: Settings dialog coverage
The test suite SHALL cover opening the settings dialog, modifying fields, and committing — verifying `EditableSettings` persists.

#### Scenario: Settings dialog opens
- **WHEN** the settings (gear) button is clicked
- **THEN** `settings_open()` is true

#### Scenario: Committing settings persists
- **WHEN** `fail_fast` is toggled and/or `location` is changed and `commit_settings` is invoked
- **THEN** `settings_open()` becomes false, and `lucy_core::config::load(repo_root)` reflects the new `fail_fast`/`location` values

### Requirement: Terminal rendering coverage
The test suite SHALL cover that a `TerminalView` renders PTY output into readable cells, using a real PTY (`/bin/sh` on Unix, `cmd.exe` on Windows) and `wait_for` polling.

#### Scenario: PTY output appears in the snapshot
- **WHEN** a terminal is spawned with `/bin/sh -c 'printf HELLO_LUCY'`
- **THEN** `wait_for(cx, |c| c.snapshot_text(path).contains("HELLO_LUCY"))` succeeds within the timeout

#### Scenario: Terminal element paints without panic
- **WHEN** `VisualTestContext::draw` renders the `TerminalElement` after PTY output
- **THEN** painting completes without panic, and the element tree contains the terminal view

### Requirement: Terminal input coverage
The test suite SHALL cover keyboard input encoding to the PTY (printable chars, function keys) and PTY echo.

#### Scenario: Printable input is echoed
- **WHEN** a terminal is spawned with `/bin/cat` and `simulate_input("abc")` is sent
- **THEN** `snapshot_text` contains `abc` (cat echoes input)

#### Scenario: Resize updates PTY dimensions
- **WHEN** the window is resized via `simulate_resize` and `run_until_parked` drains
- **THEN** `TerminalView::dimensions()` reflects the new columns/rows computed from the new pixel size

### Requirement: Terminal copy interactions regression coverage
The test suite SHALL regress the behaviors specified in the `terminal-copy` capability: double-click word select, triple-click line select, copy-on-select, trailing-whitespace trimming, select-all, Shift+click extend, right-click context menu, and copy visual feedback. Clipboard assertions SHALL use `TestAppContext::read_from_clipboard`.

#### Scenario: Double-click selects and copies a word
- **WHEN** the user double-clicks on a word character in the terminal and the row contains `hello world`
- **THEN** `selection_text()` equals `hello` and `read_from_clipboard()` equals `hello`

#### Scenario: Triple-click selects a line with trailing trim
- **WHEN** the user triple-clicks on a row containing `ls -la` followed by trailing spaces
- **THEN** `selection_text()` spans the row and `read_from_clipboard()` equals `ls -la` (no trailing spaces)

#### Scenario: Drag-select copies on release
- **WHEN** the user drags from cell A to cell B (different cells) and releases
- **THEN** `read_from_clipboard()` equals the selected text, without pressing a copy shortcut

#### Scenario: Select-all then copy
- **WHEN** the user presses Cmd+A (or Ctrl+Shift+A) and then Cmd+C
- **THEN** `read_from_clipboard()` contains the full visible viewport text

#### Scenario: Right-click context menu
- **WHEN** the user right-clicks with an active selection
- **THEN** a context menu renders with Copy enabled; clicking Copy writes the selection to the clipboard

#### Scenario: Shift+click extends selection
- **WHEN** a selection exists from (row 2, col 0) to (row 2, col 5) and the user Shift+clicks (row 2, col 10)
- **THEN** `selection_text()` spans (row 2, col 0) to (row 2, col 10) and the clipboard is updated

### Requirement: Determinism and cleanup
All `#[gpui::test]` tests SHALL be deterministic and leak-free. Tests SHALL use `TestDispatcher`'s seeded scheduling, `run_until_parked` to drain async tasks, `tempfile::tempdir` for repo/registry isolation, and explicit `shutdown_workspace` (drop terminals + drain) to avoid `leak-detection` false positives. No test SHALL depend on a real OS window, GPU, display, or network.

#### Scenario: Tests run headless and cross-platform
- **WHEN** `cargo test -p lucy-app` is run on macOS, Linux, or Windows CI without a display
- **THEN** all `#[gpui::test]` tests pass (TestPlatform provides headless rendering/input)

#### Scenario: Tests do not leak entities
- **WHEN** a test completes
- **THEN** all `Entity`s, `Subscription`s, and `Task`s are dropped (terminals shut down, `cx.spawn` polling loops terminated), so `leak-detection` does not fail the test

#### Scenario: Tests do not pollute the host registry
- **WHEN** a test registers a session
- **THEN** the registry path is a tempdir (not `~/Library/Application Support/LucyMind/`), and the tempdir is cleaned up on test completion

### Requirement: Closed-loop gate
`cargo test -p lucy-app` (including all `#[gpui::test]` integration tests) SHALL be the automated verification gate for UI behavior. `cargo fmt && cargo clippy --all-targets` SHALL cover test code. A change to UI behavior that breaks a covered scenario SHALL fail the corresponding test, eliminating the need for manual `cargo run` verification of that behavior.

#### Scenario: Behavior regression is caught
- **WHEN** a developer comments out the `agent_menu_open = true` toggle in the `+` button handler
- **THEN** `tests/agent_menu.rs`'s "Plus button opens the menu" scenario fails on the next `cargo test -p lucy-app`

#### Scenario: Green build means UI verified
- **WHEN** `cargo test -p lucy-app` and `cargo clippy --all-targets` both pass
- **THEN** the covered UI state machines and interactions are considered verified, and no manual `cargo run` button-clicking is required for those behaviors
