//! 终端渲染:PTY 输出出现在 snapshot 中。

use std::time::Duration;

use gpui::TestAppContext;

use common::{build_workspace, shutdown_workspace, temp_repo_with_agent, wait_for};

mod common;

fn marker_toml() -> String {
    if cfg!(windows) {
        "[worktree]\nlocation = \"sibling\"\ndir = \"../{repo}-worktrees\"\n\
         [agents.test]\ncommand = \"cmd.exe\"\nargs = [\"/c\", \"echo RENDER_MARKER\"]\n"
            .to_string()
    } else {
        "[worktree]\nlocation = \"sibling\"\ndir = \"../{repo}-worktrees\"\n\
         [agents.test]\ncommand = \"sh\"\nargs = [\"-c\", \"printf RENDER_MARKER\"]\n"
            .to_string()
    }
}

#[gpui::test]
async fn pty_output_appears_in_snapshot(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo_with_agent();
    std::fs::write(repo.join(".worktree.toml"), marker_toml()).unwrap();

    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.new_worktree_and_agent_for_test("test", cx));
    });

    wait_for(
        cx,
        |cx| {
            let term = cx.update(|cx| {
                workspace.update(cx, |v, _| {
                    v.active_path().and_then(|p| v.terminal_at(p)).cloned()
                })
            });
            term.is_some_and(|t| cx.read(|cx| t.read(cx).snapshot_text().contains("RENDER_MARKER")))
        },
        Duration::from_secs(30),
    );

    shutdown_workspace(cx, &workspace);
}
