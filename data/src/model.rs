//! 跨端共享的数据模型。
//!
//! 这些结构直接对应 `schema.rs` 里的表，两端共用 —— 各端自己定义一份的话，
//! 同一份数据在两端序列化出来的字段名就可能不一致（比如 `categoryId`
//! 和 `category_id`），而且这种错在编译期发现不了。
//!
//! `Rule` 是从 `worklog-core` 重导出的：它既是数据库里 `tasks.rule` 字段的
//! 形状，也是排期引擎的输入类型。**排期算法只有一份实现**，
//! 所以这个类型也必须只有一份定义。

use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};

use crate::{e2s, R};

#[derive(Serialize, Deserialize, Clone)]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub sort: i64,
}

// Rule 搬进共享层（core）了 —— 排期引擎和两端的数据库模型都要用它，
// 留在 commands 里移动端就搬不动。这里重新导出，`commands::Rule` 路径保持不变，
// 下面 Task.rule / TemplateItem.rule 和 main.rs 的调用都不用改。
pub use worklog_core::model::Rule;

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    #[serde(default)]
    pub done: i64,
    #[serde(default)]
    pub total: i64,
}

fn d_pattern() -> String { "once".into() }
fn d_status()  -> String { "todo".into() }

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    #[serde(default)]
    pub id: i64,
    pub title: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub category_id: Option<i64>,
    #[serde(default = "d_pattern")]
    pub pattern: String,
    #[serde(default = "d_status")]
    pub status: String,
    #[serde(default)]
    pub due_at: Option<String>,
    #[serde(default)]
    pub end_at: Option<String>,
    #[serde(default)]
    pub remind_at: Option<String>,
    #[serde(default)]
    pub rule: Option<Rule>,
    #[serde(default)]
    pub progress: Option<Progress>,
    #[serde(default)]
    pub parent_id: Option<i64>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
    /// 上次发出到期提醒的时刻。用来避免后台巡检每 30 秒重复弹同一条通知。
    #[serde(default)]
    pub notified_at: Option<String>,
}

/// 周期任务的一次完成记录。用户月底回看「这个月干了什么」靠它。
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Completion {
    pub id: i64,
    pub task_id: Option<i64>,
    pub title: String,
    pub category_id: Option<i64>,
    pub pattern: String,
    pub due_at: Option<String>,
    pub done_at: String,
}

pub fn row_to_task(row: &Row) -> rusqlite::Result<Task> {
    let rule: Option<String> = row.get("rule")?;
    let progress: Option<String> = row.get("progress")?;
    Ok(Task {
        id: row.get("id")?,
        title: row.get("title")?,
        note: row.get("note")?,
        category_id: row.get("category_id")?,
        pattern: row.get("pattern")?,
        status: row.get("status")?,
        due_at: row.get("due_at")?,
        end_at: row.get("end_at")?,
        remind_at: row.get("remind_at")?,
        rule: rule.and_then(|s| serde_json::from_str(&s).ok()),
        progress: progress.and_then(|s| serde_json::from_str(&s).ok()),
        parent_id: row.get("parent_id")?,
        created_at: row.get("created_at")?,
        completed_at: row.get("completed_at")?,
        notified_at: row.get("notified_at")?,
    })
}

pub fn fetch_task(conn: &Connection, id: i64) -> R<Task> {
    conn.query_row("SELECT * FROM tasks WHERE id = ?1", params![id], row_to_task)
        .map_err(e2s)
}

