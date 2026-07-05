//! 可复用、无状态的 UI 组件 —— 与具体业务视图解耦,只吃参数、吐元素。
//!
//! 设计语言集中在 [`crate::theme`];这里的组件把「按主题拼样式」的重复代码
//! 收敛成一处,各面板(见 `workspace/`)只描述结构与回调,不再手抄样式。
//!
//! - [`button`] 无彩风按钮(中性 / 危险 / 确认三种语义变体,可带图标)
//! - [`dialog`] 模态弹窗骨架(遮罩 + 居中卡片 + 底部按钮行)

pub mod button;
pub mod dialog;

pub use button::{button, ButtonVariant};
pub use dialog::{button_row, modal};
