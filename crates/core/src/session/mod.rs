//! Session 注册表:标记「哪些 worktree 是本工具开的」。
//!
//! 存在应用本地配置目录(macOS: `~/Library/Application Support/LucyMind/`),
//! 不进 git —— 「哪些 session 是我开的」是个人运行时状态。按仓库路径分组。
//!
//! 作用:关闭 worktree 时,只对本工具建的 session 提供「关闭」,避免误删
//! 用户手动建的 worktree;并记住每个 session 对应的分支/agent/创建时间。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// 一条 session 记录(本工具建的一个 worktree)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Session {
    /// worktree 绝对路径。
    pub path: PathBuf,
    pub branch: String,
    /// 启动的 agent 名(claude / codex);None = 只建了 worktree 没起 agent。
    pub agent: Option<String>,
    /// 创建时间(Unix 秒)。
    pub created_at: u64,
}

/// 注册表:仓库路径(字符串)→ 该仓库下本工具建的 session 列表。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Registry {
    /// 键为仓库根的字符串形式。
    repos: BTreeMap<String, Vec<Session>>,
}

/// 注册表错误。
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("无法定位应用配置目录")]
    NoConfigDir,
    #[error("读写注册表失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("解析注册表 JSON 失败: {0}")]
    Json(#[from] serde_json::Error),
}

impl Registry {
    /// 注册表文件的默认路径(应用本地配置目录下)。
    pub fn default_path() -> Result<PathBuf, RegistryError> {
        let dirs = directories::ProjectDirs::from("win", "rainchan", "LucyMind")
            .ok_or(RegistryError::NoConfigDir)?;
        Ok(dirs.config_dir().join("sessions.json"))
    }

    /// 从指定路径加载;文件不存在则返回空注册表。
    pub fn load(path: impl AsRef<Path>) -> Result<Self, RegistryError> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)?;
        if text.trim().is_empty() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_str(&text)?)
    }

    /// 从默认路径加载。
    pub fn load_default() -> Result<Self, RegistryError> {
        Self::load(Self::default_path()?)
    }

    /// 写回指定路径(自动建父目录)。
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), RegistryError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(path, text)?;
        Ok(())
    }

    /// 写回默认路径。
    pub fn save_default(&self) -> Result<(), RegistryError> {
        self.save(Self::default_path()?)
    }

    /// 列出某仓库下本工具建的 session。
    pub fn list_for_repo(&self, repo: impl AsRef<Path>) -> Vec<Session> {
        self.repos
            .get(&repo_key(repo.as_ref()))
            .cloned()
            .unwrap_or_default()
    }

    /// 判断某 worktree 路径是否由本工具建。
    pub fn is_ours(&self, repo: impl AsRef<Path>, worktree_path: impl AsRef<Path>) -> bool {
        let wt = worktree_path.as_ref();
        self.list_for_repo(repo).iter().any(|s| s.path == wt)
    }

    /// 注册一条 session(同路径存在则覆盖)。
    pub fn register(&mut self, repo: impl AsRef<Path>, session: Session) {
        let list = self.repos.entry(repo_key(repo.as_ref())).or_default();
        list.retain(|s| s.path != session.path);
        list.push(session);
    }

    /// 注销一条 session(按 worktree 路径)。返回是否移除了。
    pub fn unregister(&mut self, repo: impl AsRef<Path>, worktree_path: impl AsRef<Path>) -> bool {
        let key = repo_key(repo.as_ref());
        let wt = worktree_path.as_ref();
        if let Some(list) = self.repos.get_mut(&key) {
            let before = list.len();
            list.retain(|s| s.path != wt);
            let removed = list.len() != before;
            if list.is_empty() {
                self.repos.remove(&key);
            }
            return removed;
        }
        false
    }
}

/// 当前 Unix 秒(供调用方给 Session::created_at)。
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 仓库路径规范化成注册表键(去尾斜杠,尽量绝对)。
fn repo_key(repo: &Path) -> String {
    repo.to_string_lossy().trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sess(path: &str, branch: &str) -> Session {
        Session {
            path: PathBuf::from(path),
            branch: branch.into(),
            agent: Some("claude".into()),
            created_at: 1_700_000_000,
        }
    }

    #[test]
    fn register_and_list() {
        let mut reg = Registry::default();
        reg.register("/repo", sess("/wt/a", "feat-a"));
        reg.register("/repo", sess("/wt/b", "feat-b"));
        let list = reg.list_for_repo("/repo");
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn register_same_path_overwrites() {
        let mut reg = Registry::default();
        reg.register("/repo", sess("/wt/a", "old"));
        reg.register("/repo", sess("/wt/a", "new"));
        let list = reg.list_for_repo("/repo");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].branch, "new");
    }

    #[test]
    fn is_ours_detects_registered() {
        let mut reg = Registry::default();
        reg.register("/repo", sess("/wt/a", "feat-a"));
        assert!(reg.is_ours("/repo", "/wt/a"));
        assert!(!reg.is_ours("/repo", "/wt/unknown"));
    }

    #[test]
    fn unregister_removes() {
        let mut reg = Registry::default();
        reg.register("/repo", sess("/wt/a", "feat-a"));
        reg.register("/repo", sess("/wt/b", "feat-b"));
        reg.unregister("/repo", "/wt/a");
        let list = reg.list_for_repo("/repo");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].path, PathBuf::from("/wt/b"));
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sessions.json");

        let mut reg = Registry::default();
        reg.register("/repo", sess("/wt/a", "feat-a"));
        reg.save(&path).unwrap();

        let loaded = Registry::load(&path).unwrap();
        assert_eq!(loaded.list_for_repo("/repo").len(), 1);
        assert_eq!(loaded.list_for_repo("/repo")[0].branch, "feat-a");
    }

    #[test]
    fn load_missing_file_is_empty() {
        let reg = Registry::load("/nonexistent/path/sessions.json").unwrap();
        assert!(reg.list_for_repo("/repo").is_empty());
    }

    #[test]
    fn repos_grouped_separately() {
        let mut reg = Registry::default();
        reg.register("/repo1", sess("/wt/a", "a"));
        reg.register("/repo2", sess("/wt/b", "b"));
        assert_eq!(reg.list_for_repo("/repo1").len(), 1);
        assert_eq!(reg.list_for_repo("/repo2").len(), 1);
    }
}
