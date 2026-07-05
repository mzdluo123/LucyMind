//! U8-spike 的核心:自绘 cell 网格(正确处理双宽字符)。
//!
//! 数据模型 [`StaticGrid`] 是「字符 + 前景色 + 背景色 + 宽度的二维 cell 数组」——
//! 这正是 wezterm `Screen`/`Line`/`Cell` 的信息形状(wezterm 的 Cell 也带 width)。
//!
//! **宽字符处理**:CJK 等字符占 2 列。网格里:第一列放该字符(`width=2`),
//! 紧邻的第二列放一个「续格」占位(`width=0`,不重复绘制)。绘制时按列索引
//! × 单元宽度定位,双宽字符自然横跨两个 cell,不再和后文重叠。

use gpui::{
    canvas, fill, point, px, rgb, size, App, Bounds, IntoElement, Pixels, SharedString, Styled,
    TextRun, Window,
};
use unicode_width::UnicodeWidthChar;

/// 单个终端 cell:字符 + 前景色 + 背景色 + 显示宽度(0/1/2)。
///
/// - `width == 1`:普通西文/半角。
/// - `width == 2`:双宽字符(CJK 等),占据本格与右侧一格。
/// - `width == 0`:双宽字符右侧的「续格」占位,不绘制字符。
#[derive(Clone, Copy)]
pub struct Cell {
    pub ch: char,
    pub fg: u32,
    pub bg: u32,
    pub width: u8,
}

impl Cell {
    fn blank() -> Self {
        Self {
            ch: ' ',
            fg: 0xd4d4d4,
            bg: 0x1e1e1e,
            width: 1,
        }
    }

    /// 双宽字符右侧的续格。
    fn continuation(bg: u32) -> Self {
        Self {
            ch: ' ',
            fg: 0xd4d4d4,
            bg,
            width: 0,
        }
    }
}

/// 一屏静态 cell 网格(rows × cols)。cols 是**显示列数**。
pub struct StaticGrid {
    pub rows: usize,
    pub cols: usize,
    pub cells: Vec<Cell>, // row-major,长度 = rows*cols
}

impl StaticGrid {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            cells: vec![Cell::blank(); rows * cols],
        }
    }

    #[cfg(test)]
    fn at(&self, row: usize, col: usize) -> Cell {
        self.cells[row * self.cols + col]
    }

    /// 在某行写一段带色文本,按**显示宽度**推进列(双宽字符占 2 列)。
    /// 超出列宽则截断,绝不越界。
    fn put_str(&mut self, row: usize, start_col: usize, s: &str, fg: u32, bg: u32) {
        let mut col = start_col;
        for ch in s.chars() {
            let w = ch.width().unwrap_or(0);
            if w == 0 {
                continue; // 跳过零宽/控制字符
            }
            // 放不下(尤其双宽字符不能只放一半)则停止。
            if col + w > self.cols {
                break;
            }
            let idx = row * self.cols + col;
            self.cells[idx] = Cell {
                ch,
                fg,
                bg,
                width: w as u8,
            };
            if w == 2 {
                // 右侧续格:同背景色(让双宽字符的背景连贯),不绘制字符。
                self.cells[idx + 1] = Cell::continuation(bg);
            }
            col += w;
        }
    }

    /// 构造一屏演示内容:含西文、CJK 双宽字符、彩色状态条,验证渲染正确。
    pub fn demo() -> Self {
        let mut g = Self::new(16, 72);
        g.put_str(0, 1, "LucyMind — U8 spike: 自绘终端网格 (wezterm 模型)", 0x4ec9b0, 0x1e1e1e);
        g.put_str(2, 1, "$ git worktree add ../proj-worktrees/feature-x", 0xd4d4d4, 0x1e1e1e);
        g.put_str(3, 1, "  Preparing worktree (new branch 'feature-x')", 0x9cdcfe, 0x1e1e1e);
        g.put_str(4, 1, "  HEAD is now at 34e57df scaffold", 0x9cdcfe, 0x1e1e1e);
        g.put_str(6, 1, " OK  post_create 钩子完成 ", 0x1e1e1e, 0x4ec9b0);
        g.put_str(7, 1, " WARN  分支已被检出,请换名 ", 0x1e1e1e, 0xdcdcaa);
        g.put_str(8, 1, " ERR  worktree 有未提交改动 ", 0xffffff, 0xf14c4c);
        g.put_str(10, 1, "$ claude", 0xd4d4d4, 0x1e1e1e);
        g.put_str(11, 1, "  ● 会话已在 worktree 目录启动(真 PTY)", 0xce9178, 0x1e1e1e);
        g.put_str(13, 1, "宽字符测试:中文 日本語 한국어 ← 各占两列", 0xd4d4d4, 0x1e1e1e);
        g.put_str(14, 1, "abcdefghijklmnopqrstuvwxyz 0123456789", 0x808080, 0x1e1e1e);
        g
    }
}

/// 持有网格数据的 View。
pub struct GridView {
    grid: StaticGrid,
}

impl GridView {
    pub fn new(grid: StaticGrid) -> Self {
        Self { grid }
    }

    /// 产出一个占满可用空间、在 paint 回调里手绘网格的 canvas 元素。
    pub fn canvas_element(&self) -> impl IntoElement {
        let cells = self.grid.cells.clone();
        let rows = self.grid.rows;
        let cols = self.grid.cols;

        canvas(
            move |_bounds, _window, _cx| (),
            move |bounds: Bounds<Pixels>, _prepaint, window: &mut Window, cx: &mut App| {
                paint_grid(&cells, rows, cols, bounds, window, cx);
            },
        )
        .size_full()
    }
}

/// 逐格绘制:按列索引 × 单元宽度定位。双宽字符在起始列画一次,横跨两格;
/// 续格(width==0)只画背景不画字符。
fn paint_grid(
    cells: &[Cell],
    rows: usize,
    cols: usize,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let font_size = px(15.0);
    let line_height = px(20.0);

    // 单个西文 cell 的宽度:量一个半角字符。双宽字符 = 2 倍此宽。
    let probe = window
        .text_system()
        .shape_line("0".into(), font_size, &[monochrome_run(1, 0xd4d4d4)], None);
    let cell_w: Pixels = if probe.width > px(0.0) {
        probe.width
    } else {
        px(9.0)
    };

    let origin = bounds.origin;

    for row in 0..rows {
        let y = origin.y + line_height * (row as f32);
        if y > origin.y + bounds.size.height {
            break;
        }
        for col in 0..cols {
            let cell = cells[row * cols + col];
            let x = origin.x + cell_w * (col as f32);

            // 续格:不画字符(背景已在起始列随双宽字符处理时一并覆盖)。
            if cell.width == 0 {
                // 续格背景:若非默认底色,补画一格背景保证连贯。
                if cell.bg != 0x1e1e1e {
                    let b = Bounds {
                        origin: point(x, y),
                        size: size(cell_w, line_height),
                    };
                    window.paint_quad(fill(b, rgb(cell.bg)));
                }
                continue;
            }

            // 背景 quad:双宽字符覆盖 2 格宽。
            if cell.bg != 0x1e1e1e {
                let span = cell_w * (cell.width as f32);
                let b = Bounds {
                    origin: point(x, y),
                    size: size(span, line_height),
                };
                window.paint_quad(fill(b, rgb(cell.bg)));
            }

            // 字符(空格跳过)。
            if cell.ch != ' ' {
                let text: SharedString = cell.ch.to_string().into();
                let run = monochrome_run(text.len(), cell.fg);
                let shaped = window
                    .text_system()
                    .shape_line(text, font_size, &[run], None);
                let _ = shaped.paint(point(x, y), line_height, window, cx);
            }
        }
    }
}

/// 造一个单色 TextRun(继承默认字体,指定前景色)。
fn monochrome_run(len: usize, fg: u32) -> TextRun {
    TextRun {
        len,
        font: gpui::font("monospace"),
        color: rgb(fg).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_grid_has_expected_dimensions() {
        let g = StaticGrid::demo();
        assert_eq!(g.cells.len(), g.rows * g.cols);
    }

    #[test]
    fn put_str_truncates_at_col_width() {
        let mut g = StaticGrid::new(1, 3);
        g.put_str(0, 0, "abcdef", 0xffffff, 0x000000);
        assert_eq!(g.at(0, 0).ch, 'a');
        assert_eq!(g.at(0, 2).ch, 'c');
    }

    #[test]
    fn blank_cells_are_spaces() {
        let g = StaticGrid::new(2, 2);
        assert_eq!(g.at(0, 0).ch, ' ');
        assert_eq!(g.at(0, 0).width, 1);
    }

    #[test]
    fn wide_char_occupies_two_columns() {
        let mut g = StaticGrid::new(1, 6);
        // "中a":中(宽2)在 col0,续格在 col1,a 在 col2。
        g.put_str(0, 0, "中a", 0xffffff, 0x000000);
        assert_eq!(g.at(0, 0).ch, '中');
        assert_eq!(g.at(0, 0).width, 2);
        assert_eq!(g.at(0, 1).width, 0); // 续格
        assert_eq!(g.at(0, 2).ch, 'a');
        assert_eq!(g.at(0, 2).width, 1);
    }

    #[test]
    fn wide_char_not_split_at_boundary() {
        // 宽度只剩 1 列时,双宽字符不能只放一半 —— 应整体截断。
        let mut g = StaticGrid::new(1, 3);
        g.put_str(0, 0, "a中", 0xffffff, 0x000000); // a 占 col0,中需 col1+col2 → 放得下
        assert_eq!(g.at(0, 1).ch, '中');

        let mut g2 = StaticGrid::new(1, 2);
        g2.put_str(0, 1, "中", 0xffffff, 0x000000); // 从 col1 起,需 col1+col2,但 cols=2 → 放不下
        assert_eq!(g2.at(0, 1).ch, ' '); // 未写入,保持空白
    }
}
