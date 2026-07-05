//! U8-spike 的核心:自绘 cell 网格。
//!
//! 数据模型 [`StaticGrid`] 刻意做成「字符 + 前景色 + 背景色的二维 cell 数组」——
//! 这正是 wezterm `Screen`/`Line`/`Cell` 在 U6/U8 会提供的信息形状,所以本
//! spike 验证的绘制路线可以原样复用到真实内核上。
//!
//! 绘制走 GPUI 的 `canvas` 元素:在 paint 回调里拿到 `bounds`,按 cell 宽高
//! 逐格 `paint_quad(fill(...))` 画背景,再 `shape_line(...).paint(...)` 画字符。

use gpui::{
    canvas, fill, point, px, rgb, size, App, Bounds, IntoElement, Pixels, SharedString, Styled,
    TextRun, Window,
};

/// 单个终端 cell:一个字符 + 前景色 + 背景色(RGB)。
#[derive(Clone, Copy)]
pub struct Cell {
    pub ch: char,
    pub fg: u32,
    pub bg: u32,
}

impl Cell {
    fn new(ch: char, fg: u32, bg: u32) -> Self {
        Self { ch, fg, bg }
    }
    fn blank() -> Self {
        Self::new(' ', 0xd4d4d4, 0x1e1e1e)
    }
}

/// 一屏静态 cell 网格(rows × cols)。模拟 wezterm `Screen` 的可渲染内容。
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

    #[cfg(test)] // 仅测试用于读取单元格
    fn at(&self, row: usize, col: usize) -> Cell {
        self.cells[row * self.cols + col]
    }

    /// 在某行写一段带色文本(超出列宽截断)。spike 用来铺演示内容。
    fn put_str(&mut self, row: usize, col: usize, s: &str, fg: u32, bg: u32) {
        for (i, ch) in s.chars().enumerate() {
            let c = col + i;
            if c >= self.cols {
                break;
            }
            self.cells[row * self.cols + c] = Cell::new(ch, fg, bg);
        }
    }

    /// 构造一屏演示内容:标题、几行带不同前景/背景色的文本,验证颜色渲染。
    pub fn demo() -> Self {
        let mut g = Self::new(16, 60);
        g.put_str(0, 1, "LucyMind — U8 spike: 自绘终端网格 (wezterm-term 模型)", 0x4ec9b0, 0x1e1e1e);
        g.put_str(2, 1, "$ git worktree add ../proj-worktrees/feature-x", 0xd4d4d4, 0x1e1e1e);
        g.put_str(3, 1, "  Preparing worktree (new branch 'feature-x')", 0x9cdcfe, 0x1e1e1e);
        g.put_str(4, 1, "  HEAD is now at 34e57df scaffold", 0x9cdcfe, 0x1e1e1e);
        g.put_str(6, 1, " OK  post_create hook 完成 ", 0x1e1e1e, 0x4ec9b0);
        g.put_str(7, 1, " WARN  分支已被检出,请换名 ", 0x1e1e1e, 0xdcdcaa);
        g.put_str(8, 1, " ERR  worktree 有未提交改动 ", 0xffffff, 0xf14c4c);
        g.put_str(10, 1, "$ claude", 0xd4d4d4, 0x1e1e1e);
        g.put_str(11, 1, "  ● 会话已在 worktree 目录启动 (真 PTY)", 0xce9178, 0x1e1e1e);
        g.put_str(13, 1, "红 绿 蓝 黄 青 洋红 — 颜色/中文宽字符测试", 0xd4d4d4, 0x1e1e1e);
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
        // 把网格数据 clone 进闭包(spike 数据量小,简单最好;U8 正式版会共享
        // terminal session 的 Entity 而非 clone)。
        let cells = self.grid.cells.clone();
        let rows = self.grid.rows;
        let cols = self.grid.cols;

        canvas(
            // prepaint:此 spike 无需预计算,返回 ()。
            move |_bounds, _window, _cx| (),
            // paint:核心绘制。
            move |bounds: Bounds<Pixels>, _prepaint, window: &mut Window, cx: &mut App| {
                paint_grid(&cells, rows, cols, bounds, window, cx);
            },
        )
        .size_full()
    }
}

/// 逐格绘制:先量 cell 宽高,再画背景 quad + 字符。
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

    // 等宽字体的 cell 宽:shape 一个字符量其宽度。
    let probe = window
        .text_system()
        .shape_line("0".into(), font_size, &[monochrome_run(1, 0xd4d4d4)], None);
    let cell_w: Pixels = if probe.width > px(0.0) {
        probe.width
    } else {
        px(9.0) // 兜底
    };

    let origin = bounds.origin;

    for row in 0..rows {
        let y = origin.y + line_height * (row as f32);
        if y > bounds.origin.y + bounds.size.height {
            break;
        }
        for col in 0..cols {
            let cell = cells[row * cols + col];
            let x = origin.x + cell_w * (col as f32);

            // 1) 背景 quad(仅当背景非默认底色时画,省开销)。
            if cell.bg != 0x1e1e1e {
                let cell_bounds = Bounds {
                    origin: point(x, y),
                    size: size(cell_w, line_height),
                };
                window.paint_quad(fill(cell_bounds, rgb(cell.bg)));
            }

            // 2) 字符(跳过空格,省开销)。
            if cell.ch != ' ' {
                let text: SharedString = cell.ch.to_string().into();
                let run = monochrome_run(text.len(), cell.fg);
                let shaped = window
                    .text_system()
                    .shape_line(text, font_size, &[run], None);
                // 垂直居中一点点:字符基线放在 cell 内。
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
        // 第 4 个字符被截断,不越界 panic。
    }

    #[test]
    fn blank_cells_are_spaces() {
        let g = StaticGrid::new(2, 2);
        assert_eq!(g.at(0, 0).ch, ' ');
    }
}
