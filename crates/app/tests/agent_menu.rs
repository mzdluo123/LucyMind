//! builtin agent 注册表数据驱动测试(agent 菜单已移除,agent 按钮在 tab 栏)。

use gpui::TestAppContext;

use lucy_core::agent::builtin_agents;

#[gpui::test]
async fn builtin_agents_count(_cx: &mut TestAppContext) {
    // tab 栏 agent 按钮项数来自 builtin_agents() —— 数据驱动,新增 agent 只改注册表。
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
