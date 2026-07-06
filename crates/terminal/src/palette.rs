//! 默认调色板与颜色解析。
//!
//! alacritty_terminal **不内置默认配色**——它的 `Colors` 表默认全是 `None`,
//! 只存被 OSC 转义序列动态改过的颜色。所以我们必须自备一份完整默认 palette
//! (16 基础色 + 216 color cube + 24 灰阶 + 前景/背景),把内核给的
//! `vte::ansi::Color`(Named/Spec/Indexed)解析成实际 RGB。

use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};

/// 简单 RGB 三元组(与 GPUI 无关,app 层再转成 gpui 颜色)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb888 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb888 {
    const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
    /// 打包成 0xRRGGBB(app 层喂给 gpui::rgb 用)。
    pub fn packed(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }
    fn from_rgb(c: Rgb) -> Self {
        Self::new(c.r, c.g, c.b)
    }
}

/// 默认前景色 / 背景色。冷深色性冷淡主题:near-black 底 + 冷灰字。
/// (与 app 层 theme::BG / theme::TEXT 对齐,保证终端底色与外壳无缝。)
pub const DEFAULT_FG: Rgb888 = Rgb888::new(0xc8, 0xc8, 0xce);
pub const DEFAULT_BG: Rgb888 = Rgb888::new(0x0e, 0x0e, 0x10);
pub const DEFAULT_CURSOR: Rgb888 = Rgb888::new(0xc8, 0xc8, 0xce);

/// 标准 16 色(0..16):8 普通 + 8 亮色。取自常见终端配色。
const ANSI_16: [Rgb888; 16] = [
    Rgb888::new(0x0e, 0x0e, 0x10), // 0 black(与主题底色一致)
    Rgb888::new(0xf1, 0x4c, 0x4c), // 1 red
    Rgb888::new(0x4e, 0xc9, 0xb0), // 2 green
    Rgb888::new(0xdc, 0xdc, 0xaa), // 3 yellow
    Rgb888::new(0x56, 0x9c, 0xd6), // 4 blue
    Rgb888::new(0xc5, 0x86, 0xc0), // 5 magenta
    Rgb888::new(0x9c, 0xdc, 0xfe), // 6 cyan
    Rgb888::new(0xd4, 0xd4, 0xd4), // 7 white
    Rgb888::new(0x80, 0x80, 0x80), // 8 bright black (gray)
    Rgb888::new(0xff, 0x6b, 0x6b), // 9 bright red
    Rgb888::new(0x6b, 0xe5, 0xcd), // 10 bright green
    Rgb888::new(0xff, 0xf2, 0x9e), // 11 bright yellow
    Rgb888::new(0x7a, 0xb8, 0xf5), // 12 bright blue
    Rgb888::new(0xe0, 0xa0, 0xdc), // 13 bright magenta
    Rgb888::new(0xc0, 0xe8, 0xff), // 14 bright cyan
    Rgb888::new(0xff, 0xff, 0xff), // 15 bright white
];

/// 解析 256 色板索引 → RGB(标准 xterm 256 色布局)。
/// 0..16 基础色;16..232 = 6×6×6 color cube;232..256 = 24 级灰阶。
fn indexed_256(i: u8) -> Rgb888 {
    match i {
        0..=15 => ANSI_16[i as usize],
        16..=231 => {
            let i = i - 16;
            let r = i / 36;
            let g = (i % 36) / 6;
            let b = i % 6;
            // xterm cube 每级取值:0, 95, 135, 175, 215, 255。
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            Rgb888::new(level(r), level(g), level(b))
        }
        232..=255 => {
            let step = i - 232;
            let v = 8 + step * 10; // 8, 18, ..., 238
            Rgb888::new(v, v, v)
        }
    }
}

/// 默认命名色 → RGB。
fn named(n: NamedColor) -> Rgb888 {
    use NamedColor::*;
    match n {
        Black => ANSI_16[0],
        Red => ANSI_16[1],
        Green => ANSI_16[2],
        Yellow => ANSI_16[3],
        Blue => ANSI_16[4],
        Magenta => ANSI_16[5],
        Cyan => ANSI_16[6],
        White => ANSI_16[7],
        BrightBlack => ANSI_16[8],
        BrightRed => ANSI_16[9],
        BrightGreen => ANSI_16[10],
        BrightYellow => ANSI_16[11],
        BrightBlue => ANSI_16[12],
        BrightMagenta => ANSI_16[13],
        BrightCyan => ANSI_16[14],
        BrightWhite => ANSI_16[15],
        Foreground => DEFAULT_FG,
        Background => DEFAULT_BG,
        Cursor => DEFAULT_CURSOR,
        // Dim / Bright-foreground 等派生色:回落到最接近的基础色。
        BrightForeground => DEFAULT_FG,
        DimForeground => DEFAULT_FG,
        DimBlack => ANSI_16[0],
        DimRed => ANSI_16[1],
        DimGreen => ANSI_16[2],
        DimYellow => ANSI_16[3],
        DimBlue => ANSI_16[4],
        DimMagenta => ANSI_16[5],
        DimCyan => ANSI_16[6],
        DimWhite => ANSI_16[7],
    }
}

/// 把内核给的 cell 颜色解析成 RGB。
///
/// 优先用 `dynamic`(OSC 运行时改过的颜色)覆盖,否则回落默认 palette。
/// 这正是 Zed / alacritty 的解析规则。
pub fn resolve(color: Color, dynamic: &Colors) -> Rgb888 {
    match color {
        Color::Spec(rgb) => Rgb888::from_rgb(rgb),
        Color::Indexed(i) => dynamic[i as usize]
            .map(Rgb888::from_rgb)
            .unwrap_or_else(|| indexed_256(i)),
        Color::Named(n) => dynamic[n].map(Rgb888::from_rgb).unwrap_or_else(|| named(n)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_is_rrggbb() {
        assert_eq!(Rgb888::new(0x12, 0x34, 0x56).packed(), 0x123456);
    }

    #[test]
    fn cube_corners_match_xterm() {
        // index 16 = cube (0,0,0) = 黑;index 231 = cube (5,5,5) = 白。
        assert_eq!(indexed_256(16), Rgb888::new(0, 0, 0));
        assert_eq!(indexed_256(231), Rgb888::new(255, 255, 255));
    }

    #[test]
    fn grayscale_ramp() {
        assert_eq!(indexed_256(232), Rgb888::new(8, 8, 8));
        assert_eq!(indexed_256(255), Rgb888::new(238, 238, 238));
    }

    #[test]
    fn base_16_indexed_matches_named() {
        assert_eq!(indexed_256(1), named(NamedColor::Red));
    }

    #[test]
    fn spec_color_passes_through() {
        let colors = Colors::default();
        let got = resolve(
            Color::Spec(Rgb {
                r: 10,
                g: 20,
                b: 30,
            }),
            &colors,
        );
        assert_eq!(got, Rgb888::new(10, 20, 30));
    }

    #[test]
    fn named_falls_back_to_default_when_not_dynamic() {
        let colors = Colors::default(); // 全 None
        let got = resolve(Color::Named(NamedColor::Foreground), &colors);
        assert_eq!(got, DEFAULT_FG);
    }
}
