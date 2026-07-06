//! lucy-app 库入口 —— 供 bin 与集成测试共用。
//!
//! bin(`main.rs`)只调 [`run`];集成测试(`tests/`)通过 `use lucy_app::*`
//! 导入 `WorkspaceView`/`TerminalView` 等被测类型,配合 gpui `TestAppContext`
//! 做 headless UI 测试。

pub mod assets;
pub mod terminal_view;
pub mod theme;
pub mod ui;
pub mod workspace;

use gpui::{
    prelude::*, px, size, App, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions,
};

use assets::Assets;
use workspace::WorkspaceView;

/// 启动 LucyMind GUI 窗口。
///
/// bin 与测试 harness 共用此入口:bin 直接调用;测试 harness 不调用此函数
/// (它用 `TestAppContext` 自建窗口),但保留此函数让 bin 瘦身。
pub fn run() {
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
    Application::new()
        .with_assets(Assets)
        .run(move |cx: &mut App| {
            // 初始化 gpui-component(其 Input 等组件依赖 theme/global 状态)。
            gpui_component::init(cx);

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
                move |window, cx| {
                    // 把根视图包进 gpui-component 的 Root —— 它的 Input/弹层/焦点管理
                    // 依赖 Root 提供的全局上下文,否则渲染/聚焦 Input 会 panic。
                    let workspace = cx.new(|cx| WorkspaceView::new(cx, candidate.clone()));
                    let view: gpui::AnyView = workspace.into();
                    cx.new(|cx| gpui_component::Root::new(view, window, cx))
                },
            )
            .unwrap();
            cx.activate(true);
        });
}
