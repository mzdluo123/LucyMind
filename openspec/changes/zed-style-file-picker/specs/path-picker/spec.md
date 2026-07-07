## ADDED Requirements

### Requirement: Unified path input picker replaces WSL browser and native picker
The workspace SHALL replace the current `open_repo_choice_dialog` (Local/WSL two-button modal), `wsl_browser_dialog` (directory navigation list), and `open_local_picker` (native `prompt_for_paths`) with a single `PathPicker` component. `open_repo_picker` SHALL directly open the `PathPicker` modal using the current `self.host` (LocalHost or WslHost), with no intermediate choice dialog. The `PathPicker` SHALL work for both local and WSL paths via the `Host::list_dir` abstraction.

#### Scenario: Opening the repo picker shows a path input modal
- **WHEN** the user clicks the "open repository" button (folder-open icon in the sidebar)
- **THEN** a modal appears with a text input at the top (pre-filled with the initial query) and a scrollable completion list below

#### Scenario: No more Local/WSL choice dialog
- **WHEN** `open_repo_picker` is called
- **THEN** `open_repo_choice_open` is NOT set (the two-button choice dialog is removed); `PathPicker` is created directly

#### Scenario: PathPicker uses the current Host
- **WHEN** the app started with WslHost (WSL detected) and the user opens the repo picker
- **THEN** the `PathPicker` uses `WslHost::list_dir` for completions (spawning `wsl.exe ls`), and the path separator is `/`
- **WHEN** the app started with LocalHost and the user opens the repo picker
- **THEN** the `PathPicker` uses `LocalHost::list_dir` for completions (`std::fs::read_dir`), and the path separator is the OS separator

### Requirement: Path splitting into directory and suffix
The `PathPicker` SHALL split the query string into a directory part (the `list_dir` argument) and a suffix (the filter term) using `get_dir_and_suffix(query, separator)`. For Posix paths (`/` separator), the split is at the last `/`; for Windows paths (`\` separator), the split is at the last `\` or `/` (whichever is later). The directory part SHALL include the trailing separator. The `PathPicker` SHALL only re-list the directory (call `Host::list_dir`) when the directory part changes; if only the suffix changed, it SHALL re-filter the existing entries without re-listing.

#### Scenario: Splitting a Posix path with a suffix
- **WHEN** the query is `/home/user/Doc` and the separator is `/`
- **THEN** `get_dir_and_suffix` returns `("/home/user/", "Doc")`

#### Scenario: Splitting a Posix path ending with separator
- **WHEN** the query is `/home/user/` and the separator is `/`
- **THEN** `get_dir_and_suffix` returns `("/home/user/", "")` (empty suffix = show all entries)

#### Scenario: Splitting at root
- **WHEN** the query is `/` and the separator is `/`
- **THEN** `get_dir_and_suffix` returns `("/", "")` (list root directory, no filter)

#### Scenario: Re-listing only when directory changes
- **WHEN** the user types `/home/user/Do` (dir `/home/user/`, suffix `Do`) and then types `/home/user/Doc` (dir `/home/user/`, suffix `Doc`)
- **THEN** `list_dir` is called only once (for `/home/user/`); the second keystroke only re-filters the existing entries

### Requirement: Async directory listing with cancel-flag
The `PathPicker` SHALL list directory entries asynchronously (via `cx.background_executor().spawn` calling `host.list_dir(dir)`). It SHALL use an `Arc<AtomicBool>` cancel flag: each `update_matches` call flips the old flag and creates a new one; the background task checks the flag after completion and discards results if already cancelled. The `PathPicker` SHALL show a "loading…" state while the task is in flight.

#### Scenario: Typing fast cancels stale listings
- **WHEN** the user types `/home/u` then quickly types `/home/user/` (before the first listing completes)
- **THEN** the first listing's results are discarded (cancel flag flipped); only the second listing's results are shown

#### Scenario: Loading indicator while listing
- **WHEN** a `list_dir` task is in flight (has not completed yet)
- **THEN** the completion list shows "加载中…" (or a loading indicator) instead of entries

#### Scenario: Listing error is displayed
- **WHEN** `list_dir` returns an error (e.g., permission denied, path does not exist)
- **THEN** the completion list shows the error message (e.g., "错误: Permission denied") and the user can continue typing to retry

### Requirement: Fuzzy filtering of entries by suffix
The `PathPicker` SHALL filter the listed entries by the suffix: an entry matches if `entry.name.to_lowercase().contains(suffix.to_lowercase())`. When the suffix is empty, all entries match. The filtered list SHALL preserve the `Host::list_dir` sort order (directories first, then files, alphabetical). The `selected_index` SHALL reset to 0 whenever the filtered list changes.

#### Scenario: Suffix filters entries
- **WHEN** the directory `/home/user/` has entries `[src, target, .config, docs]` and the suffix is `do`
- **THEN** the filtered list shows `[docs]` (case-insensitive contains)

#### Scenario: Empty suffix shows all entries
- **WHEN** the suffix is empty
- **THEN** all non-hidden entries are shown (hidden entries starting with `.` are already filtered by `Host::list_dir`)

#### Scenario: No matches shows empty state
- **WHEN** the suffix matches no entries
- **THEN** the completion list shows "(无匹配)" (or similar empty-state text)

### Requirement: Keyboard navigation
The `PathPicker` SHALL support keyboard navigation within the completion list: `Up`/`Down` arrows move `selected_index` (wrapping at top/bottom); `Tab` completes the selected entry (appends the entry name + separator for directories, or entry name for files); `Enter` confirms the selected entry (or the typed path); `Escape` dismisses the picker. `Tab` and `Enter` SHALL `stop_propagation` to prevent the workspace-level key handler from also handling them.

#### Scenario: Down arrow wraps to first entry
- **WHEN** the filtered list has 3 entries, `selected_index` is 2 (last), and the user presses Down
- **THEN** `selected_index` becomes 0 (wraps to first)

#### Scenario: Up arrow wraps to last entry
- **WHEN** the filtered list has 3 entries, `selected_index` is 0 (first), and the user presses Up
- **THEN** `selected_index` becomes 2 (wraps to last)

#### Scenario: Tab completes a directory name
- **WHEN** the selected entry is a directory named `user` and the current query is `/home/us`
- **THEN** pressing Tab sets the query to `/home/user/` (appends `user` + `/`), which triggers `list_dir` for `/home/user/`

#### Scenario: Tab completes a file name
- **WHEN** the selected entry is a file named `README.md` and the current query is `/home/user/REA`
- **THEN** pressing Tab sets the query to `/home/user/README.md` (appends `README.md`, no trailing separator)

#### Scenario: Enter confirms and opens the repository
- **WHEN** the user presses Enter and the selected entry (or typed path) is a git repository root
- **THEN** the picker calls `on_confirm(path)`, which validates via `git::main_worktree_root` and calls `set_repo`; the picker modal closes

#### Scenario: Enter on a non-git directory shows an error
- **WHEN** the user presses Enter and the path is not a git repository
- **THEN** the picker shows "所选目录不是 git 仓库" in the completion list area and does NOT close

#### Scenario: Escape dismisses the picker
- **WHEN** the user presses Escape
- **THEN** the picker modal closes (`path_picker` set to `None`) without opening a repository

### Requirement: Completion list visual design
Each completion row SHALL show an icon (`📁 ` for directories, `📄 ` for files) and the entry name. The selected row SHALL have a highlighted background (`BTN_BG_HOVER` or `SURFACE_RAISED`). Hovering a row SHALL highlight it (same background). The list SHALL be scrollable (`overflow_y_scroll`) with a max height (e.g., 320px). Clicking a row SHALL select it (set `selected_index`); double-clicking or pressing Enter confirms it.

#### Scenario: Directories and files have distinct icons
- **WHEN** the directory `/home/user/` has entries `[src (dir), README.md (file)]`
- **THEN** the `src` row shows `📁 src` and the `README.md` row shows `📄 README.md`

#### Scenario: Selected row is highlighted
- **WHEN** `selected_index` is 1 and the list has 3 entries
- **THEN** the second row has a highlighted background; the other rows do not

#### Scenario: Clicking a row selects it
- **WHEN** the user clicks the third row in the completion list
- **THEN** `selected_index` becomes 2 (0-indexed) and the row is highlighted

### Requirement: Modal structure reuses ui::dialog::modal
The `PathPicker` modal SHALL reuse the existing `ui::dialog::modal` skeleton (scrim + centered card). The card SHALL contain: a text input (`gpui_component::input::Input`) at the top, a scrollable completion list in the middle, an optional error/loading text area, and a bottom row with a "Browse…" button (local mode only) and a "Cancel" button. Clicking the scrim (outside the card) SHALL dismiss the picker.

#### Scenario: Modal has text input and completion list
- **WHEN** the `PathPicker` modal is open
- **THEN** the card shows a text input (pre-filled with initial query, focused) at the top and a completion list below

#### Scenario: Browse button is hidden in WSL mode
- **WHEN** the host is `WslHost` (`is_remote() == true`)
- **THEN** the "Browse…" button is NOT rendered (native file picker cannot browse WSL)

#### Scenario: Browse button is shown in local mode
- **WHEN** the host is `LocalHost` (`is_remote() == false`)
- **THEN** the "Browse…" button is rendered; clicking it invokes `cx.prompt_for_paths` (native directory picker)

#### Scenario: Clicking scrim dismisses the picker
- **WHEN** the user clicks the scrim (outside the card)
- **THEN** the picker modal closes without opening a repository

### Requirement: Initial query based on host and current repo
The `PathPicker` SHALL pre-fill the text input with an initial query: if a repository is already open (`self.repo` is Some), use its path; otherwise, if the host is WslHost, use `/`; if the host is LocalHost, use the user's home directory (or current directory as fallback). The initial query SHALL trigger an initial `list_dir` to populate the completion list.

#### Scenario: Initial query for WSL with no repo
- **WHEN** the app started with WslHost and no repository is open, and the user opens the repo picker
- **THEN** the text input is pre-filled with `/` and the completion list shows the root directory entries

#### Scenario: Initial query for local with no repo
- **WHEN** the app started with LocalHost and no repository is open, and the user opens the repo picker
- **THEN** the text input is pre-filled with the user's home directory path (with trailing separator) and the completion list shows the home directory entries

#### Scenario: Initial query when switching repositories
- **WHEN** a repository is already open at `/home/user/project` and the user opens the repo picker
- **THEN** the text input is pre-filled with `/home/user/project/` and the completion list shows that directory's entries

### Requirement: PathPicker is a separate Entity
The `PathPicker` SHALL be a `gpui::Entity<PathPicker>` (not inline state on `WorkspaceView`). `WorkspaceView` SHALL hold `path_picker: Option<Entity<PathPicker>>`. Opening the picker creates a new `Entity`; closing sets it to `None`. The `PathPicker` SHALL hold its own state (query, entries, selected_index, loading, error, cancel_flag, host, on_confirm callback) isolated from `WorkspaceView`. The `PathPicker` SHALL receive an `on_confirm: Box<dyn Fn(PathBuf, &mut Window, &mut Context<PathPicker>)>` callback that the workspace sets to validate + open the repo.

#### Scenario: WorkspaceView holds an optional PathPicker entity
- **WHEN** the repo picker is not open
- **THEN** `WorkspaceView.path_picker` is `None`

#### Scenario: Opening the picker creates an entity
- **WHEN** `open_repo_picker` is called
- **THEN** `WorkspaceView.path_picker` is `Some(Entity<PathPicker>)` with the host and initial query set

#### Scenario: Confirming or dismissing sets path_picker to None
- **WHEN** the user confirms a path (Enter) or dismisses the picker (Escape / scrim click)
- **THEN** `WorkspaceView.path_picker` becomes `None`

### Requirement: Removal of old WSL browser and choice dialog
The following SHALL be removed: `WslBrowser` struct, `open_repo_choice_open` field, `wsl_browser` field, `open_repo_choice_dialog` method, `wsl_browser_dialog` method, `open_wsl_browser` method, `load_wsl_dir` method, `navigate_wsl_dir` method, `commit_wsl_browser` method. The `render` method's modal overlay section SHALL replace the `open_repo_choice_open` and `wsl_browser` conditionals with a single `if let Some(picker) = &self.path_picker { root.child(picker.clone()) }`. The `on_key_down` Esc handler SHALL remove the `open_repo_choice_open` and `wsl_browser` branches (the `PathPicker` handles its own Escape).

#### Scenario: WslBrowser struct is removed
- **WHEN** the code is compiled
- **THEN** there is no `WslBrowser` struct, no `open_repo_choice_open` field, no `wsl_browser` field in `WorkspaceView`

#### Scenario: Old dialog methods are removed
- **WHEN** the code is compiled
- **THEN** `open_repo_choice_dialog`, `wsl_browser_dialog`, `open_wsl_browser`, `load_wsl_dir`, `navigate_wsl_dir`, `commit_wsl_browser` do not exist on `WorkspaceView`

### Requirement: Test accessors for PathPicker state
The `PathPicker` SHALL expose `#[cfg(feature = "test-support")]` accessors for testing: `path_picker_open()` (returns whether the picker is open), `path_picker_query()` (returns the current query text), `path_picker_filtered_count()` (returns the number of filtered entries), `path_picker_selected_index()` (returns the selected index), `path_picker_entries()` (returns the entry names), `set_path_picker_query_for_test(query)` (sets the input and triggers `update_matches`), `path_picker_confirm_for_test()` (triggers confirm), `path_picker_cancel_for_test()` (triggers dismiss).

#### Scenario: Test can set query and check filtered entries
- **WHEN** a test sets `path_picker` query to `/home/user/` and the mock host has entries `[src, target, docs]`
- **THEN** `path_picker_filtered_count()` returns 3 and `path_picker_entries()` returns `["src", "target", "docs"]`

#### Scenario: Test can filter by suffix
- **WHEN** a test sets query to `/home/user/do` and the mock host has entries `[src, target, docs]`
- **THEN** `path_picker_filtered_count()` returns 1 and `path_picker_entries()` returns `["docs"]`

#### Scenario: Test can confirm a path
- **WHEN** a test calls `path_picker_confirm_for_test()` and the selected entry is a git repo root
- **THEN** `set_repo` is called and `path_picker_open()` returns false
