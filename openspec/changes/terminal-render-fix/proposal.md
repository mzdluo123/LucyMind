## Why

终端渲染存在三个问题:(1) box-drawing 字符(`│╮╰╯─`)在行间断裂——行高硬编码 20px > 字体实际高度 ~16px,GPUI 垂直居中导致每行上下留空,垂直线条接不上;(2) 文字逐 cell 单独 `shape_line`,同一行相同样式的字符被拆成独立 glyph,advance 亚像素差累积导致水平断裂;(3) Windows 上 `codex`/`claude` 等 npm 全局 CLI 只有 `.cmd` shim 无 `.exe`,`CreateProcessW`(alacritty PTY)找不到文件直接 panic。

## What Changes

- **动态行高**:行高从 `shape_line` probe 的 `ascent + descent` 动态测量,替代硬编码 `LINE_HEIGHT=20`。字体实际高度贴合行高,box-drawing 字符垂直无缝连接。
- **文字 batch shaping**:同一行相同样式(fg + bold)的连续 cell 拼成一个字符串,一次性 `shape_line` 交给字体引擎 shaping。字体引擎处理字符间距,水平方向也无缝。逐 cell shaping 改为 run batching。
- **字体改为 Cascadia Mono**:Windows 默认终端字体从 `Consolas` 改为 `Cascadia Mono`(VS Code / Windows Terminal 默认),专为终端 box-drawing 设计。
- **Windows PTY 命令包装**:`TerminalSession::spawn` 在 Windows 上检测命令是否非 `.exe`(无扩展名或 `.cmd`/`.ps1`),用 `cmd.exe /C` 包装,让 `cmd.exe` 解析 `PATHEXT` 找到 shim 并执行。
- **codex 参数修正**:`--full-auto` 在当前 codex 版本已移除,改为 `--dangerously-bypass-approvals-and-sandbox`。

## Capabilities

### New Capabilities

- `terminal-render`: 终端文本渲染的行高/字体/shaping 策略,确保 box-drawing 字符在行间和行内无缝连接。

### Modified Capabilities

(无 —— `openspec/specs/` 当前为空,无既有 capability 的需求被改动。)

## Impact

- **`crates/app/src/terminal_view.rs`**:`mono_font_family` Windows 改 `Cascadia Mono`;`TerminalElement::paint` 行高从 probe `ascent+descent` 动态测量;`paint_grid` 文字绘制从逐 cell 改为 batch run shaping;`on_scroll` 像素转行用 `self.line_h` 而非常量。
- **`crates/terminal/src/session.rs`**:新增 `needs_cmd_wrapper()` 函数,Windows 上非 `.exe` 命令用 `cmd.exe /C` 包装。
- **`crates/core/src/agent/mod.rs`**:codex builtin args 从 `--full-auto` 改为 `--dangerously-bypass-approvals-and-sandbox`。
- **`.worktree.toml`**:codex dogfood 配置同步更新。
