## 1. 尾随空格修剪 + 词边界工具函数

- [x] 1.1 在 `terminal_view.rs` 修改 `selected_text()`:每行拼完后 `trim_end()` 去尾随空格;空行保留为空串(不删除)
- [x] 1.2 新增 `fn word_boundary(ch: char) -> bool`:`!(ch.is_alphanumeric() || ch == '_')`
- [x] 1.3 新增 `fn select_word_at(&mut self, pos: CellPos) -> Option<(CellPos, CellPos)>`:从点击 cell 向左右扩展到词边界,返回选区(整行无非词字符则返回 None)
- [x] 1.4 新增 `fn select_line_at(&mut self, pos: CellPos) -> Option<(CellPos, CellPos)>`:col 0 到最后一个非空格 cell,整行空白返回 None
- [x] 1.5 单元测试:`trim_end` 逻辑(纯函数,可提取为 `fn trim_line(s: &str) -> &str` 测试)、`word_boundary` 判定

## 2. 双击选词 + 三击选行

- [x] 2.1 `on_mouse_down`:检查 `event.click_count`——`1` 走现有逻辑(含 Shift+扩展,见 task 4);`2` 调 `select_word_at` 设选区并 `copy_selection`;`3` 调 `select_line_at` 设选区并 `copy_selection`
- [x] 2.2 双击/三击设选区后不进入 `is_selecting` 拖动模式(离散操作,不需拖动)
- [x] 2.3 双击/三击后选区保留(不清除),用户可 Shift+扩展或手动 Cmd+C
- [x] 2.4 双击落在非词字符(空格/标点)上:不设选区,不清除已有选区(避免双击空格意外清选区)
- [x] 2.5 手动验证:双击词选中高亮+复制;三击行选中高亮+复制

## 3. Copy-on-select(释放即复制)

- [x] 3.1 `on_mouse_up`:拖选释放时若 `is_selecting == true` 且选区非空(起点 != 终点),调 `copy_selection` + 触发复制反馈(task 6)
- [x] 3.2 IME 组合中(`ime_preedit` 非空)不触发 copy-on-select
- [x] 3.3 `is_selecting` 在释放后置 false(现有行为保留),选区不清除(保留高亮供用户确认)
- [x] 3.4 手动验证:拖选一段文本,释放后无需 Cmd+C 即已在剪贴板

## 4. Shift+点击扩展选区

- [x] 4.1 `on_mouse_down`:click_count==1 且 `event.modifiers.shift == true` 时,若已有选区则只更新终点(`selection = Some((原起点, 新cell))`),不置 `is_selecting`;若无选区则当作普通点击
- [x] 4.2 Shift+扩展后触发 copy-on-select(扩展即复制新选区)
- [x] 4.3 手动验证:先拖选一段,Shift+点击更远位置,选区扩展到新位置

## 5. 全选(Cmd+A / Ctrl+Shift+A)

- [x] 5.1 `on_key`:新增 `select_all_combo = (ks.modifiers.platform && ks.key == "a") || (ks.modifiers.control && ks.modifiers.shift && ks.key == "a")`
- [x] 5.2 `select_all_combo` 为 true 时:设 `selection = Some((CellPos{row:0,col:0}, CellPos{row:rows-1,col:cols}))`,return(不送 PTY)
- [x] 5.3 确保 Ctrl+A(无 Shift)不被拦截——仍走 `keystroke_to_bytes` 送 PTY(readline 行首)
- [x] 5.4 全选后不自动复制(用户后续 Cmd+C 或 copy-on-select on release)
- [x] 5.5 手动验证:Cmd+A 全选可视区;Ctrl+A 仍送行首到 shell

## 6. 复制视觉反馈

- [x] 6.1 `TerminalView` 新增字段 `copy_flash: Option<f32>`(剩余秒数,None=无反馈)
- [x] 6.2 `copy_selection` 成功复制后设 `copy_flash = Some(0.3)`(300ms)
- [x] 6.3 后台 16ms 轮询线程(已有,在 `TerminalView::new`)每帧递减 `copy_flash`,`<= 0` 时置 None 并 `cx.notify()`
- [x] 6.4 `paint_grid` 选区高亮:若 `copy_flash` 为 Some,用更亮 alpha(如 0.9)替代正常 `SELECTION_ALPHA`(0.55)
- [x] 6.5 无选区或空选区时不闪(`copy_selection` 内部判断 text 非空才设 flash)
- [x] 6.6 手动验证:复制后选区短暂变亮 ~300ms 后恢复

## 7. 右键上下文菜单

- [x] 7.1 `TerminalView` 新增字段 `context_menu_open: bool` + `context_menu_pos: Point<Pixels>`
- [x] 7.2 `Render::render`:注册 `on_mouse_down(MouseButton::Right)` → 设 `context_menu_open=true`、`context_menu_pos=ev.position`,不转发右键到 PTY
- [x] 7.3 新增 `context_menu()` 渲染方法:半透明遮罩(点击关闭)+ 卡片(三项:Copy / Paste / Select All)。Copy 项在有选区时可点(正常文字色),无选区时灰显(`TEXT_FAINT` + 不可点击)
- [x] 7.4 菜单项点击:Copy → `copy_selection`;Paste → `paste_clipboard`;Select All → 全选。执行后 `context_menu_open=false`
- [x] 7.5 Esc 关闭菜单:`on_key` 中若 `context_menu_open` 且按 Esc,置 false 并 return(不送 PTY)
- [x] 7.6 菜单打开时,左键点击任意位置(含终端区)先关闭菜单(不触发选区/其他操作)——在 `on_mouse_down(MouseButton::Left)` 开头检查 `context_menu_open`
- [x] 7.7 菜单视觉:用 `theme` tokens(BG/SURFACE/BORDER/TEXT/TEXT_FAINT),2px 圆角,与 agent_menu overlay 风格一致
- [x] 7.8 手动验证:右键弹出菜单;有/无选区时 Copy 状态;三项功能正确;点外/Esc 关闭

## 8. 验证

- [x] 8.1 `cargo fmt && cargo clippy --all-targets` 无 warning
- [x] 8.2 `cargo test -p lucy-app` 通过(含新增的纯函数单测)
- [x] 8.3 `cargo run -p lucy-app`:手动验证全部场景——双击选词、三击选行、拖选释放即复制、Cmd+A 全选、Ctrl+A 不拦截、Shift+点击扩展、右键菜单三项、复制反馈闪烁、尾随空格已修剪(粘贴到外部编辑器确认)
