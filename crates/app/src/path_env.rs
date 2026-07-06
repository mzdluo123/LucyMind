//! 修复 GUI 启动时的 PATH —— 从 `.app`(Finder/Dock/`open`)启动时,进程只
//! 继承一个极简 PATH(`/usr/bin:/bin:/usr/sbin:/sbin` 之类),**不含**用户在
//! shell 配置里加的目录(`~/.local/bin`、`/opt/homebrew/bin`、nvm/pnpm 全局
//! 目录等)。
//!
//! 后果:点 Claude/Codex 起 agent 时,PTY 子进程继承这个残缺 PATH,`claude` /
//! `codex` 找不到 → exec 失败 → 崩。`cargo run` 从终端启动则没这问题(继承了
//! 完整 PATH)。
//!
//! 修法(业界通行,如 Zed / VS Code 的 fix-path 思路):启动时跑一次用户的
//! **登录 + 交互式** shell 打印 `$PATH`(会加载 `.zprofile`/`.zshrc` 等,拿到
//! 与用户终端一致的 PATH),用它覆盖本进程的 `PATH`。之后起的 agent、普通
//! shell、hook 命令都继承到完整 PATH。
//!
//! 一次性、启动时做;带超时兜底(避免用户 shell 配置卡住导致 app 起不来)。

#[cfg(unix)]
use std::process::Command;
#[cfg(unix)]
use std::time::Duration;

/// 跑登录 shell 取真实 PATH,覆盖本进程的 `PATH`。取不到就保持原样(不致命)。
///
/// 只在类 Unix 上有意义;Windows 无此问题,直接跳过。
pub fn fix_path_from_login_shell() {
    #[cfg(not(unix))]
    {
        // Windows:GUI 进程 PATH 正常,无需修。
    }
    #[cfg(unix)]
    {
        match login_shell_path() {
            Some(path) if !path.trim().is_empty() => {
                log::info!("从登录 shell 注入 PATH({} 字节)", path.len());
                // SAFETY: 启动早期、单线程(GPUI 尚未起线程),此时改环境变量安全。
                unsafe { std::env::set_var("PATH", path) };
            }
            _ => {
                log::warn!(
                    "未能从登录 shell 取 PATH,沿用继承的 PATH(从 .app 启动时可能不含 claude/codex)"
                );
            }
        }
    }
}

/// 跑 `$SHELL -ilc 'printf %s $PATH'` 拿登录+交互式 shell 的 PATH。
///
/// - `-i` 交互式 → 加载 `.zshrc` / `.bashrc`
/// - `-l` 登录 → 加载 `.zprofile` / `.bash_profile`
/// - `-c` 执行命令后退出
///
/// 带超时:shell 启动脚本可能有耗时逻辑,超时则放弃(返回 None),不阻塞启动。
#[cfg(unix)]
fn login_shell_path() -> Option<String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

    // 用一个独特的分隔标记把 PATH 从 shell 启动脚本可能打印的其它噪音里摘出来。
    const MARKER: &str = "__LUCY_PATH__";
    let script = format!("printf '{MARKER}%s{MARKER}' \"$PATH\"");

    let mut child = Command::new(&shell)
        .args(["-ilc", &script])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .stdin(std::process::Stdio::null())
        .spawn()
        .ok()?;

    // 超时兜底:轮询等待,最多 ~3s。超时则杀掉子进程、放弃。
    let deadline = Duration::from_secs(3);
    let step = Duration::from_millis(50);
    let mut waited = Duration::ZERO;
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => break,
            Ok(None) => {
                if waited >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    log::warn!("登录 shell 取 PATH 超时(>{}s)", deadline.as_secs());
                    return None;
                }
                std::thread::sleep(step);
                waited += step;
            }
            Err(_) => return None,
        }
    }

    let output = child.wait_with_output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    // 从两个 MARKER 之间摘出 PATH(避开启动脚本可能输出的其它内容)。
    let start = stdout.find(MARKER)? + MARKER.len();
    let rest = &stdout[start..];
    let end = rest.find(MARKER)?;
    Some(rest[..end].to_string())
}
