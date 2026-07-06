//! U6 集成测试:起真实子进程,验证 PTY 输出流进 Term、可读 cell、真 TTY。

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

#[test]
fn pty_output_flows_into_term() {
    let mut session = TerminalSession::spawn(
        dims(),
        None,
        // 用 sh -c 打印固定文本(printf 保证无额外 prompt 干扰)。
        Some((
            "/bin/sh".into(),
            vec!["-c".into(), "printf 'HELLO_LUCY'".into()],
        )),
        vec![],
    )
    .expect("spawn session");

    let text = wait_for_text(&mut session, "HELLO_LUCY", Duration::from_secs(5));
    assert!(text.is_some(), "应在 term 屏幕读到 PTY 输出");
}

#[test]
fn interactive_echo_via_cat() {
    let mut session =
        TerminalSession::spawn(dims(), None, Some(("/bin/cat".into(), vec![])), vec![])
            .expect("spawn cat");

    // 给 cat 一点时间起来,然后写入。
    std::thread::sleep(Duration::from_millis(150));
    session.write_input(b"abc\n".to_vec());

    let text = wait_for_text(&mut session, "abc", Duration::from_secs(5));
    assert!(text.is_some(), "cat 应回显写入的内容");
}

#[test]
fn is_a_real_tty() {
    // `test -t 0` 仅当 stdin 是 TTY 时退出 0 → 打印 YES。这是 claude(Ink)能跑的前提。
    let mut session = TerminalSession::spawn(
        dims(),
        None,
        Some((
            "/bin/sh".into(),
            vec![
                "-c".into(),
                "test -t 0 && printf TTY_YES || printf TTY_NO".into(),
            ],
        )),
        vec![],
    )
    .expect("spawn tty probe");

    let text = wait_for_text(&mut session, "TTY_YES", Duration::from_secs(5));
    assert!(text.is_some(), "PTY 必须是真 TTY(claude/Ink 依赖)");
}

#[test]
fn resize_does_not_panic() {
    let mut session =
        TerminalSession::spawn(dims(), None, Some(("/bin/cat".into(), vec![])), vec![])
            .expect("spawn");
    std::thread::sleep(Duration::from_millis(100));

    session.resize(TermDimensions::new(80, 24, 8, 16));
    assert_eq!(session.dimensions().columns, 80);
    assert_eq!(session.dimensions().screen_lines, 24);
}

#[test]
fn shutdown_kills_running_child() {
    // 起一个长命令,shutdown 后应触发 ChildExit(进程被杀)。
    let mut session = TerminalSession::spawn(
        dims(),
        None,
        Some(("/bin/sh".into(), vec!["-c".into(), "sleep 100".into()])),
        vec![],
    )
    .expect("spawn");
    std::thread::sleep(Duration::from_millis(150));

    session.shutdown();

    // shutdown 后应在合理时间内收到 ChildExit。
    let start = Instant::now();
    let mut exited = false;
    while start.elapsed() < Duration::from_secs(5) {
        for ev in session.drain_events() {
            if let lucy_terminal::TermEvent::ChildExit(_) = ev {
                exited = true;
            }
        }
        if exited {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(exited, "shutdown 应停掉子进程并上报 ChildExit");
}

#[test]
fn child_exit_is_reported() {
    let mut session = TerminalSession::spawn(
        dims(),
        None,
        Some(("/bin/sh".into(), vec!["-c".into(), "exit 0".into()])),
        vec![],
    )
    .expect("spawn");

    let start = Instant::now();
    let mut got_exit = false;
    while start.elapsed() < Duration::from_secs(5) {
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
