## 1. Core 查询

- [x] 1.1 新增 GitHub PR 数据结构、`gh pr view` 命令和 JSON 解析。
- [x] 1.2 将 review/check/state 归一化为紧凑状态文案。

## 2. App 集成

- [x] 2.1 active worktree 变化时异步刷新 PR，并以请求序号防止竞态。
- [x] 2.2 状态栏右侧展示可点击的 PR 图标、编号、状态图标和标题；状态文字放 tooltip，无 PR 时不渲染。

## 3. 测试

- [x] 3.1 单元测试：JSON 解析、check 汇总、状态优先级和无效响应。
- [x] 3.2 UI 状态测试：有 PR 时状态可见、点击打开正确 URL，无 PR 时状态栏 PR 区域为空。
- [x] 3.3 集成测试：切换 worktree 后只接受当前 branch 的异步查询结果。

## 4. 质量门

- [x] 4.1 `cargo fmt`。
- [x] 4.2 `cargo clippy --all-targets`。
- [x] 4.3 `cargo test`。
