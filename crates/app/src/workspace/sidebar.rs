//! 侧边栏面板:固定的品牌/仓库上下文 + 可滚动的 worktree 列表。

use std::rc::Rc;

use gpui::{
    div, prelude::*, px, rgb, Context, IntoElement, KeyDownEvent, ParentElement, SharedString,
    Stateful, Styled, Window,
};
use gpui_component::tooltip::Tooltip;

use crate::theme;

use super::{NewWorktreeLaunch, WorkspaceView};

const SIDEBAR_ACTION_SIZE: f32 = 28.0;
const WORKTREE_ROW_HEIGHT: f32 = 36.0;

type SidebarAction =
    Rc<dyn Fn(&mut WorkspaceView, &mut Window, &mut Context<WorkspaceView>) + 'static>;

impl WorkspaceView {
    pub(super) fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let repo_label = self
            .repo
            .as_ref()
            .and_then(|repo| repo.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "No repository".into());
        let repo_tooltip = self
            .repo
            .as_ref()
            .map(|repo| repo.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Open a Git repository".into());

        let brand = div()
            .pb(theme::space_md())
            .mb(theme::space_md())
            .border_b_1()
            .border_color(rgb(theme::BORDER))
            .flex()
            .items_center()
            .gap(theme::space_sm())
            .child(
                gpui::svg()
                    .flex_none()
                    .size(px(24.0))
                    .path("icons/logo.svg")
                    .text_color(rgb(theme::TEXT_BRIGHT)),
            )
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(px(17.0))
                    .text_color(rgb(theme::TEXT_BRIGHT))
                    .child("LUCYMIND"),
            );

        let repo_name_tooltip = SharedString::from(repo_tooltip);
        let repo_row = div()
            .h(px(32.0))
            .flex()
            .items_center()
            .gap(theme::space_sm())
            .child(
                div()
                    .id("repository-name")
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_color(rgb(theme::TEXT))
                    .child(SharedString::from(repo_label))
                    .tooltip(move |window, cx| {
                        Tooltip::new(repo_name_tooltip.clone()).build(window, cx)
                    }),
            )
            .child(self.sidebar_icon_button(
                "open-repo",
                "icons/folder-open.svg",
                "Open repository",
                cx,
                |this, window, cx| this.open_repo_picker(window, cx),
            ));

        let new_worktree_button = self.sidebar_icon_button(
            "new-worktree-trigger",
            "icons/plus.svg",
            "New worktree",
            cx,
            |this, _window, cx| {
                this.launcher_menu_open = false;
                this.worktree_action_menu = None;
                this.new_worktree_menu_open = !this.new_worktree_menu_open;
                cx.notify();
            },
        );
        let mut new_worktree_anchor = div().relative().flex_none().child(new_worktree_button);
        if self.new_worktree_menu_open {
            new_worktree_anchor =
                new_worktree_anchor.child(gpui::deferred(self.new_worktree_menu(cx)));
        }

        let worktree_header = div()
            .mt(theme::space_md())
            .border_b_1()
            .border_color(rgb(theme::BORDER_SUBTLE))
            .pb(theme::space_xs())
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(theme::TEXT_DIM))
                    .child("WORKTREES"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme::space_xs())
                    .child(new_worktree_anchor)
                    .child(self.sidebar_icon_button(
                        "open-settings",
                        "icons/settings.svg",
                        "Worktree settings",
                        cx,
                        |this, window, cx| this.open_settings(window, cx),
                    )),
            );

        let fixed_header = div()
            .flex_none()
            .px(theme::space_lg())
            .pt(theme::space_lg())
            .child(brand)
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(theme::TEXT_DIM))
                    .child("REPOSITORY"),
            )
            .child(repo_row)
            .child(worktree_header);

        let mut worktree_list = div().flex().flex_col().pt(theme::space_xs());
        for (i, worktree) in self.worktrees.iter().enumerate() {
            worktree_list = worktree_list.child(self.worktree_row(i, worktree, cx));
        }

        div()
            .flex_none()
            .w(px(self.sidebar_width))
            .h_full()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(theme::SURFACE))
            .border_r_1()
            .border_color(rgb(theme::BORDER))
            .text_color(rgb(theme::TEXT))
            .font_family(theme::FONT_UI)
            .child(fixed_header)
            .child(
                div()
                    .id("sidebar-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(theme::space_lg())
                    .pb(theme::space_lg())
                    .child(worktree_list),
            )
    }

    fn sidebar_icon_button(
        &self,
        id: impl Into<SharedString>,
        icon: &'static str,
        tooltip: &'static str,
        cx: &mut Context<Self>,
        on_activate: impl Fn(&mut WorkspaceView, &mut Window, &mut Context<WorkspaceView>) + 'static,
    ) -> Stateful<gpui::Div> {
        let id = id.into();
        let debug_id = id.clone();
        let action: SidebarAction = Rc::new(on_activate);
        let click_action = action.clone();

        div()
            .id(id)
            .debug_selector(move || debug_id.to_string())
            .flex_none()
            .size(px(SIDEBAR_ACTION_SIZE))
            .flex()
            .items_center()
            .justify_center()
            .rounded(theme::radius())
            .cursor_pointer()
            .focusable()
            .focus(|style| {
                style
                    .bg(rgb(theme::BTN_BG_ACTIVE))
                    .border_1()
                    .border_color(rgb(theme::TEXT_DIM))
            })
            .hover(|style| style.bg(rgb(theme::BTN_BG_HOVER)))
            .child(
                gpui::svg()
                    .flex_none()
                    .size(px(15.0))
                    .path(icon)
                    .text_color(rgb(theme::ICON_MUTED)),
            )
            .tooltip(move |window, cx| Tooltip::new(tooltip).build(window, cx))
            .on_click(cx.listener(move |this, _event, window, cx| {
                cx.stop_propagation();
                click_action(this, window, cx);
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if is_activate_key(event) {
                    cx.stop_propagation();
                    action(this, window, cx);
                }
            }))
    }

    fn worktree_row(
        &self,
        index: usize,
        worktree: &lucy_core::git::WorktreeEntry,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let branch = worktree
            .branch
            .clone()
            .unwrap_or_else(|| "detached".to_string());
        let alias = self.config.alias.get(&branch).cloned();
        let label = alias.clone().unwrap_or_else(|| branch.clone());
        let ours = self.is_ours(&worktree.path);
        let is_main = self
            .repo
            .as_deref()
            .is_some_and(|repo| worktree.path == repo);
        let is_active = self
            .active
            .as_deref()
            .is_some_and(|active| active == worktree.path);
        let worktree_path = worktree.path.clone();
        let path_for_keyboard = worktree_path.clone();
        let row_tooltip = SharedString::from(match &alias {
            Some(alias) => format!("{alias}\n{branch}"),
            None => branch.clone(),
        });

        let icon_path = if is_main {
            "icons/folder-git-2.svg"
        } else {
            "icons/git-branch.svg"
        };
        let icon_color = if is_active {
            theme::TEXT_BRIGHT
        } else if is_main || ours {
            theme::TEXT_DIM
        } else {
            theme::ICON_MUTED
        };

        let mut label_column = div()
            .id(SharedString::from(format!("worktree-label-{index}")))
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .text_color(rgb(if is_active {
                theme::TEXT_BRIGHT
            } else if is_main {
                theme::TEXT_DIM
            } else {
                theme::TEXT
            }))
            .child(SharedString::from(label))
            .tooltip(move |window, cx| Tooltip::new(row_tooltip.clone()).build(window, cx));
        if alias.is_some() {
            label_column = label_column.child(
                div()
                    .text_xs()
                    .text_color(rgb(theme::TEXT_DIM))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(SharedString::from(branch.clone())),
            );
        }

        let menu_path = worktree.path.clone();
        let trigger_path = menu_path.clone();
        let menu_button = self.sidebar_icon_button(
            SharedString::from(format!("worktree-actions-{index}")),
            "icons/ellipsis.svg",
            "Worktree actions",
            cx,
            move |this, _window, cx| {
                this.new_worktree_menu_open = false;
                if this.worktree_action_menu.as_ref() == Some(&trigger_path) {
                    this.worktree_action_menu = None;
                } else {
                    this.worktree_action_menu = Some(trigger_path.clone());
                }
                cx.notify();
            },
        );
        let mut menu_anchor = div().relative().flex_none().child(menu_button);
        if self.worktree_action_menu.as_ref() == Some(&menu_path) {
            menu_anchor = menu_anchor.child(gpui::deferred(self.worktree_action_menu(
                index,
                branch.clone(),
                alias.unwrap_or_default(),
                worktree.path.clone(),
                !is_main,
                cx,
            )));
        }

        let mut row = div()
            .id(SharedString::from(format!("wt-{index}")))
            .debug_selector(move || format!("wt-{index}"))
            .h(px(WORKTREE_ROW_HEIGHT))
            .flex_none()
            .flex()
            .items_center()
            .gap(theme::space_sm())
            .border_l_2()
            .border_color(rgb(if is_active {
                theme::TEXT_BRIGHT
            } else {
                theme::SURFACE
            }))
            .pl(theme::space_sm())
            .pr(theme::space_xs())
            .rounded(theme::radius())
            .cursor_pointer()
            .focusable()
            .focus(|style| {
                style
                    .bg(rgb(theme::BTN_BG_ACTIVE))
                    .border_color(rgb(theme::TEXT_BRIGHT))
            })
            .hover(|style| style.bg(rgb(theme::BTN_BG_HOVER)))
            .child(
                gpui::svg()
                    .flex_none()
                    .size(px(14.0))
                    .path(icon_path)
                    .text_color(rgb(icon_color)),
            )
            .child(label_column)
            .child(menu_anchor);
        if is_active {
            row = row.bg(rgb(theme::SURFACE_RAISED));
        }
        row.on_click(cx.listener(move |this, _event, _window, cx| {
            this.open_worktree(worktree_path.clone(), cx);
        }))
        .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _window, cx| {
            if is_activate_key(event) {
                cx.stop_propagation();
                this.open_worktree(path_for_keyboard.clone(), cx);
            }
        }))
    }

    fn worktree_action_menu(
        &self,
        index: usize,
        branch: String,
        alias: String,
        path: std::path::PathBuf,
        can_close: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let rename_branch = branch.clone();
        let rename_alias = alias.clone();
        let mut menu = div()
            .absolute()
            .top(px(SIDEBAR_ACTION_SIZE + 2.0))
            .right_0()
            .min_w(px(164.0))
            .bg(rgb(theme::SURFACE))
            .border_1()
            .border_color(rgb(theme::BORDER))
            .rounded(theme::radius())
            .occlude()
            .py(theme::space_xs())
            .flex()
            .flex_col()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|_this, _event, _window, cx| cx.stop_propagation()),
            )
            .child(self.worktree_action_menu_item(
                format!("worktree-rename-{index}"),
                "icons/pencil.svg",
                "Rename",
                false,
                cx,
                move |this, window, cx| {
                    this.worktree_action_menu = None;
                    this.open_alias_editor(&rename_branch, &rename_alias, window, cx);
                },
            ));

        if can_close {
            menu = menu.child(self.worktree_action_menu_item(
                format!("worktree-close-{index}"),
                "icons/circle-x.svg",
                "Close worktree",
                true,
                cx,
                move |this, _window, cx| {
                    this.worktree_action_menu = None;
                    this.request_close(path.clone(), branch.clone(), cx);
                },
            ));
        }
        menu
    }

    fn worktree_action_menu_item(
        &self,
        id: String,
        icon: &'static str,
        label: &'static str,
        destructive: bool,
        cx: &mut Context<Self>,
        on_activate: impl Fn(&mut WorkspaceView, &mut Window, &mut Context<WorkspaceView>) + 'static,
    ) -> impl IntoElement {
        let action: SidebarAction = Rc::new(on_activate);
        let click_action = action.clone();
        let debug_id = id.clone();
        let color = if destructive {
            theme::STATE_ERROR
        } else {
            theme::TEXT
        };
        div()
            .id(SharedString::from(id))
            .debug_selector(move || debug_id)
            .h(px(32.0))
            .px(theme::space_sm())
            .flex()
            .items_center()
            .gap(theme::space_sm())
            .text_color(rgb(color))
            .cursor_pointer()
            .focusable()
            .focus(|style| style.bg(rgb(theme::BTN_BG_ACTIVE)))
            .hover(|style| style.bg(rgb(theme::BTN_BG_HOVER)))
            .child(
                gpui::svg()
                    .flex_none()
                    .size(px(14.0))
                    .path(icon)
                    .text_color(rgb(color)),
            )
            .child(label)
            .on_click(cx.listener(move |this, _event, window, cx| {
                cx.stop_propagation();
                click_action(this, window, cx);
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if is_activate_key(event) {
                    cx.stop_propagation();
                    action(this, window, cx);
                }
            }))
    }

    fn new_worktree_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut menu = div()
            .absolute()
            .top(px(SIDEBAR_ACTION_SIZE + 2.0))
            .right_0()
            .min_w(px(180.0))
            .bg(rgb(theme::SURFACE))
            .border_1()
            .border_color(rgb(theme::BORDER))
            .rounded(theme::radius())
            .occlude()
            .py(theme::space_xs())
            .flex()
            .flex_col()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|_this, _event, _window, cx| cx.stop_propagation()),
            )
            .child(
                div()
                    .h(px(28.0))
                    .px(theme::space_sm())
                    .flex()
                    .items_center()
                    .text_xs()
                    .text_color(rgb(theme::TEXT_DIM))
                    .child("START WITH"),
            )
            .child(
                self.new_worktree_menu_item("Terminal", None, cx, |this, cx| {
                    this.new_worktree_menu_open = false;
                    this.new_worktree(NewWorktreeLaunch::Terminal, cx);
                }),
            );

        for agent in lucy_core::agent::builtin_agents() {
            let name = agent.name.to_string();
            let icon = crate::assets::agent_icon(agent.name).map(SharedString::from);
            menu = menu.child(self.new_worktree_menu_item(
                agent.display,
                icon,
                cx,
                move |this, cx| {
                    this.new_worktree_menu_open = false;
                    this.new_worktree(NewWorktreeLaunch::Agent(name.clone()), cx);
                },
            ));
        }
        menu
    }

    fn new_worktree_menu_item(
        &self,
        label: &str,
        icon: Option<SharedString>,
        cx: &mut Context<Self>,
        on_activate: impl Fn(&mut WorkspaceView, &mut Context<WorkspaceView>) + 'static,
    ) -> Stateful<gpui::Div> {
        let label = label.to_string();
        let action = Rc::new(on_activate);
        let click_action = action.clone();
        let mut item = div()
            .id(SharedString::from(format!("new-worktree-{label}")))
            .h(px(32.0))
            .px(theme::space_md())
            .flex()
            .items_center()
            .gap(theme::space_sm())
            .cursor_pointer()
            .text_color(rgb(theme::TEXT))
            .focusable()
            .focus(|style| style.bg(rgb(theme::BTN_BG_ACTIVE)))
            .hover(|style| style.bg(rgb(theme::BTN_BG_HOVER)));
        if let Some(path) = icon {
            item = item.child(
                gpui::svg()
                    .flex_none()
                    .size(px(14.0))
                    .path(path)
                    .text_color(rgb(theme::TEXT)),
            );
        }
        item.child(label)
            .on_click(cx.listener(move |this, _event, _window, cx| {
                cx.stop_propagation();
                click_action(this, cx);
                cx.notify();
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _window, cx| {
                if is_activate_key(event) {
                    cx.stop_propagation();
                    action(this, cx);
                    cx.notify();
                }
            }))
    }
}

fn is_activate_key(event: &KeyDownEvent) -> bool {
    matches!(event.keystroke.key.as_str(), "enter" | "space")
}
