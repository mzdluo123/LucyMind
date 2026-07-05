//! 终端 View:渲染真实 alacritty 会话 + 键盘/IME 输入 + 鼠标框选复制 + 粘贴。
//!
//! 渲染走自定义 [`Element`](TerminalElement)而非 canvas —— 因为 IME 需要在
//! paint 阶段调 `window.handle_input`、并缓存 bounds/cell 尺寸供鼠标坐标映射。
//!
//! 输入分三路:
//! - 普通键 / 功能键:`on_key_down` → 编码字节 → 写回 PTY(见 [`keystroke_to_bytes`])。
//! - IME(中文/日文等预编辑):[`EntityInputHandler`] —— 预编辑串显示在光标处(带
//!   下划线),组合完成(commit)时才把文本送 PTY。
//! - 鼠标:拖选高亮 + Cmd+C 复制;Cmd+V 走剪贴板 → bracketed-paste 写回 PTY。

use std::ops::Range;
use std::path::PathBuf;
use std::time::Duration;

use gpui::{
    div, fill, point, px, relative, rgb, size, App, AsyncApp, Bounds, ClipboardItem, Context,
    Element, ElementId, ElementInputHandler, EntityInputHandler, FocusHandle, Focusable,
    GlobalElementId, InteractiveElement, IntoElement, KeyDownEvent, Keystroke, LayoutId,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point, Render,
    ScrollDelta, ScrollWheelEvent, Style, Styled, TextRun, UnderlineStyle, WeakEntity, Window,
};

use lucy_terminal::input::{self, Key, Mods};
use lucy_terminal::{RenderSnapshot, TermDimensions, TermEvent, TerminalSession};

use crate::theme;

const FONT_SIZE: f32 = 14.0;
const LINE_HEIGHT: f32 = 20.0;
/// 终端默认底色 —— 与主题主背景一致(near-black),终端字色仍来自 alacritty 调色板。
const DEFAULT_BG: u32 = theme::BG;

/// 网格坐标(视口行、列)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CellPos {
    row: usize,
    col: usize,
}

/// 一个渲染真实终端会话的 GPUI View。
pub struct TerminalView {
    session: TerminalSession,
    focus: FocusHandle,
    /// 最近一次快照(每帧从 session 取新的)。
    snapshot: RenderSnapshot,
    exited: Option<i32>,

    // ---- 鼠标框选状态 ----
    /// 选区(起点 cell, 终点 cell);None = 无选区。终点随拖动更新。
    selection: Option<(CellPos, CellPos)>,
    is_selecting: bool,
    /// 正在拖动滚动条滑块。
    dragging_scrollbar: bool,

    // ---- IME 预编辑状态 ----
    /// 预编辑串(组合中的拼音/假名);为空表示未在组合。commit 后清空并送 PTY。
    ime_preedit: String,
    /// 预编辑串内的选中范围(UTF-8 字节),用于平台绘制候选定位。
    ime_marked: Option<Range<usize>>,

    // ---- paint 阶段缓存,供鼠标坐标映射 ----
    last_bounds: Option<Bounds<Pixels>>,
    cell_w: Pixels,
    line_h: Pixels,
}

impl TerminalView {
    pub fn new(
        cx: &mut Context<Self>,
        working_directory: Option<PathBuf>,
        command: Option<(String, Vec<String>)>,
        env: Vec<(String, String)>,
    ) -> anyhow::Result<Self> {
        let dims = TermDimensions::new(80, 24, 8, 16);
        let session = TerminalSession::spawn(dims, working_directory, command, env)?;
        let snapshot = session.snapshot();

        // 后台轮询:每 ~16ms drain 事件 + 刷新快照 + notify 重绘。
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let alive = this
                    .update(cx, |view, cx| {
                        let events = view.session.drain_events();
                        let mut dirty = false;
                        for ev in events {
                            match ev {
                                TermEvent::Wakeup | TermEvent::Title(_) | TermEvent::Bell => {
                                    dirty = true;
                                }
                                TermEvent::ChildExit(code) => {
                                    view.exited = Some(code);
                                    dirty = true;
                                }
                            }
                        }
                        if dirty {
                            view.snapshot = view.session.snapshot();
                            cx.notify();
                        }
                    })
                    .is_ok();
                if !alive {
                    break;
                }
            }
        })
        .detach();

        Ok(Self {
            session,
            focus: cx.focus_handle(),
            snapshot,
            exited: None,
            selection: None,
            is_selecting: false,
            dragging_scrollbar: false,
            ime_preedit: String::new(),
            ime_marked: None,
            last_bounds: None,
            cell_w: px(9.0),
            line_h: px(LINE_HEIGHT),
        })
    }

    /// 停掉本终端的 agent/shell 子进程(两段式)。关闭 worktree 前调用。
    pub fn shutdown(&mut self) {
        self.session.shutdown();
    }

    /// 若行列数变化则 resize PTY + Term(paint 时按 bounds 调用)。
    /// cell 像素尺寸一并传给内核用于向终端程序报告像素尺寸。
    fn maybe_resize(&mut self, cols: usize, rows: usize, cell_w: gpui::Pixels, line_h: gpui::Pixels) {
        if cols == 0 || rows == 0 {
            return;
        }
        let cur = self.session.dimensions();
        if cur.columns == cols && cur.screen_lines == rows {
            return; // 未变,避免每帧都 resize
        }
        let dims = TermDimensions::new(
            cols,
            rows,
            f32::from(cell_w) as u16,
            f32::from(line_h) as u16,
        );
        self.session.resize(dims);
        // resize 后立刻刷新一次快照,避免旧尺寸残影。
        self.snapshot = self.session.snapshot();
    }

    // ---------------- 键盘 ----------------

    fn on_key(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        // IME 组合中的按键交给 InputHandler 处理,这里不重复送。
        if !self.ime_preedit.is_empty() {
            return;
        }

        let ks = &event.keystroke;

        // Cmd+C / Cmd+V(macOS)或 Ctrl+Shift+C/V(其他平台习惯)→ 复制/粘贴。
        let copy_combo = (ks.modifiers.platform && ks.key == "c")
            || (ks.modifiers.control && ks.modifiers.shift && ks.key == "c");
        let paste_combo = (ks.modifiers.platform && ks.key == "v")
            || (ks.modifiers.control && ks.modifiers.shift && ks.key == "v");
        if copy_combo {
            self.copy_selection(cx);
            return;
        }
        if paste_combo {
            self.paste_clipboard(cx);
            return;
        }

        // 这些控制键即便带 key_char 也必须由 on_key 编码(IME 不送控制码)。
        let is_control_key = matches!(
            ks.key.as_str(),
            "enter" | "return" | "tab" | "escape" | "backspace" | "delete"
                | "up" | "down" | "left" | "right" | "home" | "end"
                | "pageup" | "pagedown"
        );

        // 关键:可打印字符(key_char 有值、无 ctrl/alt/cmd、且不是控制键)由
        // EntityInputHandler 的 replace_text_in_range 负责送 PTY —— 这里**不能**
        // 再送,否则每个字符被送两次(on_key_down + IME commit),表现为"输入一个
        // 出来两个"。on_key 只处理 InputHandler 不碰的:功能键/方向键/Ctrl 组合/Enter。
        let is_printable = !is_control_key
            && ks.key_char.as_deref().is_some_and(|s| !s.is_empty())
            && !ks.modifiers.control
            && !ks.modifiers.alt
            && !ks.modifiers.platform;
        if is_printable {
            return; // 交给 IME commit 路径
        }

        if let Some(bytes) = keystroke_to_bytes(ks) {
            self.session.write_input(bytes);
        }
    }

    // ---------------- 剪贴板 ----------------

    fn copy_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_text() {
            if !text.is_empty() {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
        }
    }

    fn paste_clipboard(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
            // 终端粘贴:尊重 bracketed-paste(这里恒用 true;更严谨可查 term mode)。
            let bytes = input::encode_paste(&text, true);
            self.session.write_input(bytes);
        }
    }

    /// 从快照按选区抽出文本(按行拼接,规范化选区顺序)。
    fn selected_text(&self) -> Option<String> {
        let (a, b) = self.selection?;
        let (start, end) = order(a, b);
        let snap = &self.snapshot;
        let mut out = String::new();
        for row in start.row..=end.row.min(snap.rows.saturating_sub(1)) {
            let col_start = if row == start.row { start.col } else { 0 };
            let col_end = if row == end.row { end.col } else { snap.cols };
            for col in col_start..col_end.min(snap.cols) {
                let cell = snap.cell(row, col);
                if cell.width != 0 {
                    out.push(cell.ch);
                }
            }
            if row != end.row {
                out.push('\n');
            }
        }
        Some(out)
    }

    // ---------------- 鼠标 ----------------

    /// 像素坐标 → cell(减去 bounds.origin,按 cell 宽高取整)。
    fn cell_at(&self, pos: Point<Pixels>) -> Option<CellPos> {
        let bounds = self.last_bounds?;
        if self.cell_w <= px(0.0) || self.line_h <= px(0.0) {
            return None;
        }
        let rel_x = (pos.x - bounds.origin.x).max(px(0.0));
        let rel_y = (pos.y - bounds.origin.y).max(px(0.0));
        let col = (f32::from(rel_x) / f32::from(self.cell_w)) as usize;
        let row = (f32::from(rel_y) / f32::from(self.line_h)) as usize;
        Some(CellPos {
            row: row.min(self.snapshot.rows.saturating_sub(1)),
            col: col.min(self.snapshot.cols),
        })
    }

    /// 鼠标 X 是否落在右侧滚动条区域(且当前有 scrollback)。
    fn in_scrollbar(&self, pos: Point<Pixels>) -> bool {
        let Some(bounds) = self.last_bounds else {
            return false;
        };
        if self.snapshot.total_lines <= self.snapshot.rows {
            return false; // 无滚动条
        }
        let right = bounds.origin.x + bounds.size.width;
        pos.x >= right - px(SCROLLBAR_HIT_W) && pos.x <= right
    }

    /// 按鼠标 Y 在滚动条轨道上的比例,设置 display_offset(顶=最大,底=0)。
    fn scroll_to_mouse_y(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(bounds) = self.last_bounds else {
            return;
        };
        let total = self.snapshot.total_lines;
        let rows = self.snapshot.rows;
        if total <= rows {
            return;
        }
        let track_h = f32::from(bounds.size.height);
        let rel_y = (f32::from(pos.y - bounds.origin.y)).clamp(0.0, track_h);
        let frac_from_top = if track_h > 0.0 { rel_y / track_h } else { 0.0 };
        let max_off = (total - rows) as i32;
        // 顶部 = max_off,底部 = 0。
        let target = (max_off as f32 * (1.0 - frac_from_top)).round() as i32;
        let cur = self.snapshot.display_offset as i32;
        let delta = target - cur;
        if delta != 0 {
            self.session.scroll_lines(delta);
            self.snapshot = self.session.snapshot();
            cx.notify();
        }
    }

    fn on_mouse_down(&mut self, ev: &MouseDownEvent, _w: &mut Window, cx: &mut Context<Self>) {
        if ev.button != MouseButton::Left {
            return;
        }
        // 滚动条区域:进入拖动模式,直接跳到点击位置。
        if self.in_scrollbar(ev.position) {
            self.dragging_scrollbar = true;
            self.scroll_to_mouse_y(ev.position, cx);
            return;
        }
        if let Some(cell) = self.cell_at(ev.position) {
            self.is_selecting = true;
            self.selection = Some((cell, cell));
            cx.notify();
        }
    }

    fn on_mouse_move(&mut self, ev: &MouseMoveEvent, _w: &mut Window, cx: &mut Context<Self>) {
        if self.dragging_scrollbar {
            self.scroll_to_mouse_y(ev.position, cx);
            return;
        }
        if self.is_selecting {
            if let Some(cell) = self.cell_at(ev.position) {
                if let Some((start, _)) = self.selection {
                    self.selection = Some((start, cell));
                    cx.notify();
                }
            }
        }
    }

    fn on_mouse_up(&mut self, _ev: &MouseUpEvent, _w: &mut Window, _cx: &mut Context<Self>) {
        self.is_selecting = false;
        self.dragging_scrollbar = false;
        // 单击(起点==终点)视为清除选区。
        if let Some((a, b)) = self.selection {
            if a == b {
                self.selection = None;
            }
        }
    }

    fn on_scroll(&mut self, ev: &ScrollWheelEvent, window: &mut Window, cx: &mut Context<Self>) {
        let lines = match ev.delta {
            ScrollDelta::Lines(p) => p.y,
            ScrollDelta::Pixels(p) => f32::from(p.y) / LINE_HEIGHT,
        };
        let n = lines as i32;
        if n == 0 {
            let _ = window;
            return;
        }

        if self.snapshot.alt_screen {
            // 备用屏(claude code/vim 等)无 scrollback —— 把滚轮转成方向键发给
            // 程序,让它自己滚(alternate-scroll 行为)。上滚=↑,下滚=↓。
            let (key, count) = if n > 0 {
                (Key::Up, n)
            } else {
                (Key::Down, -n)
            };
            let mut bytes = Vec::new();
            for _ in 0..count.min(10) {
                // 单次滚轮最多转发 10 次,避免猛滚刷太多
                bytes.extend_from_slice(&input::encode(&key, Mods::default()));
            }
            self.session.write_input(bytes);
        } else {
            // 主屏:滚 scrollback。
            self.session.scroll_lines(n);
            self.snapshot = self.session.snapshot();
            cx.notify();
        }
        let _ = window;
    }

    fn element(&self, cx: &Context<Self>) -> TerminalElement {
        TerminalElement {
            view: cx.entity(),
        }
    }
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TerminalView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus)
            .key_context("Terminal")
            .on_key_down(cx.listener(Self::on_key))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_scroll_wheel(cx.listener(Self::on_scroll))
            .size_full()
            .bg(rgb(DEFAULT_BG))
            // 左右留白,让终端内容不贴边(element 的 bounds 会随 padding 内缩,
            // 行列计算/鼠标映射/绘制全部基于内缩后的 bounds,自洽无需改坐标)。
            .px(theme::space_sm())
            .child(self.element(cx))
    }
}

// ---------------------------------------------------------------------------
// IME:EntityInputHandler。终端语义 —— 预编辑串是临时叠加(不进 PTY),
// commit(replace_text_in_range)时才把文本送 PTY。range 全走 UTF-16。
// ---------------------------------------------------------------------------

impl EntityInputHandler for TerminalView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _w: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        // 终端不暴露历史文本给 IME;仅返回当前预编辑串的对应片段。
        let s = &self.ime_preedit;
        let start = utf16_to_utf8(s, range.start);
        let end = utf16_to_utf8(s, range.end);
        *adjusted = Some(range);
        Some(s.get(start..end)?.to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled: bool,
        _w: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<gpui::UTF16Selection> {
        // 光标位置:预编辑串末尾。
        let end = utf8_to_utf16(&self.ime_preedit, self.ime_preedit.len());
        Some(gpui::UTF16Selection {
            range: end..end,
            reversed: false,
        })
    }

    fn marked_text_range(&self, _w: &mut Window, _cx: &mut Context<Self>) -> Option<Range<usize>> {
        let m = self.ime_marked.as_ref()?;
        Some(utf8_to_utf16(&self.ime_preedit, m.start)..utf8_to_utf16(&self.ime_preedit, m.end))
    }

    fn unmark_text(&mut self, _w: &mut Window, cx: &mut Context<Self>) {
        self.ime_preedit.clear();
        self.ime_marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // commit:把最终文本送 PTY,清预编辑。
        if !text.is_empty() {
            let mut buf = [0u8; 4];
            let mut bytes = Vec::new();
            for ch in text.chars() {
                bytes.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            }
            self.session.write_input(bytes);
        }
        self.ime_preedit.clear();
        self.ime_marked = None;
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        new_text: &str,
        new_selected: Option<Range<usize>>,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // 预编辑更新:存起来显示在光标处,不送 PTY。
        self.ime_preedit = new_text.to_string();
        self.ime_marked = if new_text.is_empty() {
            None
        } else {
            // new_selected 是 UTF-16 相对新文本;转 UTF-8 存。
            match new_selected {
                Some(sel) => Some(
                    utf16_to_utf8(new_text, sel.start)..utf16_to_utf8(new_text, sel.end),
                ),
                None => Some(0..new_text.len()),
            }
        };
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _w: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        // 候选窗定位到光标格。
        let cur = self.snapshot.cursor;
        let x = element_bounds.origin.x + self.cell_w * (cur.col as f32);
        let y = element_bounds.origin.y + self.line_h * (cur.line as f32);
        Some(Bounds {
            origin: point(x, y),
            size: size(self.cell_w, self.line_h),
        })
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _w: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

// ---------------------------------------------------------------------------
// 自定义 Element:绘制网格 + 选区高亮 + IME 预编辑 + 注册 handle_input。
// ---------------------------------------------------------------------------

struct TerminalElement {
    view: gpui::Entity<TerminalView>,
}

impl IntoElement for TerminalElement {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector: Option<&gpui::InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _layout: &mut (),
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _layout: &mut (),
        _prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        // 注册 IME 输入目标(须在 paint 阶段)。
        let focus = self.view.read(cx).focus.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.view.clone()),
            cx,
        );

        // 取快照 + 选区(clone 出来避免借用冲突)。
        let (snap, selection, preedit) = {
            let v = self.view.read(cx);
            (v.snapshot.clone(), v.selection, v.ime_preedit.clone())
        };

        let font_size = px(FONT_SIZE);
        let line_height = px(LINE_HEIGHT);
        let probe = window.text_system().shape_line(
            "0".into(),
            font_size,
            &[run_for(1, 0xd4d4d4, false)],
            None,
        );
        let cell_w = if probe.width > px(0.0) {
            probe.width
        } else {
            px(9.0)
        };

        // 按 bounds + cell 尺寸算出行列数,若变化则 resize PTY(让 claude/codex
        // 收到 SIGWINCH 重新排版)。这是终端跟随窗口 resize 的关键。
        let cols = (f32::from(bounds.size.width) / f32::from(cell_w)).floor() as usize;
        let rows = (f32::from(bounds.size.height) / f32::from(line_height)).floor() as usize;

        // 回存尺寸/bounds 供鼠标映射 + 按需 resize。
        self.view.update(cx, |v, _| {
            v.last_bounds = Some(bounds);
            v.cell_w = cell_w;
            v.line_h = line_height;
            v.maybe_resize(cols, rows, cell_w, line_height);
        });

        paint_grid(&snap, selection, &preedit, bounds, cell_w, line_height, window, cx);
    }
}

/// 网格 + 选区 + IME 预编辑 的完整绘制。
#[allow(clippy::too_many_arguments)]
fn paint_grid(
    snap: &RenderSnapshot,
    selection: Option<(CellPos, CellPos)>,
    preedit: &str,
    bounds: Bounds<Pixels>,
    cell_w: Pixels,
    line_height: Pixels,
    window: &mut Window,
    cx: &mut App,
) {
    let origin = bounds.origin;

    // 选区高亮(先画,压在文字下)。
    if let Some((a, b)) = selection {
        let (start, end) = order(a, b);
        for row in start.row..=end.row.min(snap.rows.saturating_sub(1)) {
            let c0 = if row == start.row { start.col } else { 0 };
            let c1 = if row == end.row { end.col } else { snap.cols };
            let c1 = c1.min(snap.cols);
            if c1 > c0 {
                let x = origin.x + cell_w * (c0 as f32);
                let y = origin.y + line_height * (row as f32);
                let w = cell_w * ((c1 - c0) as f32);
                window.paint_quad(fill(
                    Bounds {
                        origin: point(x, y),
                        size: size(w, line_height),
                    },
                    theme::with_alpha(theme::SELECTION, theme::SELECTION_ALPHA),
                ));
            }
        }
    }

    // 逐行:背景合并 + 逐 cell 文字。
    for line in 0..snap.rows {
        let y = origin.y + line_height * (line as f32);

        // 背景合并。
        let mut c = 0;
        while c < snap.cols {
            let bg = snap.cell(line, c).bg;
            if bg == DEFAULT_BG {
                c += 1;
                continue;
            }
            let start = c;
            while c < snap.cols && snap.cell(line, c).bg == bg {
                c += 1;
            }
            let x = origin.x + cell_w * (start as f32);
            let w = cell_w * ((c - start) as f32);
            window.paint_quad(fill(
                Bounds {
                    origin: point(x, y),
                    size: size(w, line_height),
                },
                rgb(bg),
            ));
        }

        // 文字。
        for col in 0..snap.cols {
            let cell = snap.cell(line, col);
            if cell.width == 0 || cell.ch == ' ' {
                continue;
            }
            let x = origin.x + cell_w * (col as f32);
            let mut buf = [0u8; 4];
            let s = cell.ch.encode_utf8(&mut buf);
            let run = run_for(s.len(), cell.fg, cell.bold);
            let shaped =
                window
                    .text_system()
                    .shape_line(s.to_string().into(), font_size_px(), &[run], None);
            let _ = shaped.paint(point(x, y), line_height, window, cx);
        }
    }

    // 光标块(冷白半透明)。
    if snap.cursor.visible {
        let x = origin.x + cell_w * (snap.cursor.col as f32);
        let y = origin.y + line_height * (snap.cursor.line as f32);
        window.paint_quad(fill(
            Bounds {
                origin: point(x, y),
                size: size(cell_w, line_height),
            },
            theme::with_alpha(theme::CURSOR, theme::CURSOR_ALPHA),
        ));
    }

    // IME 预编辑:画在光标处,带下划线。
    if !preedit.is_empty() {
        let x = origin.x + cell_w * (snap.cursor.col as f32);
        let y = origin.y + line_height * (snap.cursor.line as f32);
        let run = TextRun {
            len: preedit.len(),
            font: gpui::font(mono_font_family()),
            color: rgb(0xff_ff_ff).into(),
            background_color: Some(rgb(0x33_33_33).into()),
            underline: Some(UnderlineStyle {
                color: Some(rgb(0xff_ff_ff).into()),
                thickness: px(1.0),
                wavy: false,
            }),
            strikethrough: None,
        };
        let shaped =
            window
                .text_system()
                .shape_line(preedit.to_string().into(), font_size_px(), &[run], None);
        let _ = shaped.paint(point(x, y), line_height, window, cx);
    }

    // 滚动条:仅当有 scrollback(总行 > 可视行)才画。冷灰半透明细条,右侧。
    if let Some((track, thumb)) = scrollbar_geometry(snap, bounds) {
        // 轨道(极淡)。
        window.paint_quad(fill(track, theme::with_alpha(theme::BORDER, 0.35)));
        // 滑块(冷灰,悬浮态在 element 层再加深)。
        window.paint_quad(fill(thumb, theme::with_alpha(theme::TEXT_FAINT, 0.9)));
    }
}

/// 滚动条视觉宽度(像素)。
const SCROLLBAR_W: f32 = 8.0;
/// 滚动条命中宽度(比视觉略宽,便于抓取)。
const SCROLLBAR_HIT_W: f32 = 14.0;

/// 计算滚动条轨道与滑块的矩形。无 scrollback 返回 None(不画)。
fn scrollbar_geometry(
    snap: &RenderSnapshot,
    bounds: Bounds<Pixels>,
) -> Option<(Bounds<Pixels>, Bounds<Pixels>)> {
    let rows = snap.rows;
    let total = snap.total_lines;
    if total <= rows {
        return None; // 内容不足一屏,无需滚动条
    }

    let track_x = bounds.origin.x + bounds.size.width - px(SCROLLBAR_W);
    let track = Bounds {
        origin: point(track_x, bounds.origin.y),
        size: size(px(SCROLLBAR_W), bounds.size.height),
    };

    let track_h = f32::from(bounds.size.height);
    // 滑块高度 ∝ 可视/总,设最小高度避免太小点不到。
    let thumb_h = (track_h * rows as f32 / total as f32).max(24.0);
    // 偏移:display_offset=0 在底部,=max 在顶部。滑块位置从上到下。
    let max_off = (total - rows) as f32;
    let frac_from_top = if max_off > 0.0 {
        1.0 - (snap.display_offset as f32 / max_off)
    } else {
        1.0
    };
    let thumb_y = bounds.origin.y + px((track_h - thumb_h) * frac_from_top);
    let thumb = Bounds {
        origin: point(track_x, thumb_y),
        size: size(px(SCROLLBAR_W), px(thumb_h)),
    };
    Some((track, thumb))
}

fn font_size_px() -> Pixels {
    px(FONT_SIZE)
}

/// 规范化选区顺序(保证 start <= end,按行优先)。
fn order(a: CellPos, b: CellPos) -> (CellPos, CellPos) {
    if (a.row, a.col) <= (b.row, b.col) {
        (a, b)
    } else {
        (b, a)
    }
}

/// 平台默认等宽字体名。`"monospace"` 是 CSS 通用族名,macOS CoreText 无对应
/// 真实字体、解析会失败——必须用系统真实字体名。
fn mono_font_family() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Menlo"
    }
    #[cfg(target_os = "linux")]
    {
        "DejaVu Sans Mono"
    }
    #[cfg(target_os = "windows")]
    {
        "Consolas"
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        "monospace"
    }
}

fn run_for(len: usize, fg: u32, bold: bool) -> TextRun {
    let mut font = gpui::font(mono_font_family());
    if bold {
        font = font.bold();
    }
    TextRun {
        len,
        font,
        color: rgb(fg).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

// ---- UTF-8 / UTF-16 偏移互转(IME range 用) ----

fn utf16_to_utf8(s: &str, utf16_off: usize) -> usize {
    let mut u16c = 0;
    let mut u8c = 0;
    for ch in s.chars() {
        if u16c >= utf16_off {
            break;
        }
        u16c += ch.len_utf16();
        u8c += ch.len_utf8();
    }
    u8c.min(s.len())
}

fn utf8_to_utf16(s: &str, utf8_off: usize) -> usize {
    let mut u16c = 0;
    let mut u8c = 0;
    for ch in s.chars() {
        if u8c >= utf8_off {
            break;
        }
        u8c += ch.len_utf8();
        u16c += ch.len_utf16();
    }
    u16c
}

/// 把 GPUI Keystroke 翻译成中性 Key/Mods,再编码成 PTY 字节。
fn keystroke_to_bytes(ks: &Keystroke) -> Option<Vec<u8>> {
    let mods = Mods {
        ctrl: ks.modifiers.control,
        alt: ks.modifiers.alt,
        shift: ks.modifiers.shift,
    };

    let key = match ks.key.as_str() {
        "enter" => Key::Enter,
        "backspace" => Key::Backspace,
        "tab" => Key::Tab,
        "escape" => Key::Escape,
        "up" => Key::Up,
        "down" => Key::Down,
        "right" => Key::Right,
        "left" => Key::Left,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "delete" => Key::Delete,
        "space" => Key::Char(' '),
        other => {
            if let Some(im) = &ks.key_char {
                let mut chars = im.chars();
                if let (Some(c), None) = (chars.next(), chars.clone().next()) {
                    return Some(input::encode(
                        &Key::Char(c),
                        Mods {
                            shift: false,
                            ..mods
                        },
                    ));
                }
                return Some(im.as_bytes().to_vec());
            }
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Key::Char(c),
                _ => return None,
            }
        }
    };
    Some(input::encode(&key, mods))
}
