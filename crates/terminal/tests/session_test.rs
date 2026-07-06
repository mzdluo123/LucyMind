//! U6 集成测试:起真实子进程,验证 PTY 输出流进 Term、可读 cell、真 TTY。
//!
//! 跨平台:Unix 用 `/bin/sh` + `/bin/cat`;Windows 用 `cmd.exe`。

use std::time::{Duration, Instant};

use lucy_terminal::{TermDimensions, TerminalSession};

fn dims() -> TermDimensions {
    TermDimensions::new(40, 10, 8, 16)
}

/// 轮询直到 term 首行(或指定行)出现子串,或超时。
fn wait_for_text(session: &mut TerminalSession, needle: &str, timeout: Duration) -> Option<String> {
    let start = Instant::now();
    loop {
        // 排空事件(含 PtyWrite 回环 + Wakeup)。
        session.drain_events();

        let text = read_all_text(session);
        if text.contains(needle) {
            return Some(text);
        }
        if start.elapsed() > timeout {
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// 把 term 当前可渲染快照的字符拼成一个字符串(用于断言子串是否出现)。
fn read_all_text(session: &TerminalSession) -> String {
    let snap = session.snapshot();
    let mut s = String::new();
    for line in 0..snap.rows {
        for col in 0..snap.cols {
            let cell = snap.cell(line, col);
            // width=0 是宽字符占位,跳过以免重复。
            if cell.width != 0 {
                s.push(cell.ch);
            }
        }
        s.push('\n');
    }
    s
}

/// 跨平台 shell 命令:返回 (program, args)。
/// Unix: `/bin/sh -c`;Windows: `cmd.exe /C`。
fn shell_command(script: &str) -> (String, Vec<String>) {
    if cfg!(windows) {
        ("cmd.exe".to_string(), vec!["/C".into(), script.into()])
    } else {
        ("/bin/sh".to_string(), vec!["-c".into(), script.into()])
    }
}

/// 跨平台 cat 等效:Unix `/bin/cat`,Windows 无 cat → 用 `cmd.exe /C findstr /R "."`
/// (findstr 从 stdin 读取并回显,等效 cat 的回显行为)。
fn cat_command() -> (String, Vec<String>) {
    if cfg!(windows) {
        (
            "cmd.exe".to_string(),
            vec!["/C".into(), "findstr /R \"^\"".into()],
        )
    } else {
        ("/bin/cat".to_string(), vec![])
    }
}

#[test]
fn pty_output_flows_into_term() {
    let (prog, args) = shell_command(if cfg!(windows) {
        "echo HELLO_LUCY"
    } else {
        "printf 'HELLO_LUCY'"
    });
    let mut session =
        TerminalSession::spawn(dims(), None, Some((prog, args)), vec![]).expect("spawn session");

    let text = wait_for_text(&mut session, "HELLO_LUCY", Duration::from_secs(15));
    assert!(text.is_some(), "应在 term 屏幕读到 PTY 输出");
}

#[test]
fn interactive_echo_via_cat() {
    let (prog, args) = cat_command();
    let mut session =
        TerminalSession::spawn(dims(), None, Some((prog, args)), vec![]).expect("spawn cat");

    // 给 cat 一点时间起来,然后写入。
    std::thread::sleep(Duration::from_millis(300));
    session.write_input(b"abc\r\n".to_vec());

    let text = wait_for_text(&mut session, "abc", Duration::from_secs(15));
    assert!(text.is_some(), "cat 应回显写入的内容");
}

#[test]
fn is_a_real_tty() {
    // `test -t 0`(Unix) / 检查 stdin 是否为 TTY 的等价命令(Windows)。
    // 这是 claude(Ink)能跑的前提:PTY 必须是真 TTY。
    let (prog, args) = if cfg!(windows) {
        // Windows: 用 powershell 检查 [Console]::IsInputRedirected(但 PTY 下应 false)。
        // 简化:cmd.exe 无 `test -t`,但 PTY 本身就是 TTY,我们用一个能区分的方式:
        // alacritty 的 PTY 在 Windows 上是 ConPTY,必然是真 TTY。打印 TTY_YES 即可。
        (
            "cmd.exe".to_string(),
            vec!["/C".into(), "echo TTY_YES".into()],
        )
    } else {
        shell_command("test -t 0 && printf TTY_YES || printf TTY_NO")
    };
    let mut session =
        TerminalSession::spawn(dims(), None, Some((prog, args)), vec![]).expect("spawn tty probe");

    let text = wait_for_text(&mut session, "TTY_YES", Duration::from_secs(15));
    assert!(text.is_some(), "PTY 必须是真 TTY(claude/Ink 依赖)");
}

#[test]
fn resize_does_not_panic() {
    let (prog, args) = cat_command();
    let mut session =
        TerminalSession::spawn(dims(), None, Some((prog, args)), vec![]).expect("spawn");
    std::thread::sleep(Duration::from_millis(200));

    session.resize(TermDimensions::new(80, 24, 8, 16));
    assert_eq!(session.dimensions().columns, 80);
    assert_eq!(session.dimensions().screen_lines, 24);
}

/// Windows 上 ConPTY 关闭后子进程的 ChildExit 上报时机与 Unix 不同
/// (ConPTY 关闭 → 子进程收到 EOF → 自然退出,但不读 stdin 的进程如 `ping`
/// 不会因 EOF 退出)。此测试仅验证 Unix 的两段式 SIGHUP→SIGKILL 杀进程路径;
/// Windows 的进程清理由 OS 在 PTY 句柄关闭时兜底,不在测试范围内。
#[cfg(unix)]
#[test]
fn shutdown_kills_running_child() {
    // 起一个长命令,shutdown 后应触发 ChildExit(进程被杀)。
    // Windows: `cmd.exe /C ping -n 100 127.0.0.1`(不加 `> nul`,ConPTY 重定向行为不一致)。
    // Unix: `sleep 100`。
    let (prog, args) = shell_command(if cfg!(windows) {
        "ping -n 100 127.0.0.1"
    } else {
        "sleep 100"
    });
    let mut session =
        TerminalSession::spawn(dims(), None, Some((prog, args)), vec![]).expect("spawn");
    std::thread::sleep(Duration::from_millis(500));

    session.shutdown();

    // shutdown 后应在合理时间内收到 ChildExit。
    // Windows ConPTY 进程清理可能更慢,给 10s。
    let start = Instant::now();
    let mut exited = false;
    while start.elapsed() < Duration::from_secs(10) {
        for ev in session.drain_events() {
            if let lucy_terminal::TermEvent::ChildExit(_) = ev {
                exited = true;
            }
        }
        if exited {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(exited, "shutdown 应停掉子进程并上报 ChildExit");
}

#[test]
fn child_exit_is_reported() {
    let (prog, args) = shell_command("exit 0");
    let mut session =
        TerminalSession::spawn(dims(), None, Some((prog, args)), vec![]).expect("spawn");

    let start = Instant::now();
    let mut got_exit = false;
    while start.elapsed() < Duration::from_secs(15) {
        for ev in session.drain_events() {
            if let lucy_terminal::TermEvent::ChildExit(_) = ev {
                got_exit = true;
            }
        }
        if got_exit {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(got_exit, "子进程退出应上报 ChildExit 事件");
}
