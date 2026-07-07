## ADDED Requirements

### Requirement: FileTreePanel component renders active worktree's file tree
The workspace SHALL include a `FileTreePanel` component (a `gpui::Entity<FileTreePanel>`) that renders the active worktree's file tree. The panel SHALL be shown in the main layout between the sidebar and the terminal area (when toggled on), with a draggable splitter controlling its width. The panel SHALL use `Host::list_dir` to lazily load directory entries (only listing expanded directories' children). The panel SHALL hold an `Arc<dyn Host>` and the active worktree root path, refreshing when the active worktree changes.

#### Scenario: Panel is hidden by default
- **WHEN** the app starts and `file_tree_panel_open` is false (default)
- **THEN** no file tree panel is rendered; the layout is sidebar | splitter | main (unchanged from current)

#### Scenario: Panel appears when toggled on
- **WHEN** the user clicks the file tree toggle button in the sidebar
- **THEN** `file_tree_panel_open` becomes true, a splitter + `FileTreePanel` appear between the sidebar and the main area, and the panel lists the active worktree's root directory entries

#### Scenario: Panel uses Host::list_dir for entries
- **WHEN** the panel is shown and the active worktree is `/home/user/project` with WslHost
- **THEN** the panel calls `WslHost::list_dir("/home/user/project")` (spawning `wsl.exe ls`) to populate the root entries; the entries are cached in `dir_cache`

#### Scenario: Panel refreshes when active worktree changes
- **WHEN** the user switches from worktree A to worktree B (clicking a different worktree in the sidebar)
- **THEN** the panel clears its tree (`expanded`, `dir_cache`, `visible`, `selected`) and calls `list_dir` for worktree B's root path, auto-expanding the root

### Requirement: Directory expansion and collapse with lazy loading
The `FileTreePanel` SHALL maintain an `expanded: HashSet<PathBuf>` of expanded directory paths. Clicking a directory row SHALL toggle its expansion: if collapsed, expand it (insert into `expanded`); if expanded, collapse it (remove from `expanded`). Expanding a directory not in `dir_cache` SHALL trigger an async `Host::list_dir` (showing a loading state until complete); expanding a directory already in `dir_cache` SHALL use the cached entries (no `list_dir` call). Collapsing a directory SHALL remove its descendants from the visible list but SHALL NOT clear the `dir_cache` (re-expanding uses the cache unless a refresh was triggered).

#### Scenario: Clicking a collapsed directory expands it
- **WHEN** the user clicks a directory row `src` (which is collapsed) in the file tree
- **THEN** `src` is added to `expanded`, `list_dir("root/src")` is called (if not cached), and the children of `src` appear in the visible list indented one level deeper

#### Scenario: Clicking an expanded directory collapses it
- **WHEN** the user clicks a directory row `src` (which is expanded and showing children)
- **THEN** `src` is removed from `expanded`, and its children (and their descendants) disappear from the visible list

#### Scenario: Re-expanding uses the cache
- **WHEN** the user expands `src`, then collapses it, then expands it again (without clicking refresh)
- **THEN** the second expansion does NOT call `list_dir` (uses `dir_cache`), and the children appear instantly

#### Scenario: Loading state while listing
- **WHEN** a directory is expanded and `list_dir` is in flight (not yet completed)
- **THEN** the directory row shows a loading indicator (e.g., "…" or a spinner icon) or a "加载中…" placeholder child entry

#### Scenario: Expansion state is per-session
- **WHEN** the user expands `src` and `src/tests`, then switches to another worktree and back
- **THEN** the expansion state is reset (the panel re-lists the root only); expanded directories from the previous worktree are not remembered (Phase 1 limitation)

### Requirement: Visible entries flat list computed from expanded state
The `FileTreePanel` SHALL compute a `Vec<VisibleEntry>` flat list from the `expanded` set and `dir_cache`. Each `VisibleEntry` SHALL have `path: PathBuf`, `name: String`, `is_dir: bool`, `depth: usize` (indentation level, root's direct children = 0), `is_expanded: bool`, `is_loaded: bool` (whether children are in `dir_cache`). The list SHALL be recomputed synchronously whenever `expanded` or `dir_cache` changes. Entries SHALL preserve `Host::list_dir`'s sort order (directories first, then files, alphabetical within each group).

#### Scenario: Visible list shows only root entries when nothing is expanded
- **WHEN** the root directory has entries `[src (dir), target (dir), README.md (file)]` and nothing is expanded
- **THEN** the visible list has 3 entries: `src` (depth 0, is_dir, not expanded), `target` (depth 0, is_dir, not expanded), `README.md` (depth 0, not dir)

#### Scenario: Visible list includes children of expanded directories
- **WHEN** `src` is expanded and `dir_cache["root/src"]` has `[main.rs, lib (dir)]`
- **THEN** the visible list shows `src` (depth 0, expanded), then `main.rs` (depth 1), `lib` (depth 1, not expanded), then `target` (depth 0), `README.md` (depth 0)

#### Scenario: Collapsing removes descendants but keeps siblings
- **WHEN** `src` is expanded (showing `main.rs`, `lib`) and the user collapses `src`
- **THEN** the visible list shows `src` (depth 0, not expanded), `target` (depth 0), `README.md` (depth 0) — `main.rs` and `lib` are removed

### Requirement: Selection and keyboard navigation
The `FileTreePanel` SHALL maintain a `selected: Option<PathBuf>` (the currently highlighted entry). Clicking a row SHALL select it. Up/Down arrows SHALL move the selection within the `visible` list (wrapping at top/bottom). Left arrow SHALL collapse the selected directory (if expanded) or move selection to the parent directory (if collapsed or a file). Right arrow SHALL expand the selected directory (if collapsed) or move selection to the first child (if expanded). Enter SHALL toggle expansion for a directory (same as clicking). The panel SHALL auto-scroll to keep the selected entry visible.

#### Scenario: Clicking a row selects it
- **WHEN** the user clicks the `src` row in the file tree
- **THEN** `selected` becomes `Some("root/src")` and the row is visually highlighted

#### Scenario: Down arrow moves selection down
- **WHEN** the visible list has 3 entries and `selected` is the first entry, and the user presses Down
- **THEN** `selected` becomes the second entry

#### Scenario: Down arrow wraps to first entry
- **WHEN** `selected` is the last entry and the user presses Down
- **THEN** `selected` wraps to the first entry

#### Scenario: Left arrow collapses an expanded directory
- **WHEN** `selected` is an expanded directory `src` and the user presses Left
- **THEN** `src` is collapsed (removed from `expanded`), and `selected` stays on `src`

#### Scenario: Left arrow moves to parent when collapsed
- **WHEN** `selected` is a collapsed directory `src` (or a file) and the user presses Left
- **THEN** `selected` moves to the parent directory of `src`

#### Scenario: Right arrow expands a collapsed directory
- **WHEN** `selected` is a collapsed directory `src` and the user presses Right
- **THEN** `src` is expanded (added to `expanded`, `list_dir` triggered if not cached), and `selected` stays on `src`

#### Scenario: Right arrow moves to first child when expanded
- **WHEN** `selected` is an expanded directory `src` (with children loaded) and the user presses Right
- **THEN** `selected` moves to the first child of `src`

### Requirement: Row visual design with indentation and icons
Each row SHALL be indented by `depth * indent_size` pixels (default 16px per level). Each row SHALL show an icon: `📁` (or `folder.svg`) for directories, `📂` (or `folder-open.svg`) for expanded directories, `📄` (or `file.svg`) for files. The selected row SHALL have a highlighted background (`SURFACE_RAISED`) and a left accent border (`border_l_2 border_color(TEXT_BRIGHT)`, matching the sidebar worktree row style). Hovering a row SHALL highlight it (`BTN_BG_HOVER`). The panel SHALL use the UI font (`FONT_UI`) and theme colors (`SURFACE` background, `BORDER` right border, `TEXT` / `TEXT_DIM` / `TEXT_FAINT` text colors).

#### Scenario: Directories and files have distinct icons
- **WHEN** the visible list has `src (dir, collapsed)`, `src (dir, expanded)`, `README.md (file)`
- **THEN** the collapsed directory shows `📁`, the expanded directory shows `📂`, the file shows `📄`

#### Scenario: Indentation increases with depth
- **WHEN** `src` is at depth 0 and `src/main.rs` is at depth 1
- **THEN** `src` has `padding-left: 0px` and `main.rs` has `padding-left: 16px`

#### Scenario: Selected row is highlighted
- **WHEN** `selected` is `Some("root/src")` and `src` is in the visible list
- **THEN** the `src` row has `SURFACE_RAISED` background and a `TEXT_BRIGHT` left border; other rows have no special background

### Requirement: Draggable panel width with splitter
The `FileTreePanel` SHALL have a draggable splitter on its left edge (between the sidebar and the panel). The splitter SHALL be 4px wide, `BORDER` background, `cursor_col_resize`, and hover-highlight to `TEXT_FAINT`. Dragging the splitter SHALL adjust `file_tree_width` (clamped to `FILE_TREE_MIN_W` = 180px and `FILE_TREE_MAX_W` = 480px, matching the sidebar width range). The default width SHALL be 240px.

#### Scenario: Splitter appears when panel is open
- **WHEN** `file_tree_panel_open` is true
- **THEN** a 4px splitter is rendered between the sidebar (or sidebar splitter) and the file tree panel

#### Scenario: Dragging the splitter adjusts panel width
- **WHEN** the user drags the file tree splitter left and right
- **THEN** `file_tree_width` adjusts (clamped to 180-480px), and the panel width updates in real-time

#### Scenario: Splitter is hidden when panel is closed
- **WHEN** `file_tree_panel_open` is false
- **THEN** no file tree splitter is rendered; the layout is sidebar | splitter | main (no file tree panel)

### Requirement: Toggle button in the sidebar
The sidebar SHALL have a file tree toggle button (file-tree icon) in the WORKTREES title row (next to the settings gear button). Clicking the button SHALL toggle `file_tree_panel_open`. When the panel is open, the button SHALL be highlighted (`TEXT_BRIGHT`); when closed, the button SHALL be dim (`TEXT_FAINT`) with group-hover brightening (matching the settings gear button style). The button SHALL use `stop_propagation` to avoid triggering the settings panel.

#### Scenario: Toggle button opens the panel
- **WHEN** `file_tree_panel_open` is false and the user clicks the file tree toggle button
- **THEN** `file_tree_panel_open` becomes true, the `FileTreePanel` is created (or reused) and rendered

#### Scenario: Toggle button closes the panel
- **WHEN** `file_tree_panel_open` is true and the user clicks the file tree toggle button
- **THEN** `file_tree_panel_open` becomes false, the panel is hidden (the `Entity` may be kept or dropped)

#### Scenario: Toggle button reflects panel state
- **WHEN** `file_tree_panel_open` is true
- **THEN** the file tree toggle button icon is `TEXT_BRIGHT` color (highlighted)

#### Scenario: Toggle button click does not open settings
- **WHEN** the user clicks the file tree toggle button
- **THEN** the settings panel does NOT open (`stop_propagation` prevents the click from reaching the settings button)

### Requirement: Panel header with title and refresh button
The `FileTreePanel` SHALL have a header row at the top: a "FILES" label (dim text, matching the sidebar "WORKTREES" label style) and a refresh button (refresh icon, group-hover brightening). The header SHALL have a bottom border (`BORDER_SUBTLE`) separating it from the file list. Clicking the refresh button SHALL clear `dir_cache` and re-list all expanded directories (discarding cached entries).

#### Scenario: Header shows FILES label and refresh button
- **WHEN** the file tree panel is open
- **THEN** the top of the panel shows "FILES" (dim text) on the left and a refresh icon button on the right, separated from the file list by a bottom border

#### Scenario: Refresh clears cache and re-lists
- **WHEN** the user clicks the refresh button
- **THEN** `dir_cache` is cleared, and `list_dir` is called for all directories in `expanded` (re-populating the cache); the visible list is recomputed

### Requirement: Context menu with Reveal, Copy Path, Copy Relative Path
Right-clicking a file or directory row SHALL open a context menu at the mouse position with items: "Reveal in File Manager" (only when `!host.is_remote()`), "Copy Path" (copies the absolute path to clipboard), "Copy Relative Path" (copies the path relative to the worktree root). Clicking a menu item SHALL execute the action and close the menu. Clicking outside the menu or pressing Escape SHALL close the menu without executing an action.

#### Scenario: Right-click opens context menu
- **WHEN** the user right-clicks the `src` row
- **THEN** a context menu appears at the mouse position with "Reveal in File Manager", "Copy Path", "Copy Relative Path" items

#### Scenario: Reveal in File Manager is hidden in WSL mode
- **WHEN** the host is `WslHost` (`is_remote() == true`) and the user right-clicks a row
- **THEN** the "Reveal in File Manager" item is NOT shown (only "Copy Path" and "Copy Relative Path")

#### Scenario: Copy Path copies absolute path
- **WHEN** the user right-clicks `src` (at `root/src`) and clicks "Copy Path"
- **THEN** the absolute path (e.g., `/home/user/project/src`) is written to the clipboard

#### Scenario: Copy Relative Path copies relative path
- **WHEN** the user right-clicks `src` (at `root/src`) and clicks "Copy Relative Path"
- **THEN** the relative path (`src`) is written to the clipboard

#### Scenario: Escape closes the context menu
- **WHEN** the context menu is open and the user presses Escape
- **THEN** the menu closes without executing any action

#### Scenario: Click outside closes the context menu
- **WHEN** the context menu is open and the user clicks outside the menu
- **THEN** the menu closes without executing any action

### Requirement: Scrollable file list
The file list SHALL be scrollable (`overflow_y_scroll`) when the visible entries exceed the panel height. The panel SHALL auto-scroll to keep the selected entry visible when navigating with the keyboard (Up/Down/Left/Right). The scroll container SHALL have an `id` (required by GPUI's `overflow_y_scroll`).

#### Scenario: Long file list is scrollable
- **WHEN** the visible list has 100 entries and the panel height shows only 20
- **THEN** the list is scrollable; entries beyond the viewport are rendered when scrolled into view

#### Scenario: Auto-scroll keeps selected entry visible
- **WHEN** the user presses Down repeatedly and the selected entry moves below the viewport
- **THEN** the list scrolls to keep the selected entry visible (centered or at the edge)

### Requirement: Host-aware path handling
The `FileTreePanel` SHALL use the `Host` abstraction for all directory listing (`Host::list_dir`), path canonicalization (`Host::canonicalize`), and path style (separator). The `root` path SHALL be the active worktree's canonical path (via `canon(host, &active_path)`). The `expanded` and `dir_cache` keys SHALL use canonical paths. In WSL mode, paths are Linux-style (`/home/...`); in local mode, paths are OS-native (`C:\...` or `/Users/...`).

#### Scenario: WSL mode uses Linux paths
- **WHEN** the host is `WslHost` and the active worktree is `/home/user/project`
- **THEN** the panel lists entries via `WslHost::list_dir("/home/user/project")` (spawning `wsl.exe ls`), and paths in `expanded`/`dir_cache` are Linux-style

#### Scenario: Local mode uses OS-native paths
- **WHEN** the host is `LocalHost` (on macOS) and the active worktree is `/Users/user/project`
- **THEN** the panel lists entries via `LocalHost::list_dir("/Users/user/project")` (using `std::fs::read_dir`), and paths are OS-native

### Requirement: Empty state when no active worktree
When there is no active worktree (`self.active` is None), the `FileTreePanel` SHALL show an empty-state placeholder ("select a worktree to browse files") instead of the file list. The panel header (FILES + refresh) SHALL still be shown, but the refresh button SHALL be disabled (or no-op).

#### Scenario: No active worktree shows empty state
- **WHEN** `self.active` is None and the file tree panel is open
- **THEN** the panel shows "select a worktree to browse files" (dim text, centered) instead of the file list

#### Scenario: Active worktree changes from None to Some
- **WHEN** the user creates or opens a worktree (setting `self.active`)
- **THEN** the panel calls `set_root(active_path)` and lists the root directory entries

### Requirement: Test accessors for FileTreePanel state
The `FileTreePanel` SHALL expose `#[cfg(feature = "test-support")]` accessors: `file_tree_root()`, `file_tree_visible_count()`, `file_tree_visible_entries() -> Vec<(String, usize, bool, bool)>` (name, depth, is_dir, is_expanded), `file_tree_selected() -> Option<PathBuf>`, `file_tree_expanded() -> Vec<PathBuf>` (sorted), `file_tree_is_loading() -> bool`, `set_file_tree_root_for_test(path)`, `file_tree_toggle_expanded_for_test(path)`, `file_tree_select_for_test(path)`, `file_tree_navigate_for_test(direction)` (up/down/left/right), `file_tree_refresh_for_test()`.

#### Scenario: Test can inspect the visible list
- **WHEN** a test sets the root to a mock directory with entries `[src (dir), README.md (file)]` and calls `file_tree_visible_entries()`
- **THEN** the result is `[("src", 0, true, false), ("README.md", 0, false, false)]`

#### Scenario: Test can toggle expansion
- **WHEN** a test calls `file_tree_toggle_expanded_for_test("root/src")` and `src` has cached children `[main.rs, lib]`
- **THEN** `file_tree_expanded()` includes `root/src`, and `file_tree_visible_entries()` includes `[("src", 0, true, true), ("main.rs", 1, false, false), ("lib", 1, true, false), ("README.md", 0, false, false)]`

#### Scenario: Test can navigate with keyboard
- **WHEN** a test calls `file_tree_navigate_for_test(Down)` with `selected` on the first entry
- **THEN** `file_tree_selected()` returns the second entry's path
