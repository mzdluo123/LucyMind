//! WSL Host UI 状态测试:
//! - 13.1: LocalHost 构造 `WorkspaceView`,`is_remote_host()` 返回 false。
//! - 13.2: RemoteMockHost(is_remote=true) 构造时 launcher menu 状态正确。
//! - 13.3: `open_repo_picker` 弹选择弹窗;`open_wsl_browser` 切换 WslHost + 打开浏览器。

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

/// 13.3: `open_repo_picker` 弹出选择弹窗(open_repo_choice_open = true)。
/// `open_wsl_browser` 切换 WslHost + 打开文件浏览器。
#[gpui::test]
async fn open_repo_picker_shows_choice_dialog(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo));
    cx.run_until_parked();

    // 初始:选择弹窗未打开。
    let choice_open = cx.update(|cx| workspace.update(cx, |v, _| v.open_repo_choice_open()));
    assert!(!choice_open, "choice dialog should not be open initially");

    // 触发 open_repo_picker → 设 open_repo_choice_open = true。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.open_repo_picker_for_test(cx));
    });

    let choice_open = cx.update(|cx| workspace.update(cx, |v, _| v.open_repo_choice_open()));
    assert!(
        choice_open,
        "choice dialog should be open after open_repo_picker"
    );

    // WSL 浏览器仍未打开(需要用户先选 WSL)。
    let wsl_open = cx.update(|cx| workspace.update(cx, |v, _| v.wsl_browser_open()));
    assert!(
        !wsl_open,
        "WSL browser should not be open until user picks WSL"
    );

    shutdown_workspace(cx, &workspace);
}

/// 13.3: `open_wsl_browser` 切换到 WslHost 并打开 WSL 文件浏览器。
#[gpui::test]
async fn open_wsl_browser_switches_host_and_opens_browser(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo));
    cx.run_until_parked();

    // 初始:LocalHost,is_remote = false。
    let is_remote_before = cx.update(|cx| workspace.update(cx, |v, _| v.is_remote_host()));
    assert!(!is_remote_before, "should start as LocalHost");

    // 触发 open_wsl_browser → 切换到 WslHost + 打开浏览器。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.open_wsl_browser_for_test(cx));
    });

    let is_remote_after = cx.update(|cx| workspace.update(cx, |v, _| v.is_remote_host()));
    assert!(is_remote_after, "should switch to WslHost");

    let wsl_open = cx.update(|cx| workspace.update(cx, |v, _| v.wsl_browser_open()));
    assert!(
        wsl_open,
        "WSL browser should be open after open_wsl_browser"
    );

    shutdown_workspace(cx, &workspace);
}

/// 13.3 (反向): LocalHost 的 WSL 浏览器初始未打开,
/// `open_repo_picker` 弹选择弹窗(不直接弹 WSL 浏览器)。
#[gpui::test]
async fn local_host_wsl_dialog_not_open(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo));
    cx.run_until_parked();

    let is_remote = cx.update(|cx| workspace.update(cx, |v, _| v.is_remote_host()));
    assert!(!is_remote, "LocalHost should not be remote");

    let wsl_open = cx.update(|cx| workspace.update(cx, |v, _| v.wsl_browser_open()));
    assert!(!wsl_open, "WSL browser should not be open initially");

    // open_repo_picker 弹选择弹窗,不直接弹 WSL 浏览器。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.open_repo_picker_for_test(cx));
    });

    let choice_open = cx.update(|cx| workspace.update(cx, |v, _| v.open_repo_choice_open()));
    assert!(choice_open, "choice dialog should be open");

    let wsl_open = cx.update(|cx| workspace.update(cx, |v, _| v.wsl_browser_open()));
    assert!(
        !wsl_open,
        "WSL browser should not be open (user hasn't picked WSL yet)"
    );

    shutdown_workspace(cx, &workspace);
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

    fn is_remote(&self) -> bool {
        true
    }
}
