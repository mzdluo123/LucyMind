//! 可复用按钮组件 —— 消除各面板里手写重复的 `.px().py().bg().border_1()…` 样式。
//!
//! 设计语言(见 [`crate::theme`]):无彩 —— 深灰底 + 细描边 + 2px 微圆角,
//! 悬浮/按下靠明度微差,不用彩色强调块。主操作用更亮的灰阶描边与文字，
//! 只有危险操作保留冷红语义色。

use gpui::{
    div, prelude::*, px, rgb, App, ClickEvent, ElementId, IntoElement, SharedString, Stateful,
    Styled, Window,
};
use gpui_component::tooltip::Tooltip;

use crate::theme;

/// 按钮语义变体 —— 决定描边/文字色。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    /// 默认:中性深灰底 + 中性描边 + 主文字。
    Neutral,
    /// 危险动作(如「丢弃并关闭」):冷红描边 + 冷红字。
    Danger,
    /// 确认动作(如「保存」):亮冷灰文字 + 灰阶描边。
    Confirm,
}

impl ButtonVariant {
    /// (描边色, 文字色)。
    fn colors(self) -> (u32, u32) {
        match self {
            ButtonVariant::Neutral => (theme::BORDER, theme::TEXT),
            ButtonVariant::Danger => (theme::STATE_ERROR, theme::STATE_ERROR),
            ButtonVariant::Confirm => (theme::TEXT_FAINT, theme::TEXT_BRIGHT),
        }
    }
}

/// 点击回调类型(签名与 gpui 的 `on_click` 一致)。
type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static>;

/// 一个无彩风按钮。用 [`button`] 构造,链式设置后转成元素。
pub struct Button {
    id: ElementId,
    label: SharedString,
    variant: ButtonVariant,
    /// 左侧图标资源路径(如 `icons/claude.svg`),None = 无图标。
    icon: Option<SharedString>,
    /// 点击回调,None = 无(纯展示)。
    on_click: Option<ClickHandler>,
}

/// 构造一个按钮。`id` 需在其父容器内唯一(gpui 交互元素要求)。
pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button {
        id: id.into(),
        label: label.into(),
        variant: ButtonVariant::Neutral,
        icon: None,
        on_click: None,
    }
}

/// 固定尺寸的图标按钮，适合导航和工具栏动作。
pub fn icon_button(
    id: impl Into<ElementId>,
    icon: impl Into<SharedString>,
    tooltip: impl Into<SharedString>,
) -> IconButton {
    IconButton {
        id: id.into(),
        icon: icon.into(),
        tooltip: tooltip.into(),
        disabled: false,
        on_click: None,
    }
}

pub struct IconButton {
    id: ElementId,
    icon: SharedString,
    tooltip: SharedString,
    disabled: bool,
    on_click: Option<ClickHandler>,
}

impl IconButton {
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        if !self.disabled {
            self.on_click = Some(Box::new(handler));
        }
        self
    }
}

impl IntoElement for IconButton {
    type Element = Stateful<gpui::Div>;

    fn into_element(self) -> Self::Element {
        let disabled = self.disabled;
        let tooltip = self.tooltip;
        let mut element = div()
            .id(self.id)
            .flex_none()
            .size(px(32.0))
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(rgb(theme::BORDER))
            .rounded(theme::radius())
            .bg(rgb(theme::BTN_BG))
            .text_color(rgb(if disabled {
                theme::TEXT_FAINT
            } else {
                theme::TEXT
            }))
            .when(!disabled, |this| {
                this.cursor_pointer()
                    .hover(|style| style.bg(rgb(theme::BTN_BG_HOVER)))
            })
            .child(
                gpui::svg()
                    .flex_none()
                    .size(px(16.0))
                    .path(self.icon)
                    .text_color(rgb(if disabled {
                        theme::TEXT_FAINT
                    } else {
                        theme::TEXT
                    })),
            )
            .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx));

        if let Some(handler) = self.on_click {
            element = element.on_click(handler);
        }
        element
    }
}

impl Button {
    /// 设语义变体(默认 Neutral)。
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// 左侧图标(单色 SVG,跟按钮文字色染色)。
    #[allow(dead_code)]
    pub fn icon(mut self, path: impl Into<SharedString>) -> Self {
        self.icon = Some(path.into());
        self
    }

    /// 点击回调。签名与 gpui 的 `on_click` 一致,便于直接传 `cx.listener(...)`。
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl IntoElement for Button {
    type Element = Stateful<gpui::Div>;

    fn into_element(self) -> Self::Element {
        let (border, text) = self.variant.colors();

        let mut btn = div()
            .id(self.id)
            .flex()
            .flex_row()
            .items_center()
            .gap(theme::space_sm())
            .px(theme::space_md())
            .py(theme::space_sm())
            .bg(rgb(theme::BTN_BG))
            .border_1()
            .border_color(rgb(border))
            .rounded(theme::radius())
            .text_color(rgb(text))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(theme::BTN_BG_HOVER)));

        if let Some(icon) = self.icon {
            // 单色 SVG 必须设 text_color 才显形,跟按钮文字色染色。
            btn = btn.child(
                gpui::svg()
                    .flex_none()
                    .size(px(16.0))
                    .path(icon)
                    .text_color(rgb(text)),
            );
        }

        btn = btn.child(self.label);

        if let Some(handler) = self.on_click {
            btn = btn.on_click(handler);
        }

        btn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirm_button_is_grayscale() {
        assert_eq!(
            ButtonVariant::Confirm.colors(),
            (theme::TEXT_FAINT, theme::TEXT_BRIGHT)
        );
        assert_ne!(ButtonVariant::Confirm.colors().0, theme::STATE_OK);
        assert_ne!(ButtonVariant::Confirm.colors().1, theme::STATE_OK);
    }
}
