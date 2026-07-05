//! lucy — worktree + agent 编排桌面工具入口。
//!
//! U9:起 GPUI 窗口,加载 [`WorkspaceView`](左侧栏 + 终端区),仓库根取
//! 当前工作目录。用户点「New worktree → claude/codex」即走完整主流程:
//! 建 worktree → postCreate hook → 在 worktree 起 agent → 显示在终端。

mod assets;
mod terminal_view;
mod theme;
mod workspace;

use gpui::{
    prelude::*, px, size, App, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions,
};

use assets::Assets;
use workspace::WorkspaceView;

fn main() {
    // 默认:第三方 crate 只 warn,我们自己的 crate 开 info(能看到 [close] 计时
    // 日志,又不被 gpui/wgpu 的 info 刷屏)。仍可被 RUST_LOG 覆盖。
    env_logger::Builder::from_env(
        env_logger::Env::default()
            .default_filter_or("warn,lucy_app=info,lucy_terminal=info,lucy_core=info"),
    )
    .init();

    // 候选仓库:当前工作目录。new() 会校验它是不是 git 仓库 —— 是则用(cargo run
    // 场景),否则以空态启动并弹目录选择器(.app 双击启动 cwd 不是仓库的场景)。
    let candidate = std::env::current_dir().ok();

    // with_assets:注册内嵌 SVG 图标源,svg().path("icons/...") 才能加载。
    Application::new().with_assets(Assets).run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1100.), px(680.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                // 标准 macOS 标题栏 + 红绿灯(标题显示 LucyMind)。全屏时 macOS
                // 会自动隐藏标题栏、鼠标移到顶部才浮现 —— 这是系统标准行为。
                titlebar: Some(TitlebarOptions {
                    title: Some("LucyMind".into()),
                    appears_transparent: false,
                    traffic_light_position: None,
                }),
                app_id: Some("win.rainchan.lucymind".into()),
                ..Default::default()
            },
            move |_window, cx| cx.new(|cx| WorkspaceView::new(cx, candidate.clone())),
        )
        .unwrap();
        cx.activate(true);
    });
}
