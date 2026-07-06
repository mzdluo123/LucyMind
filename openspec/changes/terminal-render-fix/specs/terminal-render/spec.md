## ADDED Requirements

### Requirement: Dynamic line height from font metrics
The terminal line height SHALL be dynamically measured from the font's actual ascent + descent at runtime, not a hardcoded constant. This ensures box-drawing characters (`│`, `╮`, `╰`, `╯`, `─`) fill the full line height vertically, connecting seamlessly across rows without gaps.

#### Scenario: Line height matches font metrics
- **WHEN** the terminal paints with a font whose ascent + descent = 16px
- **THEN** the line height used for painting, cell mapping, and PTY resize is 16px (not a hardcoded 20px)

#### Scenario: Fallback when font metrics are zero
- **WHEN** the font probe returns ascent + descent = 0 (font not found / metrics unavailable)
- **THEN** the line height falls back to `FONT_SIZE * 1.25`

### Requirement: Batch text shaping per style run
The terminal SHALL batch consecutive cells with the same style (foreground color + bold flag) into a single `shape_line` call, rather than shaping each cell individually. Spaces and zero-width spacers break the batch. This ensures horizontal character spacing is determined by the font engine, preventing sub-pixel accumulation errors that cause box-drawing characters to disconnect horizontally.

#### Scenario: Same-style cells are batched
- **WHEN** a row contains 10 consecutive cells with the same foreground color and bold flag
- **THEN** they are shaped as a single string in one `shape_line` call, not 10 separate calls

#### Scenario: Style change breaks the batch
- **WHEN** a row contains cells with alternating foreground colors
- **THEN** each contiguous same-color run is shaped separately

### Requirement: Cascadia Mono font on Windows
The terminal SHALL use `Cascadia Mono` as the default monospace font on Windows, replacing `Consolas`. Cascadia Mono is designed for terminal box-drawing characters and ensures proper alignment of `╮│╰╯─` glyphs.

#### Scenario: Windows font selection
- **WHEN** the app runs on Windows
- **THEN** `mono_font_family()` returns `"Cascadia Mono"`

### Requirement: Windows PTY command wrapping for non-exe shims
On Windows, when the agent command does not have an `.exe` extension (e.g., bare name like `codex` resolving to `codex.cmd`, or a `.ps1` file), the terminal SHALL wrap the command with `cmd.exe /C <command> <args>` so that `CreateProcessW` can execute it via `cmd.exe`'s PATHEXT resolution. Commands with `.exe` extension SHALL be executed directly without wrapping.

#### Scenario: Bare command name is wrapped
- **WHEN** the agent command is `codex` (no extension) on Windows
- **THEN** the PTY spawns `cmd.exe /C codex --dangerously-bypass-approvals-and-sandbox`

#### Scenario: exe command is not wrapped
- **WHEN** the agent command is `opencode.exe` on Windows
- **THEN** the PTY spawns `opencode.exe` directly without `cmd.exe /C`

#### Scenario: cmd file is wrapped
- **WHEN** the agent command path ends with `.cmd` on Windows
- **THEN** the PTY wraps it with `cmd.exe /C`

### Requirement: codex uses --dangerously-bypass-approvals-and-sandbox
The builtin codex agent SHALL use `--dangerously-bypass-approvals-and-sandbox` (not the removed `--full-auto`), as the current codex CLI version no longer supports `--full-auto`. The worktree itself provides isolation; codex's own sandbox is redundant and may block git operations within the worktree.

#### Scenario: codex builtin args
- **WHEN** `AgentSpec::builtin("codex", cwd, &[])` is called
- **THEN** the returned spec args contain `--dangerously-bypass-approvals-and-sandbox`

#### Scenario: dogfood config
- **WHEN** the dogfood `.worktree.toml` is loaded
- **THEN** `[agents.codex]` has `args` containing `--dangerously-bypass-approvals-and-sandbox`
