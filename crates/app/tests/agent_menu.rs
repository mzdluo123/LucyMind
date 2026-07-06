//! agent 菜单:打开/关闭/项数。

use gpui::TestAppContext;

use lucy_core::agent::builtin_agents;

use common::{build_workspace, shutdown_workspace, temp_repo};

mod common;

#[gpui::test]
async fn open_agent_menu(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _w) = build_workspace(cx, Some(repo.clone()));
    cx.run_until_parked();

    // 直接打开菜单(绕过 UI 点击)。
    cx.update(|cx| {
        workspace.update(cx, |v, cx| v.open_agent_menu_for_test(cx));
    });
    cx.run_until_parked();
    assert!(
        cx.read(|cx| workspace.read(cx).is_agent_menu_open()),
        "agent menu should be open"
    );

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn builtin_agents_count(_cx: &mut TestAppContext) {
    // 菜单项数来自 builtin_agents() —— 数据驱动,新增 agent 只改注册表。
    let agents = builtin_agents();
    assert!(
        agents.len() >= 3,
        "should have at least claude/codex/opencode, got {}",
        agents.len()
    );
    // 确保三个都在。
    let names: Vec<&str> = agents.iter().map(|a| a.name).collect();
    assert!(names.contains(&"claude"), "claude missing: {names:?}");
    assert!(names.contains(&"codex"), "codex missing: {names:?}");
    assert!(names.contains(&"opencode"), "opencode missing: {names:?}");
}
