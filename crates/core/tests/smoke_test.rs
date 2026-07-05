//! U1 冒烟测试:确认 workspace 链接、crate 可被下游测试引用。
//! U2 起各模块补充真实测试(config_test / git_test / hooks_test / agent_test)。

#[test]
fn crate_links() {
    assert_eq!(lucy_core::config::placeholder_marker(), "config");
}
