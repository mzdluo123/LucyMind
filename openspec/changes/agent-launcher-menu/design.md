## Context

侧边栏 Agents 区(`crates/app/src/workspace/sidebar.rs:79-100`)目前硬编码一个 `for (agent, display) in [("claude","Claude"), ("codex","Codex")]` 循环,每个 agent 渲染一个全宽按钮,点击直接走 `new_worktree_and_agent`。新增 agent 要改三处(builtin、sidebar 循环、assets 图标),且按钮行数线性增长。

`AgentSpec::builtin`(`crates/core/src/agent/mod.rs:58-65`)只认 `claude`(`--dangerously-skip-permissions`)和 `codex`(空 args)。`resolve` 是 `from_config(...).or_else(builtin(...))`——配置里若有 `[agents.<name>]` 段,其 `args` **完全覆盖** builtin(见测试 `config_preset_overrides_default_claude_args`)。dogfood `.worktree.toml` 恰好为 claude/codex 各定义了空 args 预设,导致 claude 的 bypass 参数被静默丢弃。

UI 层无下拉菜单 / popover 组件;现有浮层只有全屏遮罩模态(`crates/app/src/ui/dialog.rs::modal`,用于确认关闭 / 别名编辑 / 设置)。`gpui-component` 依赖已在 app crate,但项目刻意把不稳定性圈在 app 层,菜单宜用 GPUI 原语自建以控样式。

约束:`claude` 基于 Ink,必须真 TTY(已由 terminal 层 PTY + `TERM=xterm-256color` 兜底)。worktree 本身即隔离边界,agent 在其内可放手跑。

## Goals / Non-Goals

**Goals:**
- Agents 区只一个 `+` 按钮,数量不随 agent 增多而膨胀。
- 点 `+` 弹下拉菜单,列出所有 builtin agent(图标 + 名字),选中即建 worktree 并启动。
- 新增 `opencode` agent(`opencode --auto`)。
- 三个 builtin agent 默认均以「自动工作 / bypass 权限」模式启动,零配置即无人值守。
- agent 列表数据驱动:新增 agent 只改一处注册表 + 加图标,不动 UI 循环。

**Non-Goals:**
- 不改 `resolve` 的「config 完全覆盖 builtin args」语义(刻意设计,让用户能收回权限)。
- 不做 agent 的运行时权限切换 / 菜单内编辑 args(用户仍靠 `.worktree.toml` 覆盖)。
- 不引入 gpui-component 的 PopupMenu(自建轻量 overlay 控样式与依赖边界)。
- 不改终端层、hook 引擎、session 注册表。

## Decisions

### D1:`+` 按钮放在 AGENTS 标题行右侧
AGENTS 标题行(现 `sidebar.rs:80-85`,仅有文字)改为 flex 行,右侧放 `+` 图标按钮,与 WORKTREES 标题行右侧齿轮按钮( `sidebar.rs:120-141`)视觉对称。`+` 是「新建」的通用 affordance,放在标题行比堆在内容区更符合直觉,且不占垂直空间。

**备选(否决)**:用单个全宽「New agent」按钮替代现有两按钮——仍占一行且文案不如 `+` 紧凑;标题行 `+` 更省空间、可扩展。

### D2:下拉菜单用 GPUI 原语自建 overlay
菜单是一个 `absolute()` 定位的 `div`,作为 root 的子元素叠加(与现有 modal overlay 同机制,见 `mod.rs:587-597` 的 `root.child(...)`)。结构:半透明遮罩(点击关闭)+ 贴近按钮下方的卡片(每项 = 图标 + 名字,hover 高亮,点击触发 `new_worktree_and_agent` 并关闭)。

状态:`WorkspaceView` 加 `agent_menu_open: bool`。打开 = true;选中项 / 点遮罩 / Esc = false。菜单位置固定在侧边栏 AGENTS 区下方(不需精确锚定到按钮像素,侧边栏宽度内贴左对齐即可,实现简单且够用)。

**备选(否决)**:用 `gpui-component` 的 PopupMenu / anchored overlay——引入额外组件依赖,且项目样式语言(无彩 / 2px 圆角 / 冷深色)需自定义,自建更可控。

### D3:builtin agent 注册表作为单一数据源
在 `crates/core/src/agent/mod.rs` 新增一个静态注册表(数组 / 函数返回 `&[AgentBuiltin]`),每条含:`name`(key,如 `"claude"`)、`display`(UI 名,如 `"Claude"`)、`icon`(资源 key,如 `"icons/claude.svg"`)、`command`、`args`。`AgentSpec::builtin` 改为查表;UI 菜单项也迭代此表。

这样新增 agent 只改一处注册表 + 加图标资产,sidebar 不再硬编码 agent 数组。`AgentBuiltin` 放 core 层(纯数据,无 GPUI 依赖),app 层读其 `name`/`display`/`icon` 渲染。

### D4:三个 agent 的 auto/bypass 参数
| agent | builtin args | 模式 | 理由 |
|---|---|---|---|
| claude | `--dangerously-skip-permissions` | 全 bypass | 既有;claude 无沙箱概念,跳过权限弹窗 |
| codex | `--full-auto` | 自动批准 + worktree 写沙箱 | codex 的 workspace-write 沙箱恰好锁在 cwd(worktree)内,与 LucyMind 隔离边界一致;比全 bypass 多一层网 |
| opencode | `--auto` | 自动批准非显式拒绝项 | opencode 官方 auto 模式;显式 `deny` 规则仍生效 |

codex 选 `--full-auto` 而非 `--dangerously-bypass-approvals-and-sandbox`:后者关掉 codex 自带沙箱,而 LucyMind 的 worktree 已是隔离边界,codex 沙箱叠加在 worktree 上是「 belts and suspenders」,无副作用且更安全。用户想要全 bypass 可在 `.worktree.toml` 覆盖 args。

### D5:保留 config 完全覆盖 builtin args 的语义
不改 `resolve` 合并逻辑。builtin 注册表只服务「零配置」场景(用户 `.worktree.toml` 无 `[agents.*]`)。dogfood `.worktree.toml` 已有预设,故需同步把 args 写进预设,否则 bypass 参数被空 args 覆盖丢失。这把「实际生效的参数」显式化在配置里,对 dogfood 用户可见、可改。

### D6:图标资产
新增 `crates/app/assets/icons/plus.svg`(单色 `fill="currentColor"`,跟主题染色,与现有 icons 同源风格)与 `opencode.svg`。`assets.rs` 的 `AssetSource::load` match、`list()`、`agent_icon()` 三处登记 opencode;`plus.svg` 直接在 sidebar 用路径引用(非 agent 图标,不进 `agent_icon`)。

## Risks / Trade-offs

- **[codex `--full-auto` 沙箱可能阻断某些跨 worktree 操作]** → codex workspace-write 沙箱限制写 cwd 外路径;agent 正常只在 worktree 内操作,postCreate hook 在 agent 启动前跑、不受影响。若用户 agent 需写 worktree 外,可在 `.worktree.toml` 改 args 为 `--dangerously-bypass-approvals-and-sandbox`。
- **[菜单 overlay 不精确锚定按钮位置]** → 固定贴侧边栏左对齐、在 AGENTS 标题下方,不跟随按钮像素。侧边栏窄(180-480px)内足够,实现简单。后续可改精确锚定。
- **[config 覆盖 builtin 仍可能让用户无意丢掉 bypass]** → 既有设计(D5 保留);dogfood 配置已修正;文档/注释说明「配 `[agents.*]` 需自带 args」。
- **[opencode / codex CLI 未安装时报错体验]** → 超出本变更范围(终端层 spawn 失败已有状态栏报错);后续可加「检测 agent 是否存在」。
