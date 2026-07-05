//! 终端会话:用 alacritty 自带 tty + EventLoop 起 PTY 子进程、驱动 `Term`,
//! 并把内核事件通过 channel 转出到调用方(app 层)。
//!
//! 架构(照 alacritty/Zed 的做法):
//! - `Term` 包进 `Arc<FairMutex<Term<Proxy>>>`,渲染线程与 PTY 线程共享。
//! - `EventLoop::spawn` 起后台 "PTY reader" 线程:自动读 PTY → 解析进 Term → 发 `Wakeup`。
//! - 我们的 [`Proxy`] 实现 `EventListener`,把内核事件塞进一个 mpsc channel。
//!   **`Event::PtyWrite` 会被自动回环成 `Msg::Input` 写回 PTY**(否则 vim/shell 卡死)。
//! - 写回(键盘输入)走 `EventLoopSender::send(Msg::Input(bytes))`。

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

use alacritty_terminal::event::{Event as AlacEvent, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::tty::{self, Options as PtyOptions, Shell};

use crate::palette;

/// 转发给 app 层的会话事件(从内核事件精简而来)。
#[derive(Debug, Clone)]
pub enum TermEvent {
    /// 有新内容,需要重绘。
    Wakeup,
    /// 标题变更(标签页标题)。
    Title(String),
    /// 响铃。
    Bell,
    /// 子进程退出。
    ChildExit(i32),
}

/// 终端尺寸(行 × 列 + 像素度量),实现 alacritty 的 `Dimensions`。
#[derive(Debug, Clone, Copy)]
pub struct TermDimensions {
    pub columns: usize,
    pub screen_lines: usize,
    pub cell_width: u16,
    pub cell_height: u16,
}

impl TermDimensions {
    pub fn new(columns: usize, screen_lines: usize, cell_width: u16, cell_height: u16) -> Self {
        Self {
            columns: columns.max(1),
            screen_lines: screen_lines.max(1),
            cell_width: cell_width.max(1),
            cell_height: cell_height.max(1),
        }
    }

    fn window_size(&self) -> WindowSize {
        WindowSize {
            num_lines: self.screen_lines as u16,
            num_cols: self.columns as u16,
            cell_width: self.cell_width,
            cell_height: self.cell_height,
        }
    }
}

impl Dimensions for TermDimensions {
    fn total_lines(&self) -> usize {
        self.screen_lines
    }
    fn screen_lines(&self) -> usize {
        self.screen_lines
    }
    fn columns(&self) -> usize {
        self.columns
    }
}

/// EventListener 代理:把内核事件塞进 channel。
///
/// `send_event(&self, ...)` 是不可变借用且会被 PTY 线程调用,所以内部只持有
/// `Sender`(可 Clone、线程安全)。`PtyWrite` 需要写回 PTY,但 Proxy 此时
/// 还没有 `EventLoopSender`(构造顺序所限),故 `PtyWrite` 也走同一 channel,
/// 由会话主循环回环成 `Msg::Input`(见 [`TerminalSession::drain_events`])。
#[derive(Clone)]
struct Proxy {
    tx: Sender<ProxyEvent>,
}

/// Proxy 内部事件:比 [`TermEvent`] 多一个 PtyWrite(需回环)。
enum ProxyEvent {
    Wakeup,
    Title(String),
    Bell,
    ChildExit(i32),
    /// 内核要求写回 PTY 的应答(DA/CPR/DSR 等)。
    PtyWrite(Vec<u8>),
}

impl EventListener for Proxy {
    fn send_event(&self, event: AlacEvent) {
        let mapped = match event {
            AlacEvent::Wakeup => Some(ProxyEvent::Wakeup),
            AlacEvent::Title(t) => Some(ProxyEvent::Title(t)),
            AlacEvent::Bell => Some(ProxyEvent::Bell),
            AlacEvent::ChildExit(status) => Some(ProxyEvent::ChildExit(status.code().unwrap_or(-1))),
            AlacEvent::PtyWrite(s) => Some(ProxyEvent::PtyWrite(s.into_bytes())),
            // 其余事件(剪贴板/颜色查询/标题重置等)MVP 暂不处理。
            _ => None,
        };
        if let Some(ev) = mapped {
            let _ = self.tx.send(ev); // 接收端已关闭则忽略
        }
    }
}

/// 一个活动的终端会话。
pub struct TerminalSession {
    term: Arc<FairMutex<Term<Proxy>>>,
    loop_tx: EventLoopSender,
    events_rx: Receiver<ProxyEvent>,
    dimensions: TermDimensions,
    child_exited: Option<i32>,
}

impl TerminalSession {
    /// 起一个终端会话:开 PTY 跑 `command`(cwd/env 生效),spawn 后台读线程。
    ///
    /// `command` 为 None 时用默认 shell。
    pub fn spawn(
        dimensions: TermDimensions,
        working_directory: Option<PathBuf>,
        command: Option<(String, Vec<String>)>,
        env: Vec<(String, String)>,
    ) -> anyhow::Result<Self> {
        // 确保 TERM/COLORTERM 已设置(alacritty 的 helper)。
        tty::setup_env();

        let (tx, events_rx) = std::sync::mpsc::channel::<ProxyEvent>();
        let proxy = Proxy { tx };

        // 1) Term(共享句柄)。
        let config = Config::default();
        let term = Term::new(config, &dimensions, proxy.clone());
        let term = Arc::new(FairMutex::new(term));

        // 2) PTY 选项。
        let pty_options = PtyOptions {
            shell: command.map(|(program, args)| Shell::new(program, args)),
            working_directory,
            drain_on_exit: true,
            env: env.into_iter().collect(),
        };
        let pty = tty::new(&pty_options, dimensions.window_size(), 0)?;

        // 3) EventLoop:后台线程自动读 PTY → 解析进 Term → 发 Wakeup。
        let event_loop = EventLoop::new(
            Arc::clone(&term),
            proxy,
            pty,
            pty_options_drain(&pty_options),
            false, // ref_test
        )?;
        let loop_tx = event_loop.channel();
        let _ = event_loop.spawn(); // 后台 "PTY reader" 线程

        Ok(Self {
            term,
            loop_tx,
            events_rx,
            dimensions,
            child_exited: None,
        })
    }

    /// 锁定 Term 读取一份可渲染快照(cell 网格,含宽字符标志、颜色解析后)。
    ///
    /// 这样 app 层无需接触 alacritty 内部类型(`Term`/`Proxy`),只拿到
    /// GPUI-agnostic 的 [`RenderSnapshot`],职责边界干净。
    pub fn snapshot(&self) -> RenderSnapshot {
        let term = self.term.lock();
        RenderSnapshot::capture(&term)
    }

    /// 向 PTY 写入字节(键盘输入编码后的结果,见 input 模块)。
    pub fn write_input(&self, bytes: Vec<u8>) {
        if bytes.is_empty() {
            return;
        }
        let _ = self.loop_tx.send(Msg::Input(bytes.into()));
    }

    /// resize:同步给 Term 与 PTY。
    pub fn resize(&mut self, dimensions: TermDimensions) {
        self.dimensions = dimensions;
        self.term.lock().resize(dimensions);
        let _ = self.loop_tx.send(Msg::Resize(dimensions.window_size()));
    }

    /// 排空内核事件,转成 [`TermEvent`] 返回;`PtyWrite` 就地回环写回 PTY。
    ///
    /// app 层每帧(或收到唤醒信号时)调用一次,据返回事件决定重绘/关闭 pane。
    pub fn drain_events(&mut self) -> Vec<TermEvent> {
        let mut out = Vec::new();
        while let Ok(ev) = self.events_rx.try_recv() {
            match ev {
                ProxyEvent::Wakeup => out.push(TermEvent::Wakeup),
                ProxyEvent::Title(t) => out.push(TermEvent::Title(t)),
                ProxyEvent::Bell => out.push(TermEvent::Bell),
                ProxyEvent::ChildExit(code) => {
                    self.child_exited = Some(code);
                    out.push(TermEvent::ChildExit(code));
                }
                // 内核应答:必须写回 PTY,否则程序等应答会卡死。
                ProxyEvent::PtyWrite(bytes) => {
                    let _ = self.loop_tx.send(Msg::Input(bytes.into()));
                }
            }
        }
        out
    }

    /// 子进程是否已退出(退出码)。
    pub fn child_exit_code(&self) -> Option<i32> {
        self.child_exited
    }

    pub fn dimensions(&self) -> TermDimensions {
        self.dimensions
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        // 通知后台线程关闭(尽力而为)。
        let _ = self.loop_tx.send(Msg::Shutdown);
    }
}

fn pty_options_drain(opts: &PtyOptions) -> bool {
    opts.drain_on_exit
}

// ---------------------------------------------------------------------------
// 可渲染快照:GPUI-agnostic 的一屏 cell 网格,供 app 层绘制。
// 宽字符/颜色在此就地处理好,app 层不接触 alacritty 内部类型。
// ---------------------------------------------------------------------------

/// 一个已解析好的可渲染 cell:字符 + RGB 前景/背景 + 属性 + 显示宽度。
#[derive(Debug, Clone, Copy)]
pub struct RenderCell {
    pub ch: char,
    /// 0xRRGGBB 前景。
    pub fg: u32,
    /// 0xRRGGBB 背景。
    pub bg: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// 显示宽度:
    /// - `1` 普通字符
    /// - `2` 宽字符正身(横跨本格与右格)
    /// - `0` 宽字符右侧占位(spacer):渲染时不画字符,但**必须占位**以保持列对齐
    pub width: u8,
}

impl RenderCell {
    /// 默认空白 cell(空格 + 默认前后景)。
    fn blank() -> Self {
        Self {
            ch: ' ',
            fg: palette::DEFAULT_FG.packed(),
            bg: palette::DEFAULT_BG.packed(),
            bold: false,
            italic: false,
            underline: false,
            width: 1,
        }
    }

    /// 是否与另一 cell 样式相同(供渲染 batching 判断能否合并成一个 run)。
    pub fn same_style(&self, o: &RenderCell) -> bool {
        self.fg == o.fg
            && self.bg == o.bg
            && self.bold == o.bold
            && self.italic == o.italic
            && self.underline == o.underline
    }
}

/// 光标位置(视口行列)。
#[derive(Debug, Clone, Copy)]
pub struct CursorPos {
    pub line: usize,
    pub col: usize,
    pub visible: bool,
}

/// 一屏可渲染快照:**稠密**的 `rows × cols` 网格(row-major)。
///
/// 每个格子都有一个 [`RenderCell`](含空格),保证列严格连续——渲染时
/// 按行从左到右走、把相邻同样式 cell 合并成一个文本 run 一次 `shape_line`
/// (照 Zed 的 batching 做法),并用 `force_width` 把每格钉死在网格上。
/// 这从根上避免「字符间歇性错位/多一格空隙」。
#[derive(Debug, Clone)]
pub struct RenderSnapshot {
    pub rows: usize,
    pub cols: usize,
    /// 稠密网格,长度 = rows*cols,row-major。
    pub grid: Vec<RenderCell>,
    pub cursor: CursorPos,
    pub display_offset: usize,
}

impl RenderSnapshot {
    /// 读取某行某列的 cell(越界返回空白)。
    pub fn cell(&self, line: usize, col: usize) -> RenderCell {
        if line < self.rows && col < self.cols {
            self.grid[line * self.cols + col]
        } else {
            RenderCell::blank()
        }
    }
}

impl RenderSnapshot {
    fn capture<T: EventListener>(term: &Term<T>) -> Self {
        let content = term.renderable_content();
        let colors = content.colors;
        let display_offset = content.display_offset;

        let rows = term.screen_lines();
        let cols = term.columns();

        // 稠密网格:先全填空白,再把每个 cell 放到它的精确列。
        let mut grid = vec![RenderCell::blank(); rows * cols];

        for indexed in content.display_iter {
            let cell = indexed.cell;
            let flags = cell.flags;

            // 视口坐标(display_iter 只遍历可视区)。
            let vp = alacritty_terminal::term::point_to_viewport(display_offset, indexed.point);
            let Some(vp) = vp else { continue };
            if vp.line >= rows || vp.column.0 >= cols {
                continue;
            }
            let idx = vp.line * cols + vp.column.0;

            // 宽字符右侧占位 / 行尾占位:保留为 width=0 占位(不画字符,但占列)。
            if flags.contains(Flags::WIDE_CHAR_SPACER)
                || flags.contains(Flags::LEADING_WIDE_CHAR_SPACER)
            {
                grid[idx].width = 0;
                continue;
            }

            let fg = palette::resolve(cell.fg, colors).packed();
            let bg = palette::resolve(cell.bg, colors).packed();
            // INVERSE:前后景互换。
            let (fg, bg) = if flags.contains(Flags::INVERSE) {
                (bg, fg)
            } else {
                (fg, bg)
            };
            let width = if flags.contains(Flags::WIDE_CHAR) { 2 } else { 1 };

            grid[idx] = RenderCell {
                ch: cell.c,
                fg,
                bg,
                bold: flags.intersects(Flags::BOLD | Flags::BOLD_ITALIC),
                italic: flags.intersects(Flags::ITALIC | Flags::BOLD_ITALIC),
                underline: flags.intersects(Flags::ALL_UNDERLINES),
                width,
            };
        }

        // 光标:落在宽字符 spacer 上时回退一列(照 alacritty 的做法)。
        let cursor_point = content.cursor.point;
        let cursor_vp =
            alacritty_terminal::term::point_to_viewport(display_offset, cursor_point);
        let cursor = match cursor_vp {
            Some(p) => CursorPos {
                line: p.line,
                col: p.column.0,
                visible: term.mode().contains(alacritty_terminal::term::TermMode::SHOW_CURSOR),
            },
            None => CursorPos {
                line: 0,
                col: 0,
                visible: false,
            },
        };

        Self {
            rows,
            cols,
            grid,
            cursor,
            display_offset,
        }
    }
}
