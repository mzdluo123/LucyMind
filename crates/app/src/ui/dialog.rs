//! 模态弹窗骨架 —— 全屏遮罩(压暗背景) + 居中描边卡片。
//!
//! `confirm_dialog` / `alias_dialog` 共用同一外层结构,这里抽出骨架,
//! 各弹窗只负责往卡片里塞标题/正文/按钮行。

use gpui::{div, prelude::*, px, rgb, AnyElement, IntoElement, Styled};

use crate::theme;

/// 遮罩 + 居中卡片。`width` 是卡片宽度(px),`body` 是卡片内容(通常是
/// 一列:标题 + 正文 + 按钮行)。
///
/// 卡片用界面字体(Futura),内部按 [`theme::space_md`] 间距纵向排列。
pub fn modal(width: f32, body: impl IntoElement) -> impl IntoElement {
    // 遮罩(scrim,压暗背景)。
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme::with_alpha(0x00_00_00, 0.55))
        .child(
            // 卡片。
            div()
                .w(px(width))
                .bg(rgb(theme::SURFACE))
                .border_1()
                .border_color(rgb(theme::BORDER))
                .rounded(theme::radius())
                .p(theme::space_lg())
                .flex()
                .flex_col()
                .gap(theme::space_md())
                .font_family(theme::FONT_UI)
                .child(body),
        )
}

/// 弹窗底部的按钮行(右对齐,按钮间 [`theme::space_sm`] 间距)。
pub fn button_row(buttons: impl IntoIterator<Item = AnyElement>) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .justify_end()
        .gap(theme::space_sm())
        .children(buttons)
}
