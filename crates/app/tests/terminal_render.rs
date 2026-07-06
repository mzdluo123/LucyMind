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

    // new_worktree 开 shell(command=None),不自动起 agent。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.new_worktree_for_test(cx));
    });

    // 等 shell 就绪(PTY spawn 有延迟,先等终端存在)。
    wait_for(
        cx,
        |cx| cx.update(|cx| workspace.update(cx, |v, _| v.active_path().is_some())),
        Duration::from_secs(15),
    );

    // 给 shell 进程时间启动并准备好接收输入(PTY 缓冲了输入,但 shell
    // 需要时间 spawn + 初始化)。
    std::thread::sleep(Duration::from_millis(500));

    // 通过 send_agent_command 往 shell 发 "test" agent 命令。
    // test agent 配置为 echo/printf RENDER_MARKER,命令执行后输出出现在 snapshot。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.send_agent_command_for_test("test", cx));
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
