## Why

终端的复制功能目前只有最基础的「鼠标拖选 + Cmd/Ctrl+Shift+C」:单击清除选区、拖动扩展选区、快捷键复制。这远不够「好用」——其他终端(iTerm2 / Windows Terminal / alacritty / macOS Terminal)都有的标准交互缺失:双击选词、三击选行、释放即复制(copy-on-select)、选区文本含大量尾随空格、无法全选、无右键菜单。用户从其他终端迁移过来会立刻感到不便。

## What Changes

- **双击选词**:鼠标左键双击 → 自动选中光标所在的连续「词」(字母数字下划线序列,以非词字符为边界),选区高亮并自动复制到剪贴板。
- **三击选行**:鼠标左键三击 → 自动选中整行(视口行,从 col 0 到行尾有效内容),选区高亮并自动复制到剪贴板。
- **释放即复制(copy-on-select)**:鼠标拖选释放后自动把选区文本写入剪贴板,无需再按 Cmd+C。保留 Cmd/Ctrl+Shift+C 作为手动复制(选区为空时 no-op)。
- **尾随空格修剪**:复制时逐行去掉尾随空格,只保留有效文本。消除「复制一行结果粘了 70 个空格」的问题。
- **全选(Cmd/Ctrl+A)**:选中整个可视区(当前快照的 rows × cols)。alt-screen 下也选当前可视屏(无 scrollback)。
- **Shift+点击扩展选区**:已有选区时 Shift+左键点击 → 把选区终点移到点击位置(起点不变),不重置选区。
- **右键上下文菜单**:右键点击终端区 → 弹出小菜单(Copy / Paste / Select All),点击执行对应操作并关闭菜单。无选区时 Copy 项灰显。
- **选区视觉反馈**:复制成功后选区高亮闪一下(短暂变色),让用户知道已复制(类似 iTerm2 / macOS Terminal 的视觉反馈)。

## Capabilities

### New Capabilities

- `terminal-copy`: 终端文本选择与复制的完整交互——双击选词、三击选行、copy-on-select、尾随空格修剪、全选、Shift+点击扩展选区、右键上下文菜单、复制视觉反馈。

### Modified Capabilities

(无 —— `openspec/specs/` 当前为空,无既有 capability 的需求被改动。)

## Impact

- **`crates/app/src/terminal_view.rs`**:主要改动文件。新增点击计数追踪(双击/三击计时窗口)、词/行选区逻辑、copy-on-select、尾随空格修剪、全选快捷键、Shift+点击扩展、右键菜单 overlay、复制反馈动画。现有 `on_mouse_down`/`on_mouse_up`/`on_key`/`copy_selection`/`selected_text` 均需修改。
- **`crates/app/src/workspace/mod.rs`**:右键上下文菜单需要 `WorkspaceView` 层的 overlay 状态(或 TerminalView 自建 overlay,视 design 决策)。若菜单由 TerminalView 自建则 mod.rs 无改动。
- **`crates/terminal/src/session.rs`**:`RenderSnapshot` 可能新增 `cell_text(row, col)` 便捷方法或词边界查询,视实现选择。若词边界逻辑全在 app 层用现有 `cell()` 则无需改 terminal crate。
- **测试**:词边界检测、尾随空格修剪、选区扩展逻辑可在 app 层做单元测试(纯函数)。
