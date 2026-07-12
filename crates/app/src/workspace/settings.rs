//! `.worktree.toml` 图形化设置面板(别名之外的字段)。
//!
//! 入口是 WORKTREES 标题行右侧的齿轮按钮(见 [`sidebar`](super::sidebar))。
//! 面板复用 [`crate::ui::dialog`] 的模态骨架,字段一一对应
//! [`config::EditableSettings`];提交时经 `config::set_worktree_settings`
//! 保格式写回(保留注释、别名、agents 等未涉及的段)。
//!
//! 数组字段(hook 命令 / copy 文件)用多行 Input,一行一条,提交时按行拆分、
//! 去空行。location(sibling/inside)与 fail_fast 是非文本项,点选切换。

use gpui::{
    div, prelude::*, rgb, AnyElement, Context, IntoElement, ParentElement, SharedString, Styled,
    Window,
};

use gpui_component::input::{Input, InputState};

use lucy_core::config::{self, EditableSettings, Location};

use crate::theme;
use crate::ui::{button, button_row, modal, ButtonVariant};

use super::{SettingsForm, WorkspaceView};

impl WorkspaceView {
    /// 打开设置面板:从当前配置抽出可编辑字段,建各输入框并填入初值。
    /// 无仓库时提示而非打开(没有 .worktree.toml 可写)。
    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.repo.is_none() {
            self.set_status("请先打开一个 git 仓库", true);
            return;
        }
        let s = EditableSettings::from_config(&self.config);

        // 数组字段:一行一条。文本字段:单行。
        let dir = new_input(&s.dir, false, "../{repo}-worktrees", window, cx);
        let default_base = new_input(&s.default_base, false, "main", window, cx);
        let post_create = new_input(
            &s.post_create.join("\n"),
            true,
            "每行一条 shell 命令,如 pnpm install",
            window,
            cx,
        );
        let pre_remove = new_input(
            &s.pre_remove.join("\n"),
            true,
            "每行一条 shell 命令",
            window,
            cx,
        );
        let copy_files = new_input(
            &s.copy_files.join("\n"),
            true,
            "每行一个文件,如 .env",
            window,
            cx,
        );

        self.settings = Some(SettingsForm {
            location: s.location,
            fail_fast: s.fail_fast,
            dir,
            default_base,
            post_create,
            pre_remove,
            copy_files,
        });
        cx.notify();
    }

    /// 从表单读值、写回 .worktree.toml、重载配置、关面板。
    fn commit_settings(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.settings.as_ref() else {
            return;
        };
        let Some(repo) = self.repo.clone() else {
            return;
        };

        let read = |state: &gpui::Entity<InputState>| state.read(cx).value().to_string();
        let s = EditableSettings {
            location: form.location,
            fail_fast: form.fail_fast,
            dir: read(&form.dir).trim().to_string(),
            default_base: read(&form.default_base).trim().to_string(),
            post_create: split_lines(&read(&form.post_create)),
            pre_remove: split_lines(&read(&form.pre_remove)),
            copy_files: split_lines(&read(&form.copy_files)),
        };

        let path = self.host.join_path(&repo, ".worktree.toml");
        match config::set_worktree_settings(self.host.as_ref(), &path, &s) {
            Ok(()) => {
                // 重载(拿到写回后的配置)并关面板。
                if let Ok(loaded) = config::load(self.host.as_ref(), &path) {
                    self.config = loaded.config;
                }
                self.settings = None;
                self.set_status("已保存设置", false);
            }
            // 校验失败(如 sibling 空 dir):不关面板,让用户改。
            Err(e) => self.set_status(format!("保存设置失败:{e}"), true),
        }
        cx.notify();
    }

    /// 设置面板弹窗。
    pub(super) fn settings_dialog(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(form) = self.settings.as_ref() else {
            return div().into_any_element();
        };

        modal(
            480.0,
            div()
                .flex()
                .flex_col()
                .gap(theme::space_md())
                .child(
                    div()
                        .text_color(rgb(theme::TEXT_BRIGHT))
                        .child(SharedString::from("worktree 设置")),
                )
                // location:sibling / inside 二选一。
                .child(field_label("worktree 位置"))
                .child(self.location_picker(form.location, cx))
                // dir(仅 sibling 有意义,但始终可编辑)。
                .child(field_label("目录模板({repo} = 仓库名)"))
                .child(Input::new(&form.dir))
                // default_base。
                .child(field_label("默认基分支"))
                .child(Input::new(&form.default_base))
                // post_create。
                .child(field_label("PostCreate 钩子(建好 worktree 后执行)"))
                .child(Input::new(&form.post_create))
                // pre_remove。
                .child(field_label("PreRemove 钩子(关闭 worktree 前执行)"))
                .child(Input::new(&form.pre_remove))
                // copy_files。
                .child(field_label("复制文件(建 worktree 时从主仓复制)"))
                .child(Input::new(&form.copy_files))
                // fail_fast 开关。
                .child(self.fail_fast_toggle(form.fail_fast, cx))
                // 按钮行。
                .child(button_row([
                    button("settings-cancel", "取消")
                        .on_click(cx.listener(|this, _ev, _w, cx| {
                            this.settings = None;
                            cx.notify();
                        }))
                        .into_any_element(),
                    button("settings-save", "保存")
                        .variant(ButtonVariant::Confirm)
                        .on_click(cx.listener(|this, _ev, _w, cx| {
                            this.commit_settings(cx);
                        }))
                        .into_any_element(),
                ])),
        )
        .into_any_element()
    }

    /// location 二选一(sibling / inside),当前项高亮。
    fn location_picker(&self, current: Location, cx: &mut Context<Self>) -> impl IntoElement {
        let opt = |loc: Location, label: &str, selected: bool| -> AnyElement {
            let id = SharedString::from(format!(
                "loc-{}",
                match loc {
                    Location::Sibling => "sibling",
                    Location::Inside => "inside",
                }
            ));
            div()
                .id(id)
                .flex_1()
                .px(theme::space_md())
                .py(theme::space_sm())
                .bg(rgb(if selected {
                    theme::SURFACE_RAISED
                } else {
                    theme::BTN_BG
                }))
                .border_1()
                .border_color(rgb(if selected {
                    theme::TEXT_FAINT
                } else {
                    theme::BORDER
                }))
                .rounded(theme::radius())
                .text_color(rgb(if selected {
                    theme::TEXT_BRIGHT
                } else {
                    theme::TEXT
                }))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(theme::BTN_BG_HOVER)))
                .child(SharedString::from(label.to_string()))
                .on_click(cx.listener(move |this, _ev, _w, cx| {
                    if let Some(form) = this.settings.as_mut() {
                        form.location = loc;
                        cx.notify();
                    }
                }))
                .into_any_element()
        };
        div()
            .flex()
            .flex_row()
            .gap(theme::space_sm())
            .child(opt(
                Location::Sibling,
                "仓库外(sibling)",
                current == Location::Sibling,
            ))
            .child(opt(
                Location::Inside,
                "仓库内(inside)",
                current == Location::Inside,
            ))
    }

    /// fail_fast 开关行:左描述 + 右一个可点的 [开/关] 标记。
    fn fail_fast_toggle(&self, on: bool, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("fail-fast-toggle")
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .mt(theme::space_xs())
            .cursor_pointer()
            .child(
                div()
                    .text_color(rgb(theme::TEXT_DIM))
                    .child(SharedString::from("钩子失败即停(fail-fast)")),
            )
            .child(
                div()
                    .px(theme::space_sm())
                    .py(theme::space_xs())
                    .border_1()
                    .border_color(rgb(if on { theme::TEXT_FAINT } else { theme::BORDER }))
                    .rounded(theme::radius())
                    .text_color(rgb(if on {
                        theme::TEXT_BRIGHT
                    } else {
                        theme::TEXT_DIM
                    }))
                    .child(SharedString::from(if on { "开" } else { "关" })),
            )
            .on_click(cx.listener(|this, _ev, _w, cx| {
                if let Some(form) = this.settings.as_mut() {
                    form.fail_fast = !form.fail_fast;
                    cx.notify();
                }
            }))
    }
}

/// 字段标签(小号 dim 文字)。
fn field_label(text: &str) -> impl IntoElement {
    div()
        .text_color(rgb(theme::TEXT_DIM))
        .child(SharedString::from(text.to_string()))
}

/// 建一个填了初值的 Input 状态。`multi` = 多行(数组字段用)。
fn new_input(
    init: &str,
    multi: bool,
    placeholder: &str,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) -> gpui::Entity<InputState> {
    let init = init.to_string();
    let placeholder = placeholder.to_string();
    cx.new(|cx| {
        let mut state = InputState::new(window, cx).placeholder(placeholder);
        if multi {
            state = state.multi_line(true).rows(3);
        }
        state.set_value(init, window, cx);
        state
    })
}

/// 多行文本按行拆分 → 去掉首尾空白 → 丢掉空行。
fn split_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}
