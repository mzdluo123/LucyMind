//! WSL/Remote Host UI 状态测试:
//! - 13.1: LocalHost 构造 `WorkspaceView`,`is_remote_host()` 返回 false。
//! - 13.2: RemoteMockHost(is_remote=true) 构造时 launcher menu 状态正确。
//! - 13.3: `open_repo_picker` 弹出 PathPicker(而非旧的 choice dialog / WSL browser)。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use gpui::TestAppContext;

use common::{build_workspace, build_workspace_with_host, shutdown_workspace, temp_repo};
use lucy_core::host::{Host, HostCommand, HostError, HostOutput};

mod common;

/// 13.1: LocalHost 构造的 `WorkspaceView` 的 `is_remote_host()` 返回 false。
#[gpui::test]
async fn local_host_is_not_remote(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    let is_remote = cx.update(|cx| workspace.update(cx, |v, _| v.is_remote_host()));
    assert!(!is_remote, "LocalHost should not be remote");

    shutdown_workspace(cx, &workspace);
}

/// 13.2: RemoteMockHost(is_remote=true) 构造时,is_remote_host() 返回 true,
/// 且 launcher menu 可正常开关。
#[gpui::test]
async fn remote_host_launcher_menu_state(cx: &mut TestAppContext) {
    let host: std::sync::Arc<dyn Host> = std::sync::Arc::new(RemoteMockHost::new());
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace_with_host(cx, Some(repo), host);
    cx.run_until_parked();

    let is_remote = cx.update(|cx| workspace.update(cx, |v, _| v.is_remote_host()));
    assert!(is_remote, "RemoteMockHost should be remote");

    // 打开 launcher menu,验证状态正确(不 panic)。
    cx.update(|cx| {
        workspace.update(cx, |v, _| v.set_launcher_menu_open_for_test(true));
    });
    let menu_open = cx.update(|cx| workspace.update(cx, |v, _| v.launcher_menu_open_for_test()));
    assert!(menu_open, "launcher menu should be open");

    // 关闭 launcher menu。
    cx.update(|cx| {
        workspace.update(cx, |v, _| v.set_launcher_menu_open_for_test(false));
    });
    let menu_open = cx.update(|cx| workspace.update(cx, |v, _| v.launcher_menu_open_for_test()));
    assert!(!menu_open, "launcher menu should be closed");

    shutdown_workspace(cx, &workspace);
}

/// 13.3: `open_repo_picker` 弹出 PathPicker(path_picker_open = true)。
/// 旧的 choice dialog / WSL browser 已被 PathPicker 替代。
#[gpui::test]
async fn open_repo_picker_shows_path_picker(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, window) = build_workspace(cx, Some(repo));
    window.run_until_parked();

    // 初始:PathPicker 未打开。
    let picker_open =
        window.update(|_window, cx| workspace.update(cx, |v, _| v.path_picker_open()));
    assert!(!picker_open, "PathPicker should not be open initially");

    // 触发 open_repo_picker → path_picker_open = true。
    window.update(|window, cx| {
        workspace.update(cx, |v, cx| v.open_repo_picker_for_test(window, cx));
    });

    let picker_open =
        window.update(|_window, cx| workspace.update(cx, |v, _| v.path_picker_open()));
    assert!(
        picker_open,
        "PathPicker should be open after open_repo_picker"
    );

    shutdown_workspace(window, &workspace);
}

/// 13.3 (远程 Host): RemoteMockHost 构造时,open_repo_picker 同样弹出 PathPicker。
/// 验证 PathPicker 对远程 Host(is_remote=true)也能正常创建。
#[gpui::test]
async fn remote_host_open_repo_picker(cx: &mut TestAppContext) {
    let host: std::sync::Arc<dyn Host> = std::sync::Arc::new(RemoteMockHost::new());
    let (_dir, repo) = temp_repo();
    let (workspace, window) = build_workspace_with_host(cx, Some(repo), host);
    window.run_until_parked();

    let is_remote = window.update(|_window, cx| workspace.update(cx, |v, _| v.is_remote_host()));
    assert!(is_remote, "RemoteMockHost should be remote");

    // 初始:PathPicker 未打开。
    let picker_open =
        window.update(|_window, cx| workspace.update(cx, |v, _| v.path_picker_open()));
    assert!(!picker_open, "PathPicker should not be open initially");

    // 触发 open_repo_picker → path_picker_open = true(远程 Host 也能正常弹)。
    window.update(|window, cx| {
        workspace.update(cx, |v, cx| v.open_repo_picker_for_test(window, cx));
    });

    let picker_open =
        window.update(|_window, cx| workspace.update(cx, |v, _| v.path_picker_open()));
    assert!(
        picker_open,
        "PathPicker should be open after open_repo_picker (remote host)"
    );

    shutdown_workspace(window, &workspace);
}

#[gpui::test]
async fn remote_shell_spawn_uses_wsl_cwd_only(cx: &mut TestAppContext) {
    let host: std::sync::Arc<dyn Host> = std::sync::Arc::new(RemoteMockHost::new());
    let (workspace, _window) = build_workspace_with_host(cx, None, host);
    let path = Path::new("/home/lucy/project");
    let env = vec![("TERM".to_string(), "xterm-256color".to_string())];

    let (working_directory, command, pty_env) = cx
        .update(|cx| workspace.update(cx, |view, _| view.shell_spawn_options_for_test(path, env)));

    assert_eq!(
        working_directory, None,
        "ConPTY must not receive a WSL path"
    );
    assert!(
        pty_env.is_empty(),
        "WSL env must cross via the command line"
    );
    assert_eq!(
        command,
        Some((
            "wsl.exe".to_string(),
            vec![
                "--cd".to_string(),
                "/home/lucy/project".to_string(),
                "--".to_string(),
                "env".to_string(),
                "TERM=xterm-256color".to_string(),
                "/bin/sh".to_string(),
            ]
        ))
    );

    shutdown_workspace(cx, &workspace);
}

#[cfg(target_os = "windows")]
#[gpui::test]
async fn local_picker_can_switch_to_wsl(cx: &mut TestAppContext) {
    let (workspace, window) = build_workspace(cx, None);
    window.update(|window, cx| {
        workspace.update(cx, |view, cx| view.open_repo_picker_for_test(window, cx));
    });
    let picker = window
        .update(|_window, cx| workspace.read(cx).path_picker_for_test().cloned())
        .expect("picker should be open");

    window.update(|window, cx| {
        picker.update(cx, |picker, cx| {
            picker.switch_location_for_test(true, window, cx)
        });
    });

    let (remote, separator, query) = window.update(|_window, cx| {
        let picker = picker.read(cx);
        (
            picker.is_remote(),
            picker.separator_for_test(),
            picker.query(cx),
        )
    });
    assert!(remote);
    assert_eq!(separator, '/');
    assert_eq!(query, "/");

    shutdown_workspace(window, &workspace);
}

// ---- 辅助:is_remote=true 的 MockHost(不依赖 #[cfg(test)] 的 MockHost) ----

/// RemoteMockHost:内存 Host,is_remote 返回 true。
/// 预装 git 输出(git worktree list / git rev-parse --show-toplevel),
/// 其他操作委托给内存文件系统。
struct RemoteMockHost {
    files: Mutex<std::collections::HashMap<PathBuf, String>>,
}

impl RemoteMockHost {
    fn new() -> Self {
        Self {
            files: Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl Host for RemoteMockHost {
    fn run(&self, cmd: HostCommand) -> Result<HostOutput, HostError> {
        // git worktree list --porcelain → 只有主仓(cwd 就是主仓根)。
        if cmd.program == "git" && cmd.args.iter().any(|a| a == "worktree") {
            let cwd = cmd
                .cwd
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            return Ok(HostOutput {
                stdout: format!("worktree {cwd}\nHEAD 0000000000000000000000000000000000000000\n"),
                stderr: String::new(),
                success: true,
                exit_code: Some(0),
            });
        }
        // git rev-parse --show-toplevel → 返回 cwd。
        if cmd.program == "git" && cmd.args.iter().any(|a| a == "--show-toplevel") {
            let cwd = cmd
                .cwd
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            return Ok(HostOutput {
                stdout: cwd,
                stderr: String::new(),
                success: true,
                exit_code: Some(0),
            });
        }
        // git status --porcelain → 空输出(无未提交改动)。
        if cmd.program == "git" && cmd.args.iter().any(|a| a == "status") {
            return Ok(HostOutput {
                stdout: String::new(),
                stderr: String::new(),
                success: true,
                exit_code: Some(0),
            });
        }
        // 默认:成功空输出。
        Ok(HostOutput {
            stdout: String::new(),
            stderr: String::new(),
            success: true,
            exit_code: Some(0),
        })
    }

    fn run_shell(
        &self,
        _cwd: &Path,
        _cmd: &str,
        _env: &[(String, String)],
    ) -> Result<HostOutput, HostError> {
        Ok(HostOutput {
            stdout: String::new(),
            stderr: String::new(),
            success: true,
            exit_code: Some(0),
        })
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, HostError> {
        Ok(path.to_path_buf())
    }

    fn exists(&self, path: &Path) -> bool {
        self.files.lock().unwrap().contains_key(path)
    }

    fn read_to_string(&self, path: &Path) -> Result<String, HostError> {
        self.files
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(|| HostError::NotFound(path.to_path_buf()))
    }

    fn write(&self, path: &Path, content: &str) -> Result<(), HostError> {
        self.files
            .lock()
            .unwrap()
            .insert(path.to_path_buf(), content.to_string());
        Ok(())
    }

    fn copy(&self, from: &Path, to: &Path) -> Result<(), HostError> {
        let content = self.read_to_string(from)?;
        self.write(to, &content)
    }

    fn create_dir_all(&self, _path: &Path) -> Result<(), HostError> {
        Ok(())
    }

    fn join_path(&self, base: &Path, child: &str) -> PathBuf {
        PathBuf::from(format!(
            "{}/{}",
            base.to_string_lossy().trim_end_matches('/'),
            child.replace('\\', "/")
        ))
    }

    fn list_dir(&self, _path: &Path) -> Result<Vec<lucy_core::host::DirEntry>, HostError> {
        Ok(Vec::new())
    }

    fn default_shell(&self, cwd: &Path) -> Option<(String, Vec<String>)> {
        Some((
            "wsl.exe".to_string(),
            vec!["--cd".to_string(), cwd.to_string_lossy().into_owned()],
        ))
    }

    fn shell_with_env(
        &self,
        cwd: &Path,
        env: &[(String, String)],
    ) -> Option<(String, Vec<String>)> {
        let mut args = vec!["--cd".to_string(), cwd.to_string_lossy().into_owned()];
        args.push("--".to_string());
        if !env.is_empty() {
            args.push("env".to_string());
            for (k, v) in env {
                args.push(format!("{k}={v}"));
            }
        }
        args.push("/bin/sh".to_string());
        Some(("wsl.exe".to_string(), args))
    }

    fn file_manager_command(&self, path: &Path) -> Option<(String, Vec<String>)> {
        Some((
            "wsl.exe".to_string(),
            vec![
                "--cd".to_string(),
                path.to_string_lossy().into_owned(),
                "--".to_string(),
                "explorer.exe".to_string(),
                ".".to_string(),
            ],
        ))
    }

    fn is_remote(&self) -> bool {
        true
    }
}
