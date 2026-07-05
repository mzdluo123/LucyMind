//! 内嵌静态资源(SVG 图标)。
//!
//! GPUI 的 `svg()` 元素通过 `AssetSource` 按路径加载资源。原生 app 不能引在线
//! URL,所以把 SVG 用 `include_bytes!` 编译进二进制,随 app 分发、免外部文件。
//!
//! 图标来自 lobehub(icons.lobehub.com),单色 `fill="currentColor"`,可跟主题染色。

use std::borrow::Cow;

use gpui::{AssetSource, SharedString};

/// 编译期内嵌的图标(路径 → 字节)。
const CLAUDE_SVG: &[u8] = include_bytes!("../assets/icons/claude.svg");
const CODEX_SVG: &[u8] = include_bytes!("../assets/icons/codex.svg");
const LOGO_SVG: &[u8] = include_bytes!("../assets/icons/logo.svg");

/// app 的静态资源源。
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        let bytes: Option<&'static [u8]> = match path {
            "icons/claude.svg" => Some(CLAUDE_SVG),
            "icons/codex.svg" => Some(CODEX_SVG),
            "icons/logo.svg" => Some(LOGO_SVG),
            _ => None,
        };
        Ok(bytes.map(Cow::Borrowed))
    }

    fn list(&self, _path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(vec![
            SharedString::from("icons/claude.svg"),
            SharedString::from("icons/codex.svg"),
        ])
    }
}

/// 某 agent 名对应的图标资源路径(无对应则 None)。
pub fn agent_icon(agent: &str) -> Option<&'static str> {
    match agent {
        "claude" => Some("icons/claude.svg"),
        "codex" => Some("icons/codex.svg"),
        _ => None,
    }
}
