//! 跨端共享的数据模型。
//!
//! 只放**被排期引擎直接消费**的类型。像 Task / Template 这些带数据库 id 的大结构
//! 留在各端的 commands 里 —— 它们跟着各自的界面走，共享收益不大，搬过来反而要
//! 把各端的字段差异也一起抹平。

use serde::{Deserialize, Serialize};

/// 周期任务的重复规则。前端存成 JSON 文本落在 `tasks.rule` 字段里。
///
/// 从 commands.rs 搬过来的，`commands::Rule` 这个路径通过重导出保持可用。
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    #[serde(default)]
    pub freq: String,
    /// 语义随 freq 变：
    /// - weekly   ：星期几（0=周日 … 6=周六），可多选
    /// - monthly  ：每月几号（1–31，31 表示月末，小月自动夹到最后一天）
    /// - quarterly：每季度第几个月里的几号，同上
    #[serde(default)]
    pub by_day: Option<Vec<i64>>,
    /// 季度规则的锚点月（1–12）。只取第一个；周期是「锚点月、+3、+6、+9」。
    /// 例如 3 → 3/6/9/12 月（季末），1 → 1/4/7/10 月（季初）。
    #[serde(default)]
    pub by_month: Option<Vec<i64>>,
    #[serde(default)]
    pub time: Option<String>,
}
