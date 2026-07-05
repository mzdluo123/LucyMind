//! U8:把 alacritty 终端会话的 [`RenderSnapshot`] 画到 GPUI。
//!
//! 相比 spike(静态自造网格),这里渲染**真实终端**:持有一个
//! [`TerminalSession`],起后台轮询循环 drain 事件并 `cx.notify()` 触发重绘;
//! canvas paint 回调把快照逐格画出来。宽字符从内核层已解决(snapshot 里
//! spacer 已跳过、正身 width=2、颜色已解析成 RGB)。
//!
//! 输入:GPUI 键盘事件 → 中性 `Key`/`Mods`(lucy-terminal input)→ 字节 → 写回 PTY。

use std::path::PathBuf;
use std::time::Duration;

use gpui::{
    canvas, div, fill, point, px, rgb, size, App, AsyncApp, Bounds, Context, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyDownEvent, Keystroke, ParentElement, Pixels, Render,
    Styled, TextRun, WeakEntity, Window,
};

use lucy_terminal::input::{self, Key, Mods};
use lucy_terminal::{RenderSnapshot, TermDimensions, TermEvent, TerminalSession};

const FONT_SIZE: f32 = 15.0;
const LINE_HEIGHT: f32 = 20.0;
const DEFAULT_BG: u32 = 0x1e_1e_1e;

/// 一个渲染真实终端会话的 GPUI View。
pub struct TerminalView {
    session: TerminalSession,
    focus: FocusHandle,
    /// 最近一次快照(每帧从 session 取新的;这里缓存供 paint 用)。
    snapshot: RenderSnapshot,
    exited: Option<i32>,
}

impl TerminalView {
    /// 起一个跑 `command`(None=默认 shell)的终端 View,并开始轮询刷新。
    pub fn new(
        cx: &mut Context<Self>,
        working_directory: Option<PathBuf>,
        command: Option<(String, Vec<String>)>,
        env: Vec<(String, String)>,
    ) -> anyhow::Result<Self> {
        // 初始尺寸:先给个合理默认,首帧 resize 会按窗口实际尺寸校正。
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
                    break; // View 已释放,退出循环
                }
            }
        })
        .detach();

        Ok(Self {
            session,
            focus: cx.focus_handle(),
            snapshot,
            exited: None,
        })
    }

    /// 供窗口初始化时聚焦用。
    pub fn focus_handle_for_init(&self) -> FocusHandle {
        self.focus.clone()
    }

    /// 处理键盘输入:编码成字节写回 PTY。
    fn on_key(&mut self, event: &KeyDownEvent, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(bytes) = keystroke_to_bytes(&event.keystroke) {
            self.session.write_input(bytes);
        }
    }

    /// 产出绘制快照的 canvas 元素。
    fn canvas_element(&self) -> impl IntoElement {
        let snap = self.snapshot.clone();
        canvas(
            move |_bounds, _window, _cx| (),
            move |bounds: Bounds<Pixels>, _prepaint, window: &mut Window, cx: &mut App| {
                paint_snapshot(&snap, bounds, window, cx);
            },
        )
        .size_full()
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
            .size_full()
            .bg(rgb(DEFAULT_BG))
            .child(self.canvas_element())
    }
}

/// 逐格绘制快照:按列索引 × 单元宽度定位;宽字符正身横跨两格,spacer 已被跳过。
fn paint_snapshot(snap: &RenderSnapshot, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    let font_size = px(FONT_SIZE);
    let line_height = px(LINE_HEIGHT);

    // 单个西文 cell 宽:量一个半角字符。
    let probe = window
        .text_system()
        .shape_line("0".into(), font_size, &[run_for(1, 0xd4d4d4, false)], None);
    let cell_w: Pixels = if probe.width > px(0.0) {
        probe.width
    } else {
        px(9.0)
    };

    let origin = bounds.origin;

    // 逐行渲染。每行:
    // 1) 先把所有非默认背景合并成尽量少的 quad(相邻同色合并)。
    // 2) 再把整行文本合并成**一个** shaped line(每个 cell 一段 TextRun),
    //    一次 shape + paint。这让字体在整行内连续排布,不会出现逐字符
    //    shape 造成的间歇性间隙;宽字符占两格由 spacer(width=0)对齐。
    for line in 0..snap.rows {
        let y = origin.y + line_height * (line as f32);

        // ---- 背景 pass:合并相邻同背景色的一段为一个 quad ----
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
            let span = cell_w * ((c - start) as f32);
            let b = Bounds {
                origin: point(x, y),
                size: size(span, line_height),
            };
            window.paint_quad(fill(b, rgb(bg)));
        }

        // ---- 文本 pass:逐 cell 绘制,每个字形钉在 col × cell_w ----
        // 用真实等宽字体(Menlo)后,单字符 shape 的 advance 正确;按列定位
        // 即可对齐网格。宽字符 spacer(width=0)跳过,正身自然占两格。
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
                    .shape_line(s.to_string().into(), font_size, &[run], None);
            let _ = shaped.paint(point(x, y), line_height, window, cx);
        }
    }

    // 光标:画一个前景色实心块(简化版;真实终端还有条形/下划线样式)。
    if snap.cursor.visible {
        let x = origin.x + cell_w * (snap.cursor.col as f32);
        let y = origin.y + line_height * (snap.cursor.line as f32);
        let b = Bounds {
            origin: point(x, y),
            size: size(cell_w, line_height),
        };
        // 半透明前景块,不完全盖住字符。
        let mut c = rgb(0xd4d4d4);
        c.a = 0.5;
        window.paint_quad(fill(b, c));
    }
}

/// 平台默认等宽字体名。**注意**:`"monospace"` 是 CSS 通用族名,
/// macOS CoreText 无对应真实字体、解析会失败——必须用系统真实字体名。
fn mono_font_family() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Menlo" // 系统自带,终端默认等宽字体
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

/// 造一个 TextRun(等宽字体,指定前景色 + 是否粗体)。
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

/// 把 GPUI Keystroke 翻译成中性 Key/Mods,再编码成 PTY 字节。
fn keystroke_to_bytes(ks: &Keystroke) -> Option<Vec<u8>> {
    let mods = Mods {
        ctrl: ks.modifiers.control,
        alt: ks.modifiers.alt,
        shift: ks.modifiers.shift,
    };

    // GPUI 的 keystroke.key 是标准化的键名字符串。
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
            // 优先用 GPUI 提供的已处理输入字符(含 shift 后的大写/符号)。
            if let Some(im) = &ks.key_char {
                let mut chars = im.chars();
                if let (Some(c), None) = (chars.next(), chars.clone().next()) {
                    // 单字符输入:直接用它(shift 已在字符里体现,不再重复施加)。
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
            // 回落:单字符键名当普通字符。
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Key::Char(c),
                _ => return None, // 未知功能键,忽略
            }
        }
    };
    Some(input::encode(&key, mods))
}
