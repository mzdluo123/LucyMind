//! 终端面板顶部的 tab 栏 + agent 按钮行。
//!
//! 作为 [`WorkspaceView`](super::WorkspaceView) 的 `impl` 方法(跨文件 impl)。
//! - tab 栏:每个 tab = 标题(动态 OSC 0/2 优先,回退静态 "Shell")+ `✕` 关闭;
//!   末尾 `+` 新建 shell tab。active tab 顶部标记线高亮。
//! - agent 按钮行:迭代 `builtin_agents()`,点击往当前 shell 发 agent 启动命令。

use gpui::{
    div, rgb, Context, InteractiveElement, IntoElement, ParentElement, SharedString, Stateful,
    StatefulInteractiveElement, Styled,
};

use crate::theme;

use super::{TerminalGroup, WorkspaceView};

impl WorkspaceView {
    /// tab 栏 + agent 按钮行。active worktree 无终端时返回 `h_0`(不占空间)。
    pub(super) fn tab_bar(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        // 无 active / 无 group / 空 tabs → 不渲染 tab 栏。
        let Some(group) = self.active.as_ref().and_then(|p| self.terminals.get(p)) else {
            return div().h_0().flex_none().into_any_element();
        };
        if group.tabs.is_empty() {
            return div().h_0().flex_none().into_any_element();
        }

        // tab 区(左侧,可横向滚动)+ agent 按钮区(右侧,固定)。
        div()
            .flex_none()
            .h(gpui::px(32.0))
            .flex()
            .flex_row()
            .bg(rgb(theme::SURFACE))
            .border_b_1()
            .border_color(rgb(theme::BORDER))
            .child(self.tab_list(group, cx))
            .child(self.agent_buttons(cx))
            .into_any_element()
    }

    /// tab 列表(左侧):每个 tab = 标题 + `✕`;末尾 `+` 新建 shell tab。
    fn tab_list(&self, group: &TerminalGroup, cx: &mut Context<Self>) -> impl IntoElement {
        let mut tabs = div()
            .id("tab-list")
            .flex_1()
            .min_w_0()
            .flex()
            .flex_row()
            .overflow_x_scroll();

        for (i, tab) in group.tabs.iter().enumerate() {
            tabs = tabs.child(self.tab_item(i, tab, group.active_tab == i, cx));
        }

        // 末尾 `+` 新建 shell tab。
        tabs = tabs.child(
            div()
                .id("new-tab")
                .flex_none()
                .px(theme::space_sm())
                .h_full()
                .flex()
                .items_center()
                .cursor_pointer()
                .child(
                    gpui::svg()
                        .size(gpui::px(14.0))
                        .path("icons/plus.svg")
                        .text_color(rgb(theme::TEXT_FAINT)),
                )
                .hover(|s| s.bg(rgb(theme::BTN_BG_HOVER)))
                .on_click(cx.listener(|this, _ev, _w, cx| {
                    this.new_terminal_tab(cx);
                })),
        );

        tabs
    }

    /// 单个 tab:标题(动态优先,静态回退)+ `✕` 关闭按钮。
    fn tab_item(
        &self,
        index: usize,
        tab: &super::TerminalTab,
        is_active: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // 标题:动态 OSC 0/2 优先,回退静态 "Shell"。先提取成 owned String,
        // 避免 `terminal.read(cx)` 的借用与后续 `cx.listener` 冲突。
        let dynamic_title = tab.terminal.read(cx).title().map(|s| s.to_string());
        let static_title = tab.title.clone();
        let title: SharedString = SharedString::from(dynamic_title.unwrap_or(static_title));

        let close_index = index;
        let switch_index = index;

        div()
            .id(SharedString::from(format!("tab-{index}")))
            .flex_none()
            .h_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme::space_xs())
            .px(theme::space_sm())
            .min_w(gpui::px(80.0))
            .max_w(gpui::px(200.0))
            // active tab 顶部标记线 + 抬升底色;inactive 平底 + 暗字。
            .bg(rgb(if is_active {
                theme::SURFACE_RAISED
            } else {
                theme::SURFACE
            }))
            .border_t_2()
            .border_color(rgb(if is_active {
                theme::TEXT_BRIGHT
            } else {
                theme::SURFACE
            }))
            .text_color(rgb(if is_active {
                theme::TEXT
            } else {
                theme::TEXT_DIM
            }))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(theme::BTN_BG_HOVER)))
            .overflow_hidden()
            .on_click(cx.listener(move |this, _ev, _w, cx| {
                this.switch_tab(switch_index, cx);
            }))
            .child(
                // 标题(单行省略)。
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(title),
            )
            .child(
                // `✕` 关闭:仅关该 tab,不触发 switch(stop_propagation)。
                div()
                    .id(SharedString::from(format!("tab-close-{index}")))
                    .flex_none()
                    .px(theme::space_xs())
                    .text_color(rgb(theme::TEXT_FAINT))
                    .hover(|s| s.text_color(rgb(theme::STATE_ERROR)))
                    .child(SharedString::from("✕"))
                    .on_click(cx.listener(move |this, _ev, _w, cx| {
                        cx.stop_propagation();
                        this.close_tab(close_index, cx);
                    })),
            )
    }

    /// agent 按钮行(右侧):迭代 `builtin_agents()`,点击发命令到当前 shell。
    fn agent_buttons(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut row = div()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme::space_xs())
            .pr(theme::space_sm());

        for agent in lucy_core::agent::builtin_agents() {
            let name = agent.name.to_string();
            let icon = crate::assets::agent_icon(agent.name);
            let mut btn: Stateful<gpui::Div> = div()
                .id(SharedString::from(format!("agent-btn-{name}")))
                .flex_none()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme::space_xs())
                .px(theme::space_sm())
                .h(gpui::px(24.0))
                .bg(rgb(theme::BTN_BG))
                .border_1()
                .border_color(rgb(theme::BORDER))
                .rounded(theme::radius())
                .text_color(rgb(theme::TEXT))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(theme::BTN_BG_HOVER)))
                .on_click(cx.listener(move |this, _ev, _w, cx| {
                    this.send_agent_command(&name, cx);
                }));
            if let Some(path) = icon {
                btn = btn.child(
                    gpui::svg()
                        .flex_none()
                        .size(gpui::px(14.0))
                        .path(path)
                        .text_color(rgb(theme::TEXT)),
                );
            }
            btn = btn.child(SharedString::from(agent.display));
            row = row.child(btn);
        }

        row
    }
}
