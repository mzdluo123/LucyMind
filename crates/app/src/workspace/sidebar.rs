//! 侧边栏面板:标题区 + 仓库行 + Agents 动作区 + WORKTREES 列表。
//!
//! 作为 [`WorkspaceView`](super::WorkspaceView) 的 `impl` 方法(跨文件 impl),
//! 直接访问其状态。可复用的按钮走 [`crate::ui::button`],样式 token 走
//! [`crate::theme`]。

use gpui::{div, prelude::*, rgb, Context, IntoElement, ParentElement, SharedString, Styled};

use crate::theme;
use crate::ui::button;

use super::WorkspaceView;

impl WorkspaceView {
    pub(super) fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut list = div().flex().flex_col();

        // 标题区 —— logo + 大字标题(约 3× 正文),冷白,几何字体。底部描边线把
        // 标题区与内容区清楚分隔(设计语言:分隔靠线 + 间距)。
        list = list.child(
            div()
                .pb(theme::space_md())
                .mb(theme::space_md())
                .border_b_1()
                .border_color(rgb(theme::BORDER))
                .flex()
                .flex_row()
                .items_center()
                .gap(theme::space_sm())
                // GPUI 的 svg() 是单色 mask,必须设 text_color 才显形(且多色 SVG
                // 会被填成单色剪影)。冷白填充。
                .child(
                    gpui::svg()
                        .size(gpui::px(42.0)) // 1.5× 标题字号
                        .path("icons/logo.svg")
                        .text_color(rgb(theme::TEXT_BRIGHT)),
                )
                .child(
                    div()
                        .text_size(gpui::px(28.0)) // ≈ 3× 正文(正文 ~14)
                        .text_color(rgb(theme::TEXT_BRIGHT))
                        .child(SharedString::from("LUCYMIND")),
                ),
        );

        // 仓库行:当前仓库名 + Open 按钮(切换/打开仓库)。
        let repo_label = match &self.repo {
            Some(r) => r
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "repo".into()),
            None => "no repository".into(),
        };
        list = list.child(
            div()
                .mb(theme::space_md())
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(theme::space_sm())
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_color(rgb(theme::TEXT_DIM))
                        .child(SharedString::from(repo_label)),
                )
                .child(
                    button("open-repo", "Open…").on_click(cx.listener(|this, _ev, _w, cx| {
                        this.open_repo_picker(cx);
                    })),
                ),
        );

        // 区域标签:Agents —— 标题行(label 左 + `+` 按钮右),与 WORKTREES
        // 标题行(齿轮按钮)结构对称。点 `+` 弹下拉菜单列出 builtin agent
        // (迭代 [`lucy_core::agent::builtin_agents`],不在此硬编码)。
        list = list.child(
            div()
                .mb(theme::space_sm())
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_color(rgb(theme::TEXT_DIM))
                        .child(SharedString::from("AGENTS")),
                )
                .child(
                    // `+` 启动按钮:单色 SVG,group-hover 变亮(与齿轮按钮同风格)。
                    div()
                        .id("agent-launcher")
                        .group("agent-launcher-btn")
                        .flex_none()
                        .px(theme::space_xs())
                        .cursor_pointer()
                        .child(
                            gpui::svg()
                                .size(gpui::px(14.0))
                                .path("icons/plus.svg")
                                .text_color(rgb(theme::TEXT_FAINT))
                                .group_hover("agent-launcher-btn", |s| {
                                    s.text_color(rgb(theme::TEXT))
                                }),
                        )
                        .on_click(cx.listener(|this, _ev, _window, cx| {
                            this.agent_menu_open = true;
                            cx.notify();
                        })),
                ),
        );

        // 分隔:worktree 段(用描边分隔线,不用颜色)。标题行右侧放齿轮按钮 ——
        // 图形化编辑 .worktree.toml(别名之外的设置)。
        list = list.child(
            div()
                .mt(theme::space_md())
                .mb(theme::space_sm())
                .border_b_1()
                .border_color(rgb(theme::BORDER_SUBTLE))
                .pb(theme::space_xs())
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_color(rgb(theme::TEXT_DIM))
                        .child(SharedString::from("WORKTREES")),
                )
                .child(
                    // 齿轮:GPUI 的 svg() 是单色 mask,**必须**显式设 text_color
                    // 才显形(不继承父 div 的 color),所以直接设在 svg 上。用
                    // group-hover 让悬停整个按钮时齿轮变亮 —— 与 ✎/✕(纯文字、
                    // 天然跟随父色)观感一致。
                    div()
                        .id("open-settings")
                        .group("settings-btn")
                        .flex_none()
                        .px(theme::space_xs())
                        .cursor_pointer()
                        .child(
                            gpui::svg()
                                .size(gpui::px(14.0))
                                .path("icons/settings.svg")
                                .text_color(rgb(theme::TEXT_FAINT))
                                .group_hover("settings-btn", |s| s.text_color(rgb(theme::TEXT))),
                        )
                        .on_click(cx.listener(|this, _ev, window, cx| {
                            this.open_settings(window, cx);
                        })),
                ),
        );
        for (i, wt) in self.worktrees.iter().enumerate() {
            list = list.child(self.worktree_row(i, wt, cx));
        }

        // (状态提示移到主区底部的状态栏,见 render —— 更像编辑器,不占侧边栏。)

        // 侧边栏:宽度可拖(sidebar_width),内容可垂直滚动(worktree 多不溢出)。
        // 右侧描边 = 视觉边界。整块用界面字体 Futura。
        div()
            .flex_none()
            .w(gpui::px(self.sidebar_width))
            .h_full()
            .bg(rgb(theme::SURFACE))
            .border_r_1()
            .border_color(rgb(theme::BORDER))
            .text_color(rgb(theme::TEXT))
            .font_family(theme::FONT_UI)
            .child(
                // 可滚动内容区(id 是 overflow_y_scroll 的前提)。
                div()
                    .id("sidebar-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .p(theme::space_lg())
                    .child(list),
            )
    }

    /// agent 启动下拉菜单(`+` 按钮触发):全屏遮罩 + 贴左上的卡片,列出
    /// [`lucy_core::agent::builtin_agents`] 全部 agent。点遮罩 / Esc / 选中项均关。
    /// 选中项走 [`new_worktree_and_agent`](super::WorkspaceView::new_worktree_and_agent)。
    ///
    /// 卡片不精确锚定 `+` 按钮像素,固定贴窗口左上(大致在 AGENTS 标题下方),
    /// 实现简单且侧边栏宽度内足够(见 design.md D2)。
    pub(super) fn agent_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // 遮罩:压暗背景,点空白处关菜单(卡片 stop_propagation 防误关)。
        // 卡片作为遮罩的子元素,靠 items_start + padding 贴左上。
        let mut menu = div()
            .absolute()
            .inset_0()
            .flex()
            .items_start()
            // 大致在 AGENTS 标题行 `+` 按钮下方(标题区 + 仓库行 + AGENTS 行)。
            .pt(gpui::px(132.0))
            .pl(theme::space_lg())
            .bg(theme::with_alpha(0x00_00_00, 0.55))
            .font_family(theme::FONT_UI)
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _ev, _w, cx| {
                    this.agent_menu_open = false;
                    cx.notify();
                }),
            );

        // 卡片内的 agent 列表(迭代注册表,不硬编码)。
        let mut list = div().flex().flex_col().gap(theme::space_xs());
        for a in lucy_core::agent::builtin_agents() {
            let name = a.name.to_string();
            // 图标走 agent_icon(查注册表),与旧侧边栏按钮同源。
            let icon = crate::assets::agent_icon(a.name);
            let mut row = div()
                .id(SharedString::from(format!("agent-menu-{}", a.name)))
                .flex()
                .flex_row()
                .items_center()
                .gap(theme::space_sm())
                .px(theme::space_md())
                .py(theme::space_sm())
                .min_w(gpui::px(180.0))
                .cursor_pointer()
                .text_color(rgb(theme::TEXT))
                .hover(|s| s.bg(rgb(theme::BTN_BG_HOVER)));
            if let Some(path) = icon {
                row = row.child(
                    gpui::svg()
                        .flex_none()
                        .size(gpui::px(16.0))
                        .path(path)
                        .text_color(rgb(theme::TEXT)),
                );
            }
            row = row
                .child(SharedString::from(a.display))
                .on_click(cx.listener(move |this, _ev, _w, cx| {
                    this.agent_menu_open = false;
                    this.new_worktree_and_agent(&name, cx);
                }));
            list = list.child(row);
        }

        // 卡片:描边 + 2px 圆角(与 modal/dialog 同语言)。stop_propagation
        // 让点卡片内(项间空白)不冒泡到遮罩关菜单。
        let card = div()
            .bg(rgb(theme::SURFACE))
            .border_1()
            .border_color(rgb(theme::BORDER))
            .rounded(theme::radius())
            .p(theme::space_xs())
            .child(list)
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|_this, _ev, _w, cx| {
                    cx.stop_propagation();
                }),
            );

        menu = menu.child(card);
        menu
    }

    /// 单条 worktree 行:标记条 + 图标 + 名字 + ✎ 改别名 + ✕ 关闭。
    fn worktree_row(
        &self,
        i: usize,
        wt: &lucy_core::git::WorktreeEntry,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let branch = wt.branch.clone().unwrap_or_else(|| "detached".to_string());
        // 显示名:有别名用别名,否则用分支名。别名存 .worktree.toml 的 [alias]。
        let alias = self.config.alias.get(&branch).cloned();
        let label = alias.clone().unwrap_or_else(|| branch.clone());
        let ours = self.is_ours(&wt.path);
        let is_main = self.is_main_repo(&wt.path);
        let is_active = self
            .active
            .as_deref()
            .is_some_and(|a| super::same_path(a, &wt.path));
        let wt_path_for_click = wt.path.clone();

        // 除主仓外都可点(切换/打开)、可关。
        // 所有行(含主仓)都可点开/切换;只有非主仓可关闭(主仓不是 worktree)。
        let can_close = !is_main;

        let mut row = div()
            .id(SharedString::from(format!("wt-{i}")))
            .flex()
            .flex_row()
            .items_center()
            .gap(theme::space_sm())
            // 左侧标记条:active 冷白,否则与表面同色(视觉上"无")。
            .border_l_2()
            .border_color(if is_active {
                rgb(theme::TEXT_BRIGHT)
            } else {
                rgb(theme::SURFACE)
            })
            .pl(theme::space_sm())
            .pr(theme::space_xs())
            .py(theme::space_xs())
            .text_color(rgb(if is_main {
                theme::TEXT_DIM
            } else {
                theme::TEXT
            }));

        if is_active {
            row = row.bg(rgb(theme::SURFACE_RAISED));
        }
        // 整行可点(含主仓)→ 打开/切换到该目录的终端。
        row = row
            .cursor_pointer()
            .hover(|s| s.bg(rgb(theme::BTN_BG_HOVER)));
        row = row.on_click(cx.listener(move |this, _ev, _w, cx| {
            this.open_worktree(wt_path_for_click.clone(), cx);
        }));

        // 图标(Lucide git 图标,单色跟主题):main=folder-git,其余=git-branch。
        let icon_path = if is_main {
            "icons/folder-git-2.svg"
        } else {
            "icons/git-branch.svg"
        };
        row = row.child(
            gpui::svg()
                .flex_none()
                .size(gpui::px(14.0))
                .path(icon_path)
                .text_color(rgb(if is_main {
                    theme::TEXT_DIM
                } else if ours {
                    theme::TEXT
                } else {
                    theme::TEXT_FAINT
                })),
        );
        row = row.child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(SharedString::from(label.clone())),
        );

        // ✎ 改别名:所有行(含主仓)都可设别名,固定行尾右对齐。
        {
            let edit_branch = branch.clone();
            let edit_init = alias.clone().unwrap_or_default();
            row = row.child(
                div()
                    .id(SharedString::from(format!("alias-{i}")))
                    .flex_none()
                    .px(theme::space_xs())
                    .text_color(rgb(theme::TEXT_FAINT))
                    .cursor_pointer()
                    .hover(|s| s.text_color(rgb(theme::TEXT)))
                    .child(SharedString::from("✎"))
                    .on_click(cx.listener(move |this, _ev, window, cx| {
                        cx.stop_propagation();
                        this.open_alias_editor(&edit_branch, &edit_init, window, cx);
                    })),
            );
        }

        // ✕ 关闭:仅非主仓(主仓不是 worktree,不可关)。
        if can_close {
            let close_path = wt.path.clone();
            let close_branch = branch.clone();
            row = row.child(
                div()
                    .id(SharedString::from(format!("close-{i}")))
                    .flex_none()
                    .px(theme::space_xs())
                    .text_color(rgb(theme::TEXT_FAINT))
                    .cursor_pointer()
                    .hover(|s| s.text_color(rgb(theme::STATE_ERROR)))
                    .child(SharedString::from("✕"))
                    .on_click(cx.listener(move |this, _ev, _w, cx| {
                        // 阻止冒泡到整行的 open_worktree —— 否则点 ✕ 会同时触发
                        // 关闭 + 打开,行为打架。
                        cx.stop_propagation();
                        this.request_close(close_path.clone(), close_branch.clone(), cx);
                    })),
            );
        }

        row
    }
}
