//! 生命周期钩子引擎(事件 → 命令,环境变量注入)。
//!
//! - [`events`] 事件枚举与执行上下文(环境变量注入,无模板)
//! - [`engine`] copy + 顺序执行 shell 命令 + fail 策略

mod engine;
mod events;

pub use engine::*;
pub use events::*;
