//! lucy — worktree + agent 编排桌面工具入口。
//!
//! U9:起 GPUI 窗口,加载 [`WorkspaceView`](左侧栏 + 终端区),仓库根取
//! 当前工作目录。用户点「New worktree → claude/codex」即走完整主流程:
//! 建 worktree → postCreate hook → 在 worktree 起 agent → 显示在终端。

mod assets;
mod terminal_view;
mod theme;
mod workspace;

use gpui::{prelude::*, px, size, App, Application, Bounds, WindowBounds, WindowOptions};

use assets::Assets;
use workspace::WorkspaceView;

fn main() {
    env_logger::init();

    // 仓库根:从当前目录解析出**主仓库**根(git worktree list 第一条)。
    // 不能盲信 current_dir —— 从子目录(如 crates/app/assets/icons)启动时它
    // 不是仓库根,会导致 main 保护失效、误删主仓。解析失败(非 git 仓库)才回退。
    let cwd = std::env::current_dir().expect("cannot read current dir");
    let repo = lucy_core::git::main_worktree_root(&cwd)
        .or_else(|| lucy_core::git::toplevel(&cwd))
        .unwrap_or(cwd);

    // with_assets:注册内嵌 SVG 图标源,svg().path("icons/...") 才能加载。
    Application::new().with_assets(Assets).run(move |cx: &mut App| {
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
