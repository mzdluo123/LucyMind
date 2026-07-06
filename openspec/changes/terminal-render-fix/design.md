## Context

终端渲染管线(`crates/app/src/terminal_view.rs::paint_grid`)逐行遍历 `RenderSnapshot` 的 cell 网格,每个 cell 独立调用 `window.text_system().shape_line()` 绘制。行高是硬编码常量 `LINE_HEIGHT = 20.0`(px),字体大小 `FONT_SIZE = 14.0`。

三个问题:

1. **垂直断裂**:GPUI `paint_line` 内部做 `padding_top = (line_height - ascent - descent) / 2`,即文字在 `line_height` 框内垂直居中。`LINE_HEIGHT=20` > 字体 `ascent+descent≈16`,每行上下各留 ~2px,`│` 这类需跨行连接的 box-drawing 字符在行间断裂。

2. **水平断裂**:逐 cell `shape_line` 意味着每个字符独立 shaping,字体引擎无法做字间调整(kerning / advance 一致性)。batch shaping(同样式连续 cell 拼成一个字符串)让字体引擎一次性处理整段,字符间距由字体 advance 决定,水平方向连续。

3. **Windows PTY shim**:alacritty 的 `tty::new` 最终调 `CreateProcessW`,只能执行 `.exe`。npm/pnpm 全局安装的 CLI(`codex`/`claude` 等)只有 `.cmd`/`.ps1`/bash shim,`CreateProcessW` 找不到文件报 os error 2,`TerminalView::new` 的 `.expect()` 直接 panic。

约束:
- `ShapedLine` 通过 `Deref` 到 `LineLayout`,暴露 `.ascent`/`.descent`/`.width`,可在 paint 阶段测量。
- `LINE_HEIGHT` 常量仍被 `on_scroll` 的像素转行计算引用,需改为用 `self.line_h`(运行时值)。
- `Cascadia Mono` 是 Windows 10+ 自带字体(VS Code / Windows Terminal 默认),box-drawing 字符专为终端设计。

## Goals / Non-Goals

**Goals:**
- box-drawing 字符(`│╮╰╯─`)在垂直和水平方向无缝连接。
- Windows 上 npm 全局 CLI(`.cmd` shim)能通过 PTY 正常启动。
- 行高自适应字体,不硬编码。

**Non-Goals:**
- 不改 alacritty 内核的 cell 渲染逻辑(只改 app 层 paint)。
- 不做字体配置 UI(字体名硬编码按平台选择)。
- 不处理 `.ps1` shim 的 PowerShell 执行策略(`cmd.exe /C` 能处理 `.cmd`,`.ps1` 需用户自行确保 `codex.cmd` 存在)。

## Decisions

### D1:行高从 probe `ascent + descent` 动态测量

在 `TerminalElement::paint` 中,已有 probe `shape_line("0")` 测 `cell_w`。同时从 probe 读 `ascent + descent` 作为 `line_height`。回退:`ascent+descent == 0` 时用 `FONT_SIZE * 1.25`。

这使行高与字体实际高度完全贴合,GPUI 的 `padding_top = (line_height - ascent - descent) / 2 = 0`,文字紧贴行顶,`│` 跨行无间隙。

**备选(否决)**:硬编码 `LINE_HEIGHT=16`——不同字体/DPI 下 ascent+descent 不同,硬编码仍可能不匹配。

### D2:文字 batch shaping

`paint_grid` 文字绘制从「逐 cell shape_line」改为「同样式(fg + bold)连续 cell 拼成字符串,一次 shape_line」。空格和 `width==0` 的 spacer 作为 batch 断点。字体引擎一次性 shaping 整段文本,字符 advance 由字体决定,水平方向无亚像素累积误差。

### D3:字体改为 Cascadia Mono

`mono_font_family()` Windows 分支从 `"Consolas"` 改为 `"Cascadia Mono"`。Cascadia Mono 专为终端设计,box-drawing 字符与等宽网格对齐精确。macOS/Linux 不变(Menlo/DejaVu Sans Mono 已足够)。

### D4:Windows PTY `cmd.exe /C` 包装

`TerminalSession::spawn` 在 `#[cfg(target_family = "windows")]` 下,检测 `command` 的扩展名:非 `.exe`(无扩展名或 `.cmd`/`.ps1` 等)则包装成 `cmd.exe /C <program> <args>`。`cmd.exe` 会按 `PATHEXT` 解析 bare name 到 `.cmd`/`.bat`。`.exe` 直接执行不包装。

`needs_cmd_wrapper(program)` 判断:有扩展名且不等于 `.exe`(忽略大小写)→ true;无扩展名 → true(bare name,让 cmd.exe 解析 PATHEXT)。

### D5:codex 参数 `--dangerously-bypass-approvals-and-sandbox`

codex CLI 当前版本(v0.142.5)移除了 `--full-auto`,改为 `--dangerously-bypass-approvals-and-sandbox`。worktree 本身是隔离边界,codex 自带沙箱多余且可能阻断 git 操作。builtin 注册表和 dogfood `.worktree.toml` 同步更新。

## Risks / Trade-offs

- **[Cascadia Mono 未安装的 Windows]** → Windows 10 1903+ 自带;旧系统回退到系统默认等宽(GPUI 字体回退链)。后续可加字体回退列表。
- **[动态行高与 PTY resize]** → 行高变化时 `maybe_resize` 用新 `line_h` 算 `rows`,PTY 收到正确 SIGWINCH/resize,agent 重排版。已由现有 `maybe_resize` 处理。
- **[cmd.exe /C 包装的副作用]** → `cmd.exe` 会多一层进程,`Ctrl+C` 信号传递可能延迟一层。worktree 场景下 agent 通常自己处理信号,影响可忽略。`.exe` 命令不包装,无影响。
- **[batch shaping 的性能]** → 同样式连续 cell 一次 shape_line,减少 `shape_line` 调用次数(从 N 个 cell → M 个 run,M << N),性能提升。
