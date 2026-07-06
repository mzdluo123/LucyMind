//! 模态弹窗:关闭确认(未提交改动)+ 别名编辑 + WSL 文件浏览器。
//!
//! 各弹窗共用 [`crate::ui::dialog`] 的遮罩+卡片骨架,只描述卡片内容与按钮回调。
//! 别名编辑还带 gpui-component 的 `Input`(IME + 选择 + 复制粘贴),相关的
//! 状态方法(打开/提交)也收在这里。

use gpui::{
    div, prelude::*, rgb, Context, IntoElement, ParentElement, SharedString, Styled, Window,
};

use lucy_core::config;

use crate::theme;
use crate::ui::{button, button_row, modal, ButtonVariant};

use super::WorkspaceView;
impl WorkspaceView {
    /// 未提交改动确认弹窗(性冷淡风:半透明遮罩 + 描边卡片 + 两个无彩按钮)。
    pub(super) fn confirm_dialog(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let pending = self.pending_close.as_ref();
        let (branch, count) = pending
            .map(|p| (p.branch.clone(), p.dirty_count))
            .unwrap_or_default();

        modal(
            360.0,
            div()
                .flex()
                .flex_col()
                .gap(theme::space_md())
                .child(
                    div()
                        .text_color(rgb(theme::TEXT_BRIGHT))
                        .child(SharedString::from("关闭 worktree?")),
                )
                .child(
                    div()
                        .text_color(rgb(theme::TEXT_DIM))
                        .child(SharedString::from(format!(
                            "{branch} 有 {count} 处未提交改动。关闭将丢弃这些改动。"
                        ))),
                )
                .child(button_row([
                    button("cancel-close", "取消")
                        .on_click(cx.listener(|this, _ev, _w, cx| {
                            this.cancel_close(cx);
                        }))
                        .into_any_element(),
                    button("confirm-close", "丢弃并关闭")
                        .variant(ButtonVariant::Danger)
                        .on_click(cx.listener(|this, _ev, _w, cx| {
                            this.confirm_close(cx);
                        }))
                        .into_any_element(),
                ])),
        )
    }

    /// 打开别名编辑器:懒创建 gpui-component 的 InputState,填入当前别名,聚焦。
    pub(super) fn open_alias_editor(
        &mut self,
        branch: &str,
        init: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use gpui_component::input::InputState;
        // 懒创建输入状态(需要 Window)。
        if self.alias_input.is_none() {
            self.alias_input = Some(cx.new(|cx| InputState::new(window, cx)));
        }
        if let Some(state) = &self.alias_input {
            let init = init.to_string();
            state.update(cx, |s, cx| {
                s.set_value(init, window, cx);
                s.focus(window, cx);
            });
        }
        self.editing_alias = Some(branch.to_string());
        cx.notify();
    }

    /// 别名编辑弹窗:用 gpui-component 的 Input(带 IME + 选择 + 复制粘贴)。
    pub(super) fn alias_dialog(&self, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_component::input::Input;
        let branch = self.editing_alias.clone().unwrap_or_default();
        let input_el = self
            .alias_input
            .as_ref()
            .map(|state| Input::new(state).into_any_element());

        modal(
            380.0,
            div()
                .flex()
                .flex_col()
                .gap(theme::space_md())
                .child(
                    div()
                        .text_color(rgb(theme::TEXT_DIM))
                        .child(SharedString::from(format!("为 {branch} 设置别名"))),
                )
                .children(input_el)
                .child(button_row([
                    button("alias-cancel", "取消")
                        .on_click(cx.listener(|this, _ev, _w, cx| {
                            this.editing_alias = None;
                            cx.notify();
                        }))
                        .into_any_element(),
                    button("alias-save", "保存")
                        .variant(ButtonVariant::Confirm)
                        .on_click(cx.listener(|this, _ev, _w, cx| {
                            this.commit_alias(cx);
                        }))
                        .into_any_element(),
                ])),
        )
    }

    /// 从 InputState 读值,保存别名,关弹窗。
    fn commit_alias(&mut self, cx: &mut Context<Self>) {
        let Some(branch) = self.editing_alias.clone() else {
            return;
        };
        let value = self
            .alias_input
            .as_ref()
            .map(|s| s.read(cx).value().to_string())
            .unwrap_or_default();
        self.save_alias(&branch, value.trim());
        self.editing_alias = None;
        cx.notify();
    }

    /// 保存别名到 .worktree.toml 并重载配置。
    fn save_alias(&mut self, branch: &str, alias: &str) {
        let Some(repo) = self.repo.clone() else {
            return;
        };
        let path = repo.join(".worktree.toml");
        match config::set_alias(self.host.as_ref(), &path, branch, alias) {
            Ok(()) => {
                // 重载配置(拿到新别名),刷新显示。
                if let Ok(loaded) = config::load(self.host.as_ref(), &path) {
                    self.config = loaded.config;
                }
                self.set_status(
                    if alias.trim().is_empty() {
                        format!("已清除 {branch} 的别名")
                    } else {
                        format!("已设别名 {branch} → {alias}")
                    },
                    false,
                );
            }
            Err(e) => self.set_status(format!("保存别名失败:{e}"), true),
        }
    }

    /// 打开仓库选择弹窗:Local(系统文件选择器)或 WSL(文件浏览器)。
    pub(super) fn open_repo_choice_dialog(&self, cx: &mut Context<Self>) -> impl IntoElement {
        modal(
            360.0,
            div()
                .flex()
                .flex_col()
                .gap(theme::space_md())
                .child(
                    div()
                        .text_color(rgb(theme::TEXT_BRIGHT))
                        .child(SharedString::from("打开仓库")),
                )
                .child(
                    div()
                        .text_color(rgb(theme::TEXT_DIM))
                        .child(SharedString::from("选择仓库类型:")),
                )
                .child(button_row([
                    button("repo-choice-local", "本地文件夹…")
                        .variant(ButtonVariant::Confirm)
                        .on_click(cx.listener(|this, _ev, _w, cx| {
                            this.open_repo_choice_open = false;
                            this.open_local_picker(cx);
                            cx.notify();
                        }))
                        .into_any_element(),
                    button("repo-choice-wsl", "WSL 路径…")
                        .on_click(cx.listener(|this, _ev, _w, cx| {
                            this.open_repo_choice_open = false;
                            this.open_wsl_browser(cx);
                            cx.notify();
                        }))
                        .into_any_element(),
                ]))
                .child(button("repo-choice-cancel", "取消").on_click(cx.listener(
                    |this, _ev, _w, cx| {
                        this.open_repo_choice_open = false;
                        cx.notify();
                    },
                ))),
        )
        .into_any_element()
    }

    /// WSL 文件浏览器弹窗:目录树导航 + 选择按钮。
    /// 不用 gpui-component InputState(无需 IME/文本输入),纯列表点击导航。
    pub(super) fn wsl_browser_dialog(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let browser = match &self.wsl_browser {
            Some(b) => b,
            None => return div().into_any_element(),
        };
        let current = browser.current_dir.to_string_lossy().to_string();

        // 目录列表:上方".."返回上级,然后各条目。
        let mut entries_div = div().flex().flex_col().gap(theme::space_xs());

        // ".." 返回上级。
        entries_div = entries_div.child(
            div()
                .id("wsl-up")
                .flex()
                .flex_row()
                .items_center()
                .gap(theme::space_sm())
                .px(theme::space_sm())
                .py(theme::space_xs())
                .cursor_pointer()
                .hover(|s| s.bg(rgb(theme::BTN_BG_HOVER)))
                .text_color(rgb(theme::TEXT_DIM))
                .child(SharedString::from("📁 .."))
                .on_click(cx.listener(|this, _ev, _w, cx| {
                    this.navigate_wsl_dir("..".into(), cx);
                })),
        );

        // 条目列表(可滚动)。
        let mut list = div().flex().flex_col().gap_0().max_h(gpui::px(320.0));

        if browser.loading {
            list = list.child(
                div()
                    .px(theme::space_sm())
                    .py(theme::space_xs())
                    .text_color(rgb(theme::TEXT_FAINT))
                    .child(SharedString::from("加载中…")),
            );
        } else if let Some(err) = &browser.error {
            list = list.child(
                div()
                    .px(theme::space_sm())
                    .py(theme::space_xs())
                    .text_color(rgb(theme::TEXT_DIM))
                    .child(SharedString::from(format!("错误: {err}"))),
            );
        } else if browser.entries.is_empty() {
            list = list.child(
                div()
                    .px(theme::space_sm())
                    .py(theme::space_xs())
                    .text_color(rgb(theme::TEXT_FAINT))
                    .child(SharedString::from("(空目录)")),
            );
        } else {
            for (i, entry) in browser.entries.iter().enumerate() {
                let name = entry.name.clone();
                let is_dir = entry.is_dir;
                let icon = if is_dir { "📁 " } else { "📄 " };
                let text_color = if is_dir {
                    theme::TEXT
                } else {
                    theme::TEXT_FAINT
                };
                let row = div()
                    .id(SharedString::from(format!("wsl-entry-{i}")))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(theme::space_sm())
                    .px(theme::space_sm())
                    .py(theme::space_xs())
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(theme::BTN_BG_HOVER)))
                    .text_color(rgb(text_color))
                    .child(SharedString::from(format!("{icon}{name}")));

                // 目录可点击导航;文件不可点击(灰色但仍 hover)。
                let row = if is_dir {
                    let name_clone = name.clone();
                    row.on_click(cx.listener(move |this, _ev, _w, cx| {
                        this.navigate_wsl_dir(name_clone.clone(), cx);
                    }))
                } else {
                    row
                };
                list = list.child(row);
            }
        }

        entries_div = entries_div.child(list);

        modal(
            460.0,
            div()
                .flex()
                .flex_col()
                .gap(theme::space_md())
                .child(
                    div()
                        .text_color(rgb(theme::TEXT_BRIGHT))
                        .child(SharedString::from("打开 WSL 仓库")),
                )
                .child(
                    div()
                        .text_color(rgb(theme::TEXT_DIM))
                        .child(SharedString::from(format!("当前: {current}"))),
                )
                .child(entries_div)
                .child(button_row([
                    button("wsl-browser-cancel", "取消")
                        .on_click(cx.listener(|this, _ev, _w, cx| {
                            this.wsl_browser = None;
                            cx.notify();
                        }))
                        .into_any_element(),
                    button("wsl-browser-open", "选择此目录")
                        .variant(ButtonVariant::Confirm)
                        .on_click(cx.listener(|this, _ev, _w, cx| {
                            this.commit_wsl_browser(cx);
                        }))
                        .into_any_element(),
                ])),
        )
        .into_any_element()
    }
}
