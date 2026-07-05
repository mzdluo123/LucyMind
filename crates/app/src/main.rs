//! lucy — worktree + agent 编排桌面工具入口。
//!
//! U8:起一个 GPUI 窗口,内嵌一个渲染**真实终端会话**的 [`TerminalView`]
//! (alacritty 内核 + PTY,跑默认 shell)。这替换了 spike 的静态网格,
//! 是通往 U9 端到端主流程的渲染基座。

mod terminal_view;

use gpui::{prelude::*, px, size, App, Application, Bounds, WindowBounds, WindowOptions};

use terminal_view::TerminalView;

fn main() {
    env_logger::init();

    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(900.), px(560.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(|cx| {
                    // 跑默认交互式 shell,验证真实终端渲染 + 输入回显。
                    TerminalView::new(cx, None, None, vec![])
                        .expect("failed to start terminal session")
                });
                // 聚焦终端,使键盘输入直达 PTY。
                window.focus(&view.read(cx).focus_handle_for_init());
                view
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
