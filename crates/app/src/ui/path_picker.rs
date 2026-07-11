//! 仓库目录选择器 —— 目录浏览 + 可编辑路径 + 键盘导航。
//!
//! 参考 Zed 的 `OpenPathPrompt`(`crates/open_path_prompt/src/open_path_prompt.rs`):
//! 用户输入路径,系统切分出「目录部分」(list_dir 参数)和「后缀」(过滤词),
//! 后台异步列目录,cancel-flag 取消旧任务,前台显示过滤后的条目。
//! 列表只显示目录；支持后退、前进、上一级和刷新。单击选中、双击或 Tab
//! 进入目录，Enter 确认输入的路径。
//!
//! 与 Zed 的差异:
//! - 不依赖 Zed 的 `picker` / `workspace` crate(GPUI 0.2.2 + gpui-component 0.5.1)。
//! - 用 `Host::list_dir` 抽象(LocalHost / WslHost 通用)。
//! - 模糊过滤用简单 `contains`(非 fuzzy match + 高亮)。
//! - 目录列表用手动 div(非 `uniform_list` 虚拟化)。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gpui::{
    div, prelude::*, px, rgb, AnyElement, App, ClickEvent, Context, Entity, EventEmitter,
    FocusHandle, Focusable, IntoElement, KeyDownEvent, MouseButton, ParentElement, SharedString,
    Styled, Window,
};
use gpui_component::input::{Input, InputEvent, InputState};

use lucy_core::host::{DirEntry, Host, LocalHost, WslHost};

use crate::theme;
use crate::ui::{button, button_row, icon_button, ButtonVariant};

// ───────────────────────────── 纯逻辑函数 ─────────────────────────────

/// 把查询字符串切分为 (目录部分, 后缀)。
///
/// Posix(`/`): `rfind('/')` 切分,目录部分末尾补 `/`。
/// Windows(`\`): `rfind('\\')` 或 `rfind('/')` 取最大,目录部分末尾补 `\`。
/// 无分隔符时返回 `("", query)`(相对路径,Phase 1 不处理但函数不 panic)。
pub(crate) fn get_dir_and_suffix(query: &str, separator: char) -> (String, String) {
    if separator == '/' {
        // Posix: 用最后一个 `/` 切分。
        match query.rfind('/') {
            Some(idx) => {
                let dir = &query[..=idx]; // 含分隔符
                let suffix = &query[idx + 1..];
                (dir.to_string(), suffix.to_string())
            }
            None => (String::new(), query.to_string()),
        }
    } else {
        // Windows: 取 `\` 和 `/` 中较大的位置。
        let last = query.rfind('\\').into_iter().chain(query.rfind('/')).max();
        match last {
            Some(idx) => {
                let dir = &query[..=idx]; // 含分隔符
                let suffix = &query[idx + 1..];
                (dir.to_string(), suffix.to_string())
            }
            None => (String::new(), query.to_string()),
        }
    }
}

/// 过滤目录:文件永不显示;后缀非空时再按名称匹配。
pub(crate) fn filter_entries(entries: &[DirEntry], suffix: &str) -> Vec<usize> {
    let lower = suffix.to_lowercase();
    entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.is_dir && (lower.is_empty() || e.name.to_lowercase().contains(&lower)))
        .map(|(i, _)| i)
        .collect()
}

/// 返回一个带结尾分隔符的父目录。根目录保持不变。
pub(crate) fn parent_directory(dir: &str, separator: char) -> String {
    let is_separator = |c| c == separator || (separator == '\\' && c == '/');
    let trimmed = dir.trim_end_matches(is_separator);
    if trimmed.is_empty() {
        return if separator == '\\' {
            String::new()
        } else {
            separator.to_string()
        };
    }
    // Windows drive roots have a virtual parent ("This PC") represented by
    // an empty query. That makes other drive letters reachable without a
    // separate native file dialog.
    if separator == '\\'
        && trimmed.len() == 2
        && trimmed.as_bytes()[0].is_ascii_alphabetic()
        && trimmed.as_bytes()[1] == b':'
    {
        return String::new();
    }
    let Some(index) = trimmed.rfind(is_separator) else {
        return dir.to_string();
    };
    let parent = &trimmed[..=index];
    if parent.is_empty() {
        separator.to_string()
    } else {
        parent.to_string()
    }
}

/// Converts the bitmask returned by Windows `GetLogicalDrives` into picker entries.
#[cfg(any(target_os = "windows", test))]
pub(crate) fn drive_entries(mask: u32) -> Vec<DirEntry> {
    (0..26)
        .filter(|bit| mask & (1 << bit) != 0)
        .map(|bit| DirEntry {
            name: format!("{}:", (b'A' + bit as u8) as char),
            is_dir: true,
        })
        .collect()
}

#[cfg(target_os = "windows")]
fn windows_drive_entries() -> std::io::Result<Vec<DirEntry>> {
    // GetLogicalDrives is a side-effect-free query returning one bit per drive letter.
    let mask = unsafe { windows_sys::Win32::Storage::FileSystem::GetLogicalDrives() };
    if mask == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(drive_entries(mask))
    }
}

/// 构造补全路径:`dir + name + (is_dir ? separator : "")`。
pub(crate) fn complete_path(dir: &str, name: &str, is_dir: bool, separator: char) -> String {
    if is_dir {
        format!("{dir}{name}{separator}")
    } else {
        format!("{dir}{name}")
    }
}

// ───────────────────────────── 状态 ─────────────────────────────

/// 目录列表状态。
#[derive(Default)]
struct PickerState {
    /// 当前查询文本(与 InputState 同步)。
    query: String,
    /// 当前列出的目录部分(list_dir 的参数)。
    dir: String,
    /// 该目录的条目列表(list_dir 返回,未过滤)。
    entries: Vec<DirEntry>,
    /// 过滤后的条目索引(按 suffix 过滤)。
    filtered: Vec<usize>,
    /// 当前选中条目在 filtered 中的索引。
    selected_index: usize,
    /// 后台 list_dir 进行中。
    loading: bool,
    /// 加载失败信息(None = 无错误或加载中)。
    error: Option<String>,
}

// ───────────────────────────── 事件 ─────────────────────────────

/// PathPicker 发出的事件。
#[derive(Clone, Debug)]
pub enum PathPickerEvent {
    /// 用户确认了输入框中的目录路径。
    Confirmed { path: PathBuf, is_remote: bool },
    /// 用户关闭了选择器(Esc 或点击遮罩)。
    Dismissed,
}

// ───────────────────────────── PathPicker ─────────────────────────────

/// Zed 风格的路径输入选择器。
///
/// 可编辑路径 + 下方目录列表。用户输入路径时,系统自动切分出「目录部分」
/// (list_dir 参数)和「后缀」(过滤词),后台异步列目录,cancel-flag 取消旧任务。
/// 双击或 Tab 进入目录,Enter 确认输入路径,Up/Down 导航,Esc 关闭。
pub struct PathPicker {
    /// 目录列表状态。
    state: PickerState,
    /// Host 抽象(LocalHost / WslHost),用于 list_dir。
    host: Arc<dyn Host>,
    /// cancel-flag:每次 update_matches 翻转旧 flag,新任务完成后检查 flag。
    cancel_flag: Arc<AtomicBool>,
    /// 文本输入状态(gpui-component Input,带 IME + 选择 + 复制粘贴)。
    input: Entity<InputState>,
    /// 路径分隔符(WslHost → `/`,LocalHost → OS 分隔符)。
    separator: char,
    /// focus handle。
    focus: FocusHandle,
    /// 目录导航历史，不记录输入框中的过滤文本。
    back_history: Vec<String>,
    forward_history: Vec<String>,
}

impl PathPicker {
    /// 构造 PathPicker。
    ///
    /// `host` 用于 `list_dir`。`initial_query` 预填文本框(如 `/` 或 home 目录)。
    /// `on_confirm` / `on_dismiss` 由调用方通过 `cx.subscribe` 监听事件处理。
    pub fn new(
        host: Arc<dyn Host>,
        initial_query: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let separator = if host.is_remote() {
            '/'
        } else {
            std::path::MAIN_SEPARATOR
        };

        // 创建 InputState,预填 initial_query。
        let query = initial_query.clone();
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder("输入仓库目录路径");
            state.set_value(query.clone(), window, cx);
            state.focus(window, cx);
            state
        });

        // 监听 InputState 变化 → 触发 update_matches。
        cx.subscribe(&input, |this, _, event, cx| {
            if let InputEvent::Change = event {
                let q = this.input.read(cx).value().to_string();
                this.update_matches(q, cx);
            }
        })
        .detach();

        let mut picker = Self {
            state: PickerState::default(),
            host,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            input,
            separator,
            focus: cx.focus_handle(),
            back_history: Vec::new(),
            forward_history: Vec::new(),
        };

        // 触发初始 update_matches(列初始目录)。
        picker.update_matches(query, cx);

        picker
    }

    /// 当前查询文本。
    pub fn query(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }

    /// 设置路径文本(进入目录或返回上级时调用)。
    pub fn set_query(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let q = query.to_string();
        self.input.update(cx, |s, cx| {
            s.set_value(&q, window, cx);
        });
        // set_value 会 emit InputEvent::Change → subscribe 触发 update_matches。
        // 但 subscribe 在 cx 上,this 的 &mut 可能还未更新,手动触发一次。
        self.update_matches(q, cx);
    }

    /// 设置错误信息(确认失败时调用)。
    pub fn set_error(&mut self, msg: &str, cx: &mut Context<Self>) {
        self.store_error(msg);
        cx.notify();
    }

    fn store_error(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        log::error!("{msg}");
        self.state.error = Some(msg);
    }

    /// 过滤后条目数。
    pub fn filtered_count(&self) -> usize {
        self.state.filtered.len()
    }

    /// 选中条目索引。
    pub fn selected_index(&self) -> usize {
        self.state.selected_index
    }

    /// 过滤后条目名列表(测试用)。
    pub fn filtered_names(&self) -> Vec<String> {
        self.state
            .filtered
            .iter()
            .map(|&i| self.state.entries[i].name.clone())
            .collect()
    }

    /// 是否有 loading 进行中(测试用)。
    pub fn is_loading(&self) -> bool {
        self.state.loading
    }

    /// 错误信息(测试用)。
    pub fn error(&self) -> Option<&str> {
        self.state.error.as_deref()
    }

    /// 当前是否浏览 WSL 文件系统。
    pub fn is_remote(&self) -> bool {
        self.host.is_remote()
    }

    fn local_initial_query() -> String {
        std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .map(|home| {
                if home.ends_with(std::path::MAIN_SEPARATOR) {
                    home
                } else {
                    format!("{home}{}", std::path::MAIN_SEPARATOR)
                }
            })
            .unwrap_or_else(|_| format!(".{}", std::path::MAIN_SEPARATOR))
    }

    /// 在 Windows 本机文件系统与 WSL 文件系统之间切换。
    fn switch_location(&mut self, remote: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.host.is_remote() == remote {
            return;
        }
        self.cancel_flag.store(true, Ordering::Release);
        self.host = if remote {
            Arc::new(WslHost::default())
        } else {
            Arc::new(LocalHost)
        };
        self.separator = if remote {
            '/'
        } else {
            std::path::MAIN_SEPARATOR
        };
        self.back_history.clear();
        self.forward_history.clear();
        let query = if remote {
            "/".to_string()
        } else {
            Self::local_initial_query()
        };
        self.set_query(&query, window, cx);
    }

    // ─────────────── 核心:update_matches ───────────────

    /// 更新补全列表:切分 dir/suffix,dir 变化时后台 list_dir,suffix 变化时只过滤。
    fn update_matches(&mut self, query: String, cx: &mut Context<Self>) {
        self.update_matches_inner(query, false, cx);
    }

    fn update_matches_inner(&mut self, query: String, force_reload: bool, cx: &mut Context<Self>) {
        let (dir, suffix) = get_dir_and_suffix(&query, self.separator);

        // 清除错误(用户继续输入)。
        self.state.error = None;
        self.state.query = query;

        let dir_changed = force_reload || dir != self.state.dir;

        if dir_changed {
            // 翻转 cancel-flag(取消旧任务)。
            self.cancel_flag.store(true, Ordering::Release);
            self.cancel_flag = Arc::new(AtomicBool::new(false));
            let cancel_flag = self.cancel_flag.clone();
            self.state.dir = dir.clone();
            self.state.loading = true;
            self.state.entries.clear();
            self.state.filtered.clear();
            self.state.selected_index = 0;
            cx.notify();

            #[cfg(target_os = "windows")]
            if !self.host.is_remote() && dir.is_empty() && suffix.is_empty() {
                self.state.loading = false;
                match windows_drive_entries() {
                    Ok(entries) => {
                        self.state.entries = entries;
                        self.state.filtered = filter_entries(&self.state.entries, "");
                    }
                    Err(error) => {
                        self.store_error(format!("读取 Windows 盘符失败: {error}"));
                    }
                }
                cx.notify();
                return;
            }

            // 后台 list_dir。
            let host = self.host.clone();
            let dir_task = dir.clone();
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { host.list_dir(std::path::Path::new(&dir_task)) })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    // 检查 cancel-flag(已取消则丢弃结果)。
                    if cancel_flag.load(Ordering::Acquire) {
                        return;
                    }
                    this.state.loading = false;
                    match result {
                        Ok(entries) => {
                            this.state.entries = entries;
                            this.state.filtered = filter_entries(&this.state.entries, &suffix);
                            this.state.selected_index = 0;
                        }
                        Err(e) => {
                            this.store_error(format!("列出目录 {} 失败: {e}", this.state.dir));
                            this.state.entries.clear();
                            this.state.filtered.clear();
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        } else {
            // dir 没变,只更新 suffix(重新过滤)。
            self.state.filtered = filter_entries(&self.state.entries, &suffix);
            self.state.selected_index = 0;
            cx.notify();
        }
    }

    // ─────────────── 键盘交互 ───────────────

    /// 选中条目(clamp 到 filtered 范围)。
    fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.state.filtered.is_empty() {
            self.state.selected_index = index.min(self.state.filtered.len() - 1);
            cx.notify();
        }
    }

    /// Up: 选中上移(循环到末尾)。
    fn select_prev(&mut self, cx: &mut Context<Self>) {
        if self.state.filtered.is_empty() {
            return;
        }
        let len = self.state.filtered.len();
        let idx = if self.state.selected_index == 0 {
            len - 1
        } else {
            self.state.selected_index - 1
        };
        self.state.selected_index = idx;
        cx.notify();
    }

    /// Down: 选中下移(循环到第一条)。
    fn select_next(&mut self, cx: &mut Context<Self>) {
        if self.state.filtered.is_empty() {
            return;
        }
        let len = self.state.filtered.len();
        let idx = if self.state.selected_index + 1 >= len {
            0
        } else {
            self.state.selected_index + 1
        };
        self.state.selected_index = idx;
        cx.notify();
    }

    /// 进入选中的目录。
    fn enter_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(&entry_idx) = self.state.filtered.get(self.state.selected_index) else {
            return;
        };
        let entry = &self.state.entries[entry_idx];
        if entry.name.is_empty() || !entry.is_dir {
            return;
        }
        let new_query = complete_path(&self.state.dir, &entry.name, entry.is_dir, self.separator);
        self.navigate_to(new_query, window, cx);
    }

    /// Enter / 确认按钮:始终确认输入框中的路径，不隐式替换成列表选中项。
    fn confirm(&mut self, cx: &mut Context<Self>) {
        cx.emit(PathPickerEvent::Confirmed {
            path: PathBuf::from(self.state.query.clone()),
            is_remote: self.host.is_remote(),
        });
    }

    /// 返回当前浏览目录的上一级。
    fn go_parent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let parent = parent_directory(&self.state.dir, self.separator);
        self.navigate_to(parent, window, cx);
    }

    fn navigate_to(&mut self, target: String, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.state.dir.clone();
        if !current.is_empty() && current != target {
            self.back_history.push(current);
            self.forward_history.clear();
        }
        self.set_query(&target, window, cx);
    }

    fn go_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.back_history.pop() else {
            return;
        };
        if !self.state.dir.is_empty() {
            self.forward_history.push(self.state.dir.clone());
        }
        self.set_query(&target, window, cx);
    }

    fn go_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.forward_history.pop() else {
            return;
        };
        if !self.state.dir.is_empty() {
            self.back_history.push(self.state.dir.clone());
        }
        self.set_query(&target, window, cx);
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let query = self.state.query.clone();
        self.update_matches_inner(query, true, cx);
    }

    /// Esc / 遮罩点击:关闭。
    fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.cancel_flag.store(true, Ordering::Release);
        cx.emit(PathPickerEvent::Dismissed);
    }

    // ─────────────── 渲染 ───────────────

    /// 目录列表条目。
    fn render_entry(&self, i: usize, entry: &DirEntry, cx: &mut Context<Self>) -> impl IntoElement {
        let is_selected = i == self.state.selected_index;
        let name = entry.name.clone();

        div()
            .id(SharedString::from(format!("picker-entry-{i}")))
            .flex()
            .flex_row()
            .items_center()
            .gap(theme::space_sm())
            .px(theme::space_sm())
            .py(theme::space_xs())
            .cursor_pointer()
            .when(is_selected, |d| {
                d.bg(rgb(theme::SURFACE_RAISED))
                    .border_l_2()
                    .border_color(rgb(theme::TEXT_BRIGHT))
            })
            .hover(|s| s.bg(rgb(theme::BTN_BG_HOVER)))
            .text_color(rgb(theme::TEXT))
            .child(
                gpui::svg()
                    .flex_none()
                    .size(px(16.0))
                    .debug_selector(move || format!("picker-entry-icon-{i}"))
                    .path("icons/folder-open.svg")
                    .text_color(rgb(theme::TEXT_DIM)),
            )
            .child(SharedString::from(name))
            .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                this.select(i, cx);
                if ev.click_count() == 2 {
                    this.enter_selected(window, cx);
                }
            }))
    }

    /// 目录列表区(可滚动)。
    fn render_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut list = div()
            .id("picker-list")
            .flex()
            .flex_col()
            .gap_0()
            .max_h(px(320.0))
            .overflow_y_scroll();

        if self.state.loading {
            list = list.child(
                div()
                    .px(theme::space_sm())
                    .py(theme::space_xs())
                    .text_color(rgb(theme::TEXT_FAINT))
                    .child(SharedString::from("加载中…")),
            );
        } else if let Some(err) = &self.state.error {
            list = list.child(
                div()
                    .px(theme::space_sm())
                    .py(theme::space_xs())
                    .text_color(rgb(theme::STATE_ERROR))
                    .child(SharedString::from(format!("错误: {err}"))),
            );
        } else if self.state.filtered.is_empty() {
            list = list.child(
                div()
                    .px(theme::space_sm())
                    .py(theme::space_xs())
                    .text_color(rgb(theme::TEXT_FAINT))
                    .child(SharedString::from("没有匹配的文件夹")),
            );
        } else {
            for (i, &entry_idx) in self.state.filtered.iter().enumerate() {
                let entry = &self.state.entries[entry_idx];
                list = list.child(self.render_entry(i, entry, cx));
            }
        }

        list
    }
}

impl EventEmitter<PathPickerEvent> for PathPicker {}

impl Focusable for PathPicker {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for PathPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let input_el: AnyElement = Input::new(&self.input).into_any_element();
        let can_go_back = !self.back_history.is_empty();
        let can_go_forward = !self.forward_history.is_empty();
        let can_go_parent = parent_directory(&self.state.dir, self.separator) != self.state.dir;
        let path_row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme::space_sm())
            .child(
                icon_button("picker-back", "icons/arrow-left.svg", "后退")
                    .disabled(!can_go_back)
                    .on_click(cx.listener(|this, _ev: &ClickEvent, window, cx| {
                        this.go_back(window, cx);
                    })),
            )
            .child(
                icon_button("picker-forward", "icons/arrow-right.svg", "前进")
                    .disabled(!can_go_forward)
                    .on_click(cx.listener(|this, _ev: &ClickEvent, window, cx| {
                        this.go_forward(window, cx);
                    })),
            )
            .child(
                icon_button("picker-parent", "icons/arrow-up.svg", "上一级")
                    .disabled(!can_go_parent)
                    .on_click(cx.listener(|this, _ev: &ClickEvent, window, cx| {
                        this.go_parent(window, cx);
                    })),
            )
            .child(
                icon_button("picker-refresh", "icons/refresh-cw.svg", "刷新").on_click(
                    cx.listener(|this, _ev: &ClickEvent, _window, cx| {
                        this.refresh(cx);
                    }),
                ),
            )
            .child(div().flex_1().min_w_0().child(input_el));
        let list = self.render_list(cx);

        #[cfg(target_os = "windows")]
        let location_switch: Option<AnyElement> = {
            let remote = self.host.is_remote();
            Some(
                div()
                    .flex()
                    .flex_row()
                    .gap(theme::space_xs())
                    .child(
                        button("picker-location-local", "本机")
                            .variant(if remote {
                                ButtonVariant::Neutral
                            } else {
                                ButtonVariant::Confirm
                            })
                            .on_click(cx.listener(|this, _ev, window, cx| {
                                this.switch_location(false, window, cx);
                            })),
                    )
                    .child(
                        button("picker-location-wsl", "WSL")
                            .variant(if remote {
                                ButtonVariant::Confirm
                            } else {
                                ButtonVariant::Neutral
                            })
                            .on_click(cx.listener(|this, _ev, window, cx| {
                                this.switch_location(true, window, cx);
                            })),
                    )
                    .into_any_element(),
            )
        };
        #[cfg(not(target_os = "windows"))]
        let location_switch: Option<AnyElement> = None;

        let buttons: Vec<AnyElement> = vec![
            button("picker-cancel", "取消")
                .on_click(cx.listener(|this, _ev: &ClickEvent, _w, cx| {
                    this.dismiss(cx);
                }))
                .into_any_element(),
            button("picker-confirm", "选择此目录")
                .variant(ButtonVariant::Confirm)
                .on_click(cx.listener(|this, _ev: &ClickEvent, _w, cx| {
                    this.confirm(cx);
                }))
                .into_any_element(),
        ];

        let key_handler = cx.listener(move |this, ev: &KeyDownEvent, window, cx| {
            let key = ev.keystroke.key.as_str();
            match key {
                "up" => {
                    this.select_prev(cx);
                    cx.stop_propagation();
                }
                "down" => {
                    this.select_next(cx);
                    cx.stop_propagation();
                }
                "tab" => {
                    this.enter_selected(window, cx);
                    cx.stop_propagation();
                }
                "enter" => {
                    this.confirm(cx);
                    cx.stop_propagation();
                }
                "escape" => {
                    this.dismiss(cx);
                    cx.stop_propagation();
                }
                _ => {}
            }
        });

        // 聚焦 input(每次 render 确保聚焦)。
        self.input.update(cx, |s, cx| {
            s.focus(window, cx);
        });

        // 遮罩 + 居中卡片。遮罩点击关闭;卡片内捕获键盘事件。
        // 用 Stateful<Div>(.id)让 on_mouse_down / on_key_down 可用。
        div()
            .id("path-picker-overlay")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::with_alpha(0x00_00_00, 0.55))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _ev, _w, cx| {
                    this.dismiss(cx);
                }),
            )
            .child(
                div()
                    .w(px(460.0))
                    .bg(rgb(theme::SURFACE))
                    .border_1()
                    .border_color(rgb(theme::BORDER))
                    .rounded(theme::radius())
                    .p(theme::space_lg())
                    .flex()
                    .flex_col()
                    .gap(theme::space_md())
                    .font_family(theme::FONT_UI)
                    // 卡片内捕获鼠标事件(不冒泡到遮罩)。
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_key_down(key_handler)
                    .child(
                        div()
                            .text_color(rgb(theme::TEXT_BRIGHT))
                            .child(SharedString::from("选择仓库目录")),
                    )
                    .children(location_switch)
                    .child(path_row)
                    .child(list)
                    .child(button_row(buttons)),
            )
    }
}

#[cfg(feature = "test-support")]
impl PathPicker {
    pub fn separator_for_test(&self) -> char {
        self.separator
    }

    pub fn switch_location_for_test(
        &mut self,
        remote: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_location(remote, window, cx);
    }

    pub fn confirm_for_test(&mut self, cx: &mut Context<Self>) {
        self.confirm(cx);
    }

    pub fn go_parent_for_test(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.go_parent(window, cx);
    }

    pub fn enter_selected_for_test(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.enter_selected(window, cx);
    }
}

// ───────────────────────────── 测试 ─────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_dir_and_suffix_posix_with_suffix() {
        let (dir, suffix) = get_dir_and_suffix("/home/user/Doc", '/');
        assert_eq!(dir, "/home/user/");
        assert_eq!(suffix, "Doc");
    }

    #[test]
    fn get_dir_and_suffix_posix_trailing_sep() {
        let (dir, suffix) = get_dir_and_suffix("/home/user/", '/');
        assert_eq!(dir, "/home/user/");
        assert_eq!(suffix, "");
    }

    #[test]
    fn get_dir_and_suffix_posix_root() {
        let (dir, suffix) = get_dir_and_suffix("/", '/');
        assert_eq!(dir, "/");
        assert_eq!(suffix, "");
    }

    #[test]
    fn get_dir_and_suffix_posix_no_sep() {
        let (dir, suffix) = get_dir_and_suffix("Doc", '/');
        assert_eq!(dir, "");
        assert_eq!(suffix, "Doc");
    }

    #[test]
    fn get_dir_and_suffix_windows_backslash() {
        let (dir, suffix) = get_dir_and_suffix(r"C:\Users\Doc", '\\');
        assert_eq!(dir, r"C:\Users\");
        assert_eq!(suffix, "Doc");
    }

    #[test]
    fn get_dir_and_suffix_windows_forward_slash() {
        let (dir, suffix) = get_dir_and_suffix("C:/Users/Doc", '\\');
        assert_eq!(dir, "C:/Users/");
        assert_eq!(suffix, "Doc");
    }

    #[test]
    fn filter_entries_empty_suffix_returns_directories_only() {
        let entries = vec![
            DirEntry {
                name: "src".into(),
                is_dir: true,
            },
            DirEntry {
                name: "README.md".into(),
                is_dir: false,
            },
        ];
        let filtered = filter_entries(&entries, "");
        assert_eq!(filtered, vec![0]);
    }

    #[test]
    fn filter_entries_suffix_filters() {
        let entries = vec![
            DirEntry {
                name: "src".into(),
                is_dir: true,
            },
            DirEntry {
                name: "docs".into(),
                is_dir: true,
            },
            DirEntry {
                name: "README.md".into(),
                is_dir: false,
            },
        ];
        let filtered = filter_entries(&entries, "do");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0], 1); // "docs"
    }

    #[test]
    fn filter_entries_case_insensitive() {
        let entries = vec![DirEntry {
            name: "Documents".into(),
            is_dir: true,
        }];
        let filtered = filter_entries(&entries, "DOC");
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn filter_entries_no_match() {
        let entries = vec![DirEntry {
            name: "src".into(),
            is_dir: true,
        }];
        let filtered = filter_entries(&entries, "xyz");
        assert!(filtered.is_empty());
    }

    #[test]
    fn complete_path_directory() {
        let path = complete_path("/home/user/", "src", true, '/');
        assert_eq!(path, "/home/user/src/");
    }

    #[test]
    fn complete_path_file() {
        let path = complete_path("/home/user/", "README.md", false, '/');
        assert_eq!(path, "/home/user/README.md");
    }

    #[test]
    fn complete_path_windows_separator() {
        let path = complete_path(r"C:\Users\", "Doc", true, '\\');
        assert_eq!(path, r"C:\Users\Doc\");
    }

    #[test]
    fn parent_directory_posix() {
        assert_eq!(parent_directory("/home/user/", '/'), "/home/");
        assert_eq!(parent_directory("/home/", '/'), "/");
        assert_eq!(parent_directory("/", '/'), "/");
    }

    #[test]
    fn parent_directory_windows() {
        assert_eq!(parent_directory(r"C:\Users\rain\", '\\'), r"C:\Users\");
        assert_eq!(parent_directory(r"C:\Users\", '\\'), r"C:\");
        assert_eq!(parent_directory(r"C:\", '\\'), "");
        assert_eq!(parent_directory("", '\\'), "");
    }

    #[test]
    fn drive_mask_becomes_sorted_drive_entries() {
        let entries = drive_entries((1 << 0) | (1 << 2) | (1 << 25));
        let names: Vec<_> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, vec!["A:", "C:", "Z:"]);
        assert!(entries.iter().all(|entry| entry.is_dir));
    }
}
