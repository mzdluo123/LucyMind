//! Shared button keyboard behavior: focused Enter/Space follows the click path.

use gpui::{
    point, prelude::*, px, size, Context, KeyUpEvent, Keystroke, Modifiers, Render, TestAppContext,
    Window,
};

use lucy_app::ui::{button, icon_button};

struct TextButtonHarness {
    activations: usize,
}

impl Render for TextButtonHarness {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        button("keyboard-text-button", "Activate").on_click(cx.listener(
            |this, _event, _window, cx| {
                this.activations += 1;
                cx.notify();
            },
        ))
    }
}

struct IconButtonHarness {
    activations: usize,
}

impl Render for IconButtonHarness {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        icon_button("keyboard-icon-button", "icons/refresh-cw.svg", "Refresh").on_click(
            cx.listener(|this, _event, _window, cx| {
                this.activations += 1;
                cx.notify();
            }),
        )
    }
}

fn draw<V: Render>(window: &mut gpui::VisualTestContext, view: &gpui::Entity<V>) {
    window.draw(
        point(px(0.0), px(0.0)),
        size(px(320.0), px(160.0)),
        |_, _| view.clone(),
    );
}

#[gpui::test]
async fn shared_text_button_activates_with_enter(cx: &mut TestAppContext) {
    let (view, window) = cx.add_window_view(|_window, _cx| TextButtonHarness { activations: 0 });
    draw(window, &view);

    window.simulate_click(point(px(12.0), px(12.0)), Modifiers::none());
    assert_eq!(window.read(|cx| view.read(cx).activations), 1);
    window.simulate_event(KeyUpEvent {
        keystroke: Keystroke::parse("enter").unwrap(),
    });
    assert_eq!(window.read(|cx| view.read(cx).activations), 2);
}

#[gpui::test]
async fn shared_icon_button_activates_with_space(cx: &mut TestAppContext) {
    let (view, window) = cx.add_window_view(|_window, _cx| IconButtonHarness { activations: 0 });
    draw(window, &view);

    window.simulate_click(point(px(12.0), px(12.0)), Modifiers::none());
    assert_eq!(window.read(|cx| view.read(cx).activations), 1);
    window.simulate_event(KeyUpEvent {
        keystroke: Keystroke::parse("space").unwrap(),
    });
    assert_eq!(window.read(|cx| view.read(cx).activations), 2);
}
