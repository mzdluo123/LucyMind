## Context

项目已有 `Host` 抽象负责本地/WSL 命令执行。GitHub CLI 已处理 GitHub API、认证、enterprise host 和 remote 解析，因此无需在应用中新增 HTTP client 或保存凭据。

## Decisions

### D1: 使用 `gh pr view --json`

以当前 worktree 为 cwd 执行命令，让 `gh` 根据该目录检出的 branch 定位 PR。解析 PR 基本状态、评审决定和 checks rollup。

### D2: 所有查询失败均退化为无 PR

此功能是补充信息，不应因 `gh` 缺失、未登录或无 PR 产生错误提示，也不覆盖常规操作状态。

### D3: PR 信息独立于动作状态

状态栏左侧继续显示动作反馈，右侧独立显示 PR。PR 查询不会写入现有 `Status`。

PR 入口使用 Lucide `git-pull-request` 图标，状态使用独立图标表达：草稿、合并、关闭、检查失败、检查中、需修改、批准和 Open。完整状态文字只在 hover tooltip 中显示。

### D4: 异步查询带请求序号

每次 active worktree 变化时清空 PR 并递增序号。后台结果只在序号和 active path 都匹配时提交，避免旧 branch 的慢响应覆盖新 branch。
