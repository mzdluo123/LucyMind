//! 可复用按钮组件 —— 消除各面板里手写重复的 `.px().py().bg().border_1()…` 样式。
//!
//! 设计语言(见 [`crate::theme`]):无彩 —— 深灰底 + 细描边 + 2px 微圆角,
//! 悬浮/按下靠明度微差,不用彩色强调块。语义变体(错误/成功)只把描边与
//! 文字染成极克制的冷红/冷绿,底仍是深灰。

use gpui::{
    div, prelude::*, px, rgb, ClickEvent, ElementId, IntoElement, SharedString, Stateful, Styled,
    Window,
};

use crate::theme;

/// 按钮语义变体 —— 决定描边/文字色。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    /// 默认:中性深灰底 + 中性描边 + 主文字。
    Neutral,
    /// 危险动作(如「丢弃并关闭」):冷红描边 + 冷红字。
    Danger,
    /// 确认动作(如「保存」):冷绿描边 + 冷绿字。
    Confirm,
}

impl ButtonVariant {
    /// (描边色, 文字色)。
    fn colors(self) -> (u32, u32) {
        match self {
            ButtonVariant::Neutral => (theme::BORDER, theme::TEXT),
            ButtonVariant::Danger => (theme::STATE_ERROR, theme::STATE_ERROR),
            ButtonVariant::Confirm => (theme::STATE_OK, theme::STATE_OK),
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
            btn = btn.child(gpui::svg().size(px(16.0)).path(icon).text_color(rgb(text)));
        }

        btn = btn.child(self.label);

        if let Some(handler) = self.on_click {
            btn = btn.on_click(handler);
        }

        btn
    }
}
