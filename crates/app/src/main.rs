//! lucy — worktree + agent 编排桌面工具入口。
//!
//! 当前为 **U8-spike**:验证全项目最高风险、无先例的部分 ——
//! 用 GPUI 起窗口,用自定义 `canvas` 在 paint 回调里手绘一屏带前景/背景色的
//! 「cell 网格」。这里用自造的静态网格数据(模拟 wezterm `Screen` 会提供的
//! 字符 + 颜色),独立验证「GPUI 依赖可编译 + 自绘 cell 网格路线可行」。
//! wezterm-term 真正的内核接入留给 U6。

mod terminal_grid;

use gpui::{
    div, prelude::*, px, rgb, size, App, Application, Bounds, Context, Window, WindowBounds,
    WindowOptions,
};

use terminal_grid::{GridView, StaticGrid};

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(760.), px(480.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_window, cx| cx.new(|_cx| GridView::new(StaticGrid::demo())),
        )
        .unwrap();
        cx.activate(true);
    });
}

/// 顶层 View:把网格用一个占满窗口的容器包起来,内部由 `GridView` 的
/// canvas 手绘。此处保留一个简单外壳,证明 View/Render 装配正确。
impl Render for GridView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0x1e1e1e))
            .child(self.canvas_element())
    }
}
