## ADDED Requirements

### Requirement: Tabs adapt to window width with horizontal scroll overflow

Terminal tabs SHALL use flexible width (`flex_1` with `min_w(80px)` and `max_w(200px)`) so that they automatically grow to fill available space when there are few tabs, and shrink to a readable minimum when there are many. When tabs at the minimum width still exceed the available width, the tab list scrolls horizontally.

The tab list container has `overflow_x_scroll`, which causes GPUI to automatically convert vertical mouse wheel events to horizontal scroll when `overflow.y` is not `Scroll`. Users can scroll the tab list by hovering over it and rolling the mouse wheel.

#### Scenario: Few tabs on a wide window

- **WHEN** there are 1-3 tabs and the window is wide enough
- **THEN** each tab grows to at most 200px wide (`max_w`)
- **AND** the tabs fill the available tab list width
- **AND** no horizontal scrolling is needed

#### Scenario: Many tabs shrink to minimum width then scroll

- **WHEN** the number of tabs causes each to shrink to 80px (`min_w`) and the total still exceeds the tab list width
- **THEN** all tabs remain at 80px (no further shrinking)
- **AND** the tab list scrolls horizontally
- **AND** the `+` and "reveal in file manager" buttons remain visible outside the scroll area

#### Scenario: Mouse wheel scrolls tab list horizontally

- **WHEN** the user hovers over the tab list and scrolls the mouse wheel (vertical)
- **THEN** the tab list scrolls horizontally (vertical wheel delta is converted to horizontal scroll offset)
- **AND** the `+` button and reveal button do not scroll (they are outside the scroll container)

### Requirement: Tab close button always accessible

Each tab SHALL display a close button (`✕`) that remains visible. The close button is always rendered at the right edge of each tab, and the title truncates with ellipsis when there is not enough space.

#### Scenario: Tab title truncates

- **WHEN** a tab title is longer than the available title area
- **THEN** the title is truncated with ellipsis (`text_ellipsis` + `whitespace_nowrap`)
- **AND** the close button (`✕`) remains visible and clickable

### Requirement: Reveal in file manager button

A "reveal in file manager" button SHALL be rendered in the tab bar, next to the `+` button, outside the scrollable tab list. Clicking it opens the active worktree's directory in the system file manager (Finder on macOS, Explorer on Windows).

#### Scenario: Reveal button opens file manager

- **WHEN** the active worktree has a terminal group and the user clicks the reveal button
- **THEN** the system file manager opens at the active worktree's directory path

#### Scenario: Reveal button with no active worktree

- **WHEN** there is no active worktree (no terminal group)
- **THEN** the reveal button is not rendered (the entire tab bar is hidden via `h_0`)

#### Scenario: Reveal button always visible with tabs

- **WHEN** there are many tabs that overflow the scrollable area
- **THEN** the reveal button remains visible at the right edge of the tab bar, next to the `+` button
