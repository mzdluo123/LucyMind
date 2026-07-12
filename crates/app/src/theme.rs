//! 视觉主题 —— 性冷淡 / 扁平化 / 无彩 / 硬朗设计语言的语义色 token。
//!
//! 设计原则(与用户确认):
//! - **冷深色**:near-black 底 + 冷灰文字,长时间看不累。
//! - **几乎无彩**:全灰阶 + 一点冷白;不用彩色强调块,按钮靠深灰底 + 细描边。
//! - **扁平**:无阴影、无渐变;层级靠**描边**与**间距**划分。
//! - **硬朗**:统一 2px 微圆角。
//!
//! 一切颜色/圆角/间距集中在此,不在组件里散用 hex —— 这是保证全局一致
//! 与日后换肤的地基(对应 UI 规范的 color-semantic / elevation-consistent)。
//!
//! 部分 token(SURFACE_RAISED / STATE_OK / border_width)是完整设计系统的一
//! 部分,当前 MVP 尚未全部用到,保留以备后续组件(悬浮项、成功态、描边控件)。
#![allow(dead_code)]

use gpui::{px, rgb, App, Hsla, Pixels, Rgba};

/// 把 0xRRGGBB 转成 gpui 颜色。
const fn c(hex: u32) -> Rgba {
    // gpui::rgb 不是 const,这里手动拆分。
    Rgba {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

// ---- 表面(surface):层级靠明度微差 + 描边,不靠阴影 ----

/// 最底层背景(终端区、主画布)。
pub const BG: u32 = 0x0e_0e_10;
/// 抬升一层的表面(侧边栏、面板)。
pub const SURFACE: u32 = 0x16_16_1a;
/// 再抬升(选中项、悬浮项背景)。
pub const SURFACE_RAISED: u32 = 0x1f_1f_24;

/// 描边 / 分隔线(扁平风的层级主要靠它)。
pub const BORDER: u32 = 0x2a_2a_30;
/// 更弱的描边(内部分隔)。
pub const BORDER_SUBTLE: u32 = 0x20_20_25;

// ---- 文字 ----

/// 主文字(冷灰,非纯白 —— 纯白在暗底上太刺眼)。
pub const TEXT: u32 = 0xc8_c8_ce;
/// 次要文字 / 标签。
pub const TEXT_DIM: u32 = 0x8a_8a_92;
/// 最弱文字(占位、辅助)。
pub const TEXT_FAINT: u32 = 0x5c5c64;

/// 次要图标。比 TEXT_FAINT 更亮，确保 14–16px 工具图标在暗色表面达到 3:1。
pub const ICON_MUTED: u32 = 0x76_76_7f;

/// 冷白高亮(极克制地用于当前项 / 强调文字)。
pub const TEXT_BRIGHT: u32 = 0xe8_e8_ee;

// ---- 交互(无彩:按钮=深灰底+细描边,悬浮/按下靠明度) ----

/// 按钮默认底。
pub const BTN_BG: u32 = 0x1f_1f_24;
/// 按钮悬浮底。
pub const BTN_BG_HOVER: u32 = 0x2a_2a_30;
/// 按钮按下底。
pub const BTN_BG_ACTIVE: u32 = 0x33_33_3a;

// ---- 终端专属 ----

/// 终端选区高亮(半透明冷灰,不用彩色)。
pub const SELECTION: u32 = 0x3a_3a_42;
pub const SELECTION_ALPHA: f32 = 0.55;

/// 光标(冷白半透明块)。
pub const CURSOR: u32 = 0xc8_c8_ce;
pub const CURSOR_ALPHA: f32 = 0.45;

// ---- 状态:无彩风下靠图标/文字区分,但保留极克制的语义提示 ----
// 这里给的是"降到极冷"的语义色,仅在必须一眼区分严重程度时用(如错误行)。

/// 错误(降饱和的冷红,仅错误态用)。
pub const STATE_ERROR: u32 = 0xb5_5c_5c;
/// 成功(降饱和的冷绿)。
pub const STATE_OK: u32 = 0x6f_9c_88;

// ---- 字体 ----

/// 界面字体(侧边栏/标题/按钮)。使用各平台自带字体，避免 DirectWrite/CoreText
/// 因找不到其他平台的字体而记录错误。
///
/// 之前用 Futura(几何无衬线),但它无中文字形:侧边栏里的中文(worktree
/// 别名等)回退到苹方,而苹方在同字号下字面远大于 Futura,中文显得比英文大
/// 一截、且不扁平。换成等宽的 Monaco 后,英文字面本就偏方偏大,与中文(仍
/// 回退苹方)的落差大幅缩小,中英并排更协调,也更贴合终端工具的调性。
#[cfg(target_os = "macos")]
pub const FONT_UI: &str = "Monaco";
#[cfg(target_os = "windows")]
pub const FONT_UI: &str = "Microsoft YaHei UI";
#[cfg(target_os = "linux")]
pub const FONT_UI: &str = "DejaVu Sans";
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub const FONT_UI: &str = "sans-serif";

// ---- 形状 ----

/// 统一圆角:2px 微圆角(硬朗但不割手)。
pub fn radius() -> Pixels {
    px(2.0)
}

/// 描边宽度。
pub fn border_width() -> Pixels {
    px(1.0)
}

// ---- 间距节奏(4/8 系统) ----

pub fn space_xs() -> Pixels {
    px(4.0)
}
pub fn space_sm() -> Pixels {
    px(8.0)
}
pub fn space_md() -> Pixels {
    px(12.0)
}
pub fn space_lg() -> Pixels {
    px(16.0)
}

/// 便捷:带 alpha 的颜色。
pub fn with_alpha(hex: u32, a: f32) -> Hsla {
    let mut rgba = c(hex);
    rgba.a = a;
    rgba.into()
}

/// 让 gpui-component 的控件继承 LucyMind 的视觉语言。
///
/// gpui-component 默认会跟随系统明暗模式，并使用 6px 圆角、阴影和自己的
/// 配色。LucyMind 的主界面固定为冷深色，因此必须在组件库初始化后覆盖这些
/// 全局 token；Input 的背景、文字、占位、选区和聚焦边框都会读取这里。
pub fn configure_component_theme(cx: &mut App) {
    let component = gpui_component::Theme::global_mut(cx);

    component.mode = gpui_component::ThemeMode::Dark;
    component.font_family = FONT_UI.into();
    component.radius = radius();
    component.radius_lg = radius();
    component.shadow = false;

    component.background = rgb(BG).into();
    component.foreground = rgb(TEXT).into();
    component.muted = rgb(SURFACE_RAISED).into();
    component.muted_foreground = rgb(TEXT_FAINT).into();
    component.input = rgb(BORDER).into();
    component.ring = rgb(TEXT_DIM).into();
    component.caret = rgb(TEXT_BRIGHT).into();
    component.selection = with_alpha(SELECTION, SELECTION_ALPHA);
}
