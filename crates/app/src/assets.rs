//! 内嵌静态资源(SVG 图标)。
//!
//! GPUI 的 `svg()` 元素通过 `AssetSource` 按路径加载资源。原生 app 不能引在线
//! URL,所以把 SVG 用 `include_bytes!` 编译进二进制,随 app 分发、免外部文件。
//!
//! 品牌图标来自 lobehub，界面图标采用 Lucide；单色 SVG 可跟主题染色。

use std::borrow::Cow;

use gpui::{AssetSource, SharedString};

/// 编译期内嵌的图标(路径 → 字节)。
const CLAUDE_SVG: &[u8] = include_bytes!("../assets/icons/claude.svg");
const CODEX_SVG: &[u8] = include_bytes!("../assets/icons/codex.svg");
const OPENCODE_SVG: &[u8] = include_bytes!("../assets/icons/opencode.svg");
const LOGO_SVG: &[u8] = include_bytes!("../assets/icons/logo.svg");
const FOLDER_GIT_SVG: &[u8] = include_bytes!("../assets/icons/folder-git-2.svg");
const GIT_BRANCH_SVG: &[u8] = include_bytes!("../assets/icons/git-branch.svg");
const SETTINGS_SVG: &[u8] = include_bytes!("../assets/icons/settings.svg");
const PLUS_SVG: &[u8] = include_bytes!("../assets/icons/plus.svg");
const FOLDER_OPEN_SVG: &[u8] = include_bytes!("../assets/icons/folder-open.svg");
const ARROW_LEFT_SVG: &[u8] = include_bytes!("../assets/icons/arrow-left.svg");
const ARROW_RIGHT_SVG: &[u8] = include_bytes!("../assets/icons/arrow-right.svg");
const ARROW_UP_SVG: &[u8] = include_bytes!("../assets/icons/arrow-up.svg");
const REFRESH_CW_SVG: &[u8] = include_bytes!("../assets/icons/refresh-cw.svg");

/// app 的静态资源源。
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        let bytes: Option<&'static [u8]> = match path {
            "icons/claude.svg" => Some(CLAUDE_SVG),
            "icons/codex.svg" => Some(CODEX_SVG),
            "icons/opencode.svg" => Some(OPENCODE_SVG),
            "icons/logo.svg" => Some(LOGO_SVG),
            "icons/folder-git-2.svg" => Some(FOLDER_GIT_SVG),
            "icons/git-branch.svg" => Some(GIT_BRANCH_SVG),
            "icons/settings.svg" => Some(SETTINGS_SVG),
            "icons/plus.svg" => Some(PLUS_SVG),
            "icons/folder-open.svg" => Some(FOLDER_OPEN_SVG),
            "icons/arrow-left.svg" => Some(ARROW_LEFT_SVG),
            "icons/arrow-right.svg" => Some(ARROW_RIGHT_SVG),
            "icons/arrow-up.svg" => Some(ARROW_UP_SVG),
            "icons/refresh-cw.svg" => Some(REFRESH_CW_SVG),
            _ => None,
        };
        Ok(bytes.map(Cow::Borrowed))
    }

    fn list(&self, _path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(vec![
            SharedString::from("icons/claude.svg"),
            SharedString::from("icons/codex.svg"),
            SharedString::from("icons/opencode.svg"),
            SharedString::from("icons/logo.svg"),
            SharedString::from("icons/folder-git-2.svg"),
            SharedString::from("icons/git-branch.svg"),
            SharedString::from("icons/settings.svg"),
            SharedString::from("icons/plus.svg"),
            SharedString::from("icons/folder-open.svg"),
            SharedString::from("icons/arrow-left.svg"),
            SharedString::from("icons/arrow-right.svg"),
            SharedString::from("icons/arrow-up.svg"),
            SharedString::from("icons/refresh-cw.svg"),
        ])
    }
}

/// 某 agent 名对应的图标资源路径(无对应则 None)。
///
/// 查 [`lucy_core::agent::builtin_agents`] 注册表,新增 agent 自动有图标映射,
/// 无需在此硬编码。
pub fn agent_icon(agent: &str) -> Option<&'static str> {
    lucy_core::agent::builtin_agents()
        .iter()
        .find(|a| a.name == agent)
        .map(|a| a.icon)
}
