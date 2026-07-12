//! lucy-app 库入口 —— 供 bin 与集成测试共用。
//!
//! bin(`main.rs`)只调 [`run`];集成测试(`tests/`)通过 `use lucy_app::*`
//! 导入 `WorkspaceView`/`TerminalView` 等被测类型,配合 gpui `TestAppContext`
//! 做 headless UI 测试。

pub mod assets;
pub mod logging;
mod path_env;
pub mod terminal_view;
pub mod theme;
pub mod ui;
pub mod workspace;

use std::sync::Arc;

use gpui::{
    prelude::*, px, size, App, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions,
};

use lucy_core::host::LocalHost;

use assets::Assets;
use workspace::WorkspaceView;

/// 启动 LucyMind GUI 窗口。
///
/// bin 与测试 harness 共用此入口:bin 直接调用;测试 harness 不调用此函数
/// (它用 `TestAppContext` 自建窗口),但保留此函数让 bin 瘦身。
pub fn run() {
    // 修复 PATH:从 .app(Finder/Dock)启动时进程 PATH 极简,不含用户 shell 里
    // 加的目录(claude/codex 常装在 ~/.local/bin、/opt/homebrew/bin)。这里趁
    // GPUI 尚未起线程(单线程,改 env 安全)从登录 shell 取回完整 PATH,后续起的
    // agent/shell/hook 才找得到命令。见 [`path_env`]。
    path_env::fix_path_from_login_shell();

    // 候选仓库:当前工作目录。new() 会校验它是不是 git 仓库 —— 是则用(cargo run
    // 场景),否则以空态启动并弹目录选择器(.app 双击启动 cwd 不是仓库的场景)。
    let candidate = std::env::current_dir().ok();

    // Host:默认 LocalHost。用户点「打开仓库」时可选 Local(系统文件选择器)
    // 或 WSL(路径输入),运行时切换 Host。
    let host: Arc<dyn lucy_core::host::Host> = Arc::new(LocalHost);

    // with_assets:注册内嵌 SVG 图标源,svg().path("icons/...") 才能加载。
    Application::new()
        .with_assets(Assets)
        .run(move |cx: &mut App| {
            // 初始化 gpui-component(其 Input 等组件依赖 theme/global 状态)。
            gpui_component::init(cx);
            theme::configure_component_theme(cx);

            let bounds = Bounds::centered(None, size(px(1100.), px(680.0)), cx);
            let host = host.clone();
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
                move |window, cx| {
                    // 把根视图包进 gpui-component 的 Root —— 它的 Input/弹层/焦点管理
                    // 依赖 Root 提供的全局上下文,否则渲染/聚焦 Input 会 panic。
                    let workspace = cx
                        .new(|cx| WorkspaceView::new(window, cx, candidate.clone(), host.clone()));
                    let view: gpui::AnyView = workspace.into();
                    cx.new(|cx| gpui_component::Root::new(view, window, cx))
                },
            )
            .unwrap();
            cx.activate(true);
        });
}
