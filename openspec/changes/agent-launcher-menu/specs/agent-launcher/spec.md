## ADDED Requirements

### Requirement: Single agent launcher button
The sidebar Agents section SHALL render exactly one `+` (plus) button instead of one button per agent. The `+` button SHALL be placed in the AGENTS section header row, mirroring the placement of the gear icon in the WORKTREES header row. No per-agent buttons SHALL be rendered directly in the sidebar content area.

#### Scenario: Only a plus button is shown in the Agents section
- **WHEN** the sidebar is rendered with two builtin agents (claude, codex)
- **THEN** the Agents section shows a single `+` button in its header and zero per-agent action buttons in the content area below the header

#### Scenario: Plus button placement mirrors the gear icon
- **WHEN** the sidebar is rendered
- **THEN** the AGENTS header row is a flex row with the "AGENTS" label on the left and the `+` icon button on the right, structurally symmetric to the WORKTREES header row (label left, gear icon right)

### Requirement: Agent launcher dropdown menu
Clicking the `+` button SHALL open a dropdown menu listing every builtin agent (icon + display name). Selecting an item SHALL close the menu and invoke the existing worktree-creation-and-agent-launch flow (`new_worktree_and_agent`) for that agent. The menu SHALL dismiss on: item selection, click on the backdrop outside the menu, or Esc key.

#### Scenario: Opening the menu lists all builtin agents
- **WHEN** the `+` button is clicked
- **THEN** a dropdown menu appears showing one entry per builtin agent, each with its icon and display name, in registry order

#### Scenario: Selecting an agent launches it
- **WHEN** the user clicks the "Claude" entry in the open menu
- **THEN** the menu closes and `new_worktree_and_agent("claude", cx)` is invoked

#### Scenario: Click outside dismisses the menu
- **WHEN** the menu is open and the user clicks on the backdrop area outside the menu card
- **THEN** the menu closes and no agent is launched

#### Scenario: Esc dismisses the menu
- **WHEN** the menu is open and the user presses Esc
- **THEN** the menu closes and no agent is launched

#### Scenario: Menu is closed by default
- **WHEN** the app starts
- **THEN** the agent launcher menu is not visible

### Requirement: Builtin agent registry as single source of truth
A builtin agent registry SHALL be the single source of truth for the set of agents surfaced in the UI menu and resolved by `AgentSpec::builtin`. Each entry SHALL carry: `name` (stable key), `display` (UI label), `icon` (asset path key), `command`, and `args`. The UI menu SHALL iterate this registry rather than hardcoding an agent array. Adding a builtin agent SHALL require changing only the registry (plus registering its icon asset), with no sidebar code changes.

#### Scenario: Registry drives both UI and spec resolution
- **WHEN** the registry contains entries for claude, codex, and opencode
- **THEN** the launcher menu shows exactly three entries and `AgentSpec::builtin` resolves all three names

#### Scenario: Adding an agent needs no UI code change
- **WHEN** a fourth agent entry is added to the registry and its icon is registered
- **THEN** the launcher menu shows four entries with no edits to the sidebar rendering code

### Requirement: OpenCode agent support
The registry SHALL include an `opencode` agent: command `opencode`, args `["--auto"]`. `AgentSpec::builtin("opencode", ...)` SHALL return a spec with that command and args, and `agent_icon("opencode")` SHALL return its icon path.

#### Scenario: OpenCode resolves via builtin
- **WHEN** `AgentSpec::builtin("opencode", cwd, &[])` is called
- **THEN** it returns a spec with `command == "opencode"`, `args == ["--auto"]`, `cwd` as given, and `TERM=xterm-256color` in extra_env

#### Scenario: OpenCode appears in the menu with an icon
- **WHEN** the launcher menu is rendered
- **THEN** an "OpenCode" entry is present with the opencode icon

### Requirement: Auto/bypass permission mode by default
Every builtin agent SHALL launch in an auto-approve or permission-bypass mode by default, so agents run unattended inside the isolated worktree without per-action permission prompts. The builtin defaults SHALL be: claude `["--dangerously-skip-permissions"]`, codex `["--full-auto"]`, opencode `["--auto"]`.

#### Scenario: Claude bypasses permissions by default
- **WHEN** `AgentSpec::builtin("claude", cwd, &[])` is called with no config override
- **THEN** the returned spec args contain `--dangerously-skip-permissions`

#### Scenario: Codex runs in full-auto by default
- **WHEN** `AgentSpec::builtin("codex", cwd, &[])` is called with no config override
- **THEN** the returned spec args contain `--full-auto`

#### Scenario: OpenCode runs in auto mode by default
- **WHEN** `AgentSpec::builtin("opencode", cwd, &[])` is called with no config override
- **THEN** the returned spec args contain `--auto`

#### Scenario: Config preset still fully overrides builtin args
- **WHEN** `.worktree.toml` defines `[agents.codex]` with `args = ["--yolo"]`
- **THEN** `AgentSpec::resolve` returns codex args as exactly `["--yolo"]` (config wins, builtin `--full-auto` is not merged)

### Requirement: Dogfood config carries bypass args
The repository's `.worktree.toml` (dogfood config) SHALL specify each `[agents.*]` preset with its auto/bypass args explicitly, so the bypass mode is actually applied (config presets fully replace builtin args) and is visible/self-documenting to users reading the config.

#### Scenario: Dogfood claude preset keeps bypass
- **WHEN** the dogfood `.worktree.toml` is loaded
- **THEN** `[agents.claude]` has `args` containing `--dangerously-skip-permissions`

#### Scenario: Dogfood codex preset keeps full-auto
- **WHEN** the dogfood `.worktree.toml` is loaded
- **THEN** `[agents.codex]` has `args` containing `--full-auto`

#### Scenario: Dogfood opencode preset keeps auto
- **WHEN** the dogfood `.worktree.toml` is loaded
- **THEN** `[agents.opencode]` exists with `args` containing `--auto`
