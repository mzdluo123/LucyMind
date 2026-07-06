## ADDED Requirements

### Requirement: Multiple terminals per worktree via terminal groups
The workspace SHALL support multiple terminals per worktree. The `terminals` data structure SHALL map each worktree path to a `TerminalGroup` containing a list of `TerminalTab` (each with a `TerminalView` entity and a static fallback title) and an `active_tab` index. The `active` field SHALL continue to track the active worktree path (not the active tab). Switching worktrees SHALL preserve each group's `active_tab`.

#### Scenario: A worktree can hold multiple terminals
- **WHEN** the user creates two shell terminals in the same worktree (via the `+` button)
- **THEN** both terminals exist as tabs in that worktree's `TerminalGroup`, and `tab_count(path)` returns 2

#### Scenario: Switching worktrees preserves active tab
- **WHEN** worktree A has 3 tabs with active_tab=2, the user switches to worktree B, then switches back to worktree A
- **THEN** worktree A's active_tab is still 2 and the third tab is displayed

### Requirement: Terminal tab bar at the top of the terminal panel
A horizontal tab bar SHALL be rendered at the top of the terminal panel (above the terminal area, below the sidebar splitter). The tab bar SHALL only be visible when the active worktree has at least one terminal tab. When the active worktree has no terminals, the tab bar SHALL not render (height zero) and the terminal area shows the empty-state placeholder.

#### Scenario: Tab bar is hidden when no terminals
- **WHEN** the active worktree has no terminal group (or the group is empty)
- **THEN** no tab bar is rendered and the terminal area shows "select an action to begin"

#### Scenario: Tab bar appears when a terminal exists
- **WHEN** the active worktree has one or more terminal tabs
- **THEN** a tab bar is rendered above the terminal area showing one tab per terminal

### Requirement: Tab visual design
Each tab SHALL display a title (the dynamic OSC 0/2 title or the static fallback "Shell") and a `✕` close button. The active tab SHALL be visually distinguished with a top border accent line (`TEXT_BRIGHT`) and a raised background (`SURFACE_RAISED`). Inactive tabs SHALL use `SURFACE` background with `TEXT_DIM` text, brightening on hover. The tab bar SHALL end with a `+` button that creates a new shell terminal tab in the current worktree.

#### Scenario: Active tab is visually highlighted
- **WHEN** a worktree has tabs ["Shell"(active=0), "Shell"(1)] and active_tab=0
- **THEN** the first tab has a top accent line and raised background; the second tab has a plain background

#### Scenario: Plus button creates a new shell tab
- **WHEN** the user clicks the `+` button in the tab bar
- **THEN** a new shell terminal is spawned in the active worktree, appended as a new tab, and the new tab becomes active

### Requirement: Tab title follows terminal OSC 0/2 title protocol
`TerminalView` SHALL store the latest title received via `TermEvent::Title(String)` (the OSC 0/2 escape sequence `ESC ] 0 ; <title> BEL` / `ESC ] 2 ; <title> BEL`, parsed by the alacritty core and forwarded by `TerminalSession`). The tab bar SHALL display the terminal's dynamic title when present, falling back to the static title ("Shell") when the terminal has not emitted a title. When a new title arrives, the tab bar SHALL update on the next render (driven by `cx.notify()`).

#### Scenario: Terminal title overrides static fallback
- **WHEN** a shell tab (static title "Shell") emits `printf '\033]0;vim - main.rs\007'`
- **THEN** the tab title changes from "Shell" to "vim - main.rs"

#### Scenario: No terminal title shows static fallback
- **WHEN** a tab is created and the terminal has not emitted any OSC 0/2 title
- **THEN** the tab title is "Shell" (the static fallback)

#### Scenario: Title updates are reflected on re-render
- **WHEN** a terminal emits a new title while its tab is active
- **THEN** the tab bar re-renders with the new title (child `cx.notify()` propagates to the parent `WorkspaceView` render)

#### Scenario: Title is per-terminal, not shared
- **WHEN** a worktree has two tabs: tab 0 (shell emitting "vim") and tab 1 (shell emitting "bash")
- **THEN** tab 0 shows "vim" and tab 1 shows "bash" independently

### Requirement: Tab switching
Clicking a tab (not its `✕` button) SHALL switch the active terminal to that tab. The `active_tab` index of the current worktree's group SHALL be updated to the clicked tab's index. The terminal area SHALL immediately render the newly active tab's `TerminalView`.

#### Scenario: Clicking an inactive tab switches to it
- **WHEN** the user clicks tab 1 while tab 0 is active
- **THEN** tab 1 becomes active, its terminal is displayed, and tab 0 becomes inactive

### Requirement: Tab closing
Clicking a tab's `✕` button SHALL close only that terminal (shut down its PTY and remove it from the group), without removing the worktree or affecting other tabs. The `✕` click SHALL NOT propagate to the tab's switch handler. If the closed tab was active, `active_tab` SHALL fall back to `min(closed_index, remaining_len)`. If the last tab is closed, the group SHALL be removed from `terminals` (the worktree remains in the sidebar).

#### Scenario: Closing a non-active tab does not change the displayed terminal
- **WHEN** tabs are [tab0(active), tab1] and the user closes tab1
- **THEN** tab1 terminal is shut down, tab0 remains active and displayed, `tab_count` is 1

#### Scenario: Closing the active tab falls back
- **WHEN** tabs are [tab0, tab1(active)] and the user closes tab1
- **THEN** tab1 terminal is shut down, tab0 becomes active (active_tab falls back to 0), `tab_count` is 1

#### Scenario: Closing the last tab removes the group
- **WHEN** a worktree has one tab and the user closes it
- **THEN** the terminal is shut down, the group is removed from `terminals`, the terminal area shows the empty-state placeholder, and the worktree remains in the sidebar list

#### Scenario: Closing a tab does not delete the worktree
- **WHEN** the user closes a tab via its `✕` button
- **THEN** the worktree directory and git state are unaffected; the worktree is still listed in the sidebar and can be clicked to create a new terminal

### Requirement: Worktree closing shuts down all tabs
When closing a worktree from the sidebar (the `✕` button on the worktree row), ALL terminals in that worktree's group SHALL be shut down (PTY stopped) before the git remove flow proceeds. This replaces the current behavior of shutting down a single terminal.

#### Scenario: Closing a worktree with multiple tabs stops all terminals
- **WHEN** a worktree has 3 tabs and the user clicks the worktree row `✕` (and confirms if dirty)
- **THEN** all 3 terminals are shut down, the group is removed, and the git remove flow proceeds

### Requirement: New worktree opens a shell (not an agent)
The sidebar `+` button SHALL create a new worktree and open a shell terminal tab (not spawn an agent subprocess). The sidebar `+` button SHALL NOT show an agent dropdown menu. The shell terminal SHALL be spawned with `command=None` (default shell), `cwd=worktree_path`, and env including `TERM=xterm-256color` + worktree context vars. The `Session.agent` field SHALL record `None` (the agent is chosen later via tab bar buttons, not at worktree creation).

#### Scenario: Plus button creates worktree with a shell tab
- **WHEN** the user clicks the sidebar `+` button
- **THEN** a new worktree is created (git add + postCreate hook), a `TerminalGroup` with one shell tab is created for that worktree, `active` is set to that path, and `active_tab` is 0

#### Scenario: No agent dropdown menu appears
- **WHEN** the user clicks the sidebar `+` button
- **THEN** no dropdown menu is shown; the worktree is created directly with a shell terminal

#### Scenario: Shell inherits worktree environment
- **WHEN** a worktree is created and the shell tab is spawned
- **THEN** the shell process has `TERM=xterm-256color`, `WORKTREE_PATH`, `WORKTREE_BRANCH`, `WORKTREE_NAME`, and `REPO_ROOT` environment variables set

### Requirement: Agent launcher buttons in the tab bar
The tab bar SHALL render a row of agent buttons on the right side (after the `+` button, separated by a flex spacer), one per builtin agent (iterating `builtin_agents()`). Each button SHALL show the agent's icon and display name. Clicking an agent button SHALL send the agent's command string (constructed from `AgentSpec::resolve`: `command args\n`, with shell-quoting for args containing spaces) to the current active tab's shell PTY via `TerminalView::send_text`. The agent buttons SHALL only be visible when the tab bar is visible (i.e., the active worktree has at least one terminal).

#### Scenario: Agent buttons appear when a terminal exists
- **WHEN** the active worktree has a terminal tab
- **THEN** the tab bar shows agent buttons (Claude / Codex / OpenCode) on the right side

#### Scenario: Agent buttons are hidden when no terminal
- **WHEN** the active worktree has no terminal (empty state)
- **THEN** no agent buttons are rendered (the tab bar is not rendered at all)

#### Scenario: Clicking an agent button sends the command to the shell
- **WHEN** the user clicks the "Claude" agent button while a shell tab is active
- **THEN** the string `claude --dangerously-skip-permissions\n` is written to the active tab's shell PTY, and the shell begins executing the claude command

#### Scenario: Agent command uses resolved spec (config override)
- **WHEN** `.worktree.toml` defines `[agents.claude]` with `args = ["--resume"]` and the user clicks the "Claude" button
- **THEN** the string `claude --resume\n` is written (config preset overrides builtin args, same as the old `AgentSpec::resolve` semantics)

#### Scenario: Agent button does nothing without an active tab
- **WHEN** there is no active worktree or the active worktree has no tabs, and somehow an agent button is clicked
- **THEN** no command is sent (no-op)

### Requirement: TerminalView send_text method
`TerminalView` SHALL expose a public `send_text(&self, text: &str)` method that writes the text as bytes to the terminal's PTY via `TerminalSession::write_input`. This is used by the agent launcher buttons to send commands to the shell, and may be used by future features (e.g., keyboard shortcuts to send commands).

#### Scenario: send_text writes to the PTY
- **WHEN** `terminal_view.send_text("ls\n")` is called
- **THEN** the bytes `b"ls\n"` are written to the shell PTY, and the shell executes `ls`

### Requirement: open_worktree creates a shell tab (not an agent)
`open_worktree` (clicking a worktree row) SHALL create a group with one shell tab if no group exists, or switch to the existing group (preserving its `active_tab`) if one already exists. The shell tab SHALL be spawned with `command=None` (default shell), titled "Shell".

#### Scenario: Clicking a worktree row with no existing group creates a shell tab
- **WHEN** the user clicks a worktree row that has no terminal group
- **THEN** a `TerminalGroup` with one tab titled "Shell" is created, and that tab is active

#### Scenario: Clicking a worktree row with an existing group does not create a new tab
- **WHEN** the user clicks a worktree row that already has a group with 2 tabs
- **THEN** no new tab is created; the group's existing `active_tab` is displayed

### Requirement: Terminal area renders the active tab
The terminal area SHALL render the `TerminalView` of the active worktree's active tab. If the active worktree has no group or the group is empty, the terminal area SHALL show the empty-state placeholder ("select an action to begin").

#### Scenario: Terminal area shows the active tab's terminal
- **WHEN** the active worktree has tabs [tab0(active), tab1]
- **THEN** the terminal area renders tab0's `TerminalView`

#### Scenario: Terminal area shows empty state when group is empty
- **WHEN** the active worktree's group was emptied (last tab closed)
- **THEN** the terminal area shows "select an action to begin"

### Requirement: Test accessors adapted for multi-tab
The `#[cfg(feature = "test-support")]` accessors SHALL be adapted: `terminals_contains(path)` returns true if a group exists with non-empty tabs; `terminal_at(path)` returns the active tab's `TerminalView` for that path; `shutdown_all_terminals_for_test()` shuts down all tabs in all groups. New accessors `tab_count(path)` and `active_tab_index()` SHALL be added for multi-tab test assertions. `new_worktree_and_agent_for_test(agent_name)` SHALL be replaced by `new_worktree_for_test()` (no agent parameter).

#### Scenario: terminals_contains reflects group existence
- **WHEN** a worktree path has a group with 2 tabs
- **THEN** `terminals_contains(path)` returns true

#### Scenario: terminals_contains is false after last tab closed
- **WHEN** a worktree path's last tab was closed (group removed)
- **THEN** `terminals_contains(path)` returns false

#### Scenario: terminal_at returns the active tab
- **WHEN** a worktree path has tabs [tab0(active=0), tab1(1)] and active_tab=1
- **THEN** `terminal_at(path)` returns tab1's `TerminalView`

#### Scenario: tab_count reports the number of tabs
- **WHEN** a worktree path has a group with 3 tabs
- **THEN** `tab_count(path)` returns 3

### Requirement: Sidebar agent dropdown menu removed
The sidebar `+` button SHALL NOT show an agent dropdown menu. The `agent_menu_open` state, `agent_menu()` rendering method, and `open_agent_menu_for_test()` test accessor SHALL be removed. The `+` button SHALL directly call `new_worktree` (create worktree + open shell). Agent launching is moved to the tab bar buttons (see "Agent launcher buttons in the tab bar").

#### Scenario: Plus button does not open a menu
- **WHEN** the user clicks the sidebar `+` button
- **THEN** no menu overlay appears; a new worktree with a shell tab is created directly

#### Scenario: Esc no longer closes a menu
- **WHEN** the user presses Esc (and no other overlay is open)
- **THEN** nothing happens (the agent menu no longer exists, so Esc has no menu to close)
