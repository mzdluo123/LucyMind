//! 侧边栏面板:标题区 + 仓库行 + Agents 动作区 + WORKTREES 列表。
//!
//! 作为 [`WorkspaceView`](super::WorkspaceView) 的 `impl` 方法(跨文件 impl),
//! 直接访问其状态。样式 token 走 [`crate::theme`]。

use gpui::{div, prelude::*, rgb, Context, IntoElement, ParentElement, SharedString, Styled};

use crate::theme;

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

        // 仓库行:当前仓库名 + folder-open 图标按钮(切换/打开仓库)。
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
                    // folder-open 图标按钮:与齿轮同风格(无背景无描边 +
                    // group-hover 染色)。比文字按钮更紧凑、更编辑器风。
                    div()
                        .id("open-repo")
                        .group("open-repo-btn")
                        .flex_none()
                        .px(theme::space_xs())
                        .cursor_pointer()
                        .child(
                            gpui::svg()
                                .size(gpui::px(14.0))
                                .path("icons/folder-open.svg")
                                .text_color(rgb(theme::TEXT_FAINT))
                                .group_hover("open-repo-btn", |s| s.text_color(rgb(theme::TEXT))),
                        )
                        .on_click(cx.listener(|this, _ev, _w, cx| {
                            this.open_repo_picker(cx);
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
        // render 路径直接用 PathBuf 比较(不调 canon/same_path):
        // set_repo/refresh_worktrees 已预规范化 worktrees 的 path,
        // self.repo / self.active 也已是规范化路径。
        // 若改回调 canon,WslHost 下每帧 spawn wsl.exe → UI 卡死。
        let is_main = self.repo.as_deref().is_some_and(|r| wt.path == r);
        let is_active = self.active.as_deref().is_some_and(|a| a == wt.path);
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

        // `+` 新建 worktree:仅主仓行显示(在此仓库上开 worktree)。
        // 与 ✎ / ✕ 同风格:文字按钮、group-hover 染色、stop_propagation 防误触行点击。
        if is_main {
            row = row.child(
                div()
                    .id(SharedString::from(format!("new-wt-{i}")))
                    .group("new-wt-btn")
                    .flex_none()
                    .px(theme::space_xs())
                    .cursor_pointer()
                    .text_color(rgb(theme::TEXT_FAINT))
                    .child(
                        gpui::svg()
                            .size(gpui::px(14.0))
                            .path("icons/plus.svg")
                            .text_color(rgb(theme::TEXT_FAINT))
                            .group_hover("new-wt-btn", |s| s.text_color(rgb(theme::TEXT))),
                    )
                    .on_click(cx.listener(|this, _ev, _w, cx| {
                        cx.stop_propagation();
                        this.new_worktree(cx);
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
