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

/// 在 `.worktree.toml` 里设置某分支的别名(格式保留:用 toml_edit 只改 `[alias]`
/// 表,不动用户的注释/其它配置)。`alias` 为空串则删除该别名。文件不存在则新建。
pub fn set_alias(path: impl AsRef<Path>, branch: &str, alias: &str) -> Result<(), ConfigError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| ConfigError::Validation(format!("解析 toml_edit 失败: {e}")))?;

    // 确保 [alias] 表存在。
    if !doc.contains_key("alias") {
        doc["alias"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    let table = doc["alias"]
        .as_table_mut()
        .ok_or_else(|| ConfigError::Validation("[alias] 不是表".into()))?;

    if alias.trim().is_empty() {
        table.remove(branch);
    } else {
        table[branch] = toml_edit::value(alias);
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, doc.to_string())?;
    Ok(())
}

/// 设置面板可编辑的字段(别名之外)。app 层组装后一次性写回,避免逐字段多次
/// 读写文件。字符串数组字段(hook 命令 / copy 文件)传入已按行拆好的 Vec。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditableSettings {
    pub location: Location,
    pub dir: String,
    pub default_base: String,
    pub post_create: Vec<String>,
    pub pre_remove: Vec<String>,
    pub copy_files: Vec<String>,
    pub fail_fast: bool,
}

impl EditableSettings {
    /// 从已解析的配置里抽出可编辑字段(供设置面板初始化)。
    pub fn from_config(config: &WorktreeConfig) -> Self {
        Self {
            location: config.worktree.location,
            dir: config.worktree.dir.clone(),
            default_base: config.worktree.default_base.clone(),
            post_create: config.hooks.post_create.clone(),
            pre_remove: config.hooks.pre_remove.clone(),
            copy_files: config.copy.files.clone(),
            fail_fast: config.hooks.options.fail_fast,
        }
    }
}

/// 把设置面板的字段写回 `.worktree.toml`(格式保留:用 toml_edit 只改涉及的
/// key,保留用户的注释、其它段、别名)。文件不存在则新建。
///
/// 写前做与 [`load`] 同款的语义校验(如 sibling 必须有 dir),不合法则返回
/// [`ConfigError::Validation`],不落盘。
pub fn set_worktree_settings(
    path: impl AsRef<Path>,
    s: &EditableSettings,
) -> Result<(), ConfigError> {
    // 校验:与文件加载走同一套约束,避免写出一个自己都加载不了的配置。
    if s.location == Location::Sibling && s.dir.trim().is_empty() {
        return Err(ConfigError::Validation(
            "location = \"sibling\" 时目录模板不能为空".to_string(),
        ));
    }

    let path = path.as_ref();
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| ConfigError::Validation(format!("解析 toml_edit 失败: {e}")))?;

    // ---- [worktree] ----
    let wt = ensure_table(&mut doc, "worktree")?;
    wt["location"] = toml_edit::value(match s.location {
        Location::Sibling => "sibling",
        Location::Inside => "inside",
    });
    wt["dir"] = toml_edit::value(s.dir.as_str());
    wt["default_base"] = toml_edit::value(s.default_base.as_str());

    // ---- [copy] ----
    let copy = ensure_table(&mut doc, "copy")?;
    copy["files"] = string_array(&s.copy_files);

    // ---- [hooks] ----
    let hooks = ensure_table(&mut doc, "hooks")?;
    hooks["post_create"] = string_array(&s.post_create);
    hooks["pre_remove"] = string_array(&s.pre_remove);

    // ---- [hooks.options] ----
    // 嵌套子表:确保 hooks.options 存在再写 fail_fast。
    let hooks_tbl = doc["hooks"]
        .as_table_mut()
        .ok_or_else(|| ConfigError::Validation("[hooks] 不是表".into()))?;
    if !hooks_tbl.contains_key("options") {
        hooks_tbl["options"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    hooks_tbl["options"]["fail_fast"] = toml_edit::value(s.fail_fast);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, doc.to_string())?;
    Ok(())
}

/// 确保顶层表 `key` 存在并返回其可变引用(不存在则建空表)。
fn ensure_table<'a>(
    doc: &'a mut toml_edit::DocumentMut,
    key: &str,
) -> Result<&'a mut toml_edit::Item, ConfigError> {
    if !doc.contains_key(key) {
        doc[key] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    if !doc[key].is_table() {
        return Err(ConfigError::Validation(format!("[{key}] 不是表")));
    }
    Ok(&mut doc[key])
}

/// 把字符串 Vec 转成 toml_edit 的字符串数组 Item(空 Vec → 空数组 `[]`)。
fn string_array(items: &[String]) -> toml_edit::Item {
    let mut arr = toml_edit::Array::new();
    for it in items {
        arr.push(it.as_str());
    }
    toml_edit::value(arr)
}
