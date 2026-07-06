## ADDED Requirements

### Requirement: Double-click selects a word
When the user double-clicks (click_count == 2) on a cell in the terminal, the application SHALL select the contiguous run of word characters (alphanumeric + underscore) containing the clicked cell. The selection SHALL span from the first non-word-character boundary to the left of the clicked cell to the first non-word-character boundary to the right, within the same row. The selected text SHALL be automatically copied to the clipboard immediately upon selection.

#### Scenario: Double-click in the middle of a word selects the whole word
- **WHEN** the user double-clicks on a cell containing the character `o` in the text `hello world`
- **THEN** the selection spans `hello` (col 0–4 of that row) and `hello` is written to the clipboard

#### Scenario: Double-click on a non-word character selects nothing
- **WHEN** the user double-clicks on a space or punctuation character
- **THEN** no selection is created (or the selection is empty) and nothing is copied

#### Scenario: Double-click at row edge clamps to row boundaries
- **WHEN** the user double-clicks on the first character of a row
- **THEN** the selection extends rightward to the word boundary but does not wrap to the previous or next row

### Requirement: Triple-click selects a line
When the user triple-clicks (click_count == 3) on a cell in the terminal, the application SHALL select the entire visible row from column 0 to the last non-blank cell on that row. The selected text SHALL be automatically copied to the clipboard immediately upon selection.

#### Scenario: Triple-click selects a line with content
- **WHEN** the user triple-clicks on any cell in a row containing `  echo hello  ` (with trailing spaces)
- **THEN** the selection spans from col 0 to the last non-blank cell, and the copied text is `  echo hello` (trailing spaces trimmed)

#### Scenario: Triple-click on a blank line
- **WHEN** the user triple-clicks on a row that is entirely blank (all spaces)
- **THEN** no selection is created and nothing is copied

### Requirement: Copy-on-select (release to copy)
After a mouse drag selection ends (mouse up), if the selection is non-empty (start != end), the selected text SHALL be automatically written to the clipboard. No keyboard shortcut is required. Manual copy via Cmd+C / Ctrl+Shift+C SHALL remain available as a fallback.

#### Scenario: Drag-select then release copies automatically
- **WHEN** the user drags from cell A to cell B (different cells) and releases the mouse button
- **THEN** the selection text is written to the clipboard immediately on mouse up, without pressing any key

#### Scenario: Single click does not copy
- **WHEN** the user clicks (without dragging) on a cell, producing a start == end selection
- **THEN** nothing is copied to the clipboard, and the selection is cleared

#### Scenario: IME composition suppresses copy-on-select
- **WHEN** the user releases the mouse during an active IME preedit (ime_preedit is non-empty)
- **THEN** copy-on-select does not execute (the release is ignored for copy purposes)

### Requirement: Trailing whitespace trimming on copy
When copying selected text (via any method: drag-release, double-click, triple-click, Cmd+C, Select All + copy), each line SHALL have trailing whitespace removed before writing to the clipboard. Leading and interior whitespace SHALL be preserved. Empty lines SHALL remain empty (not removed).

#### Scenario: Copy a line with trailing spaces
- **WHEN** the user selects a row from col 0 to col 79 and the line content is `ls -la` followed by 74 spaces
- **THEN** the clipboard receives `ls -la` (no trailing spaces)

#### Scenario: Copy a multi-line selection with interior spaces
- **WHEN** the user selects two rows: `  foo bar  ` and `  baz  ` (with trailing spaces)
- **THEN** the clipboard receives `  foo bar\n  baz` (interior and leading spaces preserved, trailing spaces removed per line)

### Requirement: Select all visible content
Pressing Cmd+A (macOS) or Ctrl+Shift+A (other platforms) SHALL select the entire visible terminal viewport (all rows × all columns of the current snapshot). This works in both main and alt screen modes. The selection SHALL be set but NOT automatically copied (the user presses copy afterward or copy-on-select applies on subsequent release).

#### Scenario: Cmd+A selects all visible rows
- **WHEN** the user presses Cmd+A and the terminal viewport has 24 rows × 80 cols
- **THEN** the selection spans from (row 0, col 0) to (row 23, col 80), covering the entire visible area

#### Scenario: Ctrl+A is not intercepted
- **WHEN** the user presses Ctrl+A (without Shift)
- **THEN** the keystroke is encoded and sent to the PTY as normal (terminal programs use Ctrl+A for line-start in readline); select-all is NOT triggered

### Requirement: Shift+click extends selection
When the user holds Shift and left-clicks on a cell while a selection already exists, the application SHALL move the selection endpoint to the clicked cell while keeping the selection start unchanged. If no selection exists, Shift+click SHALL behave as a normal click (start a new selection). Shift+click SHALL NOT enter drag-select mode.

#### Scenario: Shift+click extends selection forward
- **WHEN** a selection exists from (row 2, col 0) to (row 2, col 5), and the user Shift+clicks on (row 2, col 10)
- **THEN** the selection becomes (row 2, col 0) to (row 2, col 10), and the new selection text is copied to the clipboard

#### Scenario: Shift+click extends selection backward
- **WHEN** a selection exists from (row 2, col 5) to (row 2, col 10), and the user Shift+clicks on (row 2, col 0)
- **THEN** the selection becomes (row 2, col 0) to (row 2, col 10) (normalized), and the new selection text is copied to the clipboard

#### Scenario: Shift+click with no existing selection starts a new selection
- **WHEN** no selection exists and the user Shift+clicks on (row 3, col 4)
- **THEN** a new selection starts at (row 3, col 4) as a single-cell selection (start == end)

### Requirement: Right-click context menu
Right-clicking anywhere in the terminal area SHALL open a context menu at the click position with the following items: Copy, Paste, Select All. The Copy item SHALL be disabled (grayed out, non-clickable) when there is no active selection. Selecting an item SHALL perform the corresponding action and close the menu. Clicking outside the menu or pressing Esc SHALL dismiss the menu without action. Right-click SHALL NOT be forwarded to the PTY.

#### Scenario: Right-click with a selection shows enabled Copy
- **WHEN** the user right-clicks on the terminal while a selection exists
- **THEN** a context menu appears at the click position with Copy enabled (clickable), Paste, and Select All

#### Scenario: Right-click without a selection shows disabled Copy
- **WHEN** the user right-clicks on the terminal with no active selection
- **THEN** the context menu appears with Copy grayed out and non-clickable; Paste and Select All are enabled

#### Scenario: Clicking Copy copies and dismisses the menu
- **WHEN** the context menu is open with an active selection and the user clicks Copy
- **THEN** the selected text is copied to the clipboard and the menu closes

#### Scenario: Clicking Paste pastes and dismisses the menu
- **WHEN** the context menu is open and the user clicks Paste
- **THEN** the clipboard contents are pasted into the terminal (via bracketed-paste) and the menu closes

#### Scenario: Clicking Select All selects the viewport and dismisses the menu
- **WHEN** the context menu is open and the user clicks Select All
- **THEN** the entire visible viewport is selected and the menu closes

#### Scenario: Click outside dismisses the context menu
- **WHEN** the context menu is open and the user clicks outside the menu
- **THEN** the menu closes and no action is performed

#### Scenario: Esc dismisses the context menu
- **WHEN** the context menu is open and the user presses Esc
- **THEN** the menu closes and no action is performed

### Requirement: Copy visual feedback
When text is copied to the clipboard (via any method: copy-on-select, double-click, triple-click, Cmd+C, context menu Copy, Shift+click extension), the selection highlight SHALL briefly flash to a brighter color (~300ms) to indicate the copy occurred. After the flash, the selection highlight returns to its normal color and the selection remains active.

#### Scenario: Copy-on-select triggers visual flash
- **WHEN** the user drag-selects and releases, triggering copy-on-select
- **THEN** the selection highlight flashes brighter for approximately 300ms, then returns to normal

#### Scenario: Manual Cmd+C triggers visual flash
- **WHEN** the user selects text and presses Cmd+C
- **THEN** the selection highlight flashes brighter for approximately 300ms

#### Scenario: No flash when copy target is empty
- **WHEN** the user presses Cmd+C with no active selection (or an empty selection)
- **THEN** no flash occurs and nothing is copied

### Requirement: Word boundary definition
A "word character" for double-click word selection SHALL be any character matching `char::is_alphanumeric()` or the underscore character `_`. All other characters (spaces, punctuation, symbols, control characters) are word boundaries. Wide characters (CJK) are considered alphanumeric per Rust's `is_alphanumeric()`.

#### Scenario: Double-click on ASCII word
- **WHEN** the user double-clicks on `h` in `hello_world`
- **THEN** the selection spans `hello_world` (underscore is a word character)

#### Scenario: Double-click on CJK character
- **WHEN** the user double-clicks on a CJK character in `你好世界`
- **THEN** the selection spans the contiguous run of CJK characters (each CJK char is alphanumeric)
