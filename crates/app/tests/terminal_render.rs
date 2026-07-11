//! 终端渲染:PTY 输出出现在 snapshot 中。

use std::time::Duration;

use gpui::{point, px, size, TestAppContext};

use common::{
    build_workspace, shutdown_workspace, temp_repo_with_agent, wait_for, wait_for_shell_ready,
};

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

    // 等 worktree 创建完成(active_path 就绪)。
    wait_for(
        cx,
        |cx| cx.update(|cx| workspace.update(cx, |v, _| v.active_path().is_some())),
        Duration::from_secs(15),
    );
    let wt_path = cx.update(|cx| {
        workspace.update(cx, |v, _| v.active_path().map(|p| p.to_path_buf()).unwrap())
    });

    // 等 shell 就绪(PTY spawn 有延迟,CI 机器负载高时可能 >500ms)。
    // 轮询 snapshot 非空(shell 打印提示符)确保 PTY reader 已工作 + shell 可接收命令。
    wait_for_shell_ready(cx, &workspace, &wt_path, Duration::from_secs(15));

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

#[gpui::test]
async fn opened_terminal_tolerates_zero_size_paint(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo_with_agent();
    let (workspace, window) = build_workspace(cx, Some(repo.clone()));
    window.run_until_parked();

    window.update(|_, cx| {
        workspace.update(cx, |v, cx| v.open_worktree_for_test(repo.clone(), cx));
    });
    window.run_until_parked();

    window.draw(point(px(0.0), px(0.0)), size(px(0.0), px(0.0)), |_, _| {
        workspace.clone()
    });

    window.update(|_, cx| {
        workspace.update(cx, |view, cx| {
            view.shutdown_all_terminals_for_test(cx);
        });
    });
    window.run_until_parked();
}
