## ADDED Requirements

### Requirement: Launcher menu replaces agent buttons row

The tab bar SHALL NOT render a separate agent buttons row. The `+` button at the end of the tab list SHALL be a dropdown trigger that opens a launcher menu containing "New Tab" (shell type) and "Launch Agent" options.

#### Scenario: `+` button opens launcher menu

- **WHEN** the active worktree has at least one tab and the user clicks the `+` button
- **THEN** a dropdown menu appears below the tab bar, right-aligned with the `+` button
- **AND** the menu contains a "New Tab" section and a "Launch Agent" section

#### Scenario: Agent buttons row removed

- **WHEN** the tab bar is rendered with tabs present
- **THEN** no agent buttons (Claude / Codex / OpenCode) are rendered as standalone buttons in the tab bar
- **AND** agent launch options appear only inside the launcher menu

### Requirement: Launcher menu New Tab options

The launcher menu SHALL offer shell type selection in the "New Tab" section. Selecting an option creates a new terminal tab with the chosen shell type.

#### Scenario: Default Shell on all platforms

- **WHEN** the launcher menu is open and the user clicks "Default Shell"
- **THEN** a new tab is created with `command = None` (system default shell)
- **AND** the menu closes
- **AND** the new tab becomes the active tab

#### Scenario: Windows shell options

- **WHEN** the launcher menu is open on Windows
- **THEN** the "New Tab" section includes "Command Prompt" (cmd.exe), "PowerShell" (powershell.exe), and "PowerShell 7" (pwsh.exe) options in addition to "Default Shell"

#### Scenario: Non-Windows shell options

- **WHEN** the launcher menu is open on a non-Windows platform
- **THEN** the "New Tab" section includes only "Default Shell"

#### Scenario: Selecting a Windows shell type

- **WHEN** the user clicks "Command Prompt" in the launcher menu on Windows
- **THEN** a new tab is created with `command = Some(("cmd.exe", []))`
- **AND** the tab's static fallback title is "cmd"
- **AND** the menu closes

### Requirement: Launcher menu Launch Agent options

The launcher menu SHALL offer agent launch options in the "Launch Agent" section. Selecting an agent creates a new shell tab (Default shell) and immediately sends the agent command to it.

#### Scenario: Launch Claude from menu

- **WHEN** the launcher menu is open and the user clicks "Claude"
- **THEN** a new shell tab is created (Default shell)
- **AND** the agent command string (`claude --dangerously-skip-permissions\r`) is sent to the new tab's PTY
- **AND** the menu closes
- **AND** the new tab becomes the active tab

#### Scenario: Agent options match builtin registry

- **WHEN** the launcher menu is rendered
- **THEN** the "Launch Agent" section lists all agents from `builtin_agents()` in registry order (Claude, Codex, OpenCode)
- **AND** each item shows the agent's display name and icon

### Requirement: Launcher menu dismiss

The launcher menu SHALL close on outside click, Escape key, or item selection.

#### Scenario: Outside click closes menu

- **WHEN** the launcher menu is open and the user clicks outside the menu card
- **THEN** the menu closes without performing any action

#### Scenario: Escape closes menu

- **WHEN** the launcher menu is open and the user presses Escape
- **THEN** the menu closes without performing any action

#### Scenario: Item selection closes menu

- **WHEN** the user clicks any item in the launcher menu
- **THEN** the corresponding action is performed AND the menu closes

### Requirement: `+` button always visible

The `+` button SHALL be rendered outside the scrollable tab list, as a fixed element in the tab bar. It SHALL remain visible regardless of how many tabs exist or scroll position.

#### Scenario: `+` button visible with many tabs

- **WHEN** there are more tabs than can fit in the visible tab list area
- **THEN** the `+` button remains visible at the right edge of the tab bar
- **AND** the tab list scrolls horizontally to show overflow tabs
