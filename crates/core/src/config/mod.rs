//! `.worktree.toml` 配置模型与加载。
//!
//! 加载流程:读文件 → TOML 反序列化(强类型,缺失给默认)→ 语义校验。
//! 校验产出**警告**(未知 key,非致命)与**错误**(如 sibling 缺 dir,致命)。

mod schema;

pub use schema::*;

use std::path::Path;

/// 配置加载/校验错误。
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("读取配置文件失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("解析 TOML 失败: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("配置校验失败: {0}")]
    Validation(String),
}

/// 加载结果:配置 + 非致命警告列表(供 UI 展示)。
#[derive(Debug)]
pub struct Loaded {
    pub config: WorktreeConfig,
    pub warnings: Vec<String>,
}

/// 从文件加载并校验 `.worktree.toml`。
pub fn load(path: impl AsRef<Path>) -> Result<Loaded, ConfigError> {
    let text = std::fs::read_to_string(path)?;
    parse(&text)
}

/// 从字符串解析并校验(便于测试)。
pub fn parse(text: &str) -> Result<Loaded, ConfigError> {
    // 先用 toml::Value 探测未知顶层 key(收集警告而非致命)。
    let warnings = collect_unknown_key_warnings(text)?;

    let config: WorktreeConfig = toml::from_str(text)?;
    validate(&config)?;

    Ok(Loaded { config, warnings })
}

/// 已知的顶层 key 集合。出现其它顶层 key 时收集为警告(拼写错误提示)。
const KNOWN_TOP_LEVEL_KEYS: &[&str] = &["worktree", "copy", "hooks", "agents"];

fn collect_unknown_key_warnings(text: &str) -> Result<Vec<String>, ConfigError> {
    let value: toml::Value = toml::from_str(text)?;
    let mut warnings = Vec::new();
    if let Some(table) = value.as_table() {
        for key in table.keys() {
            if !KNOWN_TOP_LEVEL_KEYS.contains(&key.as_str()) {
                warnings.push(format!("未知的顶层配置项 `{key}`(已忽略)"));
            }
        }
    }
    Ok(warnings)
}

/// 语义校验(超出类型系统能表达的约束)。
fn validate(config: &WorktreeConfig) -> Result<(), ConfigError> {
    if config.worktree.location == Location::Sibling && config.worktree.dir.trim().is_empty() {
        return Err(ConfigError::Validation(
            "location = \"sibling\" 时 [worktree].dir 不能为空".to_string(),
        ));
    }
    Ok(())
}

/// 用配置里的 `dir` 模板生成 sibling worktree 的父目录名。
///
/// 仅替换 `{repo}` 为仓库目录名 —— 这是唯一允许的插值,不用于 hook 命令。
pub fn resolve_sibling_dir(dir_template: &str, repo_name: &str) -> String {
    dir_template.replace("{repo}", repo_name)
}
