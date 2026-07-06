//! U9:端到端主流程装配。
//!
//! 顶层 [`WorkspaceView`] = 左侧栏(仓库 + worktree 列表 + 动作)+ 右侧终端区。
//! 串起:选仓库 → 一键建 worktree → 跑 postCreate hook → 在 worktree 起 agent →
//! 显示在终端。这把 core(git/hooks/agent)与 app(终端渲染)接成完整闭环。
//!
//! 本模块只保留**状态与业务逻辑**;各 UI 面板拆到子模块(仍作 `WorkspaceView`
//! 的 `impl` 方法,跨文件 impl):
//! - [`sidebar`]     侧边栏(仓库 / Agents / worktree 列表)
//! - [`dialogs`]     关闭确认 + 别名编辑弹窗
//! - [`settings`]    `.worktree.toml` 图形化设置面板(别名之外的字段)
//! - [`status_bar`]  主区底部状态栏

mod dialogs;
mod settings;
mod sidebar;
mod status_bar;
mod tabs;

use std::path::PathBuf;

use gpui::{
    div, prelude::*, rgb, Context, Entity, FocusHandle, Focusable, IntoElement, KeyDownEvent,
    ParentElement, Render, SharedString, Styled, Window,
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

/// 规范化路径(消除 macOS /private 前缀、绝对/相对差异、Windows `\\?\` verbatim 前缀)。
/// 失败(如路径已删)时回退原值。所有用作 terminals map key / active 比较的路径都必须先过它,
/// 否则"点击时的路径"与"存入时的路径"字符串不等 → 同一 worktree 被当成两个
/// (表现为:不高亮当前项、点当前项又起新会话顶掉正在跑的)。
///
/// Windows 上 `Path::canonicalize` 返回 `\\?\C:\...` verbatim 前缀,git 无法
/// 在此类路径下创建带 `..` 的工作树目录(报 "could not create leading
/// directories ... Invalid argument")。剥掉前缀得到普通路径,git/ConPTY 均可正常用。
fn canon(p: &std::path::Path) -> std::path::PathBuf {
    let c = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    strip_verbatim_prefix(&c)
}

/// 剥掉 Windows verbatim 路径前缀(`\\?\` / `\\?\UNC\`),其余平台原样返回。
fn strip_verbatim_prefix(p: &std::path::Path) -> std::path::PathBuf {
    #[cfg(windows)]
    {
        let s = p.to_string_lossy();
        if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
            return std::path::PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            return std::path::PathBuf::from(rest);
        }
    }
    p.to_path_buf()
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

/// 设置面板的表单状态(打开设置弹窗时创建,含各输入框的 gpui-component
/// InputState + 两个非文本项)。字段一一对应 [`config::EditableSettings`]。
///
/// 数组字段(hook 命令 / copy 文件)用多行 Input,一行一条;提交时按行拆分、
/// 去掉空行。location / fail_fast 是非文本项,直接存值,点选切换。
struct SettingsForm {
    location: config::Location,
    fail_fast: bool,
    dir: gpui::Entity<gpui_component::input::InputState>,
    default_base: gpui::Entity<gpui_component::input::InputState>,
    post_create: gpui::Entity<gpui_component::input::InputState>,
    pre_remove: gpui::Entity<gpui_component::input::InputState>,
    copy_files: gpui::Entity<gpui_component::input::InputState>,
}

/// 用户可选的 shell 类型(launcher menu 的 New Tab 组)。
///
/// `Default` 用系统默认 shell(alacritty tty 层决定);Windows 上可选
/// cmd / PowerShell / PowerShell 7。非 Windows 只有 `Default`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellKind {
    Default,
    #[cfg(windows)]
    Cmd,
    #[cfg(windows)]
    PowerShell,
    #[cfg(windows)]
    Pwsh,
}

impl ShellKind {
    /// 转成 `TerminalView::new` 的 `command` 参数。
    /// `None` = 系统默认 shell;`Some((program, args))` = 指定程序。
    fn command(&self) -> Option<(String, Vec<String>)> {
        match self {
            ShellKind::Default => None,
            #[cfg(windows)]
            ShellKind::Cmd => Some(("cmd.exe".into(), vec![])),
            #[cfg(windows)]
            ShellKind::PowerShell => Some(("powershell.exe".into(), vec![])),
            #[cfg(windows)]
            ShellKind::Pwsh => Some(("pwsh.exe".into(), vec![])),
        }
    }

    /// tab 标题回退(终端未发 OSC 0/2 时显示)。
    fn label(&self) -> &'static str {
        match self {
            ShellKind::Default => "Shell",
            #[cfg(windows)]
            ShellKind::Cmd => "cmd",
            #[cfg(windows)]
            ShellKind::PowerShell => "PowerShell",
            #[cfg(windows)]
            ShellKind::Pwsh => "pwsh",
        }
    }
}

/// 一个终端 tab(终端 Entity + 静态回退标题)。
///
/// 静态标题在创建时由 `ShellKind::label()` 确定(Default → "Shell"、
/// Cmd → "cmd" 等),作为 tab 栏显示的回退。终端可能通过 OSC 0/2 协议
/// 发动态标题(`\x1b]0;<title>\x07`),tab 栏渲染时优先取 `terminal.title()`,
/// 无动态标题时回退到此静态标题。
struct TerminalTab {
    terminal: Entity<TerminalView>,
    /// 静态回退标题(终端未发 OSC 0/2 时显示)。
    title: String,
}

/// 一个 worktree 的终端组(多个 tab + 当前 active tab 索引)。
///
/// 切 worktree 时 `active_tab` 自动恢复(每个 group 独立记忆)。关闭最后一个
/// tab 后 group 从 `terminals` map 移除(worktree 仍在侧边栏)。
struct TerminalGroup {
    tabs: Vec<TerminalTab>,
    active_tab: usize,
}

pub struct WorkspaceView {
    /// 当前仓库根。None = 尚未选仓库(显示 pick a directory 空态)。
    repo: Option<PathBuf>,
    config: WorktreeConfig,
    worktrees: Vec<WorktreeEntry>,
    /// 本工具开的 session 注册表(标记哪些 worktree 是我们建的)。
    registry: Registry,
    /// 每个 worktree 路径 → 其终端组(多 tab)。key 用 `canon()` 规范化路径。
    terminals: std::collections::HashMap<PathBuf, TerminalGroup>,
    /// 当前显示在主区的 worktree 路径(不是 tab 索引;tab 级 active 存在 group 里)。
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
    /// 设置面板表单(Some = 设置弹窗打开中)。见 [`SettingsForm`]。
    settings: Option<SettingsForm>,
    /// launcher 菜单(`+` 按钮下拉)是否打开。
    launcher_menu_open: bool,
    focus: FocusHandle,
    /// 测试用:覆盖 registry 持久化路径(None = 用默认路径 `~/Library/...`)。
    /// 测试设为 tempdir,避免污染真实用户的 session 注册表。
    #[cfg(feature = "test-support")]
    registry_path: Option<PathBuf>,
}

/// 侧边栏宽度范围。
const SIDEBAR_MIN_W: f32 = 180.0;
const SIDEBAR_MAX_W: f32 = 480.0;
const SIDEBAR_DEFAULT_W: f32 = 248.0;

impl WorkspaceView {
    /// 启动:给一个候选仓库路径(通常来自 cwd)。若它是有效 git 仓库则用,
    /// 否则以空态启动并自动弹目录选择器(.app 双击启动时 cwd 不是仓库的场景)。
    pub fn new(cx: &mut Context<Self>, candidate: Option<PathBuf>) -> Self {
        let mut this = Self::construct(cx);
        let repo = candidate
            .as_ref()
            .and_then(lucy_core::git::main_worktree_root);
        match repo {
            Some(r) => this.set_repo(r),
            None => this.open_repo_picker(cx), // 无有效仓库 → 启动即弹目录选择器
        }
        this
    }

    /// 测试专用构造:同 [`new`](Self::new) 但**不弹 `open_repo_picker`**
    /// (TestPlatform 未实现 `prompt_for_paths`,会 panic)。空态(None / 非 git
    /// 目录)直接进 `repo == None` 空态,测试用 `set_repo_for_test` 注入仓库。
    #[cfg(feature = "test-support")]
    pub fn new_for_test(cx: &mut Context<Self>, candidate: Option<PathBuf>) -> Self {
        let mut this = Self::construct(cx);
        let repo = candidate
            .as_ref()
            .and_then(lucy_core::git::main_worktree_root);
        if let Some(r) = repo {
            this.set_repo(r);
        }
        this
    }

    /// 公共构造:填默认字段(不弹 prompt、不 set_repo)。
    fn construct(cx: &mut Context<Self>) -> Self {
        let registry = Registry::load_default().unwrap_or_default();
        Self {
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
            settings: None,
            launcher_menu_open: false,
            focus: cx.focus_handle(),
            #[cfg(feature = "test-support")]
            registry_path: None,
        }
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

    /// 打开一个 worktree:已有终端组则切过去;没有则在该目录起一个默认 shell tab。
    fn open_worktree(&mut self, wt_path: PathBuf, cx: &mut Context<Self>) {
        // 统一用规范化路径作 key —— 避免同一 worktree 因 /private 前缀被当两个。
        let wt_path = canon(&wt_path);
        if !self.terminals.contains_key(&wt_path) {
            // 没有终端组(存量 worktree)→ 起一个默认 shell tab。
            let tab = self.spawn_shell_tab(&wt_path, ShellKind::Default, cx);
            self.terminals.insert(
                wt_path.clone(),
                TerminalGroup {
                    tabs: vec![tab],
                    active_tab: 0,
                },
            );
        }
        self.active = Some(wt_path);
        cx.notify();
    }

    /// 起一个 shell 终端 tab(cwd = worktree 路径,注入 TERM + worktree env)。
    /// `shell` 决定启动哪个 shell(Default = 系统默认;Cmd/PowerShell/Pwsh = 指定程序)。
    /// 供 `open_worktree`(无 group 时建首个 tab)、`new_worktree`(侧边栏 `+` 建
    /// worktree 后开首个 tab)、`new_terminal_tab`(tab 栏 `+` 新建 tab)复用。
    fn spawn_shell_tab(
        &self,
        wt_path: &std::path::Path,
        shell: ShellKind,
        cx: &mut Context<Self>,
    ) -> TerminalTab {
        let wt_env = HookContext {
            worktree_path: wt_path.to_path_buf(),
            worktree_branch: String::new(),
            worktree_name: wt_path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            repo_root: self.repo.clone().unwrap_or_default(),
        }
        .env_vars();
        let env: Vec<(String, String)> =
            std::iter::once(("TERM".to_string(), "xterm-256color".to_string()))
                .chain(wt_env)
                .collect();

        let cwd = wt_path.to_path_buf();
        let command = shell.command();
        let terminal = cx.new(|cx| {
            TerminalView::new(cx, Some(cwd), command, env).expect("failed to start shell terminal")
        });
        TerminalTab {
            terminal,
            title: shell.label().to_string(),
        }
    }

    fn persist_registry(&self) {
        let result = {
            #[cfg(feature = "test-support")]
            {
                if let Some(p) = &self.registry_path {
                    self.registry.save(p)
                } else {
                    self.registry.save_default()
                }
            }
            #[cfg(not(feature = "test-support"))]
            {
                self.registry.save_default()
            }
        };
        if let Err(e) = result {
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

    /// 主流程:建 worktree → postCreate hook → 开一个 shell 终端 tab。
    ///
    /// agent 不再自动 spawn —— 用户在新 shell 里通过 tab 栏的 agent 按钮发命令
    /// 启动(`send_agent_command`),有更多控制空间(可先跑命令、可 Ctrl+C 回到
    /// shell、同终端跑多个 agent)。
    fn new_worktree(&mut self, cx: &mut Context<Self>) {
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
                "worktree 已建,但 postCreate hook 有失败步骤(见日志)".to_string(),
                true,
            );
            // 不回滚 worktree(计划:hook 失败不删 worktree)。
        }

        // 3) 开 shell 终端 tab(不自动起 agent;用户通过 tab 栏 agent 按钮发命令)。
        let wt_key = canon(&wt_path);
        let tab = self.spawn_shell_tab(&wt_key, ShellKind::Default, cx);
        self.terminals.insert(
            wt_key.clone(),
            TerminalGroup {
                tabs: vec![tab],
                active_tab: 0,
            },
        );
        self.active = Some(wt_key);

        // 4) agent 运行期锁定 worktree,防误删/prune。
        let _ = git::lock(&repo, &wt_path, Some("agent running"));

        // 5) 注册到 session 注册表(标记这是本工具建的)并持久化。
        //    agent 字段记 None —— 用户后续通过 tab 栏按钮选择 agent,
        //    建时不知会用哪个 agent(可能在一个 shell 里跑多个)。
        self.registry.register(
            &repo,
            Session {
                path: wt_path.clone(),
                branch: branch.clone(),
                agent: None,
                created_at: session::now_secs(),
            },
        );
        self.persist_registry();

        self.refresh_worktrees();
        self.set_status(format!("已在 {branch} 开 shell"), false);
        cx.notify();
    }

    // ---------------- 关闭 worktree ----------------

    /// 请求关闭:先停所有 tab 的终端 + 检查未提交改动。干净 → 直接关;脏 → 弹确认。
    fn request_close(&mut self, wt_path: PathBuf, branch: String, cx: &mut Context<Self>) {
        // 先停掉该 worktree 的所有 tab 终端(两段式)。用规范化 key 查。
        if let Some(group) = self.terminals.get(&canon(&wt_path)) {
            for tab in &group.tabs {
                tab.terminal.update(cx, |t, _| t.shutdown());
            }
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
        if self
            .active
            .as_deref()
            .is_some_and(|a| same_path(a, &wt_path))
        {
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
        cx.spawn(
            async move |this: gpui::WeakEntity<Self>, cx: &mut gpui::AsyncApp| {
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
            },
        )
        .detach();
    }

    // ---------------- tab 操作(多终端 per worktree)----------------

    /// 切换 active tab(点 tab 标题触发)。边界 clamp 防越界。
    fn switch_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(key) = &self.active {
            if let Some(group) = self.terminals.get_mut(key) {
                if index < group.tabs.len() {
                    group.active_tab = index;
                    cx.notify();
                }
            }
        }
    }

    /// 新建 shell tab(tab 栏 `+` 按钮 / launcher menu 触发)。在当前 active
    /// worktree 的 group 里 append 一个指定 shell 类型的 tab;无 active 则 no-op。
    fn new_terminal_tab(&mut self, shell: ShellKind, cx: &mut Context<Self>) {
        let Some(key) = self.active.clone() else {
            return;
        };
        let tab = self.spawn_shell_tab(&key, shell, cx);
        let group = self.terminals.entry(key).or_insert_with(|| TerminalGroup {
            tabs: Vec::new(),
            active_tab: 0,
        });
        group.tabs.push(tab);
        group.active_tab = group.tabs.len() - 1;
        cx.notify();
    }

    /// 启动 agent:创建新 shell tab(Default)+ 立即往新 tab 发 agent 命令。
    /// 每个 agent 独立 tab,可并行运行。launcher menu 的 "Launch Agent" 项触发。
    fn launch_agent(&mut self, agent_name: &str, cx: &mut Context<Self>) {
        self.new_terminal_tab(ShellKind::Default, cx);
        self.send_agent_command(agent_name, cx);
    }

    /// 在系统文件管理器中打开 active worktree 目录。
    /// macOS: `open`、Windows: `explorer`、Linux: `xdg-open`。
    /// 用 `spawn()`(非 `status()`),不阻塞 UI 线程。无 active 时 no-op。
    fn reveal_in_file_manager(&self, _cx: &mut Context<Self>) {
        let Some(path) = &self.active else {
            return;
        };
        let path = path.clone();
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open").arg(&path).spawn();
        #[cfg(target_os = "windows")]
        let _ = std::process::Command::new("explorer").arg(&path).spawn();
        #[cfg(target_os = "linux")]
        let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
    }

    /// 关闭指定 tab(tab `✕` 按钮触发)。只停该终端,不删 worktree。
    /// 关最后一个 tab 后 group 移除,终端区回到空态。
    fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(key) = self.active.clone() else {
            return;
        };
        let Some(group) = self.terminals.get_mut(&key) else {
            return;
        };
        if index >= group.tabs.len() {
            return;
        }
        // 停 PTY(避免 leak-detection 误报)。
        group.tabs[index].terminal.update(cx, |t, _| t.shutdown());
        group.tabs.remove(index);
        // 调整 active_tab:删的是 active 或之前的 → 回退;删之后的 → 不变。
        if group.tabs.is_empty() {
            // 最后一个 tab 关了 → 移除 group(worktree 仍在侧边栏)。
            self.terminals.remove(&key);
        } else if group.active_tab >= group.tabs.len() {
            // 删的是最后一个 tab 且是 active → 回退到前一个。
            group.active_tab = group.tabs.len() - 1;
        } else if index < group.active_tab {
            // 删的在 active 之前 → active 索引前移。
            group.active_tab -= 1;
        }
        cx.notify();
    }

    /// 往当前 active tab 的 shell 发 agent 命令(tab 栏 agent 按钮触发)。
    /// 构造 `command args\n` 写入 PTY,shell 在 worktree 目录里执行(已注入 env)。
    /// 无 active / 无 tab 则 no-op。
    fn send_agent_command(&mut self, agent_name: &str, cx: &mut Context<Self>) {
        let Some(key) = self.active.clone() else {
            return;
        };
        let Some(group) = self.terminals.get(&key) else {
            return;
        };
        let Some(tab) = group.tabs.get(group.active_tab) else {
            return;
        };
        let terminal = tab.terminal.clone();

        let Some(repo) = &self.repo else {
            return;
        };
        let wt_env = HookContext {
            worktree_path: key.clone(),
            worktree_branch: String::new(),
            worktree_name: key
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            repo_root: repo.clone(),
        }
        .env_vars();
        let Some(spec) = AgentSpec::resolve(&self.config, agent_name, key.clone(), &wt_env) else {
            self.set_status(format!("未知 agent:{agent_name}"), true);
            return;
        };
        let cmd = Self::agent_command_string(&spec);
        terminal.update(cx, |t, _| t.send_text(&cmd));
    }

    /// 构造 agent 启动命令字符串(写入 shell PTY):`command args\n`。
    /// args 含空格 / 引号 / 空时用双引号包裹 + 转义。
    fn agent_command_string(spec: &AgentSpec) -> String {
        let mut s = spec.command.clone();
        for arg in &spec.args {
            s.push(' ');
            if arg.contains(' ') || arg.contains('"') || arg.contains('\'') || arg.is_empty() {
                s.push('"');
                s.push_str(&arg.replace('\\', "\\\\").replace('"', "\\\""));
                s.push('"');
            } else {
                s.push_str(arg);
            }
        }
        // 用 \r(CR)模拟 Enter 键(input.rs: Key::Enter => b"\r"),
        // 而非 \n —— PTY 行规范把 CR 当作提交命令的行终止符。
        s.push('\r');
        s
    }

    // ---------------- 测试 accessor(仅测试构建可见)----------------
    // 集成测试(tests/)需观察状态机内部字段,但这些字段私有。以下 #[cfg(test)]
    // pub fn 不进生产二进制,API 表面不膨胀。生产代码不应调用。

    /// 当前仓库根(None = 空态未选仓库)。
    #[cfg(feature = "test-support")]
    pub fn repo(&self) -> Option<&std::path::Path> {
        self.repo.as_deref()
    }

    /// 当前 active 终端路径(None = 无活动终端)。
    #[cfg(feature = "test-support")]
    pub fn active_path(&self) -> Option<&std::path::Path> {
        self.active.as_deref()
    }

    /// worktree 列表条数(含 main 行)。
    #[cfg(feature = "test-support")]
    pub fn worktree_count(&self) -> usize {
        self.worktrees.len()
    }

    /// worktree 路径列表(规范化后)。
    #[cfg(feature = "test-support")]
    pub fn worktree_paths(&self) -> Vec<PathBuf> {
        self.worktrees.iter().map(|w| w.path.clone()).collect()
    }

    /// 当前状态消息文本(None = 无状态)。
    #[cfg(feature = "test-support")]
    pub fn current_status(&self) -> Option<&str> {
        self.status.as_ref().map(|s| s.text.as_ref())
    }

    /// 当前状态是否为错误。
    #[cfg(feature = "test-support")]
    pub fn status_is_error(&self) -> bool {
        self.status.as_ref().is_some_and(|s| s.is_error)
    }

    /// 是否有待确认的关闭(脏 worktree 弹窗)。
    #[cfg(feature = "test-support")]
    pub fn has_pending_close(&self) -> bool {
        self.pending_close.is_some()
    }

    /// 待确认关闭的分支名。
    #[cfg(feature = "test-support")]
    pub fn pending_close_branch(&self) -> Option<&str> {
        self.pending_close.as_ref().map(|p| p.branch.as_str())
    }

    /// 指定路径是否有终端组且 tabs 非空。
    #[cfg(feature = "test-support")]
    pub fn terminals_contains(&self, path: &std::path::Path) -> bool {
        self.terminals
            .get(&canon(path))
            .is_some_and(|g| !g.tabs.is_empty())
    }

    /// 设置面板是否打开。
    #[cfg(feature = "test-support")]
    pub fn settings_open(&self) -> bool {
        self.settings.is_some()
    }

    /// 正在编辑别名的分支名(None = 未在编辑)。
    #[cfg(feature = "test-support")]
    pub fn editing_alias(&self) -> Option<&str> {
        self.editing_alias.as_deref()
    }

    /// 该路径是否由本工具建(注册表标记)。
    #[cfg(feature = "test-support")]
    pub fn is_ours_path(&self, path: &std::path::Path) -> bool {
        self.is_ours(path)
    }

    /// 是否是主仓库本身(不可关闭)。
    #[cfg(feature = "test-support")]
    pub fn is_main_repo_path(&self, path: &std::path::Path) -> bool {
        self.is_main_repo(path)
    }

    /// 取某路径 active tab 的终端引用(测试需读终端 snapshot/selection)。
    #[cfg(feature = "test-support")]
    pub fn terminal_at(&self, path: &std::path::Path) -> Option<&Entity<TerminalView>> {
        self.terminals
            .get(&canon(path))
            .and_then(|g| g.tabs.get(g.active_tab))
            .map(|t| &t.terminal)
    }

    /// 指定路径 group 的 tab 数(无 group → 0)。
    #[cfg(feature = "test-support")]
    pub fn tab_count(&self, path: &std::path::Path) -> usize {
        self.terminals
            .get(&canon(path))
            .map(|g| g.tabs.len())
            .unwrap_or(0)
    }

    /// active worktree 的 active_tab 索引(无 active / 无 group → None)。
    #[cfg(feature = "test-support")]
    pub fn active_tab_index(&self) -> Option<usize> {
        self.active
            .as_ref()
            .and_then(|p| self.terminals.get(p))
            .map(|g| g.active_tab)
    }

    /// 直接设置仓库(测试绕过 open_repo_picker 注入 temp repo)。
    #[cfg(feature = "test-support")]
    pub fn set_repo_for_test(&mut self, repo: PathBuf) {
        self.set_repo(repo);
    }

    /// 直接触发 new_worktree(测试绕过 UI 点击)。
    #[cfg(feature = "test-support")]
    pub fn new_worktree_for_test(&mut self, cx: &mut Context<Self>) {
        self.new_worktree(cx);
    }

    /// 直接触发 request_close(测试绕过 UI 点击)。
    #[cfg(feature = "test-support")]
    pub fn request_close_for_test(
        &mut self,
        wt_path: PathBuf,
        branch: String,
        cx: &mut Context<Self>,
    ) {
        self.request_close(wt_path, branch, cx);
    }

    /// 直接触发 confirm_close(测试绕过 UI 点击)。
    #[cfg(feature = "test-support")]
    pub fn confirm_close_for_test(&mut self, cx: &mut Context<Self>) {
        self.confirm_close(cx);
    }

    /// 直接触发 cancel_close(测试绕过 UI 点击)。
    #[cfg(feature = "test-support")]
    pub fn cancel_close_for_test(&mut self, cx: &mut Context<Self>) {
        self.cancel_close(cx);
    }

    /// 直接触发 new_terminal_tab(测试绕过 UI 点击)。
    #[cfg(feature = "test-support")]
    pub fn new_terminal_tab_for_test(&mut self, shell: ShellKind, cx: &mut Context<Self>) {
        self.new_terminal_tab(shell, cx);
    }

    /// 直接触发 close_tab(测试绕过 UI 点击)。
    #[cfg(feature = "test-support")]
    pub fn close_tab_for_test(&mut self, index: usize, cx: &mut Context<Self>) {
        self.close_tab(index, cx);
    }

    /// 直接触发 switch_tab(测试绕过 UI 点击)。
    #[cfg(feature = "test-support")]
    pub fn switch_tab_for_test(&mut self, index: usize, cx: &mut Context<Self>) {
        self.switch_tab(index, cx);
    }

    /// 直接触发 send_agent_command(测试绕过 UI 点击)。
    #[cfg(feature = "test-support")]
    pub fn send_agent_command_for_test(&mut self, agent_name: &str, cx: &mut Context<Self>) {
        self.send_agent_command(agent_name, cx);
    }

    /// 直接触发 launch_agent(测试绕过 UI 点击)。
    /// = `new_terminal_tab(Default)` + `send_agent_command(name)`。
    #[cfg(feature = "test-support")]
    pub fn launch_agent_for_test(&mut self, agent_name: &str, cx: &mut Context<Self>) {
        self.launch_agent(agent_name, cx);
    }

    /// 直接触发 reveal_in_file_manager(测试绕过 UI 点击)。
    /// 无 active 时 no-op;有 active 时 spawn 系统命令(不阻塞)。
    #[cfg(feature = "test-support")]
    pub fn reveal_in_file_manager_for_test(&self, cx: &mut Context<Self>) {
        self.reveal_in_file_manager(cx);
    }

    /// 读 launcher 菜单是否打开。
    #[cfg(feature = "test-support")]
    pub fn launcher_menu_open_for_test(&self) -> bool {
        self.launcher_menu_open
    }

    /// 设置 launcher 菜单打开状态(测试模拟 `+` 按钮点击)。
    #[cfg(feature = "test-support")]
    pub fn set_launcher_menu_open_for_test(&mut self, open: bool) {
        self.launcher_menu_open = open;
    }

    /// 取指定路径 active tab 的静态回退标题(`TerminalTab.title`)。
    /// 用于验证 `ShellKind::label()` 生效(Default → "Shell"、Cmd → "cmd" 等)。
    #[cfg(feature = "test-support")]
    pub fn tab_title_for_test(&self, path: &std::path::Path) -> Option<String> {
        self.terminals
            .get(&canon(path))
            .and_then(|g| g.tabs.get(g.active_tab))
            .map(|t| t.title.clone())
    }

    /// 直接触发 open_worktree(测试绕过 UI 点击)。
    #[cfg(feature = "test-support")]
    pub fn open_worktree_for_test(&mut self, wt_path: PathBuf, cx: &mut Context<Self>) {
        self.open_worktree(wt_path, cx);
    }

    /// 停掉所有终端(测试清理,避免 leak-detection 误报)。
    #[cfg(feature = "test-support")]
    pub fn shutdown_all_terminals_for_test(&mut self, cx: &mut Context<Self>) {
        for group in self.terminals.values() {
            for tab in &group.tabs {
                tab.terminal.update(cx, |t, _| t.shutdown());
            }
        }
    }

    /// 设置 registry 持久化路径(测试隔离,避免污染真实用户 session 注册表)。
    #[cfg(feature = "test-support")]
    pub fn set_registry_path_for_test(&mut self, path: PathBuf) {
        self.registry_path = Some(path);
    }
}

impl Focusable for WorkspaceView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for WorkspaceView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 终端区:active worktree 的 active tab 终端;无则空态文字。
        let term_area: gpui::AnyElement =
            match self.active.as_ref().and_then(|p| self.terminals.get(p)) {
                Some(group) => {
                    if let Some(tab) = group.tabs.get(group.active_tab) {
                        div()
                            .flex_1()
                            .min_h_0()
                            .child(tab.terminal.clone())
                            .into_any_element()
                    } else {
                        div()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(rgb(theme::BG))
                            .text_color(rgb(theme::TEXT_FAINT))
                            .child(SharedString::from("select an action to begin"))
                            .into_any_element()
                    }
                }
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

        // 主列:tab 栏 + 终端区 + 底部状态栏。
        // min_w_0: 允许 main 在 root flex_row 中收缩到 sidebar+splitter 之外的剩余宽度,
        //   不因 tab_bar 内容撑宽而把 splitter/侧栏挤出窗口。
        let main = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(self.tab_bar(cx))
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
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _w, cx| {
                if this.launcher_menu_open && ev.keystroke.key == "escape" {
                    this.launcher_menu_open = false;
                    cx.notify();
                    cx.stop_propagation();
                }
            }))
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
        // 设置面板打开中 → 叠加设置弹窗。
        if self.settings.is_some() {
            root = root.child(self.settings_dialog(cx));
        }
        // launcher 菜单(`+` 按钮下拉)打开中 → 叠加菜单 overlay。
        if self.launcher_menu_open {
            root = root.child(self.launcher_menu(cx));
        }

        root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    #[cfg(windows)]
    fn strips_verbatim_prefix() {
        let p = PathBuf::from(r"\\?\C:\Users\foo");
        assert_eq!(strip_verbatim_prefix(&p), PathBuf::from(r"C:\Users\foo"));
    }

    #[test]
    #[cfg(windows)]
    fn strips_unc_verbatim_prefix() {
        let p = PathBuf::from(r"\\?\UNC\server\share\foo");
        assert_eq!(
            strip_verbatim_prefix(&p),
            PathBuf::from(r"\\server\share\foo")
        );
    }

    #[test]
    fn no_prefix_unchanged() {
        // 无 verbatim 前缀的路径原样返回。
        let p = PathBuf::from(if cfg!(windows) {
            r"C:\Users\foo"
        } else {
            "/usr/foo"
        });
        assert_eq!(strip_verbatim_prefix(&p), p);
    }

    #[test]
    fn canon_strips_verbatim_for_existing_path() {
        // canonicalize 对存在路径返回 verbatim 前缀(Windows),canon 应剥掉。
        let dir = std::env::current_dir().unwrap();
        let c = canon(&dir);
        let s = c.to_string_lossy();
        assert!(
            !s.starts_with(r"\\?\"),
            "canon should strip verbatim prefix: {s}"
        );
        // 剥前缀后应仍指向同一目录(再 canonicalize 应等价)。
        assert_eq!(c.canonicalize().unwrap(), dir.canonicalize().unwrap());
    }

    #[test]
    fn canon_falls_back_for_missing_path() {
        // 不存在的路径:canonicalize 失败,回退原值(再剥前缀)。
        let p = PathBuf::from(if cfg!(windows) {
            r"C:\this\path\does\not\exist"
        } else {
            "/this/path/does/not/exist"
        });
        let c = canon(&p);
        assert_eq!(c, strip_verbatim_prefix(&p));
    }

    #[test]
    fn same_path_treats_verbatim_and_plain_as_equal() {
        // 同一目录的 verbatim 与普通表示应判等(Windows 关键场景:
        // canonicalize 返回 \\?\ 前缀,而 git/ConPTY 需要普通路径)。
        let dir = std::env::current_dir().unwrap();
        let plain = dir.clone();
        #[cfg(windows)]
        let verbatim = PathBuf::from(format!(r"\\?\{}", dir.display()));
        #[cfg(not(windows))]
        let verbatim = plain.clone();
        assert_eq!(canon(&plain), canon(&verbatim));
        assert!(same_path(&plain, &verbatim));
    }

    // ---- agent_command_string 引号转义测试 ----

    fn spec(command: &str, args: &[&str]) -> AgentSpec {
        AgentSpec {
            name: "test".into(),
            command: command.into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            cwd: PathBuf::new(),
            extra_env: Default::default(),
        }
    }

    #[test]
    fn agent_command_string_simple_args() {
        // 无空格/引号的参数直接拼接。
        let s = WorkspaceView::agent_command_string(&spec("claude", &["--auto", "--verbose"]));
        assert_eq!(s, "claude --auto --verbose\r");
    }

    #[test]
    fn agent_command_string_arg_with_space_quoted() {
        // 含空格的参数用双引号包裹。
        let s = WorkspaceView::agent_command_string(&spec("sh", &["-c", "echo hello world"]));
        assert_eq!(s, "sh -c \"echo hello world\"\r");
    }

    #[test]
    fn agent_command_string_arg_with_double_quote_escaped() {
        // 含双引号的参数:整体加引号 + 内部双引号转义为 \"。
        let s = WorkspaceView::agent_command_string(&spec("sh", &["-c", "echo \"hi\""]));
        assert_eq!(s, "sh -c \"echo \\\"hi\\\"\"\r");
    }

    #[test]
    fn agent_command_string_arg_with_backslash_escaped() {
        // 含反斜杠的参数(且含空格触发引号):反斜杠转义为 \\。
        let s =
            WorkspaceView::agent_command_string(&spec("cmd", &["/c", "echo C:\\path to\\file"]));
        assert_eq!(s, "cmd /c \"echo C:\\\\path to\\\\file\"\r");
    }

    #[test]
    fn agent_command_string_empty_arg_quoted() {
        // 空参数用双引号包裹(否则 shell 看不到)。
        let s = WorkspaceView::agent_command_string(&spec("cmd", &[""]));
        assert_eq!(s, "cmd \"\"\r");
    }

    #[test]
    fn agent_command_string_single_quote_not_escaped() {
        // 单引号本身不转义(只在含空格时触发外层双引号包裹)。
        let s = WorkspaceView::agent_command_string(&spec("sh", &["-c", "echo 'hi'"]));
        assert_eq!(s, "sh -c \"echo 'hi'\"\r");
    }

    #[test]
    fn agent_command_string_no_args() {
        // 无参数:只有 command + \r。
        let s = WorkspaceView::agent_command_string(&spec("claude", &[]));
        assert_eq!(s, "claude\r");
    }

    // ---- ShellKind 枚举映射测试 ----

    #[test]
    fn shell_kind_default_command_is_none() {
        // Default 用系统默认 shell(command = None),交由 alacritty tty 层决定。
        assert!(ShellKind::Default.command().is_none());
    }

    #[test]
    fn shell_kind_default_label() {
        assert_eq!(ShellKind::Default.label(), "Shell");
    }

    #[test]
    #[cfg(windows)]
    fn shell_kind_cmd_command() {
        let (program, args) = ShellKind::Cmd.command().expect("Cmd should have command");
        assert_eq!(program, "cmd.exe");
        assert!(args.is_empty(), "Cmd args should be empty");
    }

    #[test]
    #[cfg(windows)]
    fn shell_kind_cmd_label() {
        assert_eq!(ShellKind::Cmd.label(), "cmd");
    }

    #[test]
    #[cfg(windows)]
    fn shell_kind_powershell_command() {
        let (program, args) = ShellKind::PowerShell
            .command()
            .expect("PowerShell should have command");
        assert_eq!(program, "powershell.exe");
        assert!(args.is_empty(), "PowerShell args should be empty");
    }

    #[test]
    #[cfg(windows)]
    fn shell_kind_powershell_label() {
        assert_eq!(ShellKind::PowerShell.label(), "PowerShell");
    }

    #[test]
    #[cfg(windows)]
    fn shell_kind_pwsh_command() {
        let (program, args) = ShellKind::Pwsh.command().expect("Pwsh should have command");
        assert_eq!(program, "pwsh.exe");
        assert!(args.is_empty(), "Pwsh args should be empty");
    }

    #[test]
    #[cfg(windows)]
    fn shell_kind_pwsh_label() {
        assert_eq!(ShellKind::Pwsh.label(), "pwsh");
    }

    #[test]
    fn shell_kind_all_variants_covered() {
        // 穷举所有变体,确保 command() 和 label() 都不 panic。
        // 防新增变体漏实现 match 分支(编译器会报 non-exhaustive,但运行时也验证)。
        let variants: &[ShellKind] = &[
            ShellKind::Default,
            #[cfg(windows)]
            ShellKind::Cmd,
            #[cfg(windows)]
            ShellKind::PowerShell,
            #[cfg(windows)]
            ShellKind::Pwsh,
        ];
        for v in variants {
            let _ = v.command();
            let _ = v.label();
        }
    }
}
