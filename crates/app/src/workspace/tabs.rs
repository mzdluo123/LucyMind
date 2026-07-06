//! 终端面板顶部的 tab 栏 + launcher 菜单。
//!
//! 作为 [`WorkspaceView`](super::WorkspaceView) 的 `impl` 方法(跨文件 impl)。
//! - tab 栏:每个 tab = 标题(动态 OSC 0/2 优先,回退静态)+ `✕` 关闭;
//!   末尾 `+` 按钮打开 launcher 菜单(新建 tab / 启动 agent)。active tab 顶部标记线高亮。
//! - launcher 菜单:`+` 按钮的下拉菜单,分 New Tab(shell 类型)和 Launch Agent 两组。

use gpui::{
    div, px, rgb, Context, InteractiveElement, IntoElement, ParentElement, SharedString, Stateful,
    StatefulInteractiveElement, Styled,
};

use crate::theme;

use super::{ShellKind, WorkspaceView};

/// tab 栏高度(px),launcher 菜单的 `top` 偏移以此为准。
const TAB_BAR_H: f32 = 32.0;

impl WorkspaceView {
    /// tab 栏。active worktree 无终端时返回 `h_0`(不占空间)。
    /// 结构:`[tab_list (flex_1, overflow_x_scroll)] [+ 按钮] [reveal 按钮] (均 flex_none)`。
    pub(super) fn tab_bar(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        // 无 active / 无 group / 空 tabs → 不渲染 tab 栏。
        let Some(group) = self.active.as_ref().and_then(|p| self.terminals.get(p)) else {
            return div().h_0().flex_none().into_any_element();
        };
        if group.tabs.is_empty() {
            return div().h_0().flex_none().into_any_element();
        }

        // tab 区(左侧,可横向滚动)+ `+` 按钮 + reveal 按钮(右侧,固定)。
        // overflow_hidden: 防止 tab_list 内容溢出时撑宽 tab_bar,把按钮顶出可视区。
        //   Overflow::Hidden 使 tab_bar 的 automatic min-size 归零(flex item 不会
        //   因 content 撑大),且裁剪越界子元素 —— tab_list 自身的 overflow_x_scroll
        //   负责实际滚动,tab_bar 只需保证不溢出。
        div()
            .flex_none()
            .h(px(TAB_BAR_H))
            .w_full()
            .overflow_hidden()
            .flex()
            .flex_row()
            .bg(rgb(theme::SURFACE))
            .border_b_1()
            .border_color(rgb(theme::BORDER))
            .child(self.tab_list(group, cx))
            .child(self.plus_button(cx))
            .child(self.reveal_button(cx))
            .into_any_element()
    }

    /// tab 列表(左侧):每个 tab = 标题 + `✕`。可横向滚动。
    fn tab_list(&self, group: &super::TerminalGroup, cx: &mut Context<Self>) -> impl IntoElement {
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

        tabs
    }

    /// `+` 按钮(右侧,固定位置):点击打开 launcher 菜单。
    /// 移出 `tab_list`(滚动区),始终可见。
    fn plus_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("launcher-trigger")
            .flex_none()
            .px(theme::space_sm())
            .h_full()
            .flex()
            .items_center()
            .cursor_pointer()
            .child(
                gpui::svg()
                    .size(px(14.0))
                    .path("icons/plus.svg")
                    .text_color(rgb(theme::TEXT_FAINT)),
            )
            .hover(|s| s.bg(rgb(theme::BTN_BG_HOVER)))
            .on_click(cx.listener(|this, _ev, _w, cx| {
                this.launcher_menu_open = !this.launcher_menu_open;
                cx.notify();
            }))
    }

    /// 「在文件管理器中打开」按钮(reveal button):点击用系统命令打开 active worktree 目录。
    /// 在 `+` 按钮右边,`tab_bar` 直接 child(滚动区外),始终可见。
    fn reveal_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("reveal-in-file-manager")
            .flex_none()
            .px(theme::space_sm())
            .h_full()
            .flex()
            .items_center()
            .cursor_pointer()
            .child(
                gpui::svg()
                    .size(px(14.0))
                    .path("icons/folder-open.svg")
                    .text_color(rgb(theme::TEXT_FAINT)),
            )
            .hover(|s| s.bg(rgb(theme::BTN_BG_HOVER)))
            .on_click(cx.listener(|this, _ev, _w, cx| {
                this.reveal_in_file_manager(cx);
            }))
    }

    /// 单个 tab:标题(动态优先,静态回退)+ `✕` 关闭按钮。
    /// tab 宽度自适应:`flex_1` + `min_w(80px)` + `max_w(200px)`,少时宽(≤200px)、
    /// 多时缩窄(≥80px)、超出 80px 下限后 `overflow_x_scroll` 横向滚动。
    fn tab_item(
        &self,
        index: usize,
        tab: &super::TerminalTab,
        is_active: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // 标题:动态 OSC 0/2 优先,回退静态标题(ShellKind::label)。
        let dynamic_title = tab.terminal.read(cx).title().map(|s| s.to_string());
        let static_title = tab.title.clone();
        let title: SharedString = SharedString::from(dynamic_title.unwrap_or(static_title));

        let close_index = index;
        let switch_index = index;

        div()
            .id(SharedString::from(format!("tab-{index}")))
            .flex_1()
            .min_w(px(80.0))
            .max_w(px(200.0))
            .h_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme::space_xs())
            .px(theme::space_sm())
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

    /// launcher 菜单(`+` 按钮下拉)。backdrop + card,点击外部 / Esc 关闭。
    /// card 在 tab 栏下方右对齐(tab 栏高 32px → `top(32px)`,`right_0`)。
    pub(super) fn launcher_menu(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        // backdrop:点击关闭。
        let backdrop = div().absolute().inset_0().on_mouse_down(
            gpui::MouseButton::Left,
            cx.listener(|this, _ev, _w, cx| {
                this.launcher_menu_open = false;
                cx.notify();
                cx.stop_propagation();
            }),
        );

        // New Tab 分组标题。
        let new_tab_header = div()
            .px(theme::space_sm())
            .py(theme::space_xs())
            .text_xs()
            .text_color(rgb(theme::TEXT_DIM))
            .child(SharedString::from("New Tab"));

        // New Tab 菜单项。
        let default_item = self.menu_item("Default Shell", None, cx, |this, cx| {
            this.new_terminal_tab(ShellKind::Default, cx);
            this.launcher_menu_open = false;
        });

        let mut card = div()
            .absolute()
            .top(px(TAB_BAR_H))
            .right_0()
            .bg(rgb(theme::SURFACE))
            .border_1()
            .border_color(rgb(theme::BORDER))
            .rounded(theme::radius())
            .py(theme::space_xs())
            .flex()
            .flex_col()
            .min_w(px(200.0))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|_this, _ev, _w, cx| {
                    cx.stop_propagation();
                }),
            )
            .child(new_tab_header)
            .child(default_item);

        // Windows 专属 shell 选项(仅在本地模式显示;WSL 模式下只有 Default)。
        #[cfg(windows)]
        {
            if !self.host.is_remote() {
                card = card
                    .child(self.menu_item("Command Prompt", None, cx, |this, cx| {
                        this.new_terminal_tab(ShellKind::Cmd, cx);
                        this.launcher_menu_open = false;
                    }))
                    .child(self.menu_item("PowerShell", None, cx, |this, cx| {
                        this.new_terminal_tab(ShellKind::PowerShell, cx);
                        this.launcher_menu_open = false;
                    }))
                    .child(self.menu_item("PowerShell 7", None, cx, |this, cx| {
                        this.new_terminal_tab(ShellKind::Pwsh, cx);
                        this.launcher_menu_open = false;
                    }));
            }
        }

        // 分隔线。
        card = card.child(
            div()
                .h_1()
                .mx(theme::space_sm())
                .my(theme::space_xs())
                .bg(rgb(theme::BORDER)),
        );

        // Launch Agent 分组标题。
        card = card.child(
            div()
                .px(theme::space_sm())
                .py(theme::space_xs())
                .text_xs()
                .text_color(rgb(theme::TEXT_DIM))
                .child(SharedString::from("Launch Agent")),
        );

        // Agent 菜单项:迭代 builtin_agents()。
        for agent in lucy_core::agent::builtin_agents() {
            let name = agent.name.to_string();
            let display = agent.display;
            let icon = crate::assets::agent_icon(agent.name).map(SharedString::from);
            card = card.child(self.menu_item(display, icon, cx, move |this, cx| {
                this.launch_agent(&name, cx);
                this.launcher_menu_open = false;
            }));
        }

        div()
            .absolute()
            .inset_0()
            .child(backdrop)
            .child(card)
            .into_any_element()
    }

    /// launcher 菜单项(图标 + 文字,hover 高亮,点击执行动作 + 关菜单)。
    fn menu_item(
        &self,
        label: &str,
        icon: Option<SharedString>,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut WorkspaceView, &mut Context<WorkspaceView>) + 'static,
    ) -> Stateful<gpui::Div> {
        let mut item = div()
            .id(SharedString::from(format!("menu-{label}")))
            .px(theme::space_md())
            .py(theme::space_xs())
            .flex()
            .flex_row()
            .items_center()
            .gap(theme::space_xs())
            .cursor_pointer()
            .text_color(rgb(theme::TEXT))
            .hover(|s| s.bg(rgb(theme::BTN_BG_HOVER)))
            .on_click(cx.listener(move |this, _ev, _w, cx| {
                on_click(this, cx);
                cx.notify();
            }));

        if let Some(path) = icon {
            item = item.child(
                gpui::svg()
                    .flex_none()
                    .size(px(14.0))
                    .path(path)
                    .text_color(rgb(theme::TEXT)),
            );
        }

        item.child(SharedString::from(label.to_string()))
    }
}
