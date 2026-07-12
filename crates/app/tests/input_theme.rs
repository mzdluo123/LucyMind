//! gpui-component Input 与 LucyMind 主题的集成测试。

use gpui::{point, px, rgb, size, TestAppContext};
use gpui_component::Theme;

use common::{build_workspace, shutdown_workspace, temp_repo};
use lucy_app::theme;

mod common;

#[gpui::test]
async fn component_input_uses_lucymind_theme_tokens(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, _window) = build_workspace(cx, Some(repo));

    cx.read(|cx| {
        let component = Theme::global(cx);
        assert_eq!(component.background, rgb(theme::BG).into());
        assert_eq!(component.foreground, rgb(theme::TEXT).into());
        assert_eq!(component.muted_foreground, rgb(theme::TEXT_FAINT).into());
        assert_eq!(component.input, rgb(theme::BORDER).into());
        assert_eq!(component.ring, rgb(theme::TEXT_DIM).into());
        assert_eq!(component.caret, rgb(theme::TEXT_BRIGHT).into());
        assert_eq!(
            component.selection,
            theme::with_alpha(theme::SELECTION, theme::SELECTION_ALPHA)
        );
        assert_eq!(component.radius, theme::radius());
        assert_eq!(component.font_family.as_ref(), theme::FONT_UI);
        assert!(!component.shadow);
        assert!(component.mode.is_dark());
    });

    shutdown_workspace(cx, &workspace);
}

#[gpui::test]
async fn themed_input_and_confirm_button_render_in_repository_picker(cx: &mut TestAppContext) {
    let (_dir, repo) = temp_repo();
    let (workspace, window) = build_workspace(cx, Some(repo));

    window.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.open_repo_picker_for_test(window, cx);
        });
    });
    window.run_until_parked();

    window.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| workspace.clone(),
    );

    let picker_open = window.update(|_window, cx| workspace.read(cx).path_picker_open());
    assert!(
        picker_open,
        "repository picker should remain open after its Input is painted"
    );

    shutdown_workspace(cx, &workspace);
}
