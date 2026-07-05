//! hook 执行引擎:copy 文件 + 顺序执行 shell 命令,注入环境变量,
//! 按 fail_fast / fail-open 策略处理失败。

use std::path::Path;
use std::process::Command;

use crate::config::{CopySection, HooksSection};

use super::{HookContext, LifecycleEvent};

/// 单步执行结果(供 UI 逐步展示进度)。
#[derive(Debug, Clone)]
pub struct StepResult {
    pub description: String,
    pub success: bool,
    /// 失败时的说明(命令非零退出码 / copy 错误)。
    pub message: Option<String>,
}

/// hook 执行的整体结果。
#[derive(Debug)]
pub struct HookRun {
    pub event: LifecycleEvent,
    pub steps: Vec<StepResult>,
}

impl HookRun {
    /// 是否存在失败步骤。
    pub fn had_failure(&self) -> bool {
        self.steps.iter().any(|s| !s.success)
    }
}

/// 执行某个生命周期事件的全部 hook。
///
/// - `PostCreate` 会先执行 `[copy]` 声明的文件复制(从 `repo_root` → worktree),
///   再执行 `post_create` 命令。
/// - `PreRemove` 只执行 `pre_remove` 命令。
/// - `on_step` 每完成一步回调一次(供 UI 实时显示进度)。
///
/// 失败策略:`fail_fast=true` 时首个失败步骤即停;`false`(fail-open)记录并继续。
pub fn run_event(
    event: LifecycleEvent,
    hooks: &HooksSection,
    copy: &CopySection,
    ctx: &HookContext,
    mut on_step: impl FnMut(&StepResult),
) -> HookRun {
    let fail_fast = hooks.options.fail_fast;
    let mut steps = Vec::new();

    // 用一个宏统一「记录步骤 + 回调 + fail_fast 短路」逻辑。
    macro_rules! record {
        ($step:expr) => {{
            let step: StepResult = $step;
            let failed = !step.success;
            on_step(&step);
            steps.push(step);
            if failed && fail_fast {
                return HookRun { event, steps };
            }
        }};
    }

    if event == LifecycleEvent::PostCreate {
        for file in &copy.files {
            record!(copy_file(&ctx.repo_root, &ctx.worktree_path, file));
        }
    }

    let commands = match event {
        LifecycleEvent::PostCreate => &hooks.post_create,
        LifecycleEvent::PreRemove => &hooks.pre_remove,
    };
    for cmd in commands {
        record!(run_command(cmd, ctx));
    }

    HookRun { event, steps }
}

/// 从主仓复制一个(通常未跟踪的)文件到 worktree。源不存在则跳过(非失败)。
fn copy_file(repo_root: &Path, worktree: &Path, rel: &str) -> StepResult {
    let src = repo_root.join(rel);
    let dst = worktree.join(rel);
    let desc = format!("copy {rel}");

    if !src.exists() {
        // 源不存在:跳过并记为成功(用户可能声明了可选文件)。
        return StepResult {
            description: format!("{desc} (源不存在,跳过)"),
            success: true,
            message: None,
        };
    }

    if let Some(parent) = dst.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return StepResult {
                description: desc,
                success: false,
                message: Some(format!("创建目标目录失败: {e}")),
            };
        }
    }

    match std::fs::copy(&src, &dst) {
        Ok(_) => StepResult {
            description: desc,
            success: true,
            message: None,
        },
        Err(e) => StepResult {
            description: desc,
            success: false,
            message: Some(format!("复制失败: {e}")),
        },
    }
}

/// 经 `sh -c` 执行一条命令,cwd = worktree,注入上下文环境变量。
fn run_command(cmd: &str, ctx: &HookContext) -> StepResult {
    let desc = format!("run `{cmd}`");

    let mut command = shell_command(cmd);
    command.current_dir(&ctx.worktree_path);
    for (k, v) in ctx.env_vars() {
        command.env(k, v);
    }

    match command.output() {
        Ok(out) if out.status.success() => StepResult {
            description: desc,
            success: true,
            message: None,
        },
        Ok(out) => StepResult {
            description: desc,
            success: false,
            message: Some(format!(
                "退出码 {}: {}",
                out.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&out.stderr).trim()
            )),
        },
        Err(e) => StepResult {
            description: desc,
            success: false,
            message: Some(format!("无法启动命令: {e}")),
        },
    }
}

/// 构造平台对应的 shell 执行命令。Unix: `sh -c`;Windows: `cmd /C`。
#[cfg(not(windows))]
fn shell_command(cmd: &str) -> Command {
    let mut c = Command::new("sh");
    c.arg("-c").arg(cmd);
    c
}

#[cfg(windows)]
fn shell_command(cmd: &str) -> Command {
    let mut c = Command::new("cmd");
    c.arg("/C").arg(cmd);
    c
}
