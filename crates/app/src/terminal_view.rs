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
    div, fill, point, px, relative, rgb, rgba, size, App, AsyncApp, Bounds, ClipboardItem, Context,
    Element, ElementId, ElementInputHandler, EntityInputHandler, FocusHandle, Focusable,
    GlobalElementId, InteractiveElement, IntoElement, KeyDownEvent, Keystroke, LayoutId,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point, Render,
    ScrollDelta, ScrollWheelEvent, Style, Styled, TextRun, UnderlineStyle, WeakEntity, Window,
};

use lucy_terminal::input::{self, Key, Mods};
use lucy_terminal::{RenderSnapshot, TermDimensions, TermEvent, TerminalSession};

const FONT_SIZE: f32 = 15.0;
const LINE_HEIGHT: f32 = 20.0;
const DEFAULT_BG: u32 = 0x1e_1e_1e;
const SELECTION_BG: u32 = 0x2e_54_88_80; // 半透明蓝,选区高亮

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
            ime_preedit: String::new(),
            ime_marked: None,
            last_bounds: None,
            cell_w: px(9.0),
            line_h: px(LINE_HEIGHT),
        })
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

    fn on_mouse_down(&mut self, ev: &MouseDownEvent, _w: &mut Window, cx: &mut Context<Self>) {
        if ev.button != MouseButton::Left {
            return;
        }
        if let Some(cell) = self.cell_at(ev.position) {
            self.is_selecting = true;
            self.selection = Some((cell, cell));
            cx.notify();
        }
    }

    fn on_mouse_move(&mut self, ev: &MouseMoveEvent, _w: &mut Window, cx: &mut Context<Self>) {
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
        if lines != 0.0 {
            self.session.scroll_lines(lines as i32);
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

        // 回存尺寸/bounds 供鼠标映射。
        self.view.update(cx, |v, _| {
            v.last_bounds = Some(bounds);
            v.cell_w = cell_w;
            v.line_h = line_height;
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
                    rgba(SELECTION_BG),
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

    // 光标块。
    if snap.cursor.visible {
        let x = origin.x + cell_w * (snap.cursor.col as f32);
        let y = origin.y + line_height * (snap.cursor.line as f32);
        let mut c = rgb(0xd4d4d4);
        c.a = 0.5;
        window.paint_quad(fill(
            Bounds {
                origin: point(x, y),
                size: size(cell_w, line_height),
            },
            c,
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
