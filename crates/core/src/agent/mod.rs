//! agent 启动规格(纯数据,不含 PTY)。
//!
//! 只描述「怎么起一个 agent CLI」——命令、参数、环境变量、工作目录。
//! **不涉及 PTY**(那是 terminal 层的职责),以保持 core 纯净可测。
//!
//! 关键约束(见计划 KTD-10):`claude` 基于 Ink,启动时必须是真 TTY,
//! 否则会崩;这里通过设置 `TERM=xterm-256color` 为其兜底,但真正的 PTY
//! 由 terminal 层用 portable-pty 提供。

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::config::WorktreeConfig;

/// 一个可被 terminal 层消费的 agent 启动规格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSpec {
    /// agent 名(如 `claude` / `codex`),用于 UI 显示。
    pub name: String,
    /// 可执行命令。
    pub command: String,
    /// 命令参数。
    pub args: Vec<String>,
    /// 工作目录(应为 worktree 根,否则 agent 在错误目录操作)。
    pub cwd: PathBuf,
    /// 在继承父进程环境之上,额外注入/覆盖的环境变量。
    pub extra_env: BTreeMap<String, String>,
}

/// 内置 agent 注册条目 —— UI 菜单与 [`AgentSpec::builtin`] 的单一数据源。
///
/// 新增 agent 只需往 [`builtin_agents`] 加一条 + 在 app 层登记图标资产,
/// 侧边栏菜单会自动列出它(菜单迭代 [`builtin_agents`],不硬编码)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentBuiltin {
    /// 稳定 key(如 `claude`),用作 `AgentSpec::resolve` 的查找名与配置段名。
    pub name: &'static str,
    /// UI 显示名(如 `Claude`)。
    pub display: &'static str,
    /// 图标资产路径 key(如 `icons/claude.svg`),app 层按此加载 SVG。
    pub icon: &'static str,
    /// 可执行命令。
    pub command: &'static str,
    /// 命令参数(默认含各自的自动 / bypass 权限开关)。
    pub args: &'static [&'static str],
}

/// 内置 agent 注册表 —— zero-config 时可用哪些 agent、各自默认怎么起。
///
/// 顺序即 UI 菜单的展示顺序。三条默认均以「自动工作 / bypass 权限」模式启动:
/// - claude:`--dangerously-skip-permissions`(claude 无沙箱概念,跳过权限弹窗)
/// - codex:`--dangerously-bypass-approvals-and-sandbox`(worktree 已是隔离边界,codex 沙箱多余且可能阻断 git 操作)
/// - opencode:`--auto`(自动批准非显式 deny 的项)
pub fn builtin_agents() -> &'static [AgentBuiltin] {
    static AGENTS: &[AgentBuiltin] = &[
        AgentBuiltin {
            name: "claude",
            display: "Claude",
            icon: "icons/claude.svg",
            command: "claude",
            args: &["--dangerously-skip-permissions"],
        },
        AgentBuiltin {
            name: "codex",
            display: "Codex",
            icon: "icons/codex.svg",
            command: "codex",
            args: &["--dangerously-bypass-approvals-and-sandbox"],
        },
        AgentBuiltin {
            name: "opencode",
            display: "OpenCode",
            icon: "icons/opencode.svg",
            command: "opencode",
            args: &["--auto"],
        },
    ];
    AGENTS
}

impl AgentSpec {
    /// 从配置里的某个 agent 预设构造规格。找不到该名字则返回 None。
    ///
    /// - `cwd` 应传 worktree 路径。
    /// - `worktree_env` 是 worktree 上下文变量(与 hook 用的一致,方便 agent 感知)。
    pub fn from_config(
        config: &WorktreeConfig,
        name: &str,
        cwd: PathBuf,
        worktree_env: &[(String, String)],
    ) -> Option<Self> {
        let preset = config.agents.get(name)?;
        Some(Self::build(
            name,
            &preset.command,
            preset.args.clone(),
            cwd,
            worktree_env,
        ))
    }

    /// 用内置默认构造规格 —— 当配置里没有对应 `[agents.*]` 时用。
    /// 未知名字返回 None。
    ///
    /// 内置 agent 注册表是 UI 菜单与 resolve 的单一数据源(见 [`builtin_agents`])。
    /// 三个 agent 默认均以「自动工作 / bypass 权限」模式启动:claude
    /// `--dangerously-skip-permissions`、codex `--dangerously-bypass-approvals-and-sandbox`、opencode `--auto`。
    /// 本工具的核心场景是「在隔离 worktree 里一键起 agent 干活」,每次手动确认
    /// 权限太碎。用户若想收回,在 `.worktree.toml` 里显式写 `[agents.<name>]`
    /// (走 [`from_config`]),自己的 args 会完全覆盖此默认。
    pub fn builtin(name: &str, cwd: PathBuf, worktree_env: &[(String, String)]) -> Option<Self> {
        let entry = builtin_agents().iter().find(|a| a.name == name)?;
        Some(Self::build(
            entry.name,
            entry.command,
            entry.args.iter().map(|s| s.to_string()).collect(),
            cwd,
            worktree_env,
        ))
    }

    /// 优先用配置预设,缺失则回落到内置默认。
    pub fn resolve(
        config: &WorktreeConfig,
        name: &str,
        cwd: PathBuf,
        worktree_env: &[(String, String)],
    ) -> Option<Self> {
        Self::from_config(config, name, cwd.clone(), worktree_env)
            .or_else(|| Self::builtin(name, cwd, worktree_env))
    }

    fn build(
        name: &str,
        command: &str,
        args: Vec<String>,
        cwd: PathBuf,
        worktree_env: &[(String, String)],
    ) -> Self {
        let mut extra_env = BTreeMap::new();
        // 为交互式 TUI(如 Ink 的 claude)兜底一个合理的 TERM。
        extra_env.insert("TERM".to_string(), "xterm-256color".to_string());
        // 注入 worktree 上下文,agent/其子进程可感知。
        for (k, v) in worktree_env {
            extra_env.insert(k.clone(), v.clone());
        }
        Self {
            name: name.to_string(),
            command: command.to_string(),
            args,
            cwd,
            extra_env,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;

    fn empty_config() -> WorktreeConfig {
        WorktreeConfig::default()
    }

    #[test]
    fn builds_from_config_preset() {
        let cfg = config::parse(
            r#"
            [agents.codex]
            command = "codex"
            args = ["--yolo"]
        "#,
        )
        .unwrap()
        .config;

        let spec = AgentSpec::from_config(&cfg, "codex", PathBuf::from("/wt"), &[]).unwrap();
        assert_eq!(spec.command, "codex");
        assert_eq!(spec.args, vec!["--yolo"]);
        assert_eq!(spec.cwd, PathBuf::from("/wt"));
    }

    #[test]
    fn builtin_agents_contains_all_three() {
        let agents = builtin_agents();
        assert_eq!(agents.len(), 3);
        assert_eq!(agents[0].name, "claude");
        assert_eq!(agents[1].name, "codex");
        assert_eq!(agents[2].name, "opencode");
        // 每条都有非空 display / icon / command。
        for a in agents {
            assert!(!a.display.is_empty());
            assert!(!a.icon.is_empty());
            assert!(!a.command.is_empty());
        }
    }

    #[test]
    fn builtin_claude_codex_opencode_available_without_config() {
        let claude = AgentSpec::builtin("claude", PathBuf::from("/wt"), &[]).unwrap();
        assert_eq!(claude.command, "claude");
        // claude 默认带 --dangerously-skip-permissions(见 builtin_agents 文档)。
        assert_eq!(claude.args, vec!["--dangerously-skip-permissions"]);
        let codex = AgentSpec::builtin("codex", PathBuf::from("/wt"), &[]).unwrap();
        assert_eq!(codex.command, "codex");
        // codex 默认 --dangerously-bypass-approvals-and-sandbox(worktree 已是隔离边界)。
        assert_eq!(
            codex.args,
            vec!["--dangerously-bypass-approvals-and-sandbox"]
        );
        let opencode = AgentSpec::builtin("opencode", PathBuf::from("/wt"), &[]).unwrap();
        assert_eq!(opencode.command, "opencode");
        // opencode 默认 --auto(自动批准非显式 deny 的项)。
        assert_eq!(opencode.args, vec!["--auto"]);
        assert!(AgentSpec::builtin("unknown", PathBuf::from("/wt"), &[]).is_none());
    }

    #[test]
    fn config_preset_overrides_default_codex_args() {
        // codex builtin 含 --dangerously-bypass-approvals-and-sandbox,但用户显式配 args → 完全覆盖,不合并。
        let cfg = config::parse(
            r#"
            [agents.codex]
            command = "codex"
            args = ["--yolo"]
        "#,
        )
        .unwrap()
        .config;

        let spec = AgentSpec::resolve(&cfg, "codex", PathBuf::from("/wt"), &[]).unwrap();
        assert_eq!(spec.args, vec!["--yolo"]);
    }

    #[test]
    fn config_preset_overrides_default_claude_args() {
        // 用户显式配 claude 且不带该参数 → 尊重用户,不强加默认。
        let cfg = config::parse(
            r#"
            [agents.claude]
            command = "claude"
            args = ["--resume"]
        "#,
        )
        .unwrap()
        .config;

        let spec = AgentSpec::resolve(&cfg, "claude", PathBuf::from("/wt"), &[]).unwrap();
        assert_eq!(spec.args, vec!["--resume"]);
    }

    #[test]
    fn resolve_prefers_config_then_builtin() {
        // 配置里覆盖 claude 的命令。
        let cfg = config::parse(
            r#"
            [agents.claude]
            command = "claude-canary"
        "#,
        )
        .unwrap()
        .config;

        let spec = AgentSpec::resolve(&cfg, "claude", PathBuf::from("/wt"), &[]).unwrap();
        assert_eq!(spec.command, "claude-canary"); // 用了配置

        // codex 未在配置里 → 回落内置。
        let codex = AgentSpec::resolve(&cfg, "codex", PathBuf::from("/wt"), &[]).unwrap();
        assert_eq!(codex.command, "codex");
    }

    #[test]
    fn env_includes_term_and_worktree_vars() {
        let wt_env = vec![
            ("WORKTREE_PATH".to_string(), "/wt".to_string()),
            ("WORKTREE_BRANCH".to_string(), "feature/x".to_string()),
        ];
        let spec = AgentSpec::builtin("claude", PathBuf::from("/wt"), &wt_env).unwrap();
        assert_eq!(spec.extra_env.get("TERM").unwrap(), "xterm-256color");
        assert_eq!(spec.extra_env.get("WORKTREE_BRANCH").unwrap(), "feature/x");
    }

    #[test]
    fn unknown_agent_yields_none() {
        let cfg = empty_config();
        assert!(AgentSpec::resolve(&cfg, "nonexistent", PathBuf::from("/wt"), &[]).is_none());
    }
}
