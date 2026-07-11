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
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point,
    Render, ScrollDelta, ScrollWheelEvent, SharedString, StatefulInteractiveElement, Style, Styled,
    TextRun, UnderlineStyle, WeakEntity, Window,
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
    /// 终端动态标题(OSC 0/2 协议,`\x1b]0;<title>\x07`)。None = 未收到,
    /// tab 栏渲染时回退到静态标题("Shell")。
    title: Option<String>,

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

    // ---- 复制视觉反馈 ----
    /// 复制成功后的剩余闪烁时间(秒);Some = 正在闪烁,None = 无。
    copy_flash: Option<f32>,

    // ---- 右键上下文菜单 ----
    context_menu_open: bool,
    /// 菜单弹出位置(窗口坐标,渲染时转成相对偏移)。
    context_menu_pos: Point<Pixels>,

    // ---- 测试专用:记录所有出现过的标题 ----
    // bash 等交互 shell 的 PROMPT_COMMAND 会在命令执行后覆写标题,
    // 导致 MARKER_TITLE 被后续 prompt 标题覆盖。测试需要检查"是否曾出现"
    // 而非"当前是否"(poll 间隔 20ms 可能错过瞬态值)。
    #[cfg(feature = "test-support")]
    test_titles: Vec<String>,
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
        cx.spawn(
            async move |this: WeakEntity<Self>, cx: &mut AsyncApp| loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let alive = this
                    .update(cx, |view, cx| {
                        let events = view.session.drain_events();
                        let mut dirty = false;
                        for ev in events {
                            match ev {
                                TermEvent::Wakeup | TermEvent::Bell => {
                                    dirty = true;
                                }
                                TermEvent::Title(t) => {
                                    view.title = Some(t);
                                    dirty = true;
                                }
                                TermEvent::ChildExit(code) => {
                                    view.exited = Some(code);
                                    dirty = true;
                                }
                            }
                        }
                        // 复制闪烁倒计时(~300ms)。
                        if let Some(flash) = &mut view.copy_flash {
                            *flash -= 0.016;
                            if *flash <= 0.0 {
                                view.copy_flash = None;
                            }
                            dirty = true;
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
            },
        )
        .detach();

        Ok(Self {
            session,
            focus: cx.focus_handle(),
            snapshot,
            exited: None,
            title: None,
            selection: None,
            is_selecting: false,
            dragging_scrollbar: false,
            ime_preedit: String::new(),
            ime_marked: None,
            last_bounds: None,
            cell_w: px(9.0),
            line_h: px(LINE_HEIGHT),
            copy_flash: None,
            context_menu_open: false,
            context_menu_pos: point(px(0.0), px(0.0)),
            #[cfg(feature = "test-support")]
            test_titles: Vec::new(),
        })
    }

    /// 停掉本终端的 agent/shell 子进程(两段式)。关闭 worktree 前调用。
    pub fn shutdown(&mut self) {
        self.session.shutdown();
    }

    /// 终端动态标题(OSC 0/2 协议)。None = 未收到,调用方应回退到静态标题。
    /// tab 栏渲染用此优先显示动态标题(如 shell 当前目录 / agent 名)。
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// 向 PTY 写入文本字节(供 agent 按钮发命令、快捷键发命令等)。
    /// 内部调 `session.write_input`,不等待 shell 处理。
    pub fn send_text(&self, text: &str) {
        self.session.write_input(text.as_bytes().to_vec());
    }

    // ---------------- 测试 accessor(仅测试构建可见)----------------
    // 集成测试(tests/)需观察内部状态(snapshot/选区/exit/dimensions),但字段私有。
    // 这些 #[cfg(test)] pub fn 不进生产二进制,API 表面不膨胀。

    /// 当前可渲染快照的文本(按行拼接,width==0 的宽字符占位跳过)。
    ///
    /// 测试时直接从 live `session.snapshot()` 读取,而非缓存的 `self.snapshot`。
    /// 缓存仅由 16ms 轮询循环更新(GPUI timer),TestAppContext 的 mock 时钟不推进,
    /// 轮询循环不会触发;`maybe_resize`(paint 时调用)仅在尺寸变化时刷新缓存。
    /// 读 live snapshot 确保 PTY reader 线程(OS 线程,不受 mock 时钟影响)
    /// 写入 Term 的内容能被测试立即读到。
    #[cfg(feature = "test-support")]
    pub fn snapshot_text(&self) -> String {
        let snap = self.session.snapshot();
        let mut s = String::new();
        for line in 0..snap.rows {
            for col in 0..snap.cols {
                let cell = snap.cell(line, col);
                if cell.width != 0 {
                    s.push(cell.ch);
                }
            }
            s.push('\n');
        }
        s
    }

    /// 手动排空 PTY 事件队列(模拟 16ms 轮询循环)。
    ///
    /// TestAppContext 的 mock 时钟不推进 `background_executor().timer(16ms)`,
    /// 轮询循环不会触发。测试需要手动调此方法排空 `drain_events()` 返回的事件
    /// (Wakeup / Title / ChildExit),更新 `title` / `exited` / `snapshot`。
    #[cfg(feature = "test-support")]
    pub fn poll_events_for_test(&mut self) {
        let events = self.session.drain_events();
        for ev in events {
            match ev {
                TermEvent::Wakeup | TermEvent::Bell => {
                    self.snapshot = self.session.snapshot();
                }
                TermEvent::Title(t) => {
                    self.title = Some(t.clone());
                    #[cfg(feature = "test-support")]
                    self.test_titles.push(t);
                    self.snapshot = self.session.snapshot();
                }
                TermEvent::ChildExit(code) => {
                    self.exited = Some(code);
                    self.snapshot = self.session.snapshot();
                }
            }
        }
    }

    /// 子进程是否已退出(及退出码)。测试断言 agent/shell 结束。
    #[cfg(feature = "test-support")]
    pub fn is_exited(&self) -> Option<i32> {
        self.exited
    }

    /// 检查是否曾出现包含 `needle` 的标题(即使后续被 shell prompt 覆盖)。
    ///
    /// bash 等交互 shell 的 PROMPT_COMMAND 会在命令后覆写标题,导致
    /// `title()` 只能看到最后一次(通常是 prompt 标题,非测试标记)。
    /// 此方法检查 `poll_events_for_test` 记录的全部标题历史。
    #[cfg(feature = "test-support")]
    pub fn title_seen_for_test(&self, needle: &str) -> bool {
        self.test_titles.iter().any(|t| t.contains(needle))
    }

    /// 当前选区文本(已 trim 尾随空格、规范化顺序)。无选区返回 None。
    #[cfg(feature = "test-support")]
    pub fn selection_text(&self) -> Option<String> {
        self.selected_text()
    }

    /// 是否有非空选区(start != end)。
    #[cfg(feature = "test-support")]
    pub fn has_selection(&self) -> bool {
        self.selection.map(|(a, b)| a != b).unwrap_or(false)
    }

    /// 当前 PTY 尺寸(列 × 行)。测试断言 resize 生效。
    #[cfg(feature = "test-support")]
    pub fn dimensions(&self) -> (usize, usize) {
        let d = self.session.dimensions();
        (d.columns, d.screen_lines)
    }

    /// 若行列数变化则 resize PTY + Term(paint 时按 bounds 调用)。
    /// cell 像素尺寸一并传给内核用于向终端程序报告像素尺寸。
    fn maybe_resize(
        &mut self,
        cols: usize,
        rows: usize,
        cell_w: gpui::Pixels,
        line_h: gpui::Pixels,
    ) {
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
        let ks = &event.keystroke;

        // Esc 关闭上下文菜单(优先于 IME 判断)。
        if self.context_menu_open && ks.key == "escape" {
            self.context_menu_open = false;
            cx.notify();
            return;
        }

        // IME 组合中的按键交给 InputHandler 处理,这里不重复送。
        if !self.ime_preedit.is_empty() {
            return;
        }

        // 全选:Cmd+A(macOS)或 Ctrl+Shift+A(其他平台)。
        // Ctrl+A 无 Shift 不拦截——终端程序用 readline 行首。
        let select_all_combo = (ks.modifiers.platform && ks.key == "a")
            || (ks.modifiers.control && ks.modifiers.shift && ks.key == "a");
        if select_all_combo {
            let rows = self.snapshot.rows;
            let cols = self.snapshot.cols;
            if rows > 0 && cols > 0 {
                self.selection = Some((
                    CellPos { row: 0, col: 0 },
                    CellPos {
                        row: rows - 1,
                        col: cols,
                    },
                ));
                cx.notify();
            }
            return;
        }

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
            "enter"
                | "return"
                | "tab"
                | "escape"
                | "backspace"
                | "delete"
                | "up"
                | "down"
                | "left"
                | "right"
                | "home"
                | "end"
                | "pageup"
                | "pagedown"
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
                // 复制视觉反馈:选区短暂闪烁。
                self.copy_flash = Some(0.3);
                cx.notify();
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
    /// 每行尾随空格被 trim,消除「复制一行粘了 70 个空格」的问题。
    fn selected_text(&self) -> Option<String> {
        let (a, b) = self.selection?;
        let (start, end) = order(a, b);
        let snap = &self.snapshot;
        let mut out = String::new();
        for row in start.row..=end.row.min(snap.rows.saturating_sub(1)) {
            let col_start = if row == start.row { start.col } else { 0 };
            let col_end = if row == end.row { end.col } else { snap.cols };
            let mut line = String::new();
            for col in col_start..col_end.min(snap.cols) {
                let cell = snap.cell(row, col);
                if cell.width != 0 {
                    line.push(cell.ch);
                }
            }
            out.push_str(line.trim_end());
            if row != end.row {
                out.push('\n');
            }
        }
        Some(out)
    }

    /// 词边界判定:非字母数字且非下划线 = 边界。
    /// 用于双击选词:连续的字母数字/下划线构成一个「词」。
    fn word_boundary(ch: char) -> bool {
        !(ch.is_alphanumeric() || ch == '_')
    }

    /// 双击选词:从点击 cell 向左右扩展到词边界,返回选区。
    /// 点击落在非词字符(空格/标点)上时返回 None。
    fn select_word_at(&self, pos: CellPos) -> Option<(CellPos, CellPos)> {
        let snap = &self.snapshot;
        if pos.row >= snap.rows {
            return None;
        }
        let cell = snap.cell(pos.row, pos.col);
        if Self::word_boundary(cell.ch) {
            return None;
        }
        // 向左扩展到第一个词字符。
        let mut start_col = pos.col;
        while start_col > 0 && !Self::word_boundary(snap.cell(pos.row, start_col - 1).ch) {
            start_col -= 1;
        }
        // 向右扩展:end_col 是 exclusive(词后第一个边界位置)。
        let mut end_col = pos.col + 1;
        while end_col < snap.cols && !Self::word_boundary(snap.cell(pos.row, end_col).ch) {
            end_col += 1;
        }
        Some((
            CellPos {
                row: pos.row,
                col: start_col,
            },
            CellPos {
                row: pos.row,
                col: end_col,
            },
        ))
    }

    /// 三击选行:col 0 到最后一个非空格 cell,整行空白返回 None。
    fn select_line_at(&self, pos: CellPos) -> Option<(CellPos, CellPos)> {
        let snap = &self.snapshot;
        if pos.row >= snap.rows {
            return None;
        }
        // 从右往左找第一个有内容的 cell。
        let mut end_col = snap.cols;
        while end_col > 0 {
            let cell = snap.cell(pos.row, end_col - 1);
            if cell.width != 0 && cell.ch != ' ' {
                break;
            }
            end_col -= 1;
        }
        if end_col == 0 {
            return None;
        }
        Some((
            CellPos {
                row: pos.row,
                col: 0,
            },
            CellPos {
                row: pos.row,
                col: end_col,
            },
        ))
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
        // 右键:打开上下文菜单。
        if ev.button == MouseButton::Right {
            self.context_menu_open = true;
            self.context_menu_pos = ev.position;
            cx.notify();
            return;
        }

        if ev.button != MouseButton::Left {
            return;
        }

        // 菜单打开时,左键先关闭菜单(不触发选区/其他操作)。
        if self.context_menu_open {
            self.context_menu_open = false;
            cx.notify();
            return;
        }

        // 滚动条区域:进入拖动模式,直接跳到点击位置。
        if self.in_scrollbar(ev.position) {
            self.dragging_scrollbar = true;
            self.scroll_to_mouse_y(ev.position, cx);
            return;
        }

        let Some(cell) = self.cell_at(ev.position) else {
            return;
        };

        match ev.click_count {
            2 => {
                // 双击选词:选中词并复制。不进入拖动模式。
                if let Some(sel) = self.select_word_at(cell) {
                    self.selection = Some(sel);
                    self.copy_selection(cx);
                }
                // 双击落在非词字符上:不清除已有选区(避免误清)。
            }
            3 => {
                // 三击选行:选中整行并复制。
                if let Some(sel) = self.select_line_at(cell) {
                    self.selection = Some(sel);
                    self.copy_selection(cx);
                }
            }
            _ => {
                // 单击。
                if ev.modifiers.shift {
                    // Shift+点击:扩展选区(起点不变,终点移到点击位置)。
                    if let Some((start, _)) = self.selection {
                        self.selection = Some((start, cell));
                        self.copy_selection(cx);
                    } else {
                        // 无选区:当作普通点击开始新选区。
                        self.is_selecting = true;
                        self.selection = Some((cell, cell));
                    }
                } else {
                    // 普通点击:开始新选区。
                    self.is_selecting = true;
                    self.selection = Some((cell, cell));
                }
            }
        }
        cx.notify();
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

    fn on_mouse_up(&mut self, _ev: &MouseUpEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let was_selecting = self.is_selecting;
        self.is_selecting = false;
        self.dragging_scrollbar = false;

        // copy-on-select:拖选释放时若选区非空,自动复制。
        // IME 组合中不触发(避免干扰预编辑)。
        if was_selecting && self.ime_preedit.is_empty() {
            let sel = self.selection;
            if let Some((a, b)) = sel {
                if a != b {
                    self.copy_selection(cx);
                } else {
                    // 单击(起点==终点):清除选区。
                    self.selection = None;
                    cx.notify();
                }
            }
        }
    }

    fn on_scroll(&mut self, ev: &ScrollWheelEvent, window: &mut Window, cx: &mut Context<Self>) {
        let lh = f32::from(self.line_h);
        let lines = match ev.delta {
            ScrollDelta::Lines(p) => p.y,
            ScrollDelta::Pixels(p) => {
                if lh > 0.0 {
                    f32::from(p.y) / lh
                } else {
                    f32::from(p.y) / LINE_HEIGHT
                }
            }
        };
        let n = lines as i32;
        if n == 0 {
            let _ = window;
            return;
        }

        if self.snapshot.alt_screen {
            // 备用屏(claude code/vim 等)无 scrollback —— 把滚轮转成方向键发给
            // 程序,让它自己滚(alternate-scroll 行为)。上滚=↑,下滚=↓。
            let (key, count) = if n > 0 { (Key::Up, n) } else { (Key::Down, -n) };
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

    /// 右键上下文菜单(Copy / Paste / Select All)。半透明遮罩 + 定位卡片。
    /// 无选区时 Copy 项灰显。点遮罩 / Esc / 选中项均关菜单。
    fn context_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let has_selection = self.selection.map(|(a, b)| a != b).unwrap_or(false);

        // 菜单定位:把窗口坐标转成相对终端 div 的偏移。
        let pad = theme::space_sm();
        let (menu_x, menu_y) = match self.last_bounds {
            Some(bounds) => (
                self.context_menu_pos.x - bounds.origin.x + pad,
                self.context_menu_pos.y - bounds.origin.y + pad,
            ),
            None => (px(0.0), px(0.0)),
        };

        // 遮罩:点左键关闭、点右键移动菜单位置。
        let backdrop = div()
            .absolute()
            .inset_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _ev, _w, cx| {
                    this.context_menu_open = false;
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, ev: &MouseDownEvent, _w, cx| {
                    this.context_menu_pos = ev.position;
                    cx.notify();
                    cx.stop_propagation();
                }),
            );

        // Copy 项:无选区时灰显、不可点。
        let copy_item = if has_selection {
            div()
                .id("ctx-copy")
                .px(theme::space_md())
                .py(theme::space_xs())
                .min_w(px(160.0))
                .cursor_pointer()
                .text_color(rgb(theme::TEXT))
                .hover(|s| s.bg(rgb(theme::BTN_BG_HOVER)))
                .child(SharedString::from("Copy"))
                .on_click(cx.listener(|this, _ev, _w, cx| {
                    this.copy_selection(cx);
                    this.context_menu_open = false;
                    cx.notify();
                }))
        } else {
            div()
                .id("ctx-copy-disabled")
                .px(theme::space_md())
                .py(theme::space_xs())
                .min_w(px(160.0))
                .text_color(rgb(theme::TEXT_FAINT))
                .child(SharedString::from("Copy"))
        };

        // Paste 项。
        let paste_item = div()
            .id("ctx-paste")
            .px(theme::space_md())
            .py(theme::space_xs())
            .cursor_pointer()
            .text_color(rgb(theme::TEXT))
            .hover(|s| s.bg(rgb(theme::BTN_BG_HOVER)))
            .child(SharedString::from("Paste"))
            .on_click(cx.listener(|this, _ev, _w, cx| {
                this.paste_clipboard(cx);
                this.context_menu_open = false;
                cx.notify();
            }));

        // Select All 项。
        let select_all_item = div()
            .id("ctx-select-all")
            .px(theme::space_md())
            .py(theme::space_xs())
            .cursor_pointer()
            .text_color(rgb(theme::TEXT))
            .hover(|s| s.bg(rgb(theme::BTN_BG_HOVER)))
            .child(SharedString::from("Select All"))
            .on_click(cx.listener(|this, _ev, _w, cx| {
                let rows = this.snapshot.rows;
                let cols = this.snapshot.cols;
                if rows > 0 && cols > 0 {
                    this.selection = Some((
                        CellPos { row: 0, col: 0 },
                        CellPos {
                            row: rows - 1,
                            col: cols,
                        },
                    ));
                }
                this.context_menu_open = false;
                cx.notify();
            }));

        // 卡片:描边 + 2px 圆角(与 agent_menu overlay 同语言)。
        // stop_propagation 让点卡片内(项间空白)不冒泡到遮罩关菜单。
        let card = div()
            .absolute()
            .left(menu_x)
            .top(menu_y)
            .bg(rgb(theme::SURFACE))
            .border_1()
            .border_color(rgb(theme::BORDER))
            .rounded(theme::radius())
            .py(theme::space_xs())
            .flex()
            .flex_col()
            .child(copy_item)
            .child(paste_item)
            .child(select_all_item)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _ev, _w, cx| {
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|_this, _ev, _w, cx| {
                    cx.stop_propagation();
                }),
            );

        backdrop.child(card)
    }

    fn element(&self, cx: &Context<Self>) -> TerminalElement {
        TerminalElement { view: cx.entity() }
    }
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TerminalView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div()
            .track_focus(&self.focus)
            .key_context("Terminal")
            .on_key_down(cx.listener(Self::on_key))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_scroll_wheel(cx.listener(Self::on_scroll))
            .size_full()
            .relative()
            .bg(rgb(DEFAULT_BG))
            // 左右留白,让终端内容不贴边(element 的 bounds 会随 padding 内缩,
            // 行列计算/鼠标映射/绘制全部基于内缩后的 bounds,自洽无需改坐标)。
            .px(theme::space_sm())
            .child(self.element(cx));

        if self.context_menu_open {
            root = root.child(self.context_menu(cx));
        }

        root
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
                Some(sel) => {
                    Some(utf16_to_utf8(new_text, sel.start)..utf16_to_utf8(new_text, sel.end))
                }
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
        if !is_renderable_area(bounds) {
            self.view.update(cx, |v, _| {
                v.last_bounds = None;
            });
            return;
        }

        // 注册 IME 输入目标(须在 paint 阶段)。
        let focus = self.view.read(cx).focus.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.view.clone()),
            cx,
        );

        // 取快照 + 选区(clone 出来避免借用冲突)。
        let (snap, selection, preedit, copy_flash) = {
            let v = self.view.read(cx);
            (
                v.snapshot.clone(),
                v.selection,
                v.ime_preedit.clone(),
                v.copy_flash.is_some(),
            )
        };

        let font_size = px(FONT_SIZE);
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
        // 行高取字体实际 ascent + descent,而非硬编码常量。
        // 这样 box-drawing 字符(│╮╰╯─)恰好填满行高,垂直线条无缝连接。
        // 硬编码 LINE_HEIGHT(20)> 字体实际高度(~16)会导致行间有空隙,
        // │ 断开。回退:ascent+descent 为 0 时用 FONT_SIZE * 1.25。
        let line_height = {
            let h = probe.ascent + probe.descent;
            if h > px(0.0) {
                h
            } else {
                px(FONT_SIZE * 1.25)
            }
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

        paint_grid(
            &snap,
            selection,
            copy_flash,
            &preedit,
            bounds,
            cell_w,
            line_height,
            window,
            cx,
        );
    }
}

/// 网格 + 选区 + IME 预编辑 的完整绘制。
#[allow(clippy::too_many_arguments)]
fn paint_grid(
    snap: &RenderSnapshot,
    selection: Option<(CellPos, CellPos)>,
    copy_flash: bool,
    preedit: &str,
    bounds: Bounds<Pixels>,
    cell_w: Pixels,
    line_height: Pixels,
    window: &mut Window,
    cx: &mut App,
) {
    if !is_renderable_area(bounds) || cell_w <= px(0.0) || line_height <= px(0.0) {
        return;
    }

    let origin = bounds.origin;

    // 选区高亮(先画,压在文字下)。复制时用更亮 alpha。
    let sel_alpha = if copy_flash {
        0.9
    } else {
        theme::SELECTION_ALPHA
    };
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
                    theme::with_alpha(theme::SELECTION, sel_alpha),
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

        // 文字:batch 相同样式的连续 cell 成一个 run,一次性 shaping。
        // 逐 cell 单独 shape_line 会导致 box-drawing 字符(╮│╰╯)间距不连续,
        // 因为每个字符的 advance 可能与 cell_w 有亚像素差。batch 后由字体引擎
        // 处理字符间距,box-drawing 线条才能无缝连接(照 Zed/alacritty 的做法)。
        let mut c = 0;
        while c < snap.cols {
            let cell = snap.cell(line, c);
            if cell.width == 0 || cell.ch == ' ' {
                c += 1;
                continue;
            }
            let start = c;
            let fg = cell.fg;
            let bold = cell.bold;
            // 收集相同样式(fg + bold)的连续 cell。
            let mut s = String::new();
            while c < snap.cols {
                let cell = snap.cell(line, c);
                if cell.width == 0 || cell.ch == ' ' {
                    break;
                }
                if cell.fg != fg || cell.bold != bold {
                    break;
                }
                s.push(cell.ch);
                c += 1;
            }
            let x = origin.x + cell_w * (start as f32);
            let run = run_for(s.len(), fg, bold);
            let shaped = window
                .text_system()
                .shape_line(s.into(), font_size_px(), &[run], None);
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
        let shaped = window.text_system().shape_line(
            preedit.to_string().into(),
            font_size_px(),
            &[run],
            None,
        );
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
    if !is_renderable_area(bounds) {
        return None;
    }

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

fn is_renderable_area(bounds: Bounds<Pixels>) -> bool {
    bounds.size.width > px(0.0) && bounds.size.height > px(0.0)
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
///
/// 优先 Cascadia Mono(VS Code / Windows Terminal 默认终端字体,box-drawing
/// 字符 `╮│╰╯─` 等专为终端设计、无缝连接),Consolas 作为回退。
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
        "Cascadia Mono"
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

#[cfg(test)]
mod tests {
    use gpui::{point, px, size, Bounds};

    use super::*;

    #[test]
    fn renderable_area_requires_positive_width_and_height() {
        assert!(is_renderable_area(Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(1.0), px(1.0)),
        }));
        assert!(!is_renderable_area(Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(0.0), px(1.0)),
        }));
        assert!(!is_renderable_area(Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(1.0), px(0.0)),
        }));
    }

    #[test]
    fn word_boundary_alphanumeric() {
        assert!(!TerminalView::word_boundary('a'));
        assert!(!TerminalView::word_boundary('Z'));
        assert!(!TerminalView::word_boundary('0'));
        assert!(!TerminalView::word_boundary('_'));
        assert!(!TerminalView::word_boundary('你'));
        assert!(!TerminalView::word_boundary('é'));
    }

    #[test]
    fn word_boundary_punctuation_and_space() {
        assert!(TerminalView::word_boundary(' '));
        assert!(TerminalView::word_boundary('.'));
        assert!(TerminalView::word_boundary('-'));
        assert!(TerminalView::word_boundary('/'));
        assert!(TerminalView::word_boundary('|'));
        assert!(TerminalView::word_boundary('\n'));
    }

    #[test]
    fn trim_end_removes_trailing_spaces() {
        assert_eq!("hello".trim_end(), "hello");
        assert_eq!("hello   ".trim_end(), "hello");
        assert_eq!("  hello  ".trim_end(), "  hello");
        assert_eq!("   ".trim_end(), "");
        assert_eq!("".trim_end(), "");
    }

    #[test]
    fn trim_end_preserves_interior_spaces() {
        assert_eq!("a b c".trim_end(), "a b c");
        assert_eq!("a b c  ".trim_end(), "a b c");
    }
}
