//! U9:端到端主流程装配。
//!
//! 顶层 [`WorkspaceView`] = 左侧栏(仓库 + worktree 列表 + 动作)+ 右侧终端区。
//! 串起:选仓库 → 一键建 worktree → 跑 postCreate hook → 在 worktree 起 agent →
//! 显示在终端。这把 core(git/hooks/agent)与 app(终端渲染)接成完整闭环。

use std::path::PathBuf;

use gpui::{
    div, prelude::*, px, rgb, Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement,
    Render, SharedString, Styled, Window,
};

use lucy_core::agent::AgentSpec;
use lucy_core::config::{self, WorktreeConfig};
use lucy_core::git::{self, CreateMode, WorktreeEntry};
use lucy_core::hooks::{self, HookContext, LifecycleEvent};

use crate::terminal_view::TerminalView;

const BG_SIDEBAR: u32 = 0x25_25_26;
const BG_MAIN: u32 = 0x1e_1e_1e;
const FG: u32 = 0xd4_d4_d4;
const FG_DIM: u32 = 0x85_85_85;
const ACCENT: u32 = 0x4e_c9_b0;
const ERR: u32 = 0xf1_4c_4c;

/// 一条状态消息(动作反馈 / 错误),显示在侧边栏底部。
#[derive(Clone)]
struct Status {
    text: SharedString,
    is_error: bool,
}

pub struct WorkspaceView {
    /// 当前仓库根(MVP:启动时取的工作目录)。
    repo: PathBuf,
    config: WorktreeConfig,
    worktrees: Vec<WorktreeEntry>,
    /// 当前活动的终端(建了 worktree 起 agent 后填充)。
    terminal: Option<Entity<TerminalView>>,
    status: Option<Status>,
    /// 自增计数,给一键新建的分支起唯一名。
    counter: usize,
    focus: FocusHandle,
}

impl WorkspaceView {
    pub fn new(cx: &mut Context<Self>, repo: PathBuf) -> Self {
        // 读仓库的 .worktree.toml(缺失/出错则用默认)。
        let config = config::load(repo.join(".worktree.toml"))
            .map(|l| l.config)
            .unwrap_or_default();

        let worktrees = git::list(&repo).unwrap_or_default();

        Self {
            repo,
            config,
            worktrees,
            terminal: None,
            status: None,
            counter: 0,
            focus: cx.focus_handle(),
        }
    }

    fn set_status(&mut self, text: impl Into<SharedString>, is_error: bool) {
        self.status = Some(Status {
            text: text.into(),
            is_error,
        });
    }

    fn refresh_worktrees(&mut self) {
        self.worktrees = git::list(&self.repo).unwrap_or_default();
    }

    /// 主流程:建 worktree → postCreate hook → 起 agent 到终端。
    fn new_worktree_and_agent(&mut self, agent_name: &str, cx: &mut Context<Self>) {
        self.counter += 1;
        let branch = format!("lucy/session-{}", self.counter);
        let base = self.config.worktree.default_base.clone();

        // worktree 路径:仓库外兄弟目录(按配置)。
        let repo_name = self
            .repo
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "repo".into());
        let parent = config::resolve_sibling_dir(&self.config.worktree.dir, &repo_name);
        let parent_dir = self.repo.join(&parent);
        let wt_path = git::sibling_worktree_path(&parent_dir, &branch);

        // 1) 建 worktree(带分支占用检查)。
        if let Err(e) = git::add(
            &self.repo,
            &wt_path,
            &CreateMode::NewBranch {
                branch: branch.clone(),
                base,
            },
        ) {
            self.set_status(format!("建 worktree 失败:{e}"), true);
            return;
        }

        // 2) postCreate hook(copy + 命令),注入环境变量。
        let ctx = HookContext {
            worktree_path: wt_path.clone(),
            worktree_branch: branch.clone(),
            worktree_name: branch.replace('/', "-"),
            repo_root: self.repo.clone(),
        };
        let run = hooks::run_event(
            LifecycleEvent::PostCreate,
            &self.config.hooks,
            &self.config.copy,
            &ctx,
            |_step| {},
        );
        if run.had_failure() {
            self.set_status(
                format!("worktree 已建,但 postCreate hook 有失败步骤(见日志)"),
                true,
            );
            // 不回滚 worktree(计划:hook 失败不删 worktree)。
        }

        // 3) 组装 agent spec + 起终端。
        let wt_env = ctx.env_vars();
        let spec = AgentSpec::resolve(&self.config, agent_name, wt_path.clone(), &wt_env);
        let Some(spec) = spec else {
            self.set_status(format!("未知 agent:{agent_name}"), true);
            return;
        };

        let command = Some((spec.command.clone(), spec.args.clone()));
        let env: Vec<(String, String)> = spec.extra_env.into_iter().collect();
        let cwd = spec.cwd.clone();

        let terminal = cx.new(|cx| {
            TerminalView::new(cx, Some(cwd), command, env)
                .expect("failed to start agent terminal")
        });
        self.terminal = Some(terminal);

        // 4) agent 运行期锁定 worktree,防误删/prune。
        let _ = git::lock(&self.repo, &wt_path, Some("agent running"));

        self.refresh_worktrees();
        self.set_status(format!("已在 {branch} 启动 {agent_name}"), false);
        cx.notify();
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut list = div().flex().flex_col().gap_1();

        // 标题。
        list = list.child(
            div()
                .text_color(rgb(ACCENT))
                .child(SharedString::from("LucyMind"))
                .mb_2(),
        );
        list = list.child(
            div()
                .text_color(rgb(FG_DIM))
                .child(SharedString::from(format!(
                    "repo: {}",
                    self.repo
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default()
                )))
                .mb_2(),
        );

        // 动作按钮:起 claude / codex。
        for agent in ["claude", "codex"] {
            let name = agent.to_string();
            list = list.child(
                div()
                    .id(SharedString::from(format!("new-{agent}")))
                    .px_2()
                    .py_1()
                    .bg(rgb(0x0e_63_9c))
                    .text_color(rgb(0xff_ff_ff))
                    .rounded_md()
                    .cursor_pointer()
                    .child(SharedString::from(format!("＋ New worktree → {agent}")))
                    .on_click(cx.listener(move |this, _ev, _window, cx| {
                        this.new_worktree_and_agent(&name, cx);
                    })),
            );
        }

        // worktree 列表。
        list = list.child(
            div()
                .mt_3()
                .text_color(rgb(FG_DIM))
                .child(SharedString::from("Worktrees")),
        );
        for wt in &self.worktrees {
            let label = wt
                .branch
                .clone()
                .unwrap_or_else(|| "(detached)".to_string());
            list = list.child(
                div()
                    .px_2()
                    .py_1()
                    .text_color(rgb(FG))
                    .child(SharedString::from(format!(
                        "{} {}",
                        if wt.locked { "🔒" } else { "•" },
                        label
                    ))),
            );
        }

        // 状态消息。
        if let Some(status) = &self.status {
            list = list.child(
                div()
                    .mt_3()
                    .p_2()
                    .text_color(rgb(if status.is_error { ERR } else { ACCENT }))
                    .child(status.text.clone()),
            );
        }

        div()
            .w(px(260.0))
            .h_full()
            .bg(rgb(BG_SIDEBAR))
            .text_color(rgb(FG))
            .p_3()
            .child(list)
    }
}

impl Focusable for WorkspaceView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for WorkspaceView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let main: gpui::AnyElement = match &self.terminal {
            Some(term) => div().flex_1().h_full().child(term.clone()).into_any_element(),
            None => div()
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(BG_MAIN))
                .text_color(rgb(FG_DIM))
                .child(SharedString::from(
                    "点击左侧「New worktree → claude/codex」开始",
                ))
                .into_any_element(),
        };

        div()
            .flex()
            .flex_row()
            .size_full()
            .bg(rgb(BG_MAIN))
            .child(self.sidebar(cx))
            .child(main)
    }
}
