## ADDED Requirements

### Requirement: Query the current branch Pull Request

When a worktree becomes active, the application SHALL asynchronously query GitHub for the Pull Request associated with the branch checked out in that worktree.

#### Scenario: Current branch has a Pull Request

- **WHEN** a worktree becomes active and `gh pr view` returns a Pull Request
- **THEN** the application stores its number, title, URL, state, review decision, and check summary

#### Scenario: Current branch has no Pull Request

- **WHEN** GitHub reports no Pull Request for the active branch
- **THEN** the application SHALL leave the Pull Request UI empty and SHALL NOT display an error

#### Scenario: GitHub integration is unavailable

- **WHEN** `gh` is missing, unauthenticated, the repository is not hosted on GitHub, or the query otherwise fails
- **THEN** existing LucyMind workflows SHALL continue without an error message

### Requirement: Display and open the Pull Request

The status bar SHALL display a Pull Request icon, the current Pull Request number, a normalized status icon, and title without replacing action feedback. The full textual status SHALL be available in a hover tooltip. Clicking it SHALL open its GitHub URL in the default browser.

#### Scenario: User opens the Pull Request

- **WHEN** the user clicks the visible Pull Request status
- **THEN** the Pull Request URL opens in the default browser

### Requirement: Ignore stale Pull Request responses

Only a response belonging to the currently active worktree SHALL update the displayed Pull Request.

#### Scenario: User switches branches while a query is running

- **WHEN** the previous worktree query finishes after a new worktree has become active
- **THEN** the previous result SHALL be discarded
