//! U9:端到端主流程装配。
//!
//! 顶层 [`WorkspaceView`] = 左侧栏(仓库 + worktree 列表 + 动作)+ 右侧终端区。
//! 串起:选仓库 → 一键建 worktree → 跑 postCreate hook → 在 worktree 起 agent →
//! 显示在终端。这把 core(git/hooks/agent)与 app(终端渲染)接成完整闭环。

use std::path::PathBuf;

use gpui::{
    div, prelude::*, rgb, Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement,
    Render, SharedString, Styled, Window,
};

use lucy_core::agent::AgentSpec;
use lucy_core::config::{self, WorktreeConfig};
use lucy_core::git::{self, CreateMode, WorktreeEntry};
use lucy_core::hooks::{self, HookContext, LifecycleEvent};
use lucy_core::session::{self, Registry, Session};

use crate::terminal_view::TerminalView;
use crate::theme;

/// 一条状态消息(动作反馈 / 错误),显示在侧边栏底部。
#[derive(Clone)]
struct Status {
    text: SharedString,
    is_error: bool,
}

/// 规范化路径(消除 macOS /private 前缀、绝对/相对差异)。失败(如路径已删)
/// 时回退原值。所有用作 terminals map key / active 比较的路径都必须先过它,
/// 否则"点击时的路径"与"存入时的路径"字符串不等 → 同一 worktree 被当成两个
/// (表现为:不高亮当前项、点当前项又起新会话顶掉正在跑的)。
fn canon(p: &std::path::Path) -> std::path::PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

/// 规范化后比较两个路径。
fn same_path(a: &std::path::Path, b: &std::path::Path) -> bool {
    canon(a) == canon(b)
}

/// 数某 worktree 的未提交改动条数(git status --porcelain 行数)。
fn count_uncommitted(worktree: &std::path::Path) -> usize {
    std::process::Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.trim().is_empty())
                .count()
        })
        .unwrap_or(0)
}

/// 待确认的关闭操作(有未提交改动时,等用户确认)。
struct PendingClose {
    worktree_path: PathBuf,
    branch: String,
    /// 未提交改动条数(展示给用户)。
    dirty_count: usize,
}

pub struct WorkspaceView {
    /// 当前仓库根。None = 尚未选仓库(显示 pick a directory 空态)。
    repo: Option<PathBuf>,
    config: WorktreeConfig,
    worktrees: Vec<WorktreeEntry>,
    /// 本工具开的 session 注册表(标记哪些 worktree 是我们建的)。
    registry: Registry,
    /// 每个本工具 worktree 路径 → 其终端 Entity(用于关闭时停 agent)。
    terminals: std::collections::HashMap<PathBuf, Entity<TerminalView>>,
    /// 当前显示在主区的终端路径。
    active: Option<PathBuf>,
    status: Option<Status>,
    /// 待确认的关闭(有未提交改动时弹窗)。
    pending_close: Option<PendingClose>,
    /// 侧边栏宽度(可拖 splitter 调整)。
    sidebar_width: f32,
    /// 正在拖 splitter 调侧边栏宽度。
    dragging_splitter: bool,
    /// 正在编辑别名的分支名(None = 未在编辑)。输入内容存 alias_input(InputState)。
    editing_alias: Option<String>,
    /// 别名输入框状态(gpui-component Input,带 IME + 选择)。懒创建。
    alias_input: Option<gpui::Entity<gpui_component::input::InputState>>,
    focus: FocusHandle,
}

/// 侧边栏宽度范围。
const SIDEBAR_MIN_W: f32 = 180.0;
const SIDEBAR_MAX_W: f32 = 480.0;
const SIDEBAR_DEFAULT_W: f32 = 248.0;

impl WorkspaceView {
    /// 启动:给一个候选仓库路径(通常来自 cwd)。若它是有效 git 仓库则用,
    /// 否则以空态启动并自动弹目录选择器(.app 双击启动时 cwd 不是仓库的场景)。
    pub fn new(cx: &mut Context<Self>, candidate: Option<PathBuf>) -> Self {
        let registry = Registry::load_default().unwrap_or_default();

        // 校验候选路径是否真的是 git 仓库。
        let repo = candidate.and_then(|c| lucy_core::git::main_worktree_root(&c));

        let mut this = Self {
            repo: None,
            config: WorktreeConfig::default(),
            worktrees: Vec::new(),
            registry,
            terminals: std::collections::HashMap::new(),
            active: None,
            status: None,
            pending_close: None,
            sidebar_width: SIDEBAR_DEFAULT_W,
            dragging_splitter: false,
            editing_alias: None,
            alias_input: None,
            focus: cx.focus_handle(),
        };

        match repo {
            Some(r) => this.set_repo(r),
            None => this.open_repo_picker(cx), // 无有效仓库 → 启动即弹目录选择器
        }
        this
    }

    /// 设置仓库根:加载其配置、刷新 worktree 列表。
    fn set_repo(&mut self, repo: PathBuf) {
        let repo = canon(&repo);
        self.config = config::load(repo.join(".worktree.toml"))
            .map(|l| l.config)
            .unwrap_or_default();
        self.worktrees = git::list(&repo).unwrap_or_default();
        self.repo = Some(repo);
    }

    /// 弹 native 目录选择器让用户选一个 git 仓库。选中后 set_repo。
    fn open_repo_picker(&self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open Git repository".into()),
        });
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                if let Some(dir) = paths.into_iter().next() {
                    let _ = this.update(cx, |view, cx| {
                        // 解析成主仓根(选的可能是仓库内子目录)。
                        match lucy_core::git::main_worktree_root(&dir) {
                            Some(root) => {
                                view.set_repo(root);
                                view.set_status("已打开仓库", false);
                            }
                            None => view.set_status("所选目录不是 git 仓库", true),
                        }
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// 该 worktree 是否由本工具建(据注册表)—— 仅用于 ●/· 标记,不作操作门槛。
    fn is_ours(&self, path: &std::path::Path) -> bool {
        match &self.repo {
            Some(r) => self.registry.is_ours(r, path),
            None => false,
        }
    }

    /// 是否是主仓库本身(主仓不是 worktree,不可关闭)。用规范化路径比较。
    fn is_main_repo(&self, path: &std::path::Path) -> bool {
        self.repo.as_deref().is_some_and(|r| same_path(path, r))
    }

    /// 打开一个 worktree:已有终端则切过去;没有则在该目录起一个默认 shell。
    fn open_worktree(&mut self, wt_path: PathBuf, cx: &mut Context<Self>) {
        // 统一用规范化路径作 key —— 避免同一 worktree 因 /private 前缀被当两个。
        let wt_path = canon(&wt_path);
        if !self.terminals.contains_key(&wt_path) {
            // 没有活动终端(存量 worktree)→ 起一个默认 shell 会话。
            let wt_env = HookContext {
                worktree_path: wt_path.clone(),
                worktree_branch: String::new(),
                worktree_name: wt_path
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                repo_root: self.repo.clone().unwrap_or_default(),
            }
            .env_vars();
            let env: Vec<(String, String)> = std::iter::once(
                ("TERM".to_string(), "xterm-256color".to_string()),
            )
            .chain(wt_env)
            .collect();

            let cwd = wt_path.clone();
            let terminal = cx.new(|cx| {
                TerminalView::new(cx, Some(cwd), None, env)
                    .expect("failed to start shell terminal")
            });
            self.terminals.insert(wt_path.clone(), terminal);
        }
        self.active = Some(wt_path);
        cx.notify();
    }

    fn persist_registry(&self) {
        if let Err(e) = self.registry.save_default() {
            log::warn!("保存 session 注册表失败: {e}");
        }
    }

    fn set_status(&mut self, text: impl Into<SharedString>, is_error: bool) {
        self.status = Some(Status {
            text: text.into(),
            is_error,
        });
    }

    fn refresh_worktrees(&mut self) {
        self.worktrees = match &self.repo {
            Some(r) => git::list(r).unwrap_or_default(),
            None => Vec::new(),
        };
    }

    /// 主流程:建 worktree → postCreate hook → 起 agent 到终端。
    fn new_worktree_and_agent(&mut self, agent_name: &str, cx: &mut Context<Self>) {
        let Some(repo) = self.repo.clone() else {
            self.set_status("请先打开一个 git 仓库", true);
            return;
        };


        // 分支名:随机四词组合(如 lucy/session-brave-cyan-fox-moon),几乎不撞名,
        // 零 git 探测(旧的逐个递增探测在大仓库要几百 ms)。撞名交 git add 兜底。
        let branch = git::random_branch_name("lucy/");
        let base = self.config.worktree.default_base.clone();

        // worktree 路径:仓库外兄弟目录(按配置)。
        let repo_name = repo
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "repo".into());
        let parent = config::resolve_sibling_dir(&self.config.worktree.dir, &repo_name);
        let parent_dir = repo.join(&parent);
        let wt_path = git::sibling_worktree_path(&parent_dir, &branch);

        // 1) 建 worktree(带分支占用检查)。
        let add_res = git::add(
            &repo,
            &wt_path,
            &CreateMode::NewBranch {
                branch: branch.clone(),
                base,
            },
        );
        if let Err(e) = add_res {
            self.set_status(format!("建 worktree 失败:{e}"), true);
            return;
        }

        // 2) postCreate hook(copy + 命令),注入环境变量。
        let ctx = HookContext {
            worktree_path: wt_path.clone(),
            worktree_branch: branch.clone(),
            worktree_name: branch.replace('/', "-"),
            repo_root: repo.clone(),
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
        // 统一用规范化路径作 key(与 open_worktree / is_active 一致)。
        let wt_key = canon(&wt_path);
        self.terminals.insert(wt_key.clone(), terminal);
        self.active = Some(wt_key);

        // 4) agent 运行期锁定 worktree,防误删/prune。
        let _ = git::lock(&repo, &wt_path, Some("agent running"));

        // 5) 注册到 session 注册表(标记这是本工具建的)并持久化。
        self.registry.register(
            &repo,
            Session {
                path: wt_path.clone(),
                branch: branch.clone(),
                agent: Some(agent_name.to_string()),
                created_at: session::now_secs(),
            },
        );
        self.persist_registry();

        self.refresh_worktrees();
        self.set_status(format!("已在 {branch} 启动 {agent_name}"), false);
        cx.notify();
    }

    // ---------------- 关闭 worktree ----------------

    /// 请求关闭:先停 agent + 检查未提交改动。干净 → 直接关;脏 → 弹确认。
    fn request_close(&mut self, wt_path: PathBuf, branch: String, cx: &mut Context<Self>) {
        // 先停掉该终端的 agent(两段式)。用规范化 key 查。
        if let Some(term) = self.terminals.get(&canon(&wt_path)) {
            term.update(cx, |t, _| t.shutdown());
        }

        // 检查未提交改动。
        let dirty = git::has_uncommitted_changes(&wt_path).unwrap_or(false);

        if dirty {
            let count = count_uncommitted(&wt_path);
            self.pending_close = Some(PendingClose {
                dirty_count: count,
                worktree_path: wt_path,
                branch,
            });
            cx.notify();
        } else {
            self.do_close(&wt_path, false, cx);
        }
    }

    /// 确认关闭(用户在弹窗点了「确认」)—— force 删。
    fn confirm_close(&mut self, cx: &mut Context<Self>) {
        if let Some(pending) = self.pending_close.take() {
            self.do_close(&pending.worktree_path, true, cx);
        }
    }

    fn cancel_close(&mut self, cx: &mut Context<Self>) {
        self.pending_close = None;
        cx.notify();
    }

    /// 执行关闭:**乐观 UI + 后台跑慢 git**。
    ///
    /// 大仓库(如 superset)下 `git worktree remove` 本身要几百 ms,若同步跑会卡
    /// UI 线程近 1 秒。所以:UI 立即移除该项(乐观),把 unlock/remove 挪到后台
    /// 线程,完成后回主线程刷新列表。
    fn do_close(&mut self, wt_path: &std::path::Path, force: bool, cx: &mut Context<Self>) {
        let wt_path = wt_path.to_path_buf();
        let Some(repo) = self.repo.clone() else {
            return;
        };

        // 安全底线:绝不删主仓库(即便 UI 层漏了保护)。
        if self.is_main_repo(&wt_path) {
            self.set_status("主仓库不可关闭", true);
            self.pending_close = None;
            cx.notify();
            return;
        }

        // 找分支名(供 hook 环境变量 + 状态提示)。
        let branch = self
            .registry
            .list_for_repo(&repo)
            .into_iter()
            .find(|s| s.path == wt_path)
            .map(|s| s.branch)
            .unwrap_or_default();

        // 1) preRemove hook(通常极快,同步跑)。
        let ctx = HookContext {
            worktree_path: wt_path.clone(),
            worktree_branch: branch.clone(),
            worktree_name: branch.replace('/', "-"),
            repo_root: repo.clone(),
        };
        hooks::run_event(
            LifecycleEvent::PreRemove,
            &self.config.hooks,
            &self.config.copy,
            &ctx,
            |_step| {},
        );

        // 2) 乐观 UI:立即从终端表/active/列表移除该项,界面瞬间响应。
        let key = canon(&wt_path);
        self.terminals.remove(&key);
        if self.active.as_deref().is_some_and(|a| same_path(a, &wt_path)) {
            self.active = self.terminals.keys().next().cloned();
        }
        self.worktrees.retain(|w| !same_path(&w.path, &wt_path));
        self.registry.unregister(&repo, &wt_path);
        self.persist_registry();
        self.set_status(format!("正在关闭 {branch}…"), false);
        cx.notify();

        // 3) 后台跑慢 git(unlock + remove),完成后回主线程刷新 + 报结果。
        let repo_bg = repo.clone();
        let wt_bg = wt_path.clone();
        let branch_bg = branch.clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx: &mut gpui::AsyncApp| {
            // 慢 git 放后台执行器(不占 UI 线程)。
            let result = cx
                .background_executor()
                .spawn(async move {
                    let _ = git::unlock(&repo_bg, &wt_bg);
                    git::remove(&repo_bg, &wt_bg, force)
                })
                .await;

            let _ = this.update(cx, |view, cx| {
                match result {
                    Ok(()) => view.set_status(format!("已关闭 {branch_bg}"), false),
                    Err(e) => view.set_status(format!("删除 worktree 失败:{e}"), true),
                }
                // 用 git 真实状态刷新列表(纠正乐观移除的偏差)。
                view.refresh_worktrees();
                cx.notify();
            });
        })
        .detach();
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
                    div()
                        .id("open-repo")
                        .px(theme::space_sm())
                        .py(theme::space_xs())
                        .bg(rgb(theme::BTN_BG))
                        .border_1()
                        .border_color(rgb(theme::BORDER))
                        .rounded(theme::radius())
                        .text_color(rgb(theme::TEXT))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(theme::BTN_BG_HOVER)))
                        .child(SharedString::from("Open…"))
                        .on_click(cx.listener(|this, _ev, _w, cx| {
                            this.open_repo_picker(cx);
                        })),
                ),
        );

        // 区域标签:Agents —— 标示下方是 agent 会话区(与 WORKTREES 标签同样式)。
        list = list.child(
            div()
                .mb(theme::space_sm())
                .text_color(rgb(theme::TEXT_DIM))
                .child(SharedString::from("AGENTS")),
        );

        // 动作按钮:图标 + 名字(Claude / Codex)。无彩 —— 深灰底 + 细描边 +
        // 2px 微圆角;悬浮/按下靠明度。图标单色跟主题染色。
        for (agent, display) in [("claude", "Claude"), ("codex", "Codex")] {
            let name = agent.to_string();
            let icon = crate::assets::agent_icon(agent);
            let mut btn = div()
                .id(SharedString::from(format!("new-{agent}")))
                .flex()
                .flex_row()
                .items_center()
                .gap(theme::space_md())
                .mb(theme::space_xs())
                .px(theme::space_md())
                .py(theme::space_sm())
                .bg(rgb(theme::BTN_BG))
                .border_1()
                .border_color(rgb(theme::BORDER))
                .rounded(theme::radius())
                .text_color(rgb(theme::TEXT))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(theme::BTN_BG_HOVER)).border_color(rgb(theme::TEXT_FAINT)))
                .active(|s| s.bg(rgb(theme::BTN_BG_ACTIVE)));

            if let Some(path) = icon {
                btn = btn.child(
                    gpui::svg()
                        .size(gpui::px(16.0))
                        .path(path)
                        .text_color(rgb(theme::TEXT)),
                );
            }
            btn = btn.child(SharedString::from(display));

            btn = btn.on_click(cx.listener(move |this, _ev, _window, cx| {
                this.new_worktree_and_agent(&name, cx);
            }));
            list = list.child(btn);
        }

        // 分隔:worktree 段(用描边分隔线,不用颜色)。
        list = list.child(
            div()
                .mt(theme::space_md())
                .mb(theme::space_sm())
                .border_b_1()
                .border_color(rgb(theme::BORDER_SUBTLE))
                .pb(theme::space_xs())
                .text_color(rgb(theme::TEXT_DIM))
                .child(SharedString::from("WORKTREES")),
        );
        for (i, wt) in self.worktrees.iter().enumerate() {
            let branch = wt
                .branch
                .clone()
                .unwrap_or_else(|| "detached".to_string());
            // 显示名:有别名用别名,否则用分支名。别名存 .worktree.toml 的 [alias]。
            let alias = self.config.alias.get(&branch).cloned();
            let label = alias.clone().unwrap_or_else(|| branch.clone());
            let ours = self.is_ours(&wt.path);
            let is_main = self.is_main_repo(&wt.path);
            let is_active = self
                .active
                .as_deref()
                .is_some_and(|a| same_path(a, &wt.path));
            let wt_path_for_click = wt.path.clone();

            // 除主仓外都可点(切换/打开)、可关。
            let interactive = !is_main;

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
            if interactive {
                row = row.cursor_pointer().hover(|s| s.bg(rgb(theme::BTN_BG_HOVER)));
                row = row.on_click(cx.listener(move |this, _ev, _w, cx| {
                    this.open_worktree(wt_path_for_click.clone(), cx);
                }));
            }

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

            // ✎ 改别名 + ✕ 关闭:固定在行尾右对齐(flex_none,不被名字挤走)。
            if interactive {
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

            list = list.child(row);
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
}

impl Focusable for WorkspaceView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl WorkspaceView {
    /// 未提交改动确认弹窗(性冷淡风:半透明遮罩 + 描边卡片 + 两个无彩按钮)。
    fn confirm_dialog(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let pending = self.pending_close.as_ref();
        let (branch, count) = pending
            .map(|p| (p.branch.clone(), p.dirty_count))
            .unwrap_or_default();

        // 遮罩(scrim,压暗背景)。
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::with_alpha(0x00_00_00, 0.55))
            .child(
                // 卡片。
                div()
                    .w(gpui::px(360.0))
                    .bg(rgb(theme::SURFACE))
                    .border_1()
                    .border_color(rgb(theme::BORDER))
                    .rounded(theme::radius())
                    .p(theme::space_lg())
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
                    .child(
                        // 按钮行:取消(默认)+ 确认关闭(语义红描边)。
                        div()
                            .flex()
                            .flex_row()
                            .justify_end()
                            .gap(theme::space_sm())
                            .child(
                                div()
                                    .id(SharedString::from("cancel-close"))
                                    .px(theme::space_md())
                                    .py(theme::space_sm())
                                    .bg(rgb(theme::BTN_BG))
                                    .border_1()
                                    .border_color(rgb(theme::BORDER))
                                    .rounded(theme::radius())
                                    .text_color(rgb(theme::TEXT))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(theme::BTN_BG_HOVER)))
                                    .child(SharedString::from("取消"))
                                    .on_click(cx.listener(|this, _ev, _w, cx| {
                                        this.cancel_close(cx);
                                    })),
                            )
                            .child(
                                div()
                                    .id(SharedString::from("confirm-close"))
                                    .px(theme::space_md())
                                    .py(theme::space_sm())
                                    .bg(rgb(theme::BTN_BG))
                                    .border_1()
                                    .border_color(rgb(theme::STATE_ERROR))
                                    .rounded(theme::radius())
                                    .text_color(rgb(theme::STATE_ERROR))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(theme::BTN_BG_HOVER)))
                                    .child(SharedString::from("丢弃并关闭"))
                                    .on_click(cx.listener(|this, _ev, _w, cx| {
                                        this.confirm_close(cx);
                                    })),
                            ),
                    ),
            )
    }

    /// 打开别名编辑器:懒创建 gpui-component 的 InputState,填入当前别名,聚焦。
    fn open_alias_editor(
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
    fn alias_dialog(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_component::input::Input;
        let branch = self.editing_alias.clone().unwrap_or_default();
        let input_el = self
            .alias_input
            .as_ref()
            .map(|state| Input::new(state).into_any_element());

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::with_alpha(0x00_00_00, 0.55))
            .child(
                div()
                    .w(gpui::px(380.0))
                    .bg(rgb(theme::SURFACE))
                    .border_1()
                    .border_color(rgb(theme::BORDER))
                    .rounded(theme::radius())
                    .p(theme::space_lg())
                    .flex()
                    .flex_col()
                    .gap(theme::space_md())
                    .font_family(theme::FONT_UI)
                    .child(
                        div()
                            .text_color(rgb(theme::TEXT_DIM))
                            .child(SharedString::from(format!("为 {branch} 设置别名"))),
                    )
                    .children(input_el)
                    .child(
                        // 按钮行:取消 + 保存。
                        div()
                            .flex()
                            .flex_row()
                            .justify_end()
                            .gap(theme::space_sm())
                            .child(
                                div()
                                    .id("alias-cancel")
                                    .px(theme::space_md())
                                    .py(theme::space_sm())
                                    .bg(rgb(theme::BTN_BG))
                                    .border_1()
                                    .border_color(rgb(theme::BORDER))
                                    .rounded(theme::radius())
                                    .text_color(rgb(theme::TEXT))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(theme::BTN_BG_HOVER)))
                                    .child(SharedString::from("取消"))
                                    .on_click(_cx.listener(|this, _ev, _w, cx| {
                                        this.editing_alias = None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("alias-save")
                                    .px(theme::space_md())
                                    .py(theme::space_sm())
                                    .bg(rgb(theme::BTN_BG))
                                    .border_1()
                                    .border_color(rgb(theme::STATE_OK))
                                    .rounded(theme::radius())
                                    .text_color(rgb(theme::STATE_OK))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(theme::BTN_BG_HOVER)))
                                    .child(SharedString::from("保存"))
                                    .on_click(_cx.listener(|this, _ev, _w, cx| {
                                        this.commit_alias(cx);
                                    })),
                            ),
                    ),
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
        match config::set_alias(&path, branch, alias) {
            Ok(()) => {
                // 重载配置(拿到新别名),刷新显示。
                if let Ok(loaded) = config::load(&path) {
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

    /// 主区底部状态栏(编辑器风格:常驻、极细、克制)。空状态也占位以稳定布局。
    fn status_bar(&self) -> impl IntoElement {
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

impl Render for WorkspaceView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 终端区(填满上方)。
        let term_area: gpui::AnyElement =
            match self.active.as_ref().and_then(|p| self.terminals.get(p)) {
                Some(term) => div().flex_1().min_h_0().child(term.clone()).into_any_element(),
                None => div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgb(theme::BG))
                    .text_color(rgb(theme::TEXT_FAINT))
                    .child(SharedString::from("select an action to begin"))
                    .into_any_element(),
            };

        // 主列:终端区 + 底部状态栏。
        let main = div()
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .child(term_area)
            .child(self.status_bar());

        // 分隔条(splitter):侧边栏与主区之间,可拖调宽度。
        let splitter = div()
            .id("splitter")
            .flex_none()
            .w(gpui::px(4.0))
            .h_full()
            .bg(rgb(theme::BORDER))
            .cursor_col_resize()
            .hover(|s| s.bg(rgb(theme::TEXT_FAINT)))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _ev, _w, cx| {
                    this.dragging_splitter = true;
                    cx.notify();
                }),
            );

        let mut root = div()
            .relative()
            .flex()
            .flex_row()
            .size_full()
            .bg(rgb(theme::BG))
            // 拖 splitter 时:全局监听鼠标移动改宽度、抬起结束。
            .on_mouse_move(cx.listener(|this, ev: &gpui::MouseMoveEvent, _w, cx| {
                if this.dragging_splitter {
                    let w = f32::from(ev.position.x).clamp(SIDEBAR_MIN_W, SIDEBAR_MAX_W);
                    this.sidebar_width = w;
                    cx.notify();
                }
            }))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _ev, _w, cx| {
                    if this.dragging_splitter {
                        this.dragging_splitter = false;
                        cx.notify();
                    }
                }),
            )
            .child(self.sidebar(cx))
            .child(splitter)
            .child(main);

        // 有待确认关闭 → 叠加确认弹窗。
        if self.pending_close.is_some() {
            root = root.child(self.confirm_dialog(cx));
        }
        // 正在编辑别名 → 叠加别名编辑弹窗。
        if self.editing_alias.is_some() {
            root = root.child(self.alias_dialog(cx));
        }

        root
    }
}
