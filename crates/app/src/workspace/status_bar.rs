//! 主区底部状态栏(编辑器风格:常驻、极细、克制)。
//!
//! 显示最近一条动作反馈 / 错误([`Status`](super::Status));空态也占位以稳定布局。

use gpui::{div, rgb, IntoElement, ParentElement, SharedString, Styled};

use crate::theme;

use super::WorkspaceView;

impl WorkspaceView {
    pub(super) fn status_bar(&self) -> impl IntoElement {
        let (text, color) = match &self.status {
            Some(s) if s.is_error => (s.text.clone(), theme::STATE_ERROR),
            Some(s) => (s.text.clone(), theme::TEXT_DIM),
            None => (SharedString::from(""), theme::TEXT_FAINT),
        };
        div()
            .h(gpui::px(24.0))
            .flex_none() // 固定高度,不被压缩
            .flex()
            .flex_row()
            .items_center()
            .px(theme::space_md())
            .bg(rgb(theme::SURFACE))
            .border_t_1()
            .border_color(rgb(theme::BORDER))
            .text_color(rgb(color))
            .overflow_hidden()
            // 单行 + 超长截断省略号,绝不换行。关键:flex 子项要 min_w_0 才会收缩,
            // 否则默认 min-width:auto 会撑开导致换行/溢出。
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(text),
            )
    }
}
