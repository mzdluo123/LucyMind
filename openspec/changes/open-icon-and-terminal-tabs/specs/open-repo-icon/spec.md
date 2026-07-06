## ADDED Requirements

### Requirement: Open repository button is an icon
The sidebar repository row SHALL use a `folder-open` SVG icon button (no background, no border, group-hover color change) instead of a text button, matching the visual style of the gear icon in the WORKTREES header and the `+` icon in the AGENTS header. The icon button SHALL trigger the existing `open_repo_picker` flow on click.

#### Scenario: Open button renders as an icon
- **WHEN** the sidebar is rendered with a repository open
- **THEN** the repository row shows a `folder-open` SVG icon on the right (not the text "Open…"), with no button background or border, and `TEXT_FAINT` color that brightens to `TEXT` on hover

#### Scenario: Open button has no repository
- **WHEN** the sidebar is rendered with no repository (`repo == None`)
- **THEN** the repository row shows "no repository" text and the `folder-open` icon button on the right, both visible; clicking the icon opens the directory picker

#### Scenario: Clicking the icon opens the directory picker
- **WHEN** the user clicks the `folder-open` icon button
- **THEN** `open_repo_picker` is invoked, opening the native directory selection dialog

### Requirement: Folder-open icon asset registered
A `folder-open.svg` icon (Lucide-style, `stroke="currentColor"`, 24×24 viewBox) SHALL be added to `crates/app/assets/icons/` and registered in `assets.rs` (`AssetSource::load` match + `list()`), making it loadable via `gpui::svg().path("icons/folder-open.svg")`.

#### Scenario: Icon is loadable via asset source
- **WHEN** `Assets.load("icons/folder-open.svg")` is called
- **THEN** it returns the SVG bytes (not `None`)

#### Scenario: Icon is listed
- **WHEN** `Assets.list("")` is called
- **THEN** the result includes `"icons/folder-open.svg"`
