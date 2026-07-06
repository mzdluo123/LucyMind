## Context

终端复制功能目前(`crates/app/src/terminal_view.rs`)只有最基础的能力:

- **选区**:`selection: Option<(CellPos, CellPos)>` 记录起点/终点 cell;`on_mouse_down` 置起点,`on_mouse_move` 更新终点,`on_mouse_up` 清除 `is_selecting` 标志(单击起点==终点清选区)。
- **复制**:`copy_selection()` 调 `selected_text()` 抽文本 → `cx.write_to_clipboard`。快捷键 Cmd+C / Ctrl+Shift+C。
- **粘贴**:Cmd+V / Ctrl+Shift+V,走 bracketed-paste。
- **`selected_text()`**:逐行遍历选区,`cell.ch` 拼接,`width==0` 跳过(宽字符 spacer),行间插 `\n`。**不修剪尾随空格**——复制一行会带上到行尾的所有空格。

缺失:双击选词、三击选行、copy-on-select、尾随空格修剪、全选、Shift+扩展选区、右键菜单、复制反馈。

约束:
- 终端渲染是自定义 `TerminalElement`(非 GPUI canvas),鼠标坐标 → cell 映射已有(`cell_at`)。
- `RenderSnapshot` 提供 `cell(row, col) -> RenderCell`(含 `ch`/`width`),词边界检测可直接用。
- GPUI 的 `on_mouse_down` 事件有 `click_count`(1/2/3),无需自己计时窗口。
- 右键菜单 overlay 机制已在 `workspace/mod.rs`(agent 菜单用 `absolute()` + 遮罩),可复用同模式。

## Goals / Non-Goals

**Goals:**
- 双击选词、三击选行,选后自动复制。
- 拖选释放即复制(copy-on-select)。
- 复制文本尾随空格修剪(每行)。
- 全选当前可视屏。
- Shift+点击扩展选区。
- 右键上下文菜单(Copy / Paste / Select All)。
- 复制成功有视觉反馈(选区短暂变色)。

**Non-Goals:**
- 不做矩形/块选(Alt+拖选)——alacritty 自己不做块选,实现复杂且非「标准终端」必备。
- 不做跨 scrollback 全选(只选可视屏)——scrollback 选择涉及滚动+选区锚点漂移,复杂度高,收益低。
- 不改 terminal crate 的 `RenderSnapshot` 结构(词边界/文本提取全在 app 层用现有 `cell()`)。
- 不做选区的键盘导航(Vim 式)——超范围。
- 不做剪贴板历史。
- 不改粘贴逻辑(现有 bracketed-paste 已够用)。

## Decisions

### D1:点击计数用 GPUI 的 `click_count`,不自建计时

GPUI `MouseDownEvent` 有 `click_count: usize`(平台级双击/三击检测,含计时窗口)。`on_mouse_down` 直接 match `click_count`:
- 1:现有逻辑(开始拖选或 Shift+扩展)
- 2:选词
- 3:选行

**备选(否决)**:自建计时(记录上次点击时间,300ms 内累加计数)——重复造轮子,平台计时更准(考虑了系统双击速度设置)。

### D2:词边界 = `is_alphanumeric` 或 `_` 的连续序列

双击时,从点击的 cell 向左右扩展,直到遇到非「字母数字下划线」字符或行/屏边界。这与 iTerm2 / alacritty 默认行为一致。

```
fn word_boundary(ch: char) -> bool {
    !(ch.is_alphanumeric() || ch == '_')
}
```

从 `cell_at(pos)` 得到 `(row, col)`,向左走 `while col > 0 && !word_boundary(cell(row, col-1).ch)` 得 `start_col`;向右走 `while col < cols && !word_boundary(cell(row, col).ch)` 得 `end_col`。选区 = `(CellPos{row, start_col}, CellPos{row, end_col})`。

**备选(否决)**:以空格为边界——太粗(双击 `foo.bar` 会选 `foo.bar` 而非 `foo`)。以标点+空格为边界——太细(双击 `foo` 紧邻标点时可能只选到边界)。`is_alphanumeric` 是终端界的常识默认。

### D3:三击选行 = col 0 到行尾有效内容

三击选中从 col 0 到「该行最后一个非空格 cell」的整行。不用 `snap.cols`(会含大量尾随空格),而是从右往左找第一个 `width != 0 && ch != ' '` 的 cell 作为 `end_col`。如果整行空白则选区为空。

**备选(否决)**:选 col 0 到 `snap.cols`——会让选区视觉上覆盖整行空格,且复制时也是纯空格。选中到有效内容末尾更实用。

### D4:copy-on-select 在 `on_mouse_up` 执行

拖选释放(`on_mouse_up`)时,如果选区非空(起点 != 终点),自动调 `copy_selection`。双击/三击的选区也在各自的 `on_mouse_down` 处理中直接复制(不等释放,因为双击/三击不需要拖动)。

单击(起点==终点)仍清除选区,且不复制(现有行为)。

**注意**:copy-on-select 与手动 Cmd+C 共存。手动 Cmd+C 仍可用(选区为空时 no-op)。copy-on-select 是「释放即复制」,不阻止后续手动复制。

### D5:尾随空格修剪在 `selected_text` 中做

`selected_text()` 当前逐行拼字符,行间插 `\n`。改为:每行拼完后 `trim_end()` 去掉尾随空格。这影响所有复制路径(手动 Cmd+C、copy-on-select、双击/三击),一处改全受益。

**注意**:不 trim 行首空格(缩进可能有意义),不 trim 中间空格。只去行尾。空行(trim 后为空)保留为空串,`selected_text` 仍返回 `Some("")`——由调用方判断是否跳过复制。

### D6:全选 = 选整个可视屏

Cmd+A(macOS)/ Ctrl+A(其他平台习惯,但终端里 Ctrl+A 是行首,有冲突)→ **只用 Cmd+A**,不绑 Ctrl+A。

选区设为 `(CellPos{row:0, col:0}, CellPos{row: rows-1, col: snap.cols})`。alt-screen 下也选当前屏(无 scrollback)。不跨 scrollback(选了看不见的内容没意义)。

**Ctrl+A 冲突说明**:终端程序(vim/bash readline)用 Ctrl+A 做行首,Cmd+A 在 macOS 不冲突。Windows/Linux 终端习惯用 Ctrl+Shift+A 全选(避免 Ctrl+A 冲突)。绑 `Cmd+A || (Ctrl+Shift+A)`。

### D7:Shift+点击扩展选区

`on_mouse_down` 时检查 `event.modifiers.shift`:
- 有选区 + Shift → 更新选区终点为点击 cell,起点不变。不置 `is_selecting`(不进入拖动模式——Shift+点击是离散操作,不是拖动起点)。
- 无选区 + Shift → 当作普通点击(开始新选区)。
- 有选区 + 无 Shift → 现有行为(开始新选区)。

### D8:右键上下文菜单——TerminalView 自建 overlay

右键(`MouseButton::Right`)在终端区点击 → 弹出小菜单。菜单由 TerminalView 自建(类似 agent_menu 的 `absolute()` overlay + 遮罩),不经过 `WorkspaceView`——因为菜单内容(复制/粘贴/全选)完全与终端相关,且 TerminalView 自有选区/剪贴板状态。

菜单项:
- **Copy**(有选区时可点,无选区时灰显):执行 `copy_selection` 并关闭。
- **Paste**:执行 `paste_clipboard` 并关闭。
- **Select All**:执行全选并关闭。

菜单位置:右键点击位置附近(偏移一点不挡光标)。状态:`context_menu_open: bool` + `context_menu_pos: Point<Pixels>`。

**备选(否决)**:把菜单状态提到 `WorkspaceView`——跨层传递终端选区状态、右键坐标,增加耦合。TerminalView 自建更内聚。

### D9:复制视觉反馈——选区短暂变色

复制成功后,设 `copy_flash: Option<f32>`(剩余时间,秒)。`paint` 时若 `copy_flash > 0`,选区高亮用更亮的颜色(如 `SELECTION` alpha 提升到 0.9)。用一个 timer(或每帧递减)在 ~300ms 后归零。

简化实现:复制时记录 `copy_flash = 0.3`(秒);后台 16ms 轮询线程(已有)每帧减 `0.016`,归零时 `cx.notify()` 重绘。不需要单独 timer。

**备选(否决)**:GPUI 动画 API——增加复杂度,16ms 轮询已有,复用即可。

### D10:词边界检测在 app 层,不改 terminal crate

`RenderSnapshot::cell(row, col)` 已返回 `RenderCell { ch, width, ... }`。词边界检测只需读 `cell.ch` 的 `is_alphanumeric()`,纯 app 层逻辑,不需要 terminal crate 新增方法。

## Risks / Trade-offs

- **[copy-on-select 与 IME 冲突]** → IME 组合中(`ime_preedit` 非空)不触发 copy-on-select(拖选在 IME 组合中极少发生,但防御性判断)。
- **[双击/三击计时窗口]** → 依赖 GPUI `click_count`,平台级计时(含系统双击速度)。Linux/Wayland 下 GPUI 的 `click_count` 可靠性待验证;若不可靠可回退到自建计时。
- **[copy-on-select 与选区清除]** → 释放后选区保留(不清除),仅复制。用户可继续 Shift+扩展或手动 Cmd+C 再复制。选区在下次单击(新选区起点)时被覆盖。
- **[右键菜单与终端右键事件]** → 终端程序可能监听右键(如 mc 的右键菜单),但 PTY 模式下右键通常不转发(终端右键是宿主 GUI 行为,非 TTY 输入)。不转发右键到 PTY。
- **[全选不含 scrollback]** → 只选可视屏。若用户滚动到历史后全选,选的是当前可视区(非全部历史)。这是 alacritty / iTerm2 的默认行为(全选只选当前屏),用户可拖选+滚动选历史。
- **[Cmd+A 与终端程序冲突]** → macOS Terminal/iTerm2 都用 Cmd+A 全选,Cmd 不进 TTY,不冲突。Ctrl+Shift+A 在 Windows Terminal 也是全选,不冲突。
