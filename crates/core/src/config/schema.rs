//! `.worktree.toml` 的强类型 schema。
//!
//! 设计原则(见计划 KTD-6/7/8):
//! - 声明式、可版本控制、团队共享。
//! - hook 命令是 shell 命令字符串数组,顺序执行,经 `sh -c` 等价方式跑。
//! - 上下文通过**环境变量**传给 hook(`$WORKTREE_PATH` 等),**不做模板占位符**
//!   —— 5 个先例(lefthook/cargo-make/mise/Conductor/ccmanager)无一使用模板。
//! - 唯一的一处插值是 `[worktree].dir` 里的 `{repo}`,仅用于生成目录名,
//!   绝不进入 hook 命令字符串。

use serde::Deserialize;

/// worktree 存放位置策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Location {
    /// 仓库外兄弟目录(默认,不污染主仓、不触发 IDE/watcher 递归扫描)。
    #[default]
    Sibling,
    /// 仓库内子目录(`.worktrees/`,需 gitignore)。
    Inside,
}

/// `[worktree]` 段。
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct WorktreeSection {
    pub location: Location,
    /// 仅 `location = "sibling"` 时使用。`{repo}` 会被替换为仓库目录名。
    /// 默认 `../{repo}-worktrees`。
    pub dir: String,
    /// 新建 worktree 时默认基于的分支。
    pub default_base: String,
}

impl Default for WorktreeSection {
    fn default() -> Self {
        Self {
            location: Location::Sibling,
            dir: "../{repo}-worktrees".to_string(),
            default_base: "main".to_string(),
        }
    }
}

/// `[copy]` 段:PostCreate 时从主 worktree 复制的(通常未跟踪的)文件。
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct CopySection {
    pub files: Vec<String>,
}

/// `[hooks.options]` 段。
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct HookOptions {
    /// true:首条命令失败即停并冒泡错误;false(fail-open):记录并继续。
    pub fail_fast: bool,
}

impl Default for HookOptions {
    fn default() -> Self {
        // 默认 fail-fast:setup 失败时用户应立刻知道,而非拿到半配置的 worktree。
        Self { fail_fast: true }
    }
}

/// `[hooks]` 段:生命周期钩子命令。
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct HooksSection {
    pub post_create: Vec<String>,
    pub pre_remove: Vec<String>,
    // PostAttach 预留但 MVP 不解析/不触发(见计划 U10)。
    pub options: HookOptions,
}

/// `[agents.<name>]` 段:单个 agent 预设。
#[derive(Debug, Clone, Deserialize)]
pub struct AgentPreset {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

/// 顶层配置:`.worktree.toml` 反序列化目标。
///
/// 所有段都有默认值 —— 缺失字段用合理默认,空文件也是合法配置。
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct WorktreeConfig {
    pub worktree: WorktreeSection,
    pub copy: CopySection,
    pub hooks: HooksSection,
    /// agent 预设表(键为 agent 名,如 `claude`/`codex`)。
    pub agents: std::collections::BTreeMap<String, AgentPreset>,
    /// worktree 别名表:分支名 → 别名。用分支名作 key(worktree 路径每人本地
    /// 不同,分支名才是共享稳定的)。
    pub alias: std::collections::BTreeMap<String, String>,
}
