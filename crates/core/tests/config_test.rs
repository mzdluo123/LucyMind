//! U2 测试:`.worktree.toml` 解析与校验。

use lucy_core::config::{self, Location};

#[test]
fn parses_full_config() {
    let text = r#"
        [worktree]
        location = "sibling"
        dir = "../{repo}-worktrees"
        default_base = "develop"

        [copy]
        files = [".env", ".env.local"]

        [hooks]
        post_create = ["pnpm install", "./setup.sh"]
        pre_remove = ["./cleanup.sh"]

        [hooks.options]
        fail_fast = false

        [agents.claude]
        command = "claude"

        [agents.codex]
        command = "codex"
        args = ["--yolo"]
    "#;

    let loaded = config::parse(text).expect("should parse");
    let c = loaded.config;

    assert_eq!(c.worktree.location, Location::Sibling);
    assert_eq!(c.worktree.dir, "../{repo}-worktrees");
    assert_eq!(c.worktree.default_base, "develop");
    assert_eq!(c.copy.files, vec![".env", ".env.local"]);
    assert_eq!(c.hooks.post_create, vec!["pnpm install", "./setup.sh"]);
    assert_eq!(c.hooks.pre_remove, vec!["./cleanup.sh"]);
    assert!(!c.hooks.options.fail_fast);
    assert_eq!(c.agents["claude"].command, "claude");
    assert_eq!(c.agents["codex"].args, vec!["--yolo"]);
    assert!(loaded.warnings.is_empty());
}

#[test]
fn empty_hooks_default_to_empty_arrays() {
    let text = r#"
        [worktree]
        default_base = "main"

        [hooks]
    "#;
    let loaded = config::parse(text).expect("should parse");
    assert!(loaded.config.hooks.post_create.is_empty());
    assert!(loaded.config.hooks.pre_remove.is_empty());
    // 未显式设置 → fail_fast 默认 true。
    assert!(loaded.config.hooks.options.fail_fast);
}

#[test]
fn missing_worktree_section_uses_defaults() {
    // 空配置合法:全默认(sibling / main)。
    let loaded = config::parse("").expect("empty is valid");
    assert_eq!(loaded.config.worktree.location, Location::Sibling);
    assert_eq!(loaded.config.worktree.default_base, "main");
    assert_eq!(loaded.config.worktree.dir, "../{repo}-worktrees");
}

#[test]
fn sibling_with_empty_dir_is_rejected() {
    let text = r#"
        [worktree]
        location = "sibling"
        dir = ""
    "#;
    let err = config::parse(text).expect_err("empty sibling dir must fail validation");
    assert!(matches!(err, config::ConfigError::Validation(_)));
}

#[test]
fn invalid_toml_reports_parse_error() {
    let text = "this is = = not valid toml ][";
    let err = config::parse(text).expect_err("invalid toml must fail");
    assert!(matches!(err, config::ConfigError::Parse(_)));
}

#[test]
fn unknown_top_level_key_warns_but_does_not_fail() {
    let text = r#"
        [worktree]
        default_base = "main"

        [typoo]
        foo = "bar"
    "#;
    let loaded = config::parse(text).expect("unknown key is non-fatal");
    assert_eq!(loaded.warnings.len(), 1);
    assert!(loaded.warnings[0].contains("typoo"));
}

#[test]
fn resolve_sibling_dir_substitutes_repo_name() {
    let resolved = config::resolve_sibling_dir("../{repo}-worktrees", "myproj");
    assert_eq!(resolved, "../myproj-worktrees");
}

#[test]
fn alias_set_read_roundtrip_preserves_comments() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".worktree.toml");
    // 带注释 + 已有配置的文件。
    std::fs::write(&path, "# 我的注释\n[worktree]\ndefault_base = \"main\"\n").unwrap();

    // 设别名。
    config::set_alias(&path, "feature/x", "登录重构").unwrap();

    // 读回:别名在,注释还在,原配置还在。
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("# 我的注释"), "注释应保留");
    assert!(text.contains("default_base"), "原配置应保留");
    let loaded = config::load(&path).unwrap();
    assert_eq!(loaded.config.alias.get("feature/x").unwrap(), "登录重构");

    // 改别名。
    config::set_alias(&path, "feature/x", "新名字").unwrap();
    let loaded = config::load(&path).unwrap();
    assert_eq!(loaded.config.alias.get("feature/x").unwrap(), "新名字");

    // 空串删除别名。
    config::set_alias(&path, "feature/x", "").unwrap();
    let loaded = config::load(&path).unwrap();
    assert!(!loaded.config.alias.contains_key("feature/x"));
}

#[test]
fn set_alias_creates_file_if_missing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".worktree.toml");
    config::set_alias(&path, "main", "主干").unwrap();
    let loaded = config::load(&path).unwrap();
    assert_eq!(loaded.config.alias.get("main").unwrap(), "主干");
}

#[test]
fn inside_location_parses() {
    let text = r#"
        [worktree]
        location = "inside"
    "#;
    let loaded = config::parse(text).expect("should parse");
    assert_eq!(loaded.config.worktree.location, Location::Inside);
}

#[test]
fn settings_write_read_roundtrip_preserves_comments_and_alias() {
    use lucy_core::config::EditableSettings;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".worktree.toml");
    // 已有注释 + 别名 + agents 段的文件 —— 设置面板不该动它们。
    std::fs::write(
        &path,
        "# 顶部注释\n[alias]\n\"feature/x\" = \"登录\"\n\n[agents.codex]\ncommand = \"codex\"\n",
    )
    .unwrap();

    let s = EditableSettings {
        location: Location::Inside,
        dir: "../{repo}-wt".to_string(),
        default_base: "develop".to_string(),
        post_create: vec!["pnpm install".to_string(), "./setup.sh".to_string()],
        pre_remove: vec!["./cleanup.sh".to_string()],
        copy_files: vec![".env".to_string()],
        fail_fast: false,
    };
    config::set_worktree_settings(&path, &s).unwrap();

    // 读回:字段都对。
    let loaded = config::load(&path).unwrap();
    let c = &loaded.config;
    assert_eq!(c.worktree.location, Location::Inside);
    assert_eq!(c.worktree.dir, "../{repo}-wt");
    assert_eq!(c.worktree.default_base, "develop");
    assert_eq!(c.hooks.post_create, vec!["pnpm install", "./setup.sh"]);
    assert_eq!(c.hooks.pre_remove, vec!["./cleanup.sh"]);
    assert_eq!(c.copy.files, vec![".env"]);
    assert!(!c.hooks.options.fail_fast);

    // 注释 / 别名 / agents 都保留。
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("# 顶部注释"), "注释应保留");
    assert_eq!(c.alias.get("feature/x").unwrap(), "登录");
    assert_eq!(c.agents["codex"].command, "codex");

    // from_config 抽回来的字段与写入一致(往返闭环)。
    assert_eq!(EditableSettings::from_config(c), s);
}

#[test]
fn settings_write_rejects_empty_sibling_dir() {
    use lucy_core::config::EditableSettings;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".worktree.toml");
    let s = EditableSettings {
        location: Location::Sibling,
        dir: "   ".to_string(), // 空白 → 非法
        default_base: "main".to_string(),
        post_create: vec![],
        pre_remove: vec![],
        copy_files: vec![],
        fail_fast: true,
    };
    let err = config::set_worktree_settings(&path, &s).expect_err("empty sibling dir must fail");
    assert!(matches!(err, config::ConfigError::Validation(_)));
    // 校验失败不落盘。
    assert!(!path.exists(), "校验失败不应写文件");
}

#[test]
fn settings_write_creates_file_if_missing() {
    use lucy_core::config::EditableSettings;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".worktree.toml");
    let s = EditableSettings {
        location: Location::Sibling,
        dir: "../{repo}-worktrees".to_string(),
        default_base: "main".to_string(),
        post_create: vec![],
        pre_remove: vec![],
        copy_files: vec![],
        fail_fast: true,
    };
    config::set_worktree_settings(&path, &s).unwrap();
    assert!(path.exists());
    let loaded = config::load(&path).unwrap();
    assert_eq!(loaded.config.worktree.default_base, "main");
}
