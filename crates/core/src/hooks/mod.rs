//! 生命周期钩子引擎(事件 → 命令,环境变量注入)。完整实现见 U4。
mod engine;
mod events;

#[allow(unused_imports)] // stub 模块,U4 填实后移除
pub use engine::*;
#[allow(unused_imports)]
pub use events::*;
