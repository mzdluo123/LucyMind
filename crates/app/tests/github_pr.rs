//! 当前 worktree branch 的 GitHub PR 状态集成测试。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use gpui::TestAppContext;
use lucy_core::host::{DirEntry, Host, HostCommand, HostError, HostOutput, LocalHost};

use common::{build_workspace_with_host, shutdown_workspace, temp_repo, wait_for};

mod common;

fn canonical_path(path: &Path) -> PathBuf {
    LocalHost.canonicalize(path).unwrap()
}

struct PrFixtureHost {
    local: LocalHost,
    responses: HashMap<PathBuf, (u64, Duration)>,
    gh_calls: AtomicUsize,
}

impl PrFixtureHost {
    fn new(responses: impl IntoIterator<Item = (PathBuf, (u64, Duration))>) -> Self {
        Self {
            local: LocalHost,
            responses: responses.into_iter().collect(),
            gh_calls: AtomicUsize::new(0),
        }
    }
}

impl Host for PrFixtureHost {
    fn run(&self, cmd: HostCommand) -> Result<HostOutput, HostError> {
        if cmd.program != "gh" {
            return self.local.run(cmd);
        }
        self.gh_calls.fetch_add(1, Ordering::SeqCst);
        let Some((number, delay)) = cmd.cwd.as_ref().and_then(|cwd| self.responses.get(cwd)) else {
            return Ok(HostOutput {
                stdout: String::new(),
                stderr: "no pull requests found".into(),
                success: false,
                exit_code: Some(1),
            });
        };
        std::thread::sleep(*delay);
        Ok(HostOutput {
            stdout: format!(
                r#"{{"number":{number},"title":"PR {number}","state":"OPEN","url":"https://github.com/o/r/pull/{number}","isDraft":false,"reviewDecision":"APPROVED","statusCheckRollup":[]}}"#
            ),
            stderr: String::new(),
            success: true,
            exit_code: Some(0),
        })
    }

    fn run_shell(
        &self,
        cwd: &Path,
        cmd: &str,
        env: &[(String, String)],
    ) -> Result<HostOutput, HostError> {
        self.local.run_shell(cwd, cmd, env)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, HostError> {
        self.local.canonicalize(path)
    }

    fn exists(&self, path: &Path) -> bool {
        self.local.exists(path)
    }

    fn read_to_string(&self, path: &Path) -> Result<String, HostError> {
        self.local.read_to_string(path)
    }

    fn write(&self, path: &Path, content: &str) -> Result<(), HostError> {
        self.local.write(path, content)
    }

    fn copy(&self, from: &Path, to: &Path) -> Result<(), HostError> {
        self.local.copy(from, to)
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), HostError> {
        self.local.create_dir_all(path)
    }

    fn list_dir(&self, path: &Path) -> Result<Vec<DirEntry>, HostError> {
        self.local.list_dir(path)
    }

    fn default_shell(&self, cwd: &Path) -> Option<(String, Vec<String>)> {
        self.local.default_shell(cwd)
    }

    fn shell_with_env(
        &self,
        cwd: &Path,
        env: &[(String, String)],
    ) -> Option<(String, Vec<String>)> {
        self.local.shell_with_env(cwd, env)
    }

    fn is_remote(&self) -> bool {
        false
    }
}

#[gpui::test]
async fn displays_pr_for_active_worktree(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let repo = canonical_path(&repo);
    let host = Arc::new(PrFixtureHost::new([(repo.clone(), (42, Duration::ZERO))]));
    let (workspace, _window) = build_workspace_with_host(cx, Some(repo.clone()), host.clone());

    cx.update(|cx| workspace.update(cx, |v, cx| v.open_worktree_for_test(repo.clone(), cx)));
    wait_for(
        cx,
        |cx| {
            cx.read(|cx| {
                workspace
                    .read(cx)
                    .current_pull_request()
                    .is_some_and(|pr| pr.number == 42 && pr.display_label().contains("已批准"))
            })
        },
        Duration::from_secs(5),
    );
    assert_eq!(host.gh_calls.load(Ordering::SeqCst), 1);
    let status_icon = cx.read(|cx| workspace.read(cx).pull_request_status_icon_for_test());
    assert_eq!(status_icon, Some("icons/circle-check-big.svg"));
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.open_pull_request_for_test(cx));
    });
    assert_eq!(
        cx.opened_url().as_deref(),
        Some("https://github.com/o/r/pull/42")
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn missing_pr_is_silent(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let repo = canonical_path(&repo);
    let host = Arc::new(PrFixtureHost::new([]));
    let (workspace, _window) = build_workspace_with_host(cx, Some(repo.clone()), host.clone());

    cx.update(|cx| workspace.update(cx, |v, cx| v.open_worktree_for_test(repo.clone(), cx)));
    wait_for(
        cx,
        |_cx| host.gh_calls.load(Ordering::SeqCst) == 1,
        Duration::from_secs(5),
    );
    cx.run_until_parked();
    cx.read(|cx| {
        let view = workspace.read(cx);
        assert!(view.current_pull_request().is_none());
        assert!(view.current_status().is_none());
    });

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn stale_pr_response_cannot_replace_new_active_worktree(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let repo = canonical_path(&repo);
    let wt = repo.parent().unwrap().join("second-worktree");
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args([
            "worktree",
            "add",
            "-q",
            "-b",
            "second",
            wt.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success());
    let wt = canonical_path(&wt);

    let host = Arc::new(PrFixtureHost::new([
        (repo.clone(), (1, Duration::from_millis(150))),
        (wt.clone(), (2, Duration::ZERO)),
    ]));
    let (workspace, _window) = build_workspace_with_host(cx, Some(repo.clone()), host);

    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.open_worktree_for_test(repo.clone(), cx));
        workspace.update(cx, |v, cx| v.open_worktree_for_test(wt.clone(), cx));
    });
    wait_for(
        cx,
        |cx| {
            cx.read(|cx| {
                workspace
                    .read(cx)
                    .current_pull_request()
                    .is_some_and(|pr| pr.number == 2)
            })
        },
        Duration::from_secs(5),
    );
    std::thread::sleep(Duration::from_millis(200));
    cx.run_until_parked();
    let number = cx.read(|cx| {
        workspace
            .read(cx)
            .current_pull_request()
            .map(|pr| pr.number)
    });
    assert_eq!(number, Some(2));

    shutdown_workspace(cx, &workspace);
}
