## Why

LucyMind 的主要工作流围绕独立 branch/worktree 展开，但用户目前必须离开应用才能确认当前 branch 是否已有 GitHub Pull Request、检查是否通过以及评审状态。

## What Changes

- 切换到一个 worktree 时，通过 GitHub CLI 异步查询其当前 branch 对应的 PR。
- 有 PR 时在状态栏右侧显示 PR 图标、编号、状态图标和标题，点击在浏览器打开 GitHub 页面；状态文字收进 tooltip。
- 没有 PR、未安装或未登录 GitHub CLI、非 GitHub 仓库、网络错误时保持静默。
- 切换 branch 后丢弃迟到的旧查询结果，避免展示错误 PR。

## Capabilities

### New Capabilities

- `github-current-branch-pr`: 查看并访问当前 worktree branch 对应的 GitHub Pull Request。

## Impact

- `crates/core/src/github.rs`: GitHub CLI 调用、JSON 解析和状态归一化。
- `crates/app/src/workspace/mod.rs`: PR 异步加载和竞态保护。
- `crates/app/src/workspace/status_bar.rs`: PR 状态入口。
- 运行环境可选依赖 `gh`;不存在时原有功能不受影响。
