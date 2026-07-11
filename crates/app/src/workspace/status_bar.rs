//! 主区底部状态栏(编辑器风格:常驻、极细、克制)。
//!
//! 显示最近一条动作反馈 / 错误([`Status`](super::Status));空态也占位以稳定布局。

use gpui::{
    div, px, rgb, AppContext, Context, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled,
};
use gpui_component::tooltip::Tooltip;
use lucy_core::github::PullRequestStatus;

use crate::theme;

use super::WorkspaceView;

pub(super) fn pull_request_status_icon(status: PullRequestStatus) -> (&'static str, u32) {
    match status {
        PullRequestStatus::Draft => ("icons/circle-dot-dashed.svg", theme::TEXT_DIM),
        PullRequestStatus::Merged => ("icons/git-merge.svg", theme::STATE_OK),
        PullRequestStatus::Closed => ("icons/circle-x.svg", theme::TEXT_DIM),
        PullRequestStatus::ChecksFailed => ("icons/circle-x.svg", theme::STATE_ERROR),
        PullRequestStatus::ChecksPending => ("icons/clock-3.svg", theme::TEXT_DIM),
        PullRequestStatus::ChangesRequested => {
            ("icons/message-circle-warning.svg", theme::STATE_ERROR)
        }
        PullRequestStatus::Approved => ("icons/circle-check-big.svg", theme::STATE_OK),
        PullRequestStatus::Open => ("icons/circle-dot.svg", theme::TEXT),
    }
}

impl WorkspaceView {
    pub(super) fn status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (text, color) = match &self.status {
            Some(s) if s.is_error => (s.text.clone(), theme::STATE_ERROR),
            Some(s) => (s.text.clone(), theme::TEXT_DIM),
            None => (SharedString::from(""), theme::TEXT_FAINT),
        };
        let mut bar = div()
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
            );

        if let Some(pr) = &self.pull_request {
            let tooltip = SharedString::from(pr.display_label());
            let number = SharedString::from(format!("#{}", pr.number));
            let title = SharedString::from(pr.title.clone());
            let (status_icon, status_color) = pull_request_status_icon(pr.status());
            bar = bar.child(
                div()
                    .id("current-pull-request")
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(theme::space_xs())
                    .max_w(gpui::relative(0.55))
                    .ml(theme::space_md())
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_color(rgb(theme::TEXT))
                    .cursor_pointer()
                    .hover(|s| s.text_color(rgb(theme::TEXT_BRIGHT)))
                    .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tooltip.clone())).into())
                    .on_click(cx.listener(|this, _ev, _window, cx| {
                        this.open_pull_request(cx);
                    }))
                    .child(
                        gpui::svg()
                            .flex_none()
                            .size(px(13.0))
                            .path("icons/git-pull-request.svg")
                            .text_color(rgb(theme::TEXT_DIM)),
                    )
                    .child(number)
                    .child(
                        gpui::svg()
                            .flex_none()
                            .size(px(13.0))
                            .path(status_icon)
                            .text_color(rgb(status_color)),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(title),
                    ),
            );
        }

        bar
    }
}
