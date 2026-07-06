## 1. 动态行高

- [x] 1.1 `TerminalElement::paint`:行高从 probe `shape_line("0")` 的 `ascent + descent` 测量,替代硬编码 `px(LINE_HEIGHT)`;回退 `FONT_SIZE * 1.25`
- [x] 1.2 `on_scroll`:像素转行用 `self.line_h`(运行时值)而非 `LINE_HEIGHT` 常量

## 2. 文字 batch shaping

- [x] 2.1 `paint_grid` 文字绘制:同样式(fg + bold)连续 cell 拼成字符串,一次 `shape_line`;空格/`width==0` spacer 作为 batch 断点
- [x] 2.2 确保样式变化(fg/bg/bold)时断开 batch,各自独立 shaping

## 3. 字体改为 Cascadia Mono

- [x] 3.1 `mono_font_family()` Windows 分支从 `"Consolas"` 改为 `"Cascadia Mono"`

## 4. Windows PTY 命令包装

- [x] 4.1 `TerminalSession::spawn` 新增 `#[cfg(target_family = "windows")]` 分支:检测命令扩展名,非 `.exe` 用 `cmd.exe /C` 包装
- [x] 4.2 新增 `needs_cmd_wrapper(program: &str) -> bool`:有扩展名且非 `.exe` → true;无扩展名 → true
- [x] 4.3 移除 `workspace/mod.rs` 中误导性的 `which` 预检查(which 找到 `.cmd` 但 PTY 仍失败)
- [x] 4.4 移除 `which` 依赖(`crates/app/Cargo.toml`)

## 5. codex 参数修正

- [x] 5.1 `crates/core/src/agent/mod.rs`:builtin codex args 从 `--full-auto` 改为 `--dangerously-bypass-approvals-and-sandbox`
- [x] 5.2 `.worktree.toml`:`[agents.codex]` args 同步更新
- [x] 5.3 更新单测:`builtin_claude_codex_opencode_available_without_config` 断言新 args

## 6. 验证

- [x] 6.1 `cargo fmt && cargo clippy --all-targets` 无 warning
- [x] 6.2 `cargo test -p lucy-core -- agent` 通过(含 codex args 更新)
- [x] 6.3 `cargo run -p lucy-app`:手动验证 box-drawing 字符(│╮╰╯)无缝连接;codex 能启动(不再 os error 2)
