# LucyMind

LucyMind 是一个面向 AI 编程工作流的桌面工具。它用 Git worktree 为每个任务创建独立工作区，并在内嵌的真实终端中启动 Claude Code、Codex 或 OpenCode，让多个 Agent 可以并行工作而不会互相覆盖文件。

> 项目目前以 macOS 为主要运行平台；代码中已包含 Windows 与 WSL 适配基础，相关体验仍在持续完善。

## 1. 项目简介

在同一个仓库中同时处理多个需求时，频繁切换分支容易打断正在运行的命令，也可能让多个 Agent 修改同一份工作区。LucyMind 把一次任务组织成一条完整流程：

1. 基于指定分支创建新的 Git worktree。
2. 复制本地配置并执行初始化 hook。
3. 在 worktree 中打开一个带完整 PTY 的终端。
4. 启动所选 AI Agent，完成后安全关闭并移除 worktree。

LucyMind 使用 Rust 和 GPUI 构建，终端内核基于 `alacritty_terminal`。项目按 `core`、`terminal`、`app` 三层组织，使 Git、配置和 hook 等核心逻辑可以独立测试。

## 2. 核心能力

- **Worktree 管理**：创建、切换和安全删除 worktree，删除前检查未提交改动，并保护主仓库不被误删。
- **多终端标签页**：每个 worktree 可以打开多个 shell 或 Agent 标签页，切换 worktree 时保留各自状态。
- **内置 Agent 支持**：零配置识别 `claude`、`codex` 和 `opencode`，也可以在项目配置中覆盖命令与参数。
- **项目生命周期 Hook**：创建后执行 `post_create`，删除前执行 `pre_remove`，并向命令注入 worktree 上下文环境变量。
- **本地文件复制**：创建 worktree 时复制 `.env` 等未纳入 Git 的项目文件。
- **完整终端交互**：支持 TTY、IME、鼠标选择、复制粘贴、动态标题和终端尺寸调整。

## 3. 安装与运行

运行前需要安装：

- Git
- [Rust stable](https://www.rust-lang.org/tools/install)
- 至少一个可选的 Agent CLI：Claude Code、Codex 或 OpenCode

从源码启动：

```bash
git clone https://github.com/mzdluo123/LucyMind.git
cd LucyMind
cargo run -p lucy-app
```

从目标仓库目录运行 LucyMind 时，应用会直接打开该仓库；从其他目录或 `.app` 启动时，可以在界面中选择 Git 仓库。

macOS 可以打包为标准应用：

```bash
cargo install cargo-bundle
cargo bundle --release
```

生成的应用位于 `target/release/bundle/osx/LucyMind.app`，最低支持 macOS 11。开发与验证常用命令：

```bash
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

## 4. 使用与配置

打开仓库后，点击侧边栏的新增按钮创建 worktree。LucyMind 默认从 `main` 创建随机命名的任务分支，并将 worktree 放在主仓库旁边的独立目录中。进入 worktree 后，可通过终端栏的 `+` 菜单打开新 shell 或启动 Agent；关闭 worktree 时，如果存在未提交改动，应用会先要求确认。

在仓库根目录添加 `.worktree.toml` 可以共享团队配置：

```toml
[worktree]
location = "sibling"
dir = "../{repo}-worktrees"
default_base = "main"

[copy]
files = [".env"]

[hooks]
post_create = ["npm install"]
pre_remove = ["echo Cleaning $WORKTREE_NAME"]

[hooks.options]
fail_fast = true

[agents.codex]
command = "codex"
args = ["--dangerously-bypass-approvals-and-sandbox"]

[alias]
"feature/login" = "登录功能"
```

Hook 可以读取以下环境变量：`WORKTREE_PATH`、`WORKTREE_BRANCH`、`WORKTREE_NAME` 和 `REPO_ROOT`。Agent 预设中的 `args` 会完整覆盖内置参数；使用自动授权参数前，请确认 worktree 隔离和项目命令符合你的安全要求。

LucyMind 使用 MIT License。问题反馈和功能建议可以通过 GitHub Issues 提交。
