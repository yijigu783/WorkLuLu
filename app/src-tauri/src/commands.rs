use crate::AppState;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::{DateTime, Local, NaiveDate, TimeZone, Timelike};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::State;
// 排期引擎从共享层进来 —— 桌面版和安卓版共用同一份实现，不会各算各的。
use worklog_core::schedule;

type R<T> = Result<T, String>;
fn e2s<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn now_local() -> DateTime<Local> {
    Local::now()
}

fn parse_dt(s: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Local))
}

/* ---------------- 数据模型 ---------------- */

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

fn row_to_task(row: &Row) -> rusqlite::Result<Task> {
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

fn fetch_task(conn: &Connection, id: i64) -> R<Task> {
    conn.query_row("SELECT * FROM tasks WHERE id = ?1", params![id], row_to_task)
        .map_err(e2s)
}

/* ---------------- 分类 ---------------- */

#[tauri::command]
pub fn list_categories(state: State<'_, AppState>) -> R<Vec<Category>> {
    let conn = state.db.lock().map_err(e2s)?;
    let mut stmt = conn
        .prepare("SELECT id, name, color, sort FROM categories ORDER BY sort, id")
        .map_err(e2s)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Category {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
                sort: r.get(3)?,
            })
        })
        .map_err(e2s)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(e2s)
}

#[tauri::command]
pub fn create_category(state: State<'_, AppState>, name: String, color: String) -> R<Category> {
    let conn = state.db.lock().map_err(e2s)?;
    let sort: i64 = conn
        .query_row("SELECT COALESCE(MAX(sort), -1) + 1 FROM categories", [], |r| r.get(0))
        .map_err(e2s)?;
    conn.execute(
        "INSERT INTO categories (name, color, sort) VALUES (?1, ?2, ?3)",
        params![name, color, sort],
    )
    .map_err(e2s)?;
    Ok(Category { id: conn.last_insert_rowid(), name, color, sort })
}

#[tauri::command]
pub fn update_category(
    state: State<'_, AppState>,
    id: i64,
    name: Option<String>,
    color: Option<String>,
    sort: Option<i64>,
) -> R<()> {
    let conn = state.db.lock().map_err(e2s)?;
    if let Some(v) = name  { conn.execute("UPDATE categories SET name = ?1 WHERE id = ?2", params![v, id]).map_err(e2s)?; }
    if let Some(v) = color { conn.execute("UPDATE categories SET color = ?1 WHERE id = ?2", params![v, id]).map_err(e2s)?; }
    if let Some(v) = sort  { conn.execute("UPDATE categories SET sort = ?1 WHERE id = ?2", params![v, id]).map_err(e2s)?; }
    Ok(())
}

/// 删除分类：其下工作自动回落到「未分类」，不会被一起删掉
#[tauri::command]
pub fn delete_category(state: State<'_, AppState>, id: i64) -> R<()> {
    let conn = state.db.lock().map_err(e2s)?;
    conn.execute("UPDATE tasks SET category_id = NULL WHERE category_id = ?1", params![id])
        .map_err(e2s)?;
    conn.execute("DELETE FROM categories WHERE id = ?1", params![id])
        .map_err(e2s)?;
    Ok(())
}

#[tauri::command]
pub fn reorder_categories(state: State<'_, AppState>, ids: Vec<i64>) -> R<()> {
    let conn = state.db.lock().map_err(e2s)?;
    for (i, id) in ids.iter().enumerate() {
        conn.execute("UPDATE categories SET sort = ?1 WHERE id = ?2", params![i as i64, id])
            .map_err(e2s)?;
    }
    Ok(())
}

/* ---------------- 工作项 ---------------- */

/// 顶层工作项。子任务存在同一张表里（靠 parent_id 挂到阶段性工作下面），
/// 但它们不该出现在主列表、待办计数和统计里——那里算的是「一件工作」，不是「一步」。
pub(crate) fn top_level_tasks(conn: &Connection) -> R<Vec<Task>> {
    let mut stmt = conn
        .prepare("SELECT * FROM tasks WHERE parent_id IS NULL ORDER BY sort, id")
        .map_err(e2s)?;
    let rows = stmt.query_map([], row_to_task).map_err(e2s)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(e2s)
}

pub(crate) fn subtask_rows(conn: &Connection) -> R<Vec<Task>> {
    // 只取「父任务还在」的那些。父任务被删时靠外键级联清掉子任务，
    // 但如果有人绕过外键直接改库，会留下挂不到任何地方的孤儿——
    // 与其自动删数据，不如让它们不出现。
    let mut stmt = conn
        .prepare(
            "SELECT * FROM tasks
             WHERE parent_id IS NOT NULL AND parent_id IN (SELECT id FROM tasks)
             ORDER BY parent_id, sort, id",
        )
        .map_err(e2s)?;
    let rows = stmt.query_map([], row_to_task).map_err(e2s)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(e2s)
}

#[tauri::command]
pub fn list_tasks(state: State<'_, AppState>) -> R<Vec<Task>> {
    let conn = state.db.lock().map_err(e2s)?;
    top_level_tasks(&conn)
}

/// 一次把全部子任务取回来，前端按 parentId 分组。
/// 阶段性工作数量不多，分次请求反而更慢也更啰嗦。
#[tauri::command]
pub fn list_subtasks(state: State<'_, AppState>) -> R<Vec<Task>> {
    let conn = state.db.lock().map_err(e2s)?;
    subtask_rows(&conn)
}

#[tauri::command]
pub fn create_task(state: State<'_, AppState>, mut task: Task) -> R<Task> {
    let conn = state.db.lock().map_err(e2s)?;
    // 周期任务建好即排期：用户只录一次「每周五交周报」，之后不用自己算下一次是几号
    if task.pattern == "recurring" {
        if let Some(rule) = task.rule.clone() {
            task.due_at = schedule::next_from(&rule, now_local()).map(|d| d.to_rfc3339());
        }
    }
    let rule = task.rule.as_ref().and_then(|r| serde_json::to_string(r).ok());
    let progress = task.progress.as_ref().and_then(|p| serde_json::to_string(p).ok());
    let now = now_local().to_rfc3339();
    conn.execute(
        "INSERT INTO tasks (title, note, category_id, pattern, status, due_at, end_at, remind_at,
                            rule, progress, parent_id, sort, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,0,?12)",
        params![
            task.title, task.note, task.category_id, task.pattern, task.status,
            task.due_at, task.end_at, task.remind_at, rule, progress, task.parent_id, now
        ],
    )
    .map_err(e2s)?;
    let id = conn.last_insert_rowid();
    fetch_task(&conn, id)
}

#[tauri::command]
pub fn update_task(state: State<'_, AppState>, mut task: Task) -> R<Task> {
    let conn = state.db.lock().map_err(e2s)?;
    let old = fetch_task(&conn, task.id).ok();

    // 改了重复规则就重新排期，否则 due_at 还停在旧规则算出来的时间上
    if task.pattern == "recurring" {
        let serial = |r: &Option<Rule>| r.as_ref().and_then(|x| serde_json::to_string(x).ok());
        let rule_changed = match &old {
            Some(o) => serial(&o.rule) != serial(&task.rule),
            None => true,
        };
        if rule_changed || task.due_at.is_none() {
            if let Some(rule) = task.rule.clone() {
                task.due_at = schedule::next_from(&rule, now_local()).map(|d| d.to_rfc3339());
            }
            task.notified_at = None; // 重新排期后允许再次提醒
        }
    }

    let rule = task.rule.as_ref().and_then(|r| serde_json::to_string(r).ok());
    let progress = task.progress.as_ref().and_then(|p| serde_json::to_string(p).ok());
    conn.execute(
        "UPDATE tasks SET title = ?1, note = ?2, category_id = ?3, pattern = ?4, status = ?5,
                          due_at = ?6, end_at = ?7, remind_at = ?8, rule = ?9, progress = ?10,
                          parent_id = ?11, completed_at = ?12, notified_at = ?13
         WHERE id = ?14",
        params![
            task.title, task.note, task.category_id, task.pattern, task.status,
            task.due_at, task.end_at, task.remind_at, rule, progress,
            task.parent_id, task.completed_at, task.notified_at, task.id
        ],
    )
    .map_err(e2s)?;
    fetch_task(&conn, task.id)
}

/// 周期任务的「完成这一次」：往 completions 留一条痕，并算出下一次时间。
/// 单独抽出来是为了能脱离 Tauri 的 State 直接测。
pub(crate) fn record_occurrence(
    conn: &Connection,
    task: &Task,
    now: DateTime<Local>,
) -> R<String> {
    conn.execute(
        "INSERT INTO completions (task_id, title, category_id, pattern, due_at, done_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            task.id, task.title, task.category_id, task.pattern,
            task.due_at, now.to_rfc3339()
        ],
    )
    .map_err(e2s)?;

    // 规则算不出来就保留原时间：宁可让人看到「还停在原来那天」，也不能把任务弄丢
    Ok(task
        .rule
        .as_ref()
        .and_then(|r| schedule::advance(r, task.due_at.as_deref().and_then(parse_dt), now))
        .map(|d| d.to_rfc3339())
        .or_else(|| task.due_at.clone())
        .unwrap_or_else(|| now.to_rfc3339()))
}

/// 勾选/取消勾选。
///
/// 周期性工作和别的不一样：打勾表示「这一次做完了」，不是整件事结束了。
/// 所以它不进入「已完成」，而是留一条完成记录后滚到下一次，条目继续留在待办里。
#[tauri::command]
pub fn set_task_status(state: State<'_, AppState>, id: i64, status: String) -> R<Task> {
    let conn = state.db.lock().map_err(e2s)?;
    let task = fetch_task(&conn, id)?;

    if task.pattern == "recurring" && status == "done" {
        let next = record_occurrence(&conn, &task, now_local())?;
        conn.execute(
            "UPDATE tasks SET status = 'todo', completed_at = NULL, notified_at = NULL, due_at = ?1
             WHERE id = ?2",
            params![next, id],
        )
        .map_err(e2s)?;
        return fetch_task(&conn, id);
    }

    let completed = if status == "done" { Some(now_local().to_rfc3339()) } else { None };
    conn.execute(
        "UPDATE tasks SET status = ?1, completed_at = ?2, notified_at = NULL WHERE id = ?3",
        params![status, completed, id],
    )
    .map_err(e2s)?;
    fetch_task(&conn, id)
}

/// 跳过这一次：只让当前这一次过去，规则本身不动。
/// 「这周周报不用写了」用它，下周照常出现。
#[tauri::command]
pub fn skip_occurrence(state: State<'_, AppState>, id: i64) -> R<Task> {
    let conn = state.db.lock().map_err(e2s)?;
    let task = fetch_task(&conn, id)?;
    if task.pattern != "recurring" {
        return Err("只有周期性工作可以跳过某一次".into());
    }
    let next = task
        .rule
        .as_ref()
        .and_then(|r| schedule::advance(r, task.due_at.as_deref().and_then(parse_dt), now_local()))
        .ok_or("无法计算下一次时间，请检查重复规则")?
        .to_rfc3339();

    conn.execute(
        "UPDATE tasks SET due_at = ?1, notified_at = NULL, status = 'todo' WHERE id = ?2",
        params![next, id],
    )
    .map_err(e2s)?;
    fetch_task(&conn, id)
}

/* ---------------- 完成记录 ---------------- */

#[tauri::command]
pub fn list_completions(state: State<'_, AppState>, limit: Option<i64>) -> R<Vec<Completion>> {
    let conn = state.db.lock().map_err(e2s)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, task_id, title, category_id, pattern, due_at, done_at
             FROM completions ORDER BY done_at DESC, id DESC LIMIT ?1",
        )
        .map_err(e2s)?;
    let rows = stmt
        .query_map(params![limit.unwrap_or(200).clamp(1, 2000)], |r| {
            Ok(Completion {
                id: r.get(0)?,
                task_id: r.get(1)?,
                title: r.get(2)?,
                category_id: r.get(3)?,
                pattern: r.get(4)?,
                due_at: r.get(5)?,
                done_at: r.get(6)?,
            })
        })
        .map_err(e2s)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(e2s)
}

/// 撤销一条完成记录：删掉记录并把该工作退回那一次。
/// 手滑点错勾的时候用，不用重新去规则里折腾。
#[tauri::command]
pub fn undo_completion(state: State<'_, AppState>, id: i64) -> R<()> {
    let conn = state.db.lock().map_err(e2s)?;
    let row: Option<(Option<i64>, Option<String>)> = conn
        .query_row("SELECT task_id, due_at FROM completions WHERE id = ?1", params![id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()
        .map_err(e2s)?;
    let (task_id, due) = row.ok_or("这条完成记录已经不存在了")?;

    if let (Some(tid), Some(due_iso)) = (task_id, due) {
        // notified_at 设成原定时间，避免撤销后立刻又弹一次提醒
        conn.execute(
            "UPDATE tasks SET due_at = ?1, notified_at = ?1, status = 'todo', completed_at = NULL
             WHERE id = ?2",
            params![due_iso, tid],
        )
        .map_err(e2s)?;
    }
    conn.execute("DELETE FROM completions WHERE id = ?1", params![id])
        .map_err(e2s)?;
    Ok(())
}

#[tauri::command]
pub fn delete_task(state: State<'_, AppState>, id: i64) -> R<()> {
    let conn = state.db.lock().map_err(e2s)?;
    conn.execute("DELETE FROM tasks WHERE id = ?1", params![id])
        .map_err(e2s)?;
    Ok(())
}

/* ---------------- 子任务（阶段性工作的拆解） ---------------- */

/// 拆解层数上限。顶层工作算第 1 层，往下最多再拆 2 层。
///
/// 为什么封顶：工作记录的场景里「这件事分几步、每步再分几步」就到头了。
/// 再深对使用者没有实际意义，却会让缩进、折叠、进度统计一起复杂化——
/// 深度没有上界的话，递归渲染也少了一道最关键的护栏。
pub(crate) const MAX_DEPTH: i64 = 3;

/// 一个节点在第几层。顶层工作（parent_id 为空）返回 1。
/// 顺带兜住环：万一数据被外部工具改出 A→B→A 这种环，也不会把程序挂死。
fn depth_of(conn: &Connection, id: i64) -> R<i64> {
    let mut cur = Some(id);
    let mut depth = 0i64;
    while let Some(x) = cur {
        depth += 1;
        if depth > 16 {
            return Err("层级异常，请检查这条工作的父子关系".into());
        }
        let row: Option<Option<i64>> = conn
            .query_row("SELECT parent_id FROM tasks WHERE id = ?1", params![x], |r| {
                r.get::<_, Option<i64>>(0)
            })
            .optional()
            .map_err(e2s)?;
        match row {
            Some(p) => cur = p,
            None => return Err("要挂在哪个工作下面？父任务找不到了".into()),
        }
    }
    Ok(depth)
}

/// 新建子任务。分类跟随父任务——子任务单独挂分类没有意义，
/// 而且统计按分类汇总时会把「一步」算成「一件工作」。
///
/// 抽成自由函数而不是直接写在命令里，是为了能脱离 Tauri 的 State 写测试。
pub(crate) fn insert_subtask(conn: &Connection, parent_id: i64, title: &str) -> R<Task> {
    let title = title.trim();
    if title.is_empty() {
        return Err("子任务不能没有名字".into());
    }

    let parent_depth = depth_of(conn, parent_id)?;
    let (pattern, category_id): (String, Option<i64>) = conn
        .query_row(
            "SELECT pattern, category_id FROM tasks WHERE id = ?1",
            params![parent_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| "要挂在哪个工作下面？父任务找不到了".to_string())?;

    if parent_depth >= MAX_DEPTH {
        return Err(format!("最多拆到第 {MAX_DEPTH} 层"));
    }
    // 顶层工作必须是「阶段性」才谈得上拆解：
    // 一次性、周期性工作本身就是一件事，把「一步」挂在它下面会让待办计数失真。
    // 第 2 层以下不受这条约束——那已经是「一步」了，给它再分小步是合理的。
    if parent_depth == 1 && pattern != "stage" {
        return Err("只有阶段性工作可以拆分子任务".into());
    }

    let sort: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(sort), -1) + 1 FROM tasks WHERE parent_id = ?1",
            params![parent_id],
            |r| r.get(0),
        )
        .map_err(e2s)?;

    conn.execute(
        "INSERT INTO tasks (title, note, category_id, pattern, status, parent_id, sort, created_at)
         VALUES (?1, '', ?2, 'once', 'todo', ?3, ?4, ?5)",
        params![title, category_id, parent_id, sort, now_local().to_rfc3339()],
    )
    .map_err(e2s)?;
    fetch_task(conn, conn.last_insert_rowid())
}

#[tauri::command]
pub fn create_subtask(state: State<'_, AppState>, parent_id: i64, title: String) -> R<Task> {
    let conn = state.db.lock().map_err(e2s)?;
    insert_subtask(&conn, parent_id, &title)
}

/// 改名。子任务在抽屉里是可直接编辑的输入框，改完即存。
#[tauri::command]
pub fn rename_subtask(state: State<'_, AppState>, id: i64, title: String) -> R<Task> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("子任务不能没有名字".into());
    }
    let conn = state.db.lock().map_err(e2s)?;
    conn.execute(
        "UPDATE tasks SET title = ?1 WHERE id = ?2 AND parent_id IS NOT NULL",
        params![title, id],
    )
    .map_err(e2s)?;
    fetch_task(&conn, id)
}

/* ---------------- 复制一份 ---------------- */

/// 按「父先于子」的顺序展开一棵子树（不含根）。
/// 复制和存模板都要照这个顺序落库，父的 id 映射才能先就位。
fn collect_subtree(conn: &Connection, root: i64, out: &mut Vec<Task>) -> R<()> {
    let kids = {
        let mut stmt = conn
            .prepare("SELECT * FROM tasks WHERE parent_id = ?1 ORDER BY sort, id")
            .map_err(e2s)?;
        let rows = stmt.query_map(params![root], row_to_task).map_err(e2s)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(e2s)?
    };
    for k in kids {
        let id = k.id;
        out.push(k);
        collect_subtree(conn, id, out)?;
    }
    Ok(())
}

/// 把 ISO 时间整体挪几天，时分保持不变。
fn shift_days(iso: &Option<String>, days: i64) -> Option<String> {
    if days == 0 {
        return iso.clone();
    }
    let d = parse_dt(iso.as_ref()?)?;
    Some((d + chrono::Duration::days(days)).to_rfc3339())
}

/// 复制一件工作，连同它下面拆出来的所有层级。
///
/// 「这件事我下周还要再做一遍」是高频场景，靠模板库去覆盖太重了——
/// 就地复制最省事：不碰表结构，也用不着先攒出一个模板。
///
/// 日期按「整体平移」处理：原件从哪天开始，副本就从 `base` 那天开始，
/// 内部各步骤之间的相对间隔原样保留。不平移的话，复制出来的东西排期全落在过去。
pub(crate) fn duplicate_task_inner(conn: &Connection, id: i64, base: Option<&str>) -> R<Task> {
    let src = fetch_task(conn, id)?;
    let mut kids = Vec::new();
    collect_subtree(conn, id, &mut kids)?;

    let from = src.due_at.as_deref().and_then(parse_dt).map(|d| d.date_naive());
    let to = base
        .and_then(|s| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok())
        .or(from);
    let shift = match (from, to) {
        (Some(f), Some(t)) => (t - f).num_days(),
        _ => 0,
    };

    // 周期任务不照搬旧时间：照搬会让复制出来的那条一出生就是逾期状态，
    // 按规则重新排一次期才是它该在的位置。
    let due = if src.pattern == "recurring" {
        src.rule
            .as_ref()
            .and_then(|r| schedule::next_from(r, now_local()))
            .map(|d| d.to_rfc3339())
            .or_else(|| shift_days(&src.due_at, shift))
    } else {
        shift_days(&src.due_at, shift)
    };

    // 顶层挂个「副本」字样，一眼分得清哪条是复制出来的；
    // 子任务不加——否则一整棵树上全是这几个字，反而看不清。
    let title = if src.parent_id.is_none() {
        format!("{}（副本）", src.title)
    } else {
        src.title.clone()
    };
    let rule = src.rule.as_ref().and_then(|r| serde_json::to_string(r).ok());
    let now = now_local().to_rfc3339();

    conn.execute(
        "INSERT INTO tasks (title, note, category_id, pattern, status, due_at, end_at, remind_at,
                            rule, progress, parent_id, sort, created_at)
         VALUES (?1,?2,?3,?4,'todo',?5,?6,?7,?8,NULL,?9,0,?10)",
        params![
            title, src.note, src.category_id, src.pattern, due,
            shift_days(&src.end_at, shift), shift_days(&src.remind_at, shift),
            rule, src.parent_id, now
        ],
    )
    .map_err(e2s)?;
    let root_id = conn.last_insert_rowid();

    let mut map: HashMap<i64, i64> = HashMap::new();
    map.insert(src.id, root_id);
    // kids 是深度优先的前序序列，父一定在子之前；用它在兄弟间维持原有先后
    for (i, k) in kids.iter().enumerate() {
        let Some(pid) = k.parent_id.and_then(|p| map.get(&p).copied()) else { continue };
        let krule = k.rule.as_ref().and_then(|r| serde_json::to_string(r).ok());
        conn.execute(
            "INSERT INTO tasks (title, note, category_id, pattern, status, due_at, end_at, remind_at,
                                rule, progress, parent_id, sort, created_at)
             VALUES (?1,?2,?3,?4,'todo',?5,?6,?7,?8,NULL,?9,?10,?11)",
            params![
                k.title, k.note, k.category_id, k.pattern,
                shift_days(&k.due_at, shift), shift_days(&k.end_at, shift),
                shift_days(&k.remind_at, shift), krule, pid, i as i64, now
            ],
        )
        .map_err(e2s)?;
        map.insert(k.id, conn.last_insert_rowid());
    }

    fetch_task(conn, root_id)
}

/// `base` 是副本的起始日（YYYY-MM-DD）。不给就原地复制、日期不动。
#[tauri::command]
pub fn duplicate_task(state: State<'_, AppState>, id: i64, base: Option<String>) -> R<Task> {
    let conn = state.db.lock().map_err(e2s)?;
    duplicate_task_inner(&conn, id, base.as_deref())
}

/* ---------------- 模板 ---------------- */

/// 模板里的一个条目。日期是**相对基准日的偏移**，不是绝对日期——
/// 这是整个模板功能的关键，见 db.rs 里建表时的说明。
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TemplateItem {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub title: String,
    pub note: String,
    pub category_id: Option<i64>,
    pub pattern: String,
    pub rule: Option<Rule>,
    pub due_offset: Option<i64>,
    pub end_offset: Option<i64>,
    pub due_time: Option<String>,
    pub end_time: Option<String>,
    pub sort: i64,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub id: i64,
    pub name: String,
    pub note: String,
    pub created_at: Option<String>,
    pub items: Vec<TemplateItem>,
}

/// 从 ISO 时间里取出「HH:MM」。取不出来就返回 None，由调用方决定用什么默认值。
fn hm_of(iso: &Option<String>) -> Option<String> {
    let d = parse_dt(iso.as_ref()?)?;
    Some(format!("{:02}:{:02}", d.hour(), d.minute()))
}

/// 「HH:MM」→ (时, 分)。解析不出来就用默认值，绝不因为一个时间串不正常就整批建不出来。
fn parse_hm(s: &Option<String>, dflt: (u32, u32)) -> (u32, u32) {
    s.as_deref()
        .and_then(|t| t.split_once(':'))
        .and_then(|(h, m)| Some((h.trim().parse::<u32>().ok()?, m.trim().parse::<u32>().ok()?)))
        .filter(|(h, m)| *h < 24 && *m < 60)
        .unwrap_or(dflt)
}

fn read_template(conn: &Connection, id: i64) -> R<Template> {
    let (name, note, created_at): (String, String, Option<String>) = conn
        .query_row(
            "SELECT name, note, created_at FROM templates WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(e2s)?;

    let mut stmt = conn
        .prepare(
            "SELECT id, parent_id, title, note, category_id, pattern, rule,
                    due_offset, end_offset, due_time, end_time, sort
             FROM template_items WHERE template_id = ?1 ORDER BY sort, id",
        )
        .map_err(e2s)?;
    let items = stmt
        .query_map(params![id], |r| {
            let rule: Option<String> = r.get(6)?;
            Ok(TemplateItem {
                id: r.get(0)?,
                parent_id: r.get(1)?,
                title: r.get(2)?,
                note: r.get(3)?,
                category_id: r.get(4)?,
                pattern: r.get(5)?,
                rule: rule.and_then(|s| serde_json::from_str(&s).ok()),
                due_offset: r.get(7)?,
                end_offset: r.get(8)?,
                due_time: r.get(9)?,
                end_time: r.get(10)?,
                sort: r.get(11)?,
            })
        })
        .map_err(e2s)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(e2s)?;

    Ok(Template { id, name, note, created_at, items })
}

/// 把一件工作（含各层步骤）存成模板。
/// 基准日取根任务的开始日——所有偏移都相对它算，这样模板才有「结构」而没有「日期」。
pub(crate) fn template_from_task(conn: &Connection, task_id: i64, name: &str) -> R<Template> {
    let name = name.trim();
    if name.is_empty() {
        return Err("给模板起个名字吧".into());
    }
    let root = fetch_task(conn, task_id)?;
    let mut kids = Vec::new();
    collect_subtree(conn, task_id, &mut kids)?;

    let base = root
        .due_at
        .as_deref()
        .and_then(parse_dt)
        .map(|d| d.date_naive())
        .unwrap_or_else(|| now_local().date_naive());
    let off = |iso: &Option<String>| {
        iso.as_deref()
            .and_then(parse_dt)
            .map(|d| (d.date_naive() - base).num_days())
    };

    let sort: i64 = conn
        .query_row("SELECT COALESCE(MAX(sort), -1) + 1 FROM templates", [], |r| r.get(0))
        .map_err(e2s)?;
    conn.execute(
        "INSERT INTO templates (name, note, sort, created_at) VALUES (?1, '', ?2, ?3)",
        params![name, sort, now_local().to_rfc3339()],
    )
    .map_err(e2s)?;
    let tpl_id = conn.last_insert_rowid();

    let mut all = vec![root.clone()];
    all.extend(kids);
    let mut map: HashMap<i64, i64> = HashMap::new();
    for (i, t) in all.iter().enumerate() {
        let parent_item = t.parent_id.and_then(|p| map.get(&p).copied());
        let rule = t.rule.as_ref().and_then(|r| serde_json::to_string(r).ok());
        conn.execute(
            "INSERT INTO template_items (template_id, parent_id, title, note, category_id,
                                         pattern, rule, due_offset, end_offset,
                                         due_time, end_time, sort)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![
                tpl_id, parent_item, t.title, t.note, t.category_id, t.pattern, rule,
                off(&t.due_at), off(&t.end_at), hm_of(&t.due_at), hm_of(&t.end_at),
                i as i64
            ],
        )
        .map_err(e2s)?;
        map.insert(t.id, conn.last_insert_rowid());
    }

    read_template(conn, tpl_id)
}

/// 按模板建一批工作。`base` 是基准日，各条目的真实日期 = 基准日 + 各自偏移。
/// 返回新建的顶层工作条数。
pub(crate) fn apply_template_inner(conn: &Connection, template_id: i64, base: &str) -> R<i64> {
    let base_date = NaiveDate::parse_from_str(base.trim(), "%Y-%m-%d")
        .map_err(|_| "基准日期格式不对，应该像 2026-10-01 这样".to_string())?;
    let tpl = read_template(conn, template_id)?;
    if tpl.items.is_empty() {
        return Err("这个模板里还没有内容".into());
    }

    let at = |off: Option<i64>, hm: (u32, u32)| -> Option<String> {
        let d = base_date + chrono::Duration::days(off?);
        let ndt = d.and_hms_opt(hm.0, hm.1, 0)?;
        Local.from_local_datetime(&ndt).earliest().map(|x| x.to_rfc3339())
    };

    let now = now_local().to_rfc3339();
    let mut map: HashMap<i64, i64> = HashMap::new();
    let mut top = 0i64;

    // items 是按写入顺序读出来的（父一定排在子前面），所以父的 id 映射必然已经就位
    for it in &tpl.items {
        let parent = it.parent_id.and_then(|p| map.get(&p).copied());
        // 分类可能早被删了。为一个分类没了就拒绝建整批工作，代价太大，落到「未分类」即可
        let cat: Option<i64> = match it.category_id {
            Some(c) => conn
                .query_row("SELECT id FROM categories WHERE id = ?1", params![c], |r| {
                    r.get::<_, i64>(0)
                })
                .optional()
                .map_err(e2s)?,
            None => None,
        };

        let due = if it.pattern == "recurring" {
            it.rule
                .as_ref()
                .and_then(|r| schedule::next_from(r, now_local()))
                .map(|d| d.to_rfc3339())
        } else {
            at(it.due_offset, parse_hm(&it.due_time, (18, 0)))
        };
        let end = at(it.end_offset, parse_hm(&it.end_time, (23, 59)));
        let rule = it.rule.as_ref().and_then(|r| serde_json::to_string(r).ok());

        conn.execute(
            "INSERT INTO tasks (title, note, category_id, pattern, status, due_at, end_at, remind_at,
                                rule, progress, parent_id, sort, created_at)
             VALUES (?1,?2,?3,?4,'todo',?5,?6,NULL,?7,NULL,?8,?9,?10)",
            params![it.title, it.note, cat, it.pattern, due, end, rule, parent, it.sort, now],
        )
        .map_err(e2s)?;
        map.insert(it.id, conn.last_insert_rowid());
        if parent.is_none() {
            top += 1;
        }
    }

    Ok(top)
}

/// 全部模板，含各自的条目。抽出来是为了能脱离 Tauri 的 State 写测试。
pub(crate) fn all_templates(conn: &Connection) -> R<Vec<Template>> {
    let ids = {
        let mut stmt = conn
            .prepare("SELECT id FROM templates ORDER BY sort, id")
            .map_err(e2s)?;
        let rows = stmt.query_map([], |r| r.get::<_, i64>(0)).map_err(e2s)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(e2s)?
    };
    ids.into_iter().map(|id| read_template(conn, id)).collect()
}

#[tauri::command]
pub fn list_templates(state: State<'_, AppState>) -> R<Vec<Template>> {
    let conn = state.db.lock().map_err(e2s)?;
    all_templates(&conn)
}

#[tauri::command]
pub fn save_template(state: State<'_, AppState>, task_id: i64, name: String) -> R<Template> {
    let conn = state.db.lock().map_err(e2s)?;
    template_from_task(&conn, task_id, &name)
}

#[tauri::command]
pub fn apply_template(state: State<'_, AppState>, id: i64, base: String) -> R<i64> {
    let conn = state.db.lock().map_err(e2s)?;
    apply_template_inner(&conn, id, &base)
}

#[tauri::command]
pub fn delete_template(state: State<'_, AppState>, id: i64) -> R<()> {
    let conn = state.db.lock().map_err(e2s)?;
    conn.execute("DELETE FROM templates WHERE id = ?1", params![id])
        .map_err(e2s)?;
    Ok(())
}

/* ---------------- 设置 ---------------- */

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> R<HashMap<String, String>> {
    let conn = state.db.lock().map_err(e2s)?;
    let mut stmt = conn.prepare("SELECT key, value FROM settings").map_err(e2s)?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(e2s)?;
    let mut map = HashMap::new();
    for r in rows {
        let (k, v) = r.map_err(e2s)?;
        map.insert(k, v);
    }
    Ok(map)
}

/// 设置项落库。一律用 upsert，不存在就建。
const SETTING_UPSERT: &str = "INSERT INTO settings (key, value) VALUES (?1, ?2)
     ON CONFLICT(key) DO UPDATE SET value = excluded.value";

#[tauri::command]
pub fn set_setting(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> R<()> {
    // 「贴边隐藏」和「边缘分屏」盯的是同一条屏幕边：窗口拖到左边到底是摆半屏
    // 还是藏起来，同一时刻只能有一个说了算，所以打开一个必须关掉另一个。
    //
    // 这条互斥由后端执行，界面上的互斥只是提前把结果显示出来。
    // 判据不能只活在前端 —— 以后多一个改设置的入口，就得多记一次「别忘了互斥」，
    // 放在这里是一劳永逸。
    let on = matches!(value.as_str(), "1" | "true" | "on");
    let counterpart = match key.as_str() {
        "snap" if on => Some("edge"),
        "edge" if on => Some("snap"),
        _ => None,
    };
    {
        // 一个事务里写两行：中途失败不会留下「一个开了、另一个也开着」的状态
        let mut conn = state.db.lock().map_err(e2s)?;
        let tx = conn.transaction().map_err(e2s)?;
        tx.execute(SETTING_UPSERT, params![key, value])
            .map_err(e2s)?;
        if let Some(other) = counterpart {
            tx.execute(SETTING_UPSERT, params![other, "0"])
                .map_err(e2s)?;
        }
        tx.commit().map_err(e2s)?;
    }

    // 开机自启是要真切生效的系统行为，不能只写个开关就当设过了
    if key == "autostart" {
        use tauri_plugin_autostart::ManagerExt;
        let mgr = app.autolaunch();
        let done = if on { mgr.enable() } else { mgr.disable() };
        done.map_err(|e| format!("写入开机自启失败：{e}"))?;
    }
    Ok(())
}

/* ---------------- 数据存放位置（标准 / 便携） ----------------
   需求来自使用反馈：数据位置能不能自己定，想做成便携版 —— 整个文件夹拷到 U 盘就能带走。
   判定与搬家的实现在 db.rs，这里只做「编排」和给界面看的信息。 */

/// 「数据存放位置」那张卡片要的全部信息：画徽标、拼确认框文案、显示启动时的搬迁提示。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataInfo {
    /// 实际在用的目录
    pub path: String,
    /// 当前是不是便携模式
    pub portable: bool,
    /// 切成便携模式后会用的目录
    pub portable_path: String,
    /// 切回标准模式后会用的目录
    pub standard_path: String,
    /// 便携模式不可用的原因（exe 在只读位置之类）。空串表示没查出问题
    pub blocked: String,
    /// 启动时若刚搬过家，给用户一句话。只给一次，之后是 null
    pub note: Option<String>,
}

#[tauri::command]
pub fn data_info(app: tauri::AppHandle, state: State<'_, AppState>) -> R<DataInfo> {
    let exe = crate::db::exe_dir().ok_or("拿不到程序所在目录")?;
    let portable_path = crate::db::portable_data_dir(&exe);
    let standard_path = crate::db::standard_data_dir(&app);
    let current = crate::db::data_dir(&app);
    let portable = current == portable_path;

    // 预检查只探 exe 目录能不能写，**绝不碰 portable_path** ——
    // 一旦把 data 目录建出来，它自己就成了便携模式的触发器
    let blocked = if portable {
        String::new()
    } else {
        crate::db::probe_writable(&exe).err().unwrap_or_default()
    };

    let note = state.portable_note.lock().ok().and_then(|mut g| g.take());
    Ok(DataInfo {
        path: current.to_string_lossy().to_string(),
        portable,
        portable_path: portable_path.to_string_lossy().to_string(),
        standard_path: standard_path.to_string_lossy().to_string(),
        blocked,
        note,
    })
}

/// 把连接暂时换成内存库，旧的随之 drop —— 文件句柄这才释放，搬移才做得成。
///
/// 顺带一个好处：SQLite 在最后一个连接关闭时会把 `-wal` 并回主文件并删掉它，
/// 所以真正要搬的文件常常只剩数据库本体一个。
fn close_db(state: &State<'_, AppState>) -> R<()> {
    let mut guard = state.db.lock().map_err(e2s)?;
    let placeholder = Connection::open_in_memory().map_err(e2s)?;
    drop(std::mem::replace(&mut *guard, placeholder));
    Ok(())
}

/// 把连接重新指到指定目录的库上
fn reopen_db(state: &State<'_, AppState>, dir: &std::path::Path) -> R<()> {
    let conn = Connection::open(dir.join(crate::db::DB_FILE)).map_err(e2s)?;
    let mut guard = state.db.lock().map_err(e2s)?;
    *guard = conn;
    Ok(())
}

/// 切换数据的存放位置。
///
/// 顺序很讲究：先探可写 → 再留快照 → 关连接释放文件锁 → 搬 → 补标记 → 重开连接。
/// 中途失败要么当场中止（还没动手），要么把连接支回原处让程序接着能用。
#[tauri::command]
pub fn set_portable(app: tauri::AppHandle, state: State<'_, AppState>, on: bool) -> R<String> {
    let exe = crate::db::exe_dir().ok_or("拿不到程序所在目录，没法切便携模式")?;
    let portable_dir = crate::db::portable_data_dir(&exe);
    let current = crate::db::data_dir(&app);
    let target = if on {
        portable_dir.clone()
    } else {
        crate::db::standard_data_dir(&app)
    };
    if target == current {
        return Ok("已经是这个模式了，没有改动".into());
    }

    // 1) 目标写得进去吗。便携模式最常踩的坑是 exe 放在只读位置（Program Files / 只读 U 盘）
    crate::db::ensure_writable(&target)?;

    // 2) 动手之前先给「现在」留一份。它落在当前目录的 backups 里，会跟着一起搬走
    {
        let conn = state.db.lock().map_err(e2s)?;
        let dir = current.join("backups");
        std::fs::create_dir_all(&dir).map_err(e2s)?;
        let snapshot = dir.join(format!(
            "换存储位置前-{}.db",
            now_local().format("%Y%m%d-%H%M%S")
        ));
        write_backup(&conn, &snapshot)
            .map_err(|e| format!("切换前没能保存当前数据快照，为安全起见已中止：{e}"))?;
    }

    // 3) 关连接，把文件句柄让出来
    close_db(&state)?;

    // 4) 搬。搬不动就把连接支回原处，程序接着能用
    if let Err(e) = crate::db::move_data(&current, &target) {
        let _ = reopen_db(&state, &current);
        return Err(e);
    }

    // 5) 补上 / 撤掉标记。便携模式下 data 目录本身就是触发器，portable.txt 是再补一个显式的
    if on {
        std::fs::write(exe.join(crate::db::PORTABLE_MARKER), b"")
            .map_err(|e| format!("数据已经搬过去了，但标记文件没写成：{e}"))?;
    } else {
        let _ = std::fs::remove_file(exe.join(crate::db::PORTABLE_MARKER));
        // 空的才删得掉；里面若还有别的东西就留着，不硬删用户的东西
        let _ = std::fs::remove_dir(&portable_dir);
    }

    // 6) 连接指到新位置
    reopen_db(&state, &target)?;

    let what = if on {
        "已切到便携模式，数据搬到程序旁的 data 文件夹"
    } else {
        "已切回标准模式，数据搬回用户目录"
    };
    Ok(format!("{what}：{}", target.to_string_lossy()))
}

/// 界面「关于」里显示的版本号。
/// 从打包进 exe 的包信息里读（AppHandle 自带的方法），不在前端另写一份常量 ——
/// 两处各写一个版本号，改了一处忘了另一处，用户看到的就和 exe 属性对不上了。
#[tauri::command]
pub fn app_version(app: tauri::AppHandle) -> R<String> {
    Ok(app.package_info().version.to_string())
}

/// 在资源管理器里打开数据目录。
/// 走 Rust 侧 API 而非 JS 的 `plugin:opener|open_path`，免去 ACL 路径白名单配置。
#[tauri::command]
pub fn open_data_dir(app: tauri::AppHandle) -> R<()> {
    let dir = crate::db::data_dir(&app);
    let _ = std::fs::create_dir_all(&dir);
    tauri_plugin_opener::open_path(&dir, None::<String>).map_err(e2s)
}

/* ---------------- 贴边自动隐藏 ----------------
 *
 *  判定与窗口操作全在 `edge.rs` 里，由后台轮询线程驱动 —— 展开 / 收回要看鼠标的
 *  绝对位置和左键状态，前端拿不到这些，也判断不出显示器边界和最大化状态。
 *  这里只留一个 `edge_reset`，给「关掉开关」用。
 */

/// 关掉贴边开关时调用：窗口可能正滑在屏幕外，先请回屏幕里。
#[tauri::command]
pub fn edge_reset(window: tauri::Window) -> R<()> {
    crate::edge::reset(&window);
    Ok(())
}

/* ---------------- 附件（粘贴的图片 / 上传的文件） ----------------
 *
 *  场景来自使用反馈：把跟别人的聊天截图直接粘进来留痕。
 *
 *  图片本体存进数据库，不落成数据目录里的散文件 —— 这样 `VACUUM INTO` 导出备份时
 *  会连图片一起带走，「一个 .db 拷到 U 盘就是全部数据」这条语义不用改。
 *  代价是库会变大，所以粘贴时前端会先压一道（见 app.js 的 prepareImage）。
 */

/// 单个附件的体积上限。留痕用的截图和文档远到不了这个量级，
/// 但一个手滑拖进来的视频能把库撑到没法备份，所以还是要拦。
const MAX_ATTACHMENT_BYTES: usize = 20 * 1024 * 1024;

/// 列表用的列：**不含** data 本体。列表只要元信息，把几十 MB 的图片
/// 跟着列表一起搬过来，抽屉一打开就会卡住。
const ATT_COLS: &str =
    "id, task_id, name, mime, size, kind, created_at, (thumb IS NOT NULL) AS has_thumb";

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: i64,
    pub task_id: i64,
    pub name: String,
    pub mime: String,
    pub size: i64,
    /// image（界面里直接看）/ file（交给系统默认程序打开）
    pub kind: String,
    /// 有没有缩略图。图片才有，列表靠它决定要不要去取小图
    pub has_thumb: bool,
    pub created_at: Option<String>,
}

fn row_to_attachment(row: &Row) -> rusqlite::Result<Attachment> {
    Ok(Attachment {
        id: row.get("id")?,
        task_id: row.get("task_id")?,
        name: row.get("name")?,
        mime: row.get("mime")?,
        size: row.get("size")?,
        kind: row.get("kind")?,
        has_thumb: row.get("has_thumb")?,
        created_at: row.get("created_at")?,
    })
}

fn fetch_attachment(conn: &Connection, id: i64) -> R<Attachment> {
    conn.query_row(
        &format!("SELECT {ATT_COLS} FROM attachments WHERE id = ?1"),
        params![id],
        row_to_attachment,
    )
    .map_err(e2s)
}

/// 拼成浏览器能直接用的 data URL。
/// mime 为空（少数文件读不出类型）时给个通用的，免得 `data:;base64,` 这种拼出来没人认。
fn data_url(mime: &str, bytes: &[u8]) -> String {
    let m = if mime.trim().is_empty() { "application/octet-stream" } else { mime };
    format!("data:{m};base64,{}", STANDARD.encode(bytes))
}

/// 附件元信息。`task_id` 传了就只取那一条工作的，不传取全部。
///
/// 不传时前端一次拿全，好处是列表/看板上的「有附件」标记不用再逐条问一次；
/// 元信息很小（名字、大小、类型），几十上百条也不值得分次取。
///
/// 抽成自由函数而不是直接写在命令里，是为了能脱离 Tauri 的 State 写测试。
pub(crate) fn list_attachments_of(conn: &Connection, task_id: Option<i64>) -> R<Vec<Attachment>> {
    let (sql, ids): (String, Vec<i64>) = match task_id {
        Some(id) => (
            format!("SELECT {ATT_COLS} FROM attachments WHERE task_id = ?1 ORDER BY id"),
            vec![id],
        ),
        None => (format!("SELECT {ATT_COLS} FROM attachments ORDER BY task_id, id"), vec![]),
    };
    let mut stmt = conn.prepare(&sql).map_err(e2s)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(ids.iter()), row_to_attachment)
        .map_err(e2s)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(e2s)
}

#[tauri::command]
pub fn list_attachments(state: State<'_, AppState>, task_id: Option<i64>) -> R<Vec<Attachment>> {
    let conn = state.db.lock().map_err(e2s)?;
    list_attachments_of(&conn, task_id)
}

/// 存一个附件。`data_b64` / `thumb_b64` 都是**不带前缀**的 base64
/// （前端已经剥掉 `data:...;base64,`）。
///
/// 前端先把图片压过一道再送过来，所以这里只做体积与合法性检查，不做二次压缩 ——
/// 后端再编一遍图会让粘贴有明显停顿。
pub(crate) fn insert_attachment(
    conn: &Connection,
    task_id: i64,
    name: &str,
    mime: &str,
    kind: &str,
    data_b64: &str,
    thumb_b64: Option<&str>,
) -> R<Attachment> {
    let bytes = STANDARD
        .decode(data_b64.as_bytes())
        .map_err(|_| "附件数据读不出来，请重试".to_string())?;
    if bytes.is_empty() {
        return Err("这是个空文件，没有内容可存".into());
    }
    if bytes.len() > MAX_ATTACHMENT_BYTES {
        return Err(format!(
            "单个附件不能超过 {} MB",
            MAX_ATTACHMENT_BYTES / 1024 / 1024
        ));
    }
    // 缩略图坏掉不该让整条失败：没有它顶多是列表多取一次原图
    let thumb_bytes = thumb_b64
        .filter(|t| !t.is_empty())
        .and_then(|t| STANDARD.decode(t.as_bytes()).ok());

    let kind = if kind == "image" { "image" } else { "file" }.to_string();

    // 外键其实会拦住，但那样报出来的是一句 SQL 错误，用户看不懂
    let exists: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks WHERE id = ?1", params![task_id], |r| r.get(0))
        .map_err(e2s)?;
    if exists == 0 {
        return Err("这条工作已经不在了".into());
    }

    let name = if name.trim().is_empty() { "未命名附件".to_string() } else { name.to_string() };
    conn.execute(
        "INSERT INTO attachments (task_id, name, mime, size, kind, data, thumb, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            task_id,
            name,
            mime,
            bytes.len() as i64,
            kind,
            bytes,
            thumb_bytes,
            now_local().to_rfc3339()
        ],
    )
    .map_err(e2s)?;
    fetch_attachment(conn, conn.last_insert_rowid())
}

#[tauri::command]
pub fn add_attachment(
    state: State<'_, AppState>,
    task_id: i64,
    name: String,
    mime: String,
    kind: String,
    data: String,
    thumb: Option<String>,
) -> R<Attachment> {
    let conn = state.db.lock().map_err(e2s)?;
    insert_attachment(&conn, task_id, &name, &mime, &kind, &data, thumb.as_deref())
}

/// 取附件内容，返回 data URL。
///
/// `thumb = true` 时优先给缩略图：列表里一格一格的小图不该把原图整块拉过来，
/// 几十张截图一起加载会让抽屉卡住。没有缩略图（或不是图片）就退回原图。
pub(crate) fn attachment_data_url(conn: &Connection, id: i64, thumb: bool) -> R<String> {
    if thumb {
        let t: Option<Vec<u8>> = conn
            .query_row("SELECT thumb FROM attachments WHERE id = ?1", params![id], |r| r.get(0))
            .optional()
            .map_err(e2s)?
            .flatten();
        if let Some(bytes) = t {
            // 缩略图统一编成 JPEG（前端生成时就是），省得再往库里存一份 mime
            return Ok(data_url("image/jpeg", &bytes));
        }
    }

    let (mime, data): (String, Vec<u8>) = conn
        .query_row(
            "SELECT mime, data FROM attachments WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| "这个附件已经不在了".to_string())?;
    Ok(data_url(&mime, &data))
}

#[tauri::command]
pub fn get_attachment(state: State<'_, AppState>, id: i64, thumb: Option<bool>) -> R<String> {
    let conn = state.db.lock().map_err(e2s)?;
    attachment_data_url(&conn, id, thumb.unwrap_or(false))
}

pub(crate) fn remove_attachment(conn: &Connection, id: i64) -> R<()> {
    conn.execute("DELETE FROM attachments WHERE id = ?1", params![id])
        .map_err(e2s)?;
    Ok(())
}

#[tauri::command]
pub fn delete_attachment(state: State<'_, AppState>, id: i64) -> R<()> {
    let conn = state.db.lock().map_err(e2s)?;
    remove_attachment(&conn, id)
}

/// 把文件名清洗成一个能落盘的裸名字。
/// 名字是从剪贴板/文件系统来的，可能带路径分隔符或 Windows 非法字符，
/// 直接拿去 join 会被写到别处去（`..\..\` 这类）。
fn safe_file_name(name: &str, id: i64) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("").trim();
    let cleaned: String = base
        .chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || (c as u32) < 32 {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches(['.', ' ']).to_string();
    if cleaned.is_empty() {
        format!("附件-{id}")
    } else {
        cleaned
    }
}

/// 用系统默认程序打开附件。
///
/// 图片在界面里就能看，这个入口主要是给 PDF / Word 这类文件用的。
/// 落到临时目录而不是数据目录：用户改完不需要回写，也不该在数据目录里留一堆散文件。
#[tauri::command]
pub fn open_attachment(state: State<'_, AppState>, id: i64) -> R<()> {
    let (name, data): (String, Vec<u8>) = {
        let conn = state.db.lock().map_err(e2s)?;
        conn.query_row(
            "SELECT name, data FROM attachments WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| "这个附件已经不在了".to_string())?
    };

    let dir = std::env::temp_dir().join("工作记录本-附件");
    std::fs::create_dir_all(&dir).map_err(|e| format!("没法建临时目录：{e}"))?;
    let path = dir.join(safe_file_name(&name, id));
    std::fs::write(&path, &data).map_err(|e| format!("写临时文件失败：{e}"))?;
    tauri_plugin_opener::open_path(&path, None::<String>).map_err(e2s)
}

/* ---------------- 备份 / 恢复 / 导出 ---------------- */

/// 一份完整备份要有的四张表。少一张就不认。
/// 备份里**必须**有的表。少一张就不是本程序导出的备份。
const TABLES: [&str; 4] = ["categories", "tasks", "completions", "settings"];

/// 后来才加进来的模块（模板库、附件）。
/// 老备份里没有它们——为一张新增的表去拒绝一个旧备份，用户会平白丢掉全部数据。
/// 所以按「有就一起搬、没有就跳过」处理：可选，但一旦存在就得列数对得上。
const OPTIONAL_TABLES: [&str; 3] = ["templates", "template_items", "attachments"];

/// 恢复前的自动快照放这儿，跟数据文件同目录，方便一起搬走
fn backup_dir_of(app: &tauri::AppHandle) -> std::path::PathBuf {
    crate::db::data_dir(app).join("backups")
}

/// 两个路径是不是同一个文件。
/// 用 canonicalize 兜底，免得 `C:\a\b.db` 和 `c:/A/B.DB` 被当成两个文件。
fn same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    if let (Ok(x), Ok(y)) = (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        return x == y;
    }
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

/// 把当前库导出成一个独立的文件。
///
/// 用 `VACUUM INTO` 而不是复制文件 —— 库跑在 WAL 模式下，最近的改动可能还留在 `-wal` 里
/// 没并回主文件。直接 `copy` 会导出一份缺数据的备份，而且缺得**悄无声息**：
/// 文件能打开、表也在，等真要用的时候才发现少了最近几天。
/// `VACUUM INTO` 生成的是事务一致快照，顺带把空闲页压掉，文件还更小。
fn write_backup(conn: &Connection, dest: &std::path::Path) -> R<()> {
    // VACUUM INTO 要求目标不存在；「另存为」对话框已经替我们确认过覆盖了
    if dest.exists() {
        std::fs::remove_file(dest).map_err(|e| format!("没法覆盖已有文件：{e}"))?;
    }
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(e2s)?;
        }
    }
    let dest_str = dest.to_string_lossy().to_string();
    conn.execute("VACUUM INTO ?1", params![dest_str])
        .map_err(|e| format!("导出失败：{e}"))?;
    Ok(())
}

/// 用备份文件的内容整体替换当前库。
///
/// 走 ATTACH + 事务搬迁，而不是替换文件 —— 直接换文件会跟正在用的连接打架
/// （WAL 还开着、句柄还指着旧文件），而且换到一半失败就彻底没救了。
/// 放进一个事务里，要么全成，要么原样不动。
fn read_backup(conn: &mut Connection, src: &std::path::Path) -> R<()> {
    check_backup(conn, src)?;

    let src_str = src.to_string_lossy().to_string();
    conn.execute("ATTACH DATABASE ?1 AS bak", params![src_str])
        .map_err(|e| format!("没法挂载备份文件：{e}"))?;

    // 备份里有没有模板表 / 附件表，决定这次要不要一起搬。
    // 1.0.x 建的备份里没有模板那两张；附件表更晚，老备份同样没有。
    // 硬搬会直接报「no such table」
    let has_tpl: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM bak.sqlite_master WHERE type = 'table' AND name = 'templates'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let has_att: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM bak.sqlite_master WHERE type = 'table' AND name = 'attachments'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let copied = (|| -> R<()> {
        let tx = conn.transaction().map_err(e2s)?;
        // 删的顺序顺着外键：先删引用别人的。
        // 附件挂在 tasks 上（CASCADE），但这里显式先删一遍——
        // 不依赖级联的时机，读起来也一眼能看出「附件是跟着工作走的」。
        // 模板那两张只在有新数据的备份里才动，否则会把用户现有的模板平白清掉
        for t in ["attachments", "completions", "tasks"] {
            tx.execute(&format!("DELETE FROM main.{t}"), []).map_err(e2s)?;
        }
        if has_tpl > 0 {
            for t in ["template_items", "templates"] {
                tx.execute(&format!("DELETE FROM main.{t}"), []).map_err(e2s)?;
            }
        }
        for t in ["categories", "settings"] {
            tx.execute(&format!("DELETE FROM main.{t}"), []).map_err(e2s)?;
        }
        // 插的顺序正好相反：先插被引用的
        tx.execute("INSERT INTO main.categories SELECT * FROM bak.categories", [])
            .map_err(e2s)?;
        if has_tpl > 0 {
            for t in ["templates", "template_items"] {
                tx.execute(&format!("INSERT INTO main.{t} SELECT * FROM bak.{t}"), [])
                    .map_err(e2s)?;
            }
        }
        for t in ["tasks", "completions", "settings"] {
            tx.execute(&format!("INSERT INTO main.{t} SELECT * FROM bak.{t}"), [])
                .map_err(e2s)?;
        }
        // 附件必须排在 tasks 之后：它外键指着 tasks
        if has_att > 0 {
            tx.execute("INSERT INTO main.attachments SELECT * FROM bak.attachments", [])
                .map_err(e2s)?;
        }
        tx.commit().map_err(e2s)
    })();

    // 不管成败都要摘掉，否则这个连接再也 ATTACH 不了同名库
    let _ = conn.execute("DETACH DATABASE bak", []);
    copied
}

/// 把当前数据导出成一个独立的 .db 文件，返回落盘的路径。
#[tauri::command]
pub fn backup_to(state: State<'_, AppState>, dest: String) -> R<String> {
    let conn = state.db.lock().map_err(e2s)?;
    write_backup(&conn, std::path::Path::new(&dest))?;
    Ok(dest)
}

/// 校验一个文件确实是本程序导出的备份，且结构跟当前版本对得上。
///
/// 宁可在这儿拦下来，也不要恢复一半 —— 半旧半新的数据比直接失败更难收拾。
fn check_backup(conn: &Connection, path: &std::path::Path) -> R<()> {
    let probe = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("打不开这个文件：{e}"))?;

    for t in TABLES.iter().copied().chain(OPTIONAL_TABLES.iter().copied()) {
        let want: i64 = conn
            .query_row("SELECT count(*) FROM pragma_table_info(?1)", params![t], |r| r.get(0))
            .map_err(e2s)?;
        let got: i64 = probe
            .query_row("SELECT count(*) FROM pragma_table_info(?1)", params![t], |r| r.get(0))
            .map_err(|_| format!("这个文件里读不到 `{t}` 表，不像是工作记录本导出的备份"))?;

        // 可选表允许整个不存在（1.0.x 建的备份里就没有模板表），但存在就得对得上列数
        if got == 0 {
            if OPTIONAL_TABLES.contains(&t) {
                continue;
            }
            return Err(format!("这个文件里没有 `{t}` 表，不像是工作记录本导出的备份"));
        }
        if got != want {
            return Err(format!(
                "备份里的 `{t}` 表是 {got} 列，当前版本是 {want} 列，两边结构对不上，不能直接恢复"
            ));
        }
    }
    Ok(())
}

/// 从备份文件恢复。成功时返回「恢复前自动留存的那一份」的路径，
/// 用户要是点错了还能再切回来。
#[tauri::command]
pub fn restore_from(app: tauri::AppHandle, state: State<'_, AppState>, src: String) -> R<String> {
    let src_path = std::path::PathBuf::from(&src);
    if !src_path.is_file() {
        return Err("找不到这个备份文件".into());
    }

    let mut conn = state.db.lock().map_err(e2s)?;

    // 别把自己还原到自己：ATTACH 同一个文件会撞锁
    if let Some(live) = conn.path() {
        if same_file(std::path::Path::new(live), &src_path) {
            return Err("这就是当前正在用的数据文件，不需要恢复".into());
        }
    }

    // 恢复是破坏性操作，先把「现在」留一份
    let dir = backup_dir_of(&app);
    std::fs::create_dir_all(&dir).map_err(e2s)?;
    let snapshot = dir.join(format!("恢复前-{}.db", now_local().format("%Y%m%d-%H%M%S")));
    write_backup(&conn, &snapshot)
        .map_err(|e| format!("恢复前没法保存当前数据快照，为安全起见已中止：{e}"))?;

    read_backup(&mut conn, &src_path)?;
    Ok(snapshot.to_string_lossy().to_string())
}

fn pattern_cn(p: &str) -> &'static str {
    match p {
        "recurring" => "周期性",
        "stage" => "阶段性",
        _ => "一次性",
    }
}

fn status_cn(s: &str) -> &'static str {
    match s {
        "done" => "已完成",
        _ => "进行中",
    }
}

/// CSV 转义：字段里有逗号、引号或换行就整体加引号，内部引号翻倍
fn csv_cell(s: &str) -> String {
    if s.chars().any(|c| matches!(c, ',' | '"' | '\n' | '\r')) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// 把 RFC3339 转成本地年月日时分；解析不了就原样返回
fn human_time(s: &str) -> String {
    parse_dt(s)
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| s.to_string())
}

/// 导出全部工作为 CSV。开头写 UTF-8 BOM —— 不然 Excel 会把中文读成乱码。
/// 返回导出的行数（不含表头）。
#[tauri::command]
pub fn export_csv(state: State<'_, AppState>, dest: String) -> R<usize> {
    const HEAD: &str = "类型,标题,分类,状态,截止/结束,提醒时间,重复规则,备注,创建时间,完成时间";

    let conn = state.db.lock().map_err(e2s)?;
    let mut stmt = conn
        .prepare(
            "SELECT CAST(t.id AS TEXT), t.title, COALESCE(c.name, '未分类'), t.pattern, t.status,
                    COALESCE(t.end_at, t.due_at), t.remind_at, t.rule, t.note, t.created_at, t.completed_at
             FROM tasks t LEFT JOIN categories c ON c.id = t.category_id
             WHERE t.parent_id IS NULL
             ORDER BY t.created_at, t.id",
        )
        .map_err(e2s)?;

    let rows = stmt
        .query_map([], |r| {
            let v: Vec<Option<String>> = (0..11)
                .map(|i| r.get::<_, Option<String>>(i))
                .collect::<rusqlite::Result<_>>()?;
            Ok(v)
        })
        .map_err(e2s)?;

    let mut body = String::from(HEAD);
    body.push_str("\r\n");
    let mut n = 0usize;

    for row in rows {
        let v = row.map_err(e2s)?;
        let get = |i: usize| v.get(i).and_then(|x| x.clone()).unwrap_or_default();

        let pattern = get(3);
        let rule = get(7);
        let rule_text = if pattern == "recurring" {
            schedule::describe_rule(&rule)
        } else {
            String::new()
        };

        let cells = [
            pattern_cn(&pattern).to_string(),
            get(1),                       // 标题
            get(2),                       // 分类
            status_cn(&get(4)).to_string(),
            human_time(&get(5)),          // 截止/结束
            human_time(&get(6)),          // 提醒
            rule_text,
            get(8),                       // 备注
            human_time(&get(9)),          // 创建
            human_time(&get(10)),         // 完成
        ];

        body.push_str(&cells.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(","));
        body.push_str("\r\n");
        n += 1;
    }

    let path = std::path::PathBuf::from(&dest);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(e2s)?;
        }
    }
    std::fs::write(&path, format!("\u{FEFF}{body}")).map_err(|e| format!("写文件失败：{e}"))?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, TimeZone};

    fn db() -> Connection {
        let c = Connection::open_in_memory().expect("内存库");
        c.execute_batch(crate::db::SCHEMA).expect("建表");
        // 建一个分类：tasks.category_id 有外键约束，测试里要能真的挂上去
        c.execute(
            "INSERT INTO categories (name, color, sort) VALUES ('本职工作', '#4F5BE8', 0)",
            [],
        )
        .expect("建分类");
        c
    }

    /// 造一个阶段性工作（能挂子任务的那种）
    fn stage(conn: &Connection) -> Task {
        conn.execute(
            "INSERT INTO tasks (title, note, category_id, pattern, status, sort)
             VALUES ('完成官网改版', '', 1, 'stage', 'todo', 0)",
            [],
        )
        .expect("插入阶段性工作");
        fetch_task(conn, conn.last_insert_rowid()).expect("取回")
    }

    /// 造一个「每周五 17:00」的周期任务，到期时间由参数指定
    fn recurring(conn: &Connection, due: &str) -> Task {
        conn.execute(
            r#"INSERT INTO tasks (title, note, pattern, status, due_at, rule, sort)
               VALUES ('提交项目周报', '', 'recurring', 'todo', ?1,
                       '{"freq":"weekly","byDay":[5],"time":"17:00"}', 0)"#,
            params![due],
        )
        .expect("插入周期任务");
        fetch_task(conn, conn.last_insert_rowid()).expect("取回")
    }

    #[test]
    fn completing_a_recurring_task_records_it_and_rolls_forward() {
        let conn = db();
        let task = recurring(&conn, "2026-09-25T17:00:00+08:00");
        let now = Local.with_ymd_and_hms(2026, 9, 25, 17, 30, 0).unwrap();

        let next = record_occurrence(&conn, &task, now).expect("记一次完成");

        // 准点完成 → 下一次是下周五
        let next_dt = parse_dt(&next).expect("可解析");
        assert_eq!(next_dt.date_naive().to_string(), "2026-10-02");

        // 历史记录里的标题与分类是冗余存的，任务被删掉后仍然可读
        let (title, cat, n): (String, Option<i64>, i64) = (
            conn.query_row("SELECT title FROM completions", [], |r| r.get(0)).unwrap(),
            conn.query_row("SELECT category_id FROM completions", [], |r| r.get(0)).unwrap(),
            conn.query_row("SELECT COUNT(*) FROM completions", [], |r| r.get(0)).unwrap(),
        );
        assert_eq!(title, "提交项目周报");
        assert_eq!(cat, None);
        assert_eq!(n, 1);
    }

    #[test]
    fn early_completion_does_not_stop_the_rhythm() {
        let conn = db();
        let task = recurring(&conn, "2026-09-25T17:00:00+08:00");
        // 周五的活儿周三就交了
        let now = Local.with_ymd_and_hms(2026, 9, 23, 10, 0, 0).unwrap();
        let next = record_occurrence(&conn, &task, now).unwrap();
        assert_eq!(
            parse_dt(&next).unwrap().date_naive().to_string(),
            "2026-10-02",
            "本周五那次已了结，下一次应是下周五"
        );
    }

    #[test]
    fn deleting_the_task_keeps_its_history() {
        let conn = db();
        let task = recurring(&conn, "2026-09-25T17:00:00+08:00");
        record_occurrence(&conn, &task, Local::now()).unwrap();

        conn.execute("DELETE FROM tasks WHERE id = ?1", params![task.id]).unwrap();

        let n: i64 = conn.query_row("SELECT COUNT(*) FROM completions", [], |r| r.get(0)).unwrap();
        let tid: Option<i64> =
            conn.query_row("SELECT task_id FROM completions", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1, "删掉周期任务不该把历史一起删掉");
        assert_eq!(tid, None, "task_id 置空而不是级联删除");
    }

    #[test]
    fn a_task_without_a_usable_rule_keeps_its_original_time() {
        let conn = db();
        conn.execute(
            r#"INSERT INTO tasks (title, note, pattern, status, due_at, sort)
               VALUES ('没有规则的周期任务', '', 'recurring', 'todo', '2026-09-25T17:00:00+08:00', 0)"#,
            [],
        )
        .unwrap();
        let task = fetch_task(&conn, 1).unwrap();
        let next = record_occurrence(&conn, &task, Local::now()).unwrap();
        assert_eq!(
            parse_dt(&next).unwrap().date_naive().to_string(),
            "2026-09-25",
            "算不出下一次时应保留原时间，而不是留下空值让任务消失"
        );
    }

    /* ---------------- 子任务 ---------------- */

    #[test]
    fn subtasks_hang_under_their_parent_not_in_the_main_list() {
        let conn = db();
        let parent = stage(&conn);
        insert_subtask(&conn, parent.id, "首页终稿").unwrap();
        insert_subtask(&conn, parent.id, "产品页终稿").unwrap();

        let top = top_level_tasks(&conn).unwrap();
        let subs = subtask_rows(&conn).unwrap();
        assert_eq!(top.len(), 1, "主列表只该看到阶段性工作本身");
        assert_eq!(top[0].id, parent.id);
        assert_eq!(subs.len(), 2, "两个子任务要能取回来");
        assert!(subs.iter().all(|s| s.parent_id == Some(parent.id)));
    }

    #[test]
    fn subtask_inherits_the_parent_category() {
        let conn = db();
        let parent = stage(&conn);
        let sub = insert_subtask(&conn, parent.id, "首页终稿").unwrap();
        assert_eq!(
            sub.category_id,
            Some(1),
            "子任务单独挂分类没意义，应跟随父任务"
        );
    }

    #[test]
    fn deleting_a_stage_task_takes_its_subtasks_with_it() {
        let conn = db();
        let parent = stage(&conn);
        insert_subtask(&conn, parent.id, "首页终稿").unwrap();

        conn.execute("DELETE FROM tasks WHERE id = ?1", params![parent.id]).unwrap();

        let left: i64 = conn
            .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0, "父任务删掉后不该留下无主的子任务");
    }

    #[test]
    fn only_stage_tasks_can_hold_subtasks() {
        let conn = db();
        conn.execute(
            "INSERT INTO tasks (title, note, pattern, status, sort)
             VALUES ('回一封邮件', '', 'once', 'todo', 0)",
            [],
        )
        .unwrap();
        let err = match insert_subtask(&conn, 1, "拆一步") {
            Err(e) => e,
            Ok(_) => panic!("一次性工作不该能拆子任务"),
        };
        assert!(err.contains("阶段性"), "报错要说清是哪种工作才能拆：{err}");
    }

    #[test]
    fn orphan_subtasks_are_not_handed_to_the_ui() {
        let conn = db();
        let parent = stage(&conn);
        insert_subtask(&conn, parent.id, "首页终稿").unwrap();

        // 模拟绕过外键的改库：留下一条挂不到任何父任务的子任务
        conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
        conn.execute("DELETE FROM tasks WHERE id = ?1", params![parent.id]).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();

        assert_eq!(subtask_rows(&conn).unwrap().len(), 0, "孤儿不该出现在界面上");
    }

    #[test]
    fn subtask_title_cannot_be_blank() {
        let conn = db();
        let parent = stage(&conn);
        assert!(insert_subtask(&conn, parent.id, "   ").is_err());
        assert_eq!(subtask_rows(&conn).unwrap().len(), 0);
    }

    #[test]
    fn subtasks_nest_three_levels_but_no_deeper() {
        let conn = db();
        let l1 = stage(&conn);
        let l2 = insert_subtask(&conn, l1.id, "移动端适配").unwrap();
        let l3 = insert_subtask(&conn, l2.id, "窄屏走查").expect("第二层下面还能再拆一层");

        assert_eq!(l3.parent_id, Some(l2.id));
        assert_eq!(subtask_rows(&conn).unwrap().len(), 2);

        // 第三层是底，再往下就该被挡住
        let err = match insert_subtask(&conn, l3.id, "再分一层") {
            Err(e) => e,
            Ok(_) => panic!("第四层不该允许"),
        };
        assert!(err.contains("最多"), "报错要说清是层数到了：{err}");
        assert_eq!(subtask_rows(&conn).unwrap().len(), 2, "被拒的那条不该落库");
    }

    #[test]
    fn a_nested_step_can_hold_children_even_though_it_is_not_stage() {
        let conn = db();
        let l1 = stage(&conn);
        let l2 = insert_subtask(&conn, l1.id, "移动端适配").unwrap();

        // 第二层的步骤本身是 once，但它已经是「一步」了，
        // 给它再分小步是合理的——「只有阶段性才能拆」这条只对顶层成立
        let l3 = insert_subtask(&conn, l2.id, "窄屏走查").unwrap();
        assert_eq!(l3.pattern, "once");
        assert_eq!(l3.category_id, Some(1), "隔了一层也要继承最上面那个分类");
    }

    #[test]
    fn depth_guard_survives_a_cycle_in_the_data() {
        let conn = db();
        let a = stage(&conn);
        let b = insert_subtask(&conn, a.id, "第一步").unwrap();
        // 人为造一个环：把父任务挂到它自己的子任务下面。
        // 外部工具改库可能出现这种数据，遍历必须能自己停下来
        conn.execute("UPDATE tasks SET parent_id = ?1 WHERE id = ?2", params![b.id, a.id])
            .unwrap();
        assert!(depth_of(&conn, b.id).is_err(), "有环时必须报错，而不是转不出来");
    }

    /* ---------------- 复制一份 ---------------- */

    /// 把某个任务的开始 / 交期固定下来，方便对着算天数
    fn set_span(conn: &Connection, id: i64, due: Option<&str>, end: Option<&str>) {
        conn.execute(
            "UPDATE tasks SET due_at = ?1, end_at = ?2 WHERE id = ?3",
            params![due, end, id],
        )
        .expect("设置日期");
    }

    #[test]
    fn duplicating_a_task_brings_its_steps_and_shifts_the_dates() {
        let conn = db();
        let parent = stage(&conn);
        set_span(&conn, parent.id, Some("2026-10-01T09:00:00+08:00"), Some("2026-10-10T23:59:00+08:00"));
        insert_subtask(&conn, parent.id, "第一步").unwrap();
        insert_subtask(&conn, parent.id, "第二步").unwrap();

        let copy = duplicate_task_inner(&conn, parent.id, Some("2026-11-01")).expect("复制");

        assert_ne!(copy.id, parent.id);
        assert!(copy.title.ends_with("（副本）"), "顶层要一眼看出是复制来的：{}", copy.title);
        assert_eq!(copy.status, "todo", "副本从没做过开始");

        // 整体平移：10/1 → 11/1 是 +31 天，内部跨度（9 天）不变
        let due = parse_dt(copy.due_at.as_deref().unwrap()).unwrap();
        let end = parse_dt(copy.end_at.as_deref().unwrap()).unwrap();
        assert_eq!(due.date_naive(), NaiveDate::from_ymd_opt(2026, 11, 1).unwrap());
        assert_eq!(due.time().to_string(), "09:00:00", "时分不该被平移改掉");
        assert_eq!((end.date_naive() - due.date_naive()).num_days(), 9, "跨度不能被改");

        let kids: Vec<Task> = subtask_rows(&conn)
            .unwrap()
            .into_iter()
            .filter(|s| s.parent_id == Some(copy.id))
            .collect();
        assert_eq!(kids.len(), 2, "子任务要跟着复制");
        assert!(
            kids.iter().all(|k| !k.title.contains("副本")),
            "只有顶层加「副本」字样，否则一棵树上全是这几个字"
        );
    }

    #[test]
    fn duplicating_keeps_the_whole_nested_tree() {
        let conn = db();
        let l1 = stage(&conn);
        let l2 = insert_subtask(&conn, l1.id, "移动端适配").unwrap();
        insert_subtask(&conn, l2.id, "窄屏走查").unwrap();

        let copy = duplicate_task_inner(&conn, l1.id, None).expect("复制");
        let all = subtask_rows(&conn).unwrap();
        let mine: Vec<&Task> = all.iter().filter(|s| s.parent_id == Some(copy.id)).collect();
        assert_eq!(mine.len(), 1, "第二层要在");
        let grand: Vec<&Task> =
            all.iter().filter(|s| s.parent_id == Some(mine[0].id)).collect();
        assert_eq!(grand.len(), 1, "第三层也要跟着过来，否则复制出来的是棵断树");
    }

    #[test]
    fn duplicating_without_a_base_keeps_the_original_dates() {
        let conn = db();
        let parent = stage(&conn);
        set_span(&conn, parent.id, Some("2026-10-01T09:00:00+08:00"), None);

        let copy = duplicate_task_inner(&conn, parent.id, None).expect("复制");
        // 取回最新的一条再比：parent 是设日期之前拿的快照，此时它的 due_at 还是空的
        let fresh = fetch_task(&conn, parent.id).unwrap();
        assert_eq!(copy.due_at, fresh.due_at, "没给基准日就原地复制，日期不动");
    }

    #[test]
    fn duplicating_a_recurring_task_does_not_copy_a_stale_date() {
        let conn = db();
        let task = recurring(&conn, "2020-01-03T17:00:00+08:00");   // 一个早就过去的周五

        let copy = duplicate_task_inner(&conn, task.id, None).expect("复制");
        let due = parse_dt(copy.due_at.as_deref().unwrap()).unwrap();
        assert!(
            due > Local.with_ymd_and_hms(2020, 6, 1, 0, 0, 0).unwrap(),
            "周期任务照搬旧时间的话，复制出来一出生就是逾期的：{due}"
        );
    }

    /* ---------------- 模板 ---------------- */

    #[test]
    fn a_template_stores_offsets_not_absolute_dates() {
        let conn = db();
        let parent = stage(&conn);
        set_span(&conn, parent.id, Some("2026-10-01T09:00:00+08:00"), Some("2026-10-05T23:59:00+08:00"));
        let sub = insert_subtask(&conn, parent.id, "第一步").unwrap();
        set_span(&conn, sub.id, Some("2026-10-02T18:00:00+08:00"), None);

        let tpl = template_from_task(&conn, parent.id, "季度复盘").expect("存模板");
        assert_eq!(tpl.items.len(), 2);
        assert_eq!(tpl.name, "季度复盘");

        let root = tpl.items.iter().find(|i| i.parent_id.is_none()).unwrap();
        assert_eq!(root.due_offset, Some(0), "根任务的开始日就是基准日");
        assert_eq!(root.end_offset, Some(4));
        assert_eq!(root.due_time.as_deref(), Some("09:00"), "时分要单独存，光留天数不够");
        assert_eq!(root.pattern, "stage");

        let kid = tpl.items.iter().find(|i| i.parent_id.is_some()).unwrap();
        assert_eq!(kid.due_offset, Some(1), "第二步相对基准日是第 1 天");
        assert_eq!(kid.due_time.as_deref(), Some("18:00"), "第二层的时间也要留住");

        // 库里绝不能出现绝对日期：存了「10 月 8 日截止」，下个月调用就过期了
        let mut stmt = conn.prepare("PRAGMA table_info(template_items)").unwrap();
        let cols: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert!(
            !cols.iter().any(|c| c == "due_at" || c == "end_at"),
            "模板表里不该有绝对日期字段：{cols:?}"
        );
    }

    #[test]
    fn applying_a_template_lays_the_tree_onto_the_base_date() {
        let conn = db();
        let parent = stage(&conn);
        set_span(&conn, parent.id, Some("2026-10-01T09:00:00+08:00"), None);
        let sub = insert_subtask(&conn, parent.id, "第一步").unwrap();
        set_span(&conn, sub.id, Some("2026-10-03T15:00:00+08:00"), None);
        let l3 = insert_subtask(&conn, sub.id, "再分一小步").unwrap();
        set_span(&conn, l3.id, None, None);

        let tpl = template_from_task(&conn, parent.id, "季度复盘").unwrap();
        // 原件清干净，只留模板——这样测的才是模板本身
        conn.execute("DELETE FROM tasks", []).unwrap();

        let top = apply_template_inner(&conn, tpl.id, "2026-12-01").expect("应用模板");
        assert_eq!(top, 1, "模板里只有 1 件顶层工作");

        let all = subtask_rows(&conn).unwrap();
        assert_eq!(all.len(), 2, "两层步骤都要建出来");

        let root = &top_level_tasks(&conn).unwrap()[0];
        let due = parse_dt(root.due_at.as_deref().unwrap()).unwrap();
        assert_eq!(due.date_naive(), NaiveDate::from_ymd_opt(2026, 12, 1).unwrap());
        assert_eq!(due.time().to_string(), "09:00:00", "时分从模板里还原");

        let step2 = all.iter().find(|s| s.title == "第一步").unwrap();
        let d2 = parse_dt(step2.due_at.as_deref().unwrap()).unwrap();
        assert_eq!(d2.date_naive(), NaiveDate::from_ymd_opt(2026, 12, 3).unwrap(), "第 2 天");
        assert_eq!(d2.time().to_string(), "15:00:00");

        let deep = all.iter().find(|s| s.title == "再分一小步").unwrap();
        assert!(deep.due_at.is_none(), "模板里没有日期的步骤，建出来也不该凭空多一个日期");
        assert_eq!(deep.parent_id, Some(step2.id), "第三层要挂在第二层下面");
    }

    #[test]
    fn a_template_keeps_working_after_its_category_is_deleted() {
        let conn = db();
        let parent = stage(&conn);   // 挂在分类 1 上
        let tpl = template_from_task(&conn, parent.id, "带分类的模板").unwrap();
        conn.execute("DELETE FROM categories WHERE id = 1", []).unwrap();

        let top = apply_template_inner(&conn, tpl.id, "2026-12-01").expect("分类没了也要能建");
        assert_eq!(top, 1);
        assert_eq!(
            top_level_tasks(&conn).unwrap()[0].category_id,
            None,
            "分类没了就落到「未分类」，不能为这个拦下整批"
        );
    }

    #[test]
    fn a_template_needs_a_name_and_some_content() {
        let conn = db();
        let parent = stage(&conn);
        assert!(template_from_task(&conn, parent.id, "   ").is_err());
        assert_eq!(all_templates(&conn).unwrap().len(), 0, "没名字的模板不该留下");

        // 空模板（删掉条目后）应用时要给出说得清的话，而不是默默建出零条
        let tpl = template_from_task(&conn, parent.id, "空壳").unwrap();
        conn.execute("DELETE FROM template_items WHERE template_id = ?1", params![tpl.id])
            .unwrap();
        assert!(apply_template_inner(&conn, tpl.id, "2026-12-01").is_err());
    }

    #[test]
    fn deleting_a_template_leaves_its_tasks_alone() {
        let conn = db();
        let parent = stage(&conn);
        let tpl = template_from_task(&conn, parent.id, "随手存的").unwrap();
        apply_template_inner(&conn, tpl.id, "2026-12-01").unwrap();

        let before = top_level_tasks(&conn).unwrap().len();
        conn.execute("DELETE FROM templates WHERE id = ?1", params![tpl.id]).unwrap();

        assert_eq!(top_level_tasks(&conn).unwrap().len(), before, "删模板不能连累已建出来的工作");
        assert_eq!(all_templates(&conn).unwrap().len(), 0);
        // 条目靠外键级联清掉，不留垃圾
        let left: i64 = conn
            .query_row("SELECT COUNT(*) FROM template_items", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0, "模板删了，它的条目也该跟着走");
    }

    /* ---- 备份 / 恢复 ---- */

    /// 每个测试用独立的临时目录，避免并行跑的时候互相踩
    fn tmp_dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("wl-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("建临时目录");
        d
    }

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .expect("计数")
    }

    fn task(conn: &Connection, title: &str) {
        conn.execute(
            "INSERT INTO tasks (title, note, pattern, status, sort) VALUES (?1, '', 'once', 'todo', 0)",
            params![title],
        )
        .expect("建任务");
    }

    /// 备份 → 把数据改坏 → 恢复，最后必须回到备份那一刻的样子。
    /// 这是整个功能的核心承诺，破了其他都白搭。
    #[test]
    fn backup_then_restore_round_trips() {
        let dir = tmp_dir("round");
        let bak = dir.join("bak.db");

        let mut live = db();
        task(&live, "甲");
        task(&live, "乙");
        write_backup(&live, &bak).expect("导出");

        // 备份之后又乱动一通
        live.execute("DELETE FROM tasks", []).unwrap();
        task(&live, "丙");
        live.execute("DELETE FROM categories", []).unwrap();
        assert_eq!(count(&live, "tasks"), 1);

        read_backup(&mut live, &bak).expect("恢复");

        assert_eq!(count(&live, "tasks"), 2, "任务数要回到备份时");
        assert_eq!(count(&live, "categories"), 1, "分类也要一起回来");
        let titles: Vec<String> = live
            .prepare("SELECT title FROM tasks ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(titles, vec!["甲", "乙"], "连内容都得对上，不能只剩个数量");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 备份要把模板一起带走。
    /// 模板表是后加的，容易在「备份搬哪几张表」这类清单里被漏掉，
    /// 漏掉的后果是用户恢复完发现模板全没了——而且当时不会报任何错。
    #[test]
    fn a_backup_carries_templates_too() {
        let dir = tmp_dir("tpl");
        let bak = dir.join("bak.db");

        let mut live = db();
        let p = stage(&live);
        insert_subtask(&live, p.id, "第一步").unwrap();
        template_from_task(&live, p.id, "季度复盘").unwrap();
        write_backup(&live, &bak).expect("导出");

        live.execute("DELETE FROM templates", []).unwrap();
        assert_eq!(count(&live, "templates"), 0);

        read_backup(&mut live, &bak).expect("恢复");

        assert_eq!(count(&live, "templates"), 1, "模板要跟着备份回来");
        assert_eq!(count(&live, "template_items"), 2, "模板里的条目也要回来");
        let t = all_templates(&live).unwrap();
        assert_eq!(t[0].name, "季度复盘");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 老版本建的备份里没有模板表，不能因此就把整个备份拒掉——
    /// 用户可能只有那一份备份。这种情况按「跳过模板」处理。
    #[test]
    fn restoring_an_old_backup_without_template_tables_still_works() {
        let dir = tmp_dir("old");
        let old = dir.join("old.db");

        // 手工造一个 1.0.x 时期的备份：四张表，没有模板表
        {
            let c = Connection::open(&old).expect("建老备份");
            c.execute_batch(
                "CREATE TABLE categories (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL,
                                          color TEXT NOT NULL DEFAULT '#000',
                                          sort INTEGER NOT NULL DEFAULT 0);
                 CREATE TABLE tasks (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL,
                                     note TEXT NOT NULL DEFAULT '', category_id INTEGER,
                                     pattern TEXT NOT NULL DEFAULT 'once',
                                     status TEXT NOT NULL DEFAULT 'todo',
                                     due_at TEXT, end_at TEXT, remind_at TEXT, rule TEXT,
                                     progress TEXT, parent_id INTEGER,
                                     sort INTEGER NOT NULL DEFAULT 0,
                                     created_at TEXT, completed_at TEXT, notified_at TEXT);
                 CREATE TABLE completions (id INTEGER PRIMARY KEY AUTOINCREMENT, task_id INTEGER,
                                           title TEXT NOT NULL, category_id INTEGER,
                                           pattern TEXT NOT NULL DEFAULT 'recurring',
                                           due_at TEXT, done_at TEXT NOT NULL);
                 CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 INSERT INTO categories (name, color, sort) VALUES ('老分类', '#123456', 0);
                 INSERT INTO tasks (title, note, pattern, status, sort)
                       VALUES ('老任务', '', 'once', 'todo', 0);",
            )
            .expect("建老备份的表");
        }

        let mut live = db();
        let p = stage(&live);
        template_from_task(&live, p.id, "恢复前就有的模板").unwrap();

        read_backup(&mut live, &old).expect("老备份必须能恢复");

        assert_eq!(count(&live, "tasks"), 1, "老备份里的工作要进来");
        assert_eq!(
            all_templates(&live).unwrap().len(),
            1,
            "老备份里没有模板信息，就不该动现有的模板"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 恢复要能顶掉「现在的分类」，否则外键会指向已经不存在的行
    #[test]
    fn restore_replaces_categories_not_just_tasks() {
        let dir = tmp_dir("cat");
        let bak = dir.join("bak.db");

        let mut live = db(); // db() 自带一个「本职工作」
        write_backup(&live, &bak).expect("导出");

        live.execute("DELETE FROM categories", []).unwrap();
        live.execute(
            "INSERT INTO categories (name, color, sort) VALUES ('临时分类', '#000', 0)",
            [],
        )
        .unwrap();

        read_backup(&mut live, &bak).expect("恢复");

        let names: Vec<String> = live
            .prepare("SELECT name FROM categories")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(names, vec!["本职工作"]);
        assert_eq!(count(&live, "tasks"), 0, "顺带把任务也清回备份状态");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 随便挑个 sqlite 文件不能当备份用 —— 得在动数据之前就拦下来
    #[test]
    fn restore_rejects_a_file_that_is_not_a_backup() {
        let dir = tmp_dir("bad");
        let junk = dir.join("junk.db");
        {
            let c = Connection::open(&junk).unwrap();
            c.execute_batch("CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT);")
                .unwrap();
        }

        let mut live = db();
        task(&live, "不该被弄丢的工作");

        let err = read_backup(&mut live, &junk).expect_err("应当拒绝");
        assert!(err.contains("不像是工作记录本导出的备份"), "报错要说人话: {err}");
        assert_eq!(count(&live, "tasks"), 1, "拒绝之后原数据必须毫发无损");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 老版本导出的备份结构不一样，宁可明确报错也不要恢复出半旧半新的库
    #[test]
    fn restore_rejects_a_backup_with_a_different_shape() {
        let dir = tmp_dir("shape");
        let old = dir.join("old.db");
        {
            let c = Connection::open(&old).unwrap();
            // 四张表都在，但列数跟当前版本对不上
            c.execute_batch(
                "CREATE TABLE categories (id INTEGER PRIMARY KEY);
                 CREATE TABLE tasks (id INTEGER PRIMARY KEY);
                 CREATE TABLE completions (id INTEGER PRIMARY KEY);
                 CREATE TABLE settings (id INTEGER PRIMARY KEY);",
            )
            .unwrap();
        }

        let mut live = db();
        task(&live, "也别动我");

        let err = read_backup(&mut live, &old).expect_err("应当拒绝");
        assert!(err.contains("结构对不上"), "报错要指明原因: {err}");
        assert_eq!(count(&live, "tasks"), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 恢复失败之后连接还得是好的，不能把 ATTACH 挂着不放
    #[test]
    fn failed_restore_leaves_the_connection_usable() {
        let dir = tmp_dir("usable");
        let junk = dir.join("junk.db");
        {
            let c = Connection::open(&junk).unwrap();
            c.execute_batch("CREATE TABLE x (id INTEGER);").unwrap();
        }

        let mut live = db();
        assert!(read_backup(&mut live, &junk).is_err());
        // 还能正常读写就说明没被挂住
        task(&live, "恢复失败后照样能建工作");
        assert_eq!(count(&live, "tasks"), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 备份文件里装的是完整数据，换台机器打开就能用
    #[test]
    fn backup_file_is_a_standalone_database() {
        let dir = tmp_dir("standalone");
        let bak = dir.join("bak.db");

        let live = db();
        task(&live, "拿去另一台机器");
        write_backup(&live, &bak).expect("导出");

        let other = Connection::open(&bak).expect("备份文件本身就该是个能打开的库");
        assert_eq!(count(&other, "tasks"), 1);
        assert_eq!(count(&other, "categories"), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 目标已存在时要能覆盖 —— 「另存为」里选个同名文件是最常见的操作
    #[test]
    fn backup_overwrites_an_existing_file() {
        let dir = tmp_dir("overwrite");
        let bak = dir.join("bak.db");
        std::fs::write(&bak, "占位文件，等着被覆盖").unwrap();

        let live = db();
        task(&live, "覆盖测试");
        write_backup(&live, &bak).expect("应当覆盖");

        let other = Connection::open(&bak).unwrap();
        assert_eq!(count(&other, "tasks"), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /* ---- CSV 转义 ---- */

    #[test]
    fn csv_escapes_only_when_needed() {
        assert_eq!(csv_cell("普通标题"), "普通标题");
        assert_eq!(csv_cell("带,逗号"), "\"带,逗号\"");
        assert_eq!(csv_cell("带\"引号"), "\"带\"\"引号\"");
        assert_eq!(csv_cell("带\n换行"), "\"带\n换行\"");
    }

    #[test]
    fn human_time_formats_or_passes_through() {
        assert!(!human_time("2026-09-25T17:00:00+08:00").contains('T'));
        // 解析不了就原样返回，不能把内容吃掉
        assert_eq!(human_time("乱七八糟"), "乱七八糟");
    }

    /* ---- 附件（粘贴的图片 / 上传的文件） ---- */

    fn b64(bytes: &[u8]) -> String {
        STANDARD.encode(bytes)
    }

    /// 存进去 → 列出来 → 取内容，三处口径必须一致
    #[test]
    fn attachment_round_trips_through_the_database() {
        let conn = db();
        task(&conn, "和甲方的沟通留痕");
        let tid: i64 = conn.last_insert_rowid();

        let saved = insert_attachment(
            &conn,
            tid,
            "聊天记录.png",
            "image/png",
            "image",
            &b64(b"PNGDATA"),
            Some(&b64(b"THUMB")),
        )
        .expect("存附件");

        assert_eq!(saved.task_id, tid);
        assert_eq!(saved.kind, "image");
        assert_eq!(saved.size, 7);
        assert!(saved.has_thumb);

        let list = list_attachments_of(&conn, Some(tid)).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "聊天记录.png");

        // 原图与缩略图各取各的
        assert_eq!(attachment_data_url(&conn, saved.id, false).unwrap(), "data:image/png;base64,UE5HREFUQQ==");
        assert_eq!(attachment_data_url(&conn, saved.id, true).unwrap(), "data:image/jpeg;base64,VEhVTUI=");

        remove_attachment(&conn, saved.id).unwrap();
        assert!(list_attachments_of(&conn, Some(tid)).unwrap().is_empty());
    }

    /// 列表里**不能**带图片本体 —— 带上就意味着抽屉一打开要搬几十 MB
    #[test]
    fn attachment_metadata_is_selectable_without_the_body() {
        let conn = db();
        task(&conn, "带图的工作");
        let tid: i64 = conn.last_insert_rowid();
        // 非图片没有缩略图，has_thumb 要是 false，列表才不会去取不存在的小图
        insert_attachment(&conn, tid, "合同.pdf", "application/pdf", "file", &b64(b"PDF"), None).unwrap();

        let list = list_attachments_of(&conn, None).unwrap();
        assert_eq!(list.len(), 1);
        assert!(!list[0].has_thumb);
        assert_eq!(list[0].kind, "file");
        // 没有缩略图时退回原图，而不是返回空
        assert_eq!(
            attachment_data_url(&conn, list[0].id, true).unwrap(),
            "data:application/pdf;base64,UERG"
        );
    }

    /// 只取某一条工作的附件，别的不能混进来
    #[test]
    fn attachments_are_filtered_by_task() {
        let conn = db();
        task(&conn, "甲");
        let a: i64 = conn.last_insert_rowid();
        task(&conn, "乙");
        let b: i64 = conn.last_insert_rowid();

        insert_attachment(&conn, a, "a.png", "image/png", "image", &b64(b"A"), None).unwrap();
        insert_attachment(&conn, b, "b.png", "image/png", "image", &b64(b"B"), None).unwrap();

        assert_eq!(list_attachments_of(&conn, Some(a)).unwrap().len(), 1);
        assert_eq!(list_attachments_of(&conn, Some(b)).unwrap()[0].name, "b.png");
        assert_eq!(list_attachments_of(&conn, None).unwrap().len(), 2);
    }

    /// 附件是这条工作的证据，工作删了证据跟着走（外键 CASCADE）
    #[test]
    fn deleting_the_task_takes_its_attachments_with_it() {
        let conn = db();
        task(&conn, "要被删掉的工作");
        let tid: i64 = conn.last_insert_rowid();
        insert_attachment(&conn, tid, "截图.png", "image/png", "image", &b64(b"X"), None).unwrap();

        conn.execute("DELETE FROM tasks WHERE id = ?1", params![tid]).unwrap();

        assert_eq!(count(&conn, "attachments"), 0, "工作没了，挂在它上面的附件不该留下来");
    }

    /// 脏数据不能进库：坏 base64、空文件、超限的都要拦下来
    #[test]
    fn attachment_rejects_bad_input() {
        let conn = db();
        task(&conn, "目标");
        let tid: i64 = conn.last_insert_rowid();

        assert!(insert_attachment(&conn, tid, "x", "image/png", "image", "这不是 base64!!", None).is_err());
        assert!(insert_attachment(&conn, tid, "x", "image/png", "image", "", None).is_err());

        let huge = vec![0u8; MAX_ATTACHMENT_BYTES + 1];
        let err = insert_attachment(&conn, tid, "巨无霸", "image/png", "image", &b64(&huge), None).unwrap_err();
        assert!(err.contains("MB"), "超限要给人话，不是 SQL 报错：{err}");

        // 上面全部失败之后库里不该留下任何一条
        assert_eq!(count(&conn, "attachments"), 0);
    }

    /// 挂到不存在的工作上要拦住，并给一句人话（外键报的是 SQL 错误，用户看不懂）
    #[test]
    fn attachment_needs_an_existing_task() {
        let conn = db();
        let err = insert_attachment(&conn, 999, "x", "image/png", "image", &b64(b"X"), None).unwrap_err();
        assert_eq!(err, "这条工作已经不在了");
    }

    /// 缩略图坏了不该让整条附件存不进去 —— 顶多列表多取一次原图
    #[test]
    fn a_broken_thumbnail_does_not_lose_the_attachment() {
        let conn = db();
        task(&conn, "目标");
        let tid: i64 = conn.last_insert_rowid();

        let saved =
            insert_attachment(&conn, tid, "x.png", "image/png", "image", &b64(b"FULL"), Some("坏缩略图"))
                .expect("仍应存进去");
        assert!(!saved.has_thumb);
        assert_eq!(
            attachment_data_url(&conn, saved.id, true).unwrap(),
            "data:image/png;base64,RlVMTA=="
        );
    }

    /// 文件名是从剪贴板/文件系统来的，带路径分隔符时不能被写到别的地方去
    #[test]
    fn safe_file_name_strips_paths_and_illegal_chars() {
        assert_eq!(safe_file_name("C:\\Users\\a\\截图.png", 1), "截图.png");
        assert_eq!(safe_file_name("../../etc/passwd", 1), "passwd");
        assert_eq!(safe_file_name("a:b*c?.png", 1), "a_b_c_.png");
        // 全被清空时退回一个兜底名字，不能拼出个空路径
        assert_eq!(safe_file_name("...", 7), "附件-7");
    }

    /* ---- 附件与备份 / 恢复 ---- */

    /// 备份要连图片一起带走，恢复要能原样还回来 —— 这正是把图片存进库的理由
    #[test]
    fn backup_and_restore_carry_attachments() {
        let dir = tmp_dir("att-round");
        let bak = dir.join("bak.db");

        let mut live = db();
        task(&live, "有留痕的工作");
        let tid: i64 = live.last_insert_rowid();
        insert_attachment(&live, tid, "聊天.png", "image/png", "image", &b64(b"CHAT"), Some(&b64(b"T")))
            .unwrap();
        write_backup(&live, &bak).expect("导出");

        // 备份之后把附件删干净，再恢复
        live.execute("DELETE FROM attachments", []).unwrap();
        assert_eq!(count(&live, "attachments"), 0);

        read_backup(&mut live, &bak).expect("恢复");

        assert_eq!(count(&live, "attachments"), 1, "恢复后图片必须还在");
        let id: i64 = live.query_row("SELECT id FROM attachments", [], |r| r.get(0)).unwrap();
        assert_eq!(
            attachment_data_url(&live, id, false).unwrap(),
            "data:image/png;base64,Q0hBVA==",
            "图片本体要一字不差地还回来"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 老版本（1.0.x / 1.1.x）建的备份里没有 attachments 表。
    /// 这种备份必须照样能恢复 —— 为一张新增的表去拒绝，用户会平白丢掉全部数据。
    #[test]
    fn restore_accepts_an_old_backup_without_attachments() {
        let dir = tmp_dir("att-old");
        let bak = dir.join("old.db");

        let live = db();
        task(&live, "甲");
        task(&live, "乙");
        write_backup(&live, &bak).expect("导出");

        // 把备份降级成「老版本」的样子
        let old = Connection::open(&bak).unwrap();
        old.execute("DROP TABLE attachments", []).unwrap();
        drop(old);

        let mut target = db();
        task(&mut target, "恢复前就有的");
        let tid: i64 = target.last_insert_rowid();
        insert_attachment(&mut target, tid, "旧附件.png", "image/png", "image", &b64(b"OLD"), None).unwrap();

        read_backup(&mut target, &bak).expect("老备份应当能恢复");

        assert_eq!(count(&target, "tasks"), 2, "备份里的工作要回来");
        assert_eq!(
            count(&target, "attachments"),
            0,
            "备份里没有附件表，恢复后不该留着恢复前的那条孤儿"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
