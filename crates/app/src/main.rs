//! lucy — worktree + agent 编排桌面工具入口。
//!
//! U9:起 GPUI 窗口,加载 [`WorkspaceView`](左侧栏 + 终端区),仓库根取
//! 当前工作目录。用户点「New worktree → claude/codex」即走完整主流程:
//! 建 worktree → postCreate hook → 在 worktree 起 agent → 显示在终端。

mod terminal_view;
mod theme;
mod workspace;

use gpui::{prelude::*, px, size, App, Application, Bounds, WindowBounds, WindowOptions};

use workspace::WorkspaceView;

fn main() {
    env_logger::init();

    // 仓库根:当前工作目录(须是 git 仓库;否则 worktree 列表为空,仍可运行)。
    let repo = std::env::current_dir().expect("cannot read current dir");

    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1100.), px(680.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            move |_window, cx| cx.new(|cx| WorkspaceView::new(cx, repo.clone())),
        )
        .unwrap();
        cx.activate(true);
    });
}
