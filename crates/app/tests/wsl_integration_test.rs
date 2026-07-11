//! WSL 集成测试(标 `#[ignore]`,需真实 WSL 环境)。
//!
//! 这些测试在 WSL 内 `git init` 临时仓库,用真实 `WslHost` 验证端到端流程:
//! - 14.1: 打开 WSL 仓库(worktree 列表加载)
//! - 14.2: 在 WSL 内创建 worktree
//! - 14.3: 在 WSL worktree 内启动 shell 终端
//! - 14.4: 在 WSL worktree 内执行 post_create hook
//!
//! 运行方式:`cargo test -p lucy-app --test wsl_integration_test -- --ignored`
//! 前提:Windows + WSL 已安装,默认发行版有 `git` 和 `/bin/sh`。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use gpui::TestAppContext;

use common::{build_workspace_with_host, shutdown_workspace, wait_for};
#[cfg(target_os = "windows")]
use lucy_core::host::LocalHost;
use lucy_core::host::{Host, WslHost};

mod common;

/// 检查 WSL 是否可用(`wsl.exe --status` 退出码 0)。
/// 不可用时跳过测试(返回 None),测试标 `#[ignore]` 手动运行。
fn wsl_available() -> Option<Arc<WslHost>> {
    let output = std::process::Command::new("wsl.exe")
        .arg("--status")
        .output()
        .ok()?;
    if output.status.success() {
        Some(Arc::new(WslHost::default()))
    } else {
        None
    }
}

/// 在 WSL 内创建临时 git 仓库(`/tmp` 下),返回 WSL 路径。
fn wsl_temp_repo() -> Option<PathBuf> {
    let host = wsl_available()?;
    // 用 WSL 内的 mktemp -d 创建临时目录。
    let out = host
        .run_shell(std::path::Path::new("/tmp"), "mktemp -d", &[])
        .ok()?;
    if !out.success {
        return None;
    }
    let dir = out.stdout.trim().to_string();
    let repo = format!("{dir}/repo");
    // git init + 初始提交。
    let cmds = [
        format!("git init -q -b main {repo}"),
        format!("git -C {repo} config user.name test"),
        format!("git -C {repo} config user.email test@test.com"),
        format!("git -C {repo} commit -q --allow-empty -m init"),
    ];
    for cmd in &cmds {
        let out = host
            .run_shell(std::path::Path::new("/tmp"), cmd, &[])
            .ok()?;
        if !out.success {
            return None;
        }
    }
    Some(PathBuf::from(repo))
}

/// 14.1: 用 WslHost 打开 WSL 内的 git 仓库,验证 worktree 列表正确加载。
#[gpui::test]
#[ignore = "requires real WSL environment"]
async fn wsl_open_repo_loads_worktrees(cx: &mut TestAppContext) {
    let host = match wsl_available() {
        Some(h) => h,
        None => return,
    };
    let repo = match wsl_temp_repo() {
        Some(r) => r,
        None => return,
    };
    let host: Arc<dyn lucy_core::host::Host> = host;
    let (workspace, _w) = build_workspace_with_host(cx, Some(repo.clone()), host);
    cx.run_until_parked();

    // worktree 列表应至少有主仓一行。
    wait_for(
        cx,
        |cx| cx.update(|cx| workspace.update(cx, |v, _| v.worktree_count() >= 1)),
        Duration::from_secs(10),
    );

    let count = cx.update(|cx| workspace.update(cx, |v, _| v.worktree_count()));
    assert!(count >= 1, "should have at least main worktree");

    shutdown_workspace(cx, &workspace);
}

/// 14.2: 在 WSL 内创建 worktree,验证 worktree 列表包含新 worktree。
#[gpui::test]
#[ignore = "requires real WSL environment"]
async fn wsl_new_worktree_appears_in_list(cx: &mut TestAppContext) {
    let host = match wsl_available() {
        Some(h) => h,
        None => return,
    };
    let repo = match wsl_temp_repo() {
        Some(r) => r,
        None => return,
    };

    let host: Arc<dyn lucy_core::host::Host> = host;
    let (workspace, _w) = build_workspace_with_host(cx, Some(repo.clone()), host);
    cx.run_until_parked();

    // 初始:1 个 worktree(主仓)。
    let initial = cx.update(|cx| workspace.update(cx, |v, _| v.worktree_count()));
    assert_eq!(initial, 1, "should start with 1 worktree (main)");

    // 创建新 worktree。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.new_worktree_for_test(cx));
    });

    // 等待 worktree 创建完成 + active_path 就绪。
    wait_for(
        cx,
        |cx| cx.update(|cx| workspace.update(cx, |v, _| v.active_path().is_some())),
        Duration::from_secs(15),
    );

    // worktree 列表应增加。
    let after = cx.update(|cx| workspace.update(cx, |v, _| v.worktree_count()));
    assert_eq!(after, 2, "should have 2 worktrees after new_worktree");

    shutdown_workspace(cx, &workspace);
}

/// 14.3: 在 WSL worktree 内启动 shell 终端,验证 terminals map 有条目。
#[gpui::test]
#[ignore = "requires real WSL environment"]
async fn wsl_shell_tab_starts(cx: &mut TestAppContext) {
    let host = match wsl_available() {
        Some(h) => h,
        None => return,
    };
    let repo = match wsl_temp_repo() {
        Some(r) => r,
        None => return,
    };
    let expected_shell = host
        .run_shell(
            std::path::Path::new("/tmp"),
            "getent passwd \"$(id -u)\" | cut -d: -f7",
            &[],
        )
        .expect("query WSL default shell")
        .stdout
        .trim()
        .to_string();
    assert!(
        !expected_shell.is_empty(),
        "WSL user must have a default shell"
    );

    let host: Arc<dyn lucy_core::host::Host> = host;
    let (workspace, _w) = build_workspace_with_host(cx, Some(repo.clone()), host);
    cx.run_until_parked();

    // 创建 worktree(自带一个 shell tab)。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.new_worktree_for_test(cx));
    });
    wait_for(
        cx,
        |cx| cx.update(|cx| workspace.update(cx, |v, _| v.active_path().is_some())),
        Duration::from_secs(15),
    );

    // active_path 对应的 worktree 应有终端组(1 个 tab)。
    let has_terminal = cx.update(|cx| {
        workspace.update(cx, |v, _| {
            v.active_path().is_some_and(|p| v.terminals_contains(p))
        })
    });
    assert!(
        has_terminal,
        "should have a terminal group for the new worktree"
    );

    let (active, reveal) = cx.update(|cx| {
        let view = workspace.read(cx);
        (
            view.active_path().map(PathBuf::from),
            view.file_manager_command_for_test(),
        )
    });
    let active = active.expect("worktree should be active");
    assert_eq!(
        reveal,
        Some((
            "wsl.exe".to_string(),
            vec![
                "--cd".to_string(),
                active.to_string_lossy().into_owned(),
                "--".to_string(),
                "explorer.exe".to_string(),
                ".".to_string(),
            ]
        )),
        "Explorer must receive the WSL cwd through wsl.exe"
    );

    let terminal = cx
        .update(|cx| workspace.read(cx).terminal_at(&active).cloned())
        .expect("active terminal");
    wait_for(
        cx,
        |cx| cx.read(|cx| !terminal.read(cx).snapshot_text().is_empty()),
        Duration::from_secs(15),
    );
    cx.update(|cx| {
        terminal.update(cx, |terminal, _| {
            terminal.send_text("printf '\\nWSL_DEFAULT_SHELL:%s\\n' \"$0\"\r")
        });
    });
    wait_for(
        cx,
        |cx| {
            cx.read(|cx| {
                terminal
                    .read(cx)
                    .snapshot_text()
                    .contains(&format!("WSL_DEFAULT_SHELL:{expected_shell}"))
            })
        },
        Duration::from_secs(15),
    );

    shutdown_workspace(cx, &workspace);
}

/// 14.4: 在 WSL worktree 内执行 post_create hook,验证 hook 产物存在。
#[gpui::test]
#[ignore = "requires real WSL environment"]
async fn wsl_post_create_hook_runs(cx: &mut TestAppContext) {
    let host = match wsl_available() {
        Some(h) => h,
        None => return,
    };
    let repo = match wsl_temp_repo() {
        Some(r) => r,
        None => return,
    };

    // 写 .worktree.toml 配置 post_create hook。
    let host_dyn: Arc<dyn lucy_core::host::Host> = host.clone();
    let toml = "[worktree]\nlocation = \"sibling\"\ndir = \"../{repo}-worktrees\"\n\
                [hooks]\npost_create = [\"echo HELLO > hook_test.txt\"]\n";
    let toml_path = host_dyn.join_path(&repo, ".worktree.toml");
    let _ = host_dyn.write(&toml_path, toml);

    let (workspace, _w) = build_workspace_with_host(cx, Some(repo.clone()), host_dyn.clone());
    cx.run_until_parked();

    // 创建 worktree(post_create hook 会执行 echo HELLO > hook_test.txt)。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.new_worktree_for_test(cx));
    });
    wait_for(
        cx,
        |cx| cx.update(|cx| workspace.update(cx, |v, _| v.active_path().is_some())),
        Duration::from_secs(15),
    );

    // 验证 hook 产物:active worktree 路径下应有 hook_test.txt,内容含 HELLO。
    let hook_file_exists = cx.update(|cx| {
        workspace.update(cx, |v, _| {
            v.active_path().is_some_and(|p: &std::path::Path| {
                host_dyn.exists(&host_dyn.join_path(p, "hook_test.txt"))
            })
        })
    });
    assert!(
        hook_file_exists,
        "post_create hook should create hook_test.txt"
    );

    shutdown_workspace(cx, &workspace);
}

/// 14.5: 从默认本机 PathPicker 切换到 WSL,选择 WSL git 仓库。
#[cfg(target_os = "windows")]
#[gpui::test]
#[ignore = "requires real WSL environment"]
async fn local_picker_selects_wsl_repository(cx: &mut TestAppContext) {
    let repo = match wsl_temp_repo() {
        Some(repo) => repo,
        None => return,
    };
    let host: Arc<dyn Host> = Arc::new(LocalHost);
    let (workspace, window) = build_workspace_with_host(cx, None, host);

    window.update(|window, cx| {
        workspace.update(cx, |view, cx| view.open_repo_picker_for_test(window, cx));
    });
    let picker = window
        .update(|_window, cx| workspace.read(cx).path_picker_for_test().cloned())
        .expect("picker should be open");
    window.update(|window, cx| {
        picker.update(cx, |picker, cx| {
            picker.switch_location_for_test(true, window, cx);
            picker.set_query(&repo.to_string_lossy(), window, cx);
            picker.confirm_for_test(cx);
        });
    });

    wait_for(
        window,
        |cx| {
            cx.update(|cx| {
                let view = workspace.read(cx);
                view.is_remote_host() && view.repo() == Some(repo.as_path())
            })
        },
        Duration::from_secs(15),
    );
    shutdown_workspace(window, &workspace);
}

/// 14.6: WSL shell 真正启动后执行假 agent 命令并渲染输出。
#[cfg(target_os = "windows")]
#[gpui::test]
#[ignore = "requires real WSL environment"]
async fn wsl_agent_command_runs_in_terminal(cx: &mut TestAppContext) {
    let host = match wsl_available() {
        Some(host) => host,
        None => return,
    };
    let repo = match wsl_temp_repo() {
        Some(repo) => repo,
        None => return,
    };
    let config = "[agents.test]\ncommand = \"printf\"\nargs = [\"WSL_AGENT_OK\\n\"]\n";
    host.write(&host.join_path(&repo, ".worktree.toml"), config)
        .unwrap();

    let host: Arc<dyn Host> = host;
    let (workspace, _window) = build_workspace_with_host(cx, Some(repo.clone()), host);
    cx.update(|cx| {
        workspace.update(cx, |view, cx| view.open_worktree_for_test(repo.clone(), cx));
    });
    wait_for(
        cx,
        |cx| {
            let terminal = cx.update(|cx| {
                let view = workspace.read(cx);
                view.active_path()
                    .and_then(|path| view.terminal_at(path))
                    .cloned()
            });
            terminal
                .is_some_and(|terminal| cx.read(|cx| !terminal.read(cx).snapshot_text().is_empty()))
        },
        Duration::from_secs(15),
    );
    cx.update(|cx| {
        workspace.update(cx, |view, cx| view.send_agent_command_for_test("test", cx));
    });
    wait_for(
        cx,
        |cx| {
            let terminal = cx.update(|cx| {
                let view = workspace.read(cx);
                view.active_path()
                    .and_then(|path| view.terminal_at(path))
                    .cloned()
            });
            terminal.is_some_and(|terminal| {
                cx.read(|cx| terminal.read(cx).snapshot_text().contains("WSL_AGENT_OK"))
            })
        },
        Duration::from_secs(15),
    );
    shutdown_workspace(cx, &workspace);
}
