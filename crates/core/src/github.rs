//! GitHub Pull Request 查询。
//!
//! 复用 GitHub CLI (`gh`) 的登录态，避免应用自行保存 token。查询失败、未登录、
//! 当前分支没有 PR 都返回 `None`，调用方可以安静地忽略。

use std::path::Path;

use serde::Deserialize;

use crate::host::{Host, HostCommand};

/// 当前分支关联的 GitHub Pull Request。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub state: PullRequestState,
    pub url: String,
    pub is_draft: bool,
    pub review_decision: Option<String>,
    pub checks: CheckSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullRequestState {
    Open,
    Closed,
    Merged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullRequestStatus {
    Draft,
    Merged,
    Closed,
    ChecksFailed,
    ChecksPending,
    ChangesRequested,
    Approved,
    Open,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CheckSummary {
    pub pending: usize,
    pub failed: usize,
}

impl PullRequest {
    pub fn status(&self) -> PullRequestStatus {
        if self.is_draft {
            return PullRequestStatus::Draft;
        }
        match self.state {
            PullRequestState::Merged => PullRequestStatus::Merged,
            PullRequestState::Closed => PullRequestStatus::Closed,
            PullRequestState::Open if self.checks.failed > 0 => PullRequestStatus::ChecksFailed,
            PullRequestState::Open if self.checks.pending > 0 => PullRequestStatus::ChecksPending,
            PullRequestState::Open
                if self.review_decision.as_deref() == Some("CHANGES_REQUESTED") =>
            {
                PullRequestStatus::ChangesRequested
            }
            PullRequestState::Open if self.review_decision.as_deref() == Some("APPROVED") => {
                PullRequestStatus::Approved
            }
            PullRequestState::Open => PullRequestStatus::Open,
        }
    }

    /// 适合状态栏的紧凑状态文案。
    pub fn status_label(&self) -> &'static str {
        match self.status() {
            PullRequestStatus::Draft => "草稿",
            PullRequestStatus::Merged => "已合并",
            PullRequestStatus::Closed => "已关闭",
            PullRequestStatus::ChecksFailed => "检查失败",
            PullRequestStatus::ChecksPending => "检查中",
            PullRequestStatus::ChangesRequested => "需修改",
            PullRequestStatus::Approved => "已批准",
            PullRequestStatus::Open => "Open",
        }
    }

    pub fn display_label(&self) -> String {
        format!(
            "PR #{} · {} · {}",
            self.number,
            self.status_label(),
            self.title
        )
    }
}

/// 查询 `worktree` 当前分支的 PR。
///
/// `gh pr view` 会依据 cwd 中检出的分支定位 PR。任何失败均返回 `None`：这包含
/// 未安装/未登录 `gh`、非 GitHub remote、网络失败，以及当前分支没有 PR。
pub fn current_pull_request(host: &dyn Host, worktree: impl AsRef<Path>) -> Option<PullRequest> {
    let output = host
        .run(HostCommand {
            program: "gh".into(),
            args: vec![
                "pr".into(),
                "view".into(),
                "--json".into(),
                "number,title,state,url,isDraft,reviewDecision,statusCheckRollup".into(),
            ],
            cwd: Some(worktree.as_ref().to_path_buf()),
            env: vec![],
        })
        .ok()?;
    if !output.success {
        return None;
    }
    parse_pull_request(&output.stdout)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhPullRequest {
    number: u64,
    title: String,
    state: String,
    url: String,
    is_draft: bool,
    review_decision: Option<String>,
    #[serde(default)]
    status_check_rollup: Vec<GhCheck>,
}

#[derive(Deserialize)]
struct GhCheck {
    status: Option<String>,
    conclusion: Option<String>,
    state: Option<String>,
}

fn parse_pull_request(text: &str) -> Option<PullRequest> {
    let raw: GhPullRequest = serde_json::from_str(text).ok()?;
    let state = match raw.state.as_str() {
        "OPEN" => PullRequestState::Open,
        "CLOSED" => PullRequestState::Closed,
        "MERGED" => PullRequestState::Merged,
        _ => return None,
    };
    let mut checks = CheckSummary::default();
    for check in raw.status_check_rollup {
        if matches!(check.state.as_deref(), Some("ERROR" | "FAILURE")) {
            checks.failed += 1;
        } else if matches!(check.state.as_deref(), Some("EXPECTED" | "PENDING")) {
            checks.pending += 1;
        } else if check.state.as_deref() == Some("SUCCESS") {
            continue;
        } else if check.status.as_deref() != Some("COMPLETED") {
            checks.pending += 1;
        } else if !matches!(
            check.conclusion.as_deref(),
            Some("SUCCESS" | "NEUTRAL" | "SKIPPED")
        ) {
            checks.failed += 1;
        }
    }
    Some(PullRequest {
        number: raw.number,
        title: raw.title,
        state,
        url: raw.url,
        is_draft: raw.is_draft,
        review_decision: raw.review_decision,
        checks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pr_and_summarizes_checks() {
        let pr = parse_pull_request(
            r#"{"number":42,"title":"Add PR status","state":"OPEN","url":"https://github.com/o/r/pull/42","isDraft":false,"reviewDecision":"APPROVED","statusCheckRollup":[{"status":"COMPLETED","conclusion":"SUCCESS"},{"status":"IN_PROGRESS","conclusion":null}]}"#,
        )
        .unwrap();
        assert_eq!(pr.number, 42);
        assert_eq!(
            pr.checks,
            CheckSummary {
                pending: 1,
                failed: 0
            }
        );
        assert_eq!(pr.status_label(), "检查中");
        assert_eq!(pr.display_label(), "PR #42 · 检查中 · Add PR status");
    }

    #[test]
    fn failed_check_has_priority_over_review() {
        let pr = parse_pull_request(
            r#"{"number":7,"title":"Fix","state":"OPEN","url":"https://github.com/o/r/pull/7","isDraft":false,"reviewDecision":"APPROVED","statusCheckRollup":[{"status":"COMPLETED","conclusion":"FAILURE"}]}"#,
        )
        .unwrap();
        assert_eq!(pr.status_label(), "检查失败");
    }

    #[test]
    fn rejects_unknown_state_and_invalid_json() {
        assert!(parse_pull_request("not json").is_none());
        assert!(parse_pull_request(
            r#"{"number":1,"title":"x","state":"UNKNOWN","url":"x","isDraft":false,"reviewDecision":null,"statusCheckRollup":[]}"#
        )
        .is_none());
    }

    #[test]
    fn understands_status_context_state() {
        let pr = parse_pull_request(
            r#"{"number":8,"title":"Legacy status","state":"OPEN","url":"https://github.com/o/r/pull/8","isDraft":false,"reviewDecision":null,"statusCheckRollup":[{"state":"SUCCESS"},{"state":"FAILURE"}]}"#,
        )
        .unwrap();
        assert_eq!(
            pr.checks,
            CheckSummary {
                pending: 0,
                failed: 1
            }
        );
    }
}
