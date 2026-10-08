//! 工作记录本 (WorkLuLu) 的跨端数据访问层。
//!
//! 这里只放**与界面无关、与平台无关的数据访问**：建表、增删改查、备份快照。
//! 桌面版（Tauri desktop）和安卓版（Tauri mobile）都依赖它。
//!
//! # 为什么要有这一层
//!
//! 和 `worklog-core` 是同一个道理：同一份数据在两端的读写行为必须一致。
//! 各端各写一遍的话，迟早会出现这类问题：一端建库时少加了一列，
//! 另一端的查询就默默少返回一个字段；一端的附件存进库里、另一端存成数据目录下的散文件，
//! 「拷一个 .db 就是全部数据」这条承诺就只在其中一端成立 ——
//! 用户根本不会想到是端的问题，极难定位。
//!
//! # 和 core 的分工
//!
//! - `core` 是**纯计算**：数据模型与排期引擎。不碰数据库，不碰文件，不取时钟。
//! - `data` 是**纯数据访问**：允许用 rusqlite，但仍然不碰 tauri、不自己决定数据放哪儿。
//!
//! # 加代码之前先看这三条纪律
//!
//! 1. **不得依赖 tauri** —— 否则移动端就搬不动了。需要 `AppHandle` 的东西
//!    （便携模式、文件对话框、系统通知）留在各端的命令层里，不要下沉到这里。
//! 2. **不自己决定库放哪儿** —— 「数据放哪儿」是各端的事（桌面有便携模式和标准模式，
//!    安卓是 app 私有目录）。所以本层绝大多数函数一律收 `&Connection`，
//!    只有 `schema::open_at` 收一个**调用方给的**路径，它自己不猜。
//! 3. **默认不取系统时钟** —— 需要「现在几点」的地方优先由调用方传进来。
//!    唯一的例外是 `time` 模块：数据层给记录盖的时间戳不影响任何判定的正确性，
//!    破例的理由和边界写在 `time.rs` 顶部，不要把它当成可以随便取时钟的许可。
//!
//! 违反任何一条，安卓端就得复制一份代码，而复制的那一刻起两端行为就开始漂移。

pub mod attachments;
pub mod categories;
pub mod model;
pub mod schema;
pub mod settings;
pub mod tasks;
pub mod templates;
pub mod time;

// 数据层的单测。跟着代码走 —— 它们测的本来就是这一层的行为。
#[cfg(test)]
mod tests;

// 让调用方直接写 worklog_data::Task，不用记它在哪个子模块下。
pub use attachments::Attachment;
pub use model::{Category, Completion, Progress, Task};
pub use templates::{Template, TemplateItem};
// Rule 既是数据库字段的形状，也是排期引擎的输入 —— 只有一份定义。
pub use worklog_core::model::Rule;

/// 数据层的统一错误类型：错误信息直接给用户看，所以用 String 而不是自定义枚举。
///
/// 和桌面端 `commands.rs` 里那个 `R<T>` 是同一个形状 —— 那边保留同名别名，
/// 好让命令层写起来跟以前一样。
pub type R<T> = Result<T, String>;

/// 把任意错误转成能显示给用户的字符串。
pub fn e2s<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}
