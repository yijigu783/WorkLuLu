use crate::schedule;
use crate::AppState;
use chrono::{DateTime, Local};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::State;

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

/// 新建子任务。分类跟随父任务——子任务单独挂分类没有意义，
/// 而且统计按分类汇总时会把「一步」算成「一件工作」。
///
/// 抽成自由函数而不是直接写在命令里，是为了能脱离 Tauri 的 State 写测试。
pub(crate) fn insert_subtask(conn: &Connection, parent_id: i64, title: &str) -> R<Task> {
    let title = title.trim();
    if title.is_empty() {
        return Err("子任务不能没有名字".into());
    }
    let (pattern, category_id): (String, Option<i64>) = conn
        .query_row(
            "SELECT pattern, category_id FROM tasks WHERE id = ?1",
            params![parent_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| "要挂在哪个工作下面？父任务找不到了".to_string())?;
    if pattern != "stage" {
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

#[tauri::command]
pub fn set_setting(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> R<()> {
    {
        let conn = state.db.lock().map_err(e2s)?;
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map_err(e2s)?;
    }

    // 开机自启是要真切生效的系统行为，不能只写个开关就当设过了
    if key == "autostart" {
        use tauri_plugin_autostart::ManagerExt;
        let on = matches!(value.as_str(), "1" | "true" | "on");
        let mgr = app.autolaunch();
        let done = if on { mgr.enable() } else { mgr.disable() };
        done.map_err(|e| format!("写入开机自启失败：{e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn data_dir(app: tauri::AppHandle) -> R<String> {
    Ok(crate::db::data_dir(&app).to_string_lossy().to_string())
}

/// 在资源管理器里打开数据目录。
/// 走 Rust 侧 API 而非 JS 的 `plugin:opener|open_path`，免去 ACL 路径白名单配置。
#[tauri::command]
pub fn open_data_dir(app: tauri::AppHandle) -> R<()> {
    let dir = crate::db::data_dir(&app);
    let _ = std::fs::create_dir_all(&dir);
    tauri_plugin_opener::open_path(&dir, None::<String>).map_err(e2s)
}

/* ---------------- 备份 / 恢复 / 导出 ---------------- */

/// 一份完整备份要有的四张表。少一张就不认。
const TABLES: [&str; 4] = ["categories", "tasks", "completions", "settings"];

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

    let copied = (|| -> R<()> {
        let tx = conn.transaction().map_err(e2s)?;
        // 删的顺序顺着外键：先删引用别人的
        for t in ["completions", "tasks", "categories", "settings"] {
            tx.execute(&format!("DELETE FROM main.{t}"), []).map_err(e2s)?;
        }
        // 插的顺序正好相反：先插被引用的
        for t in ["categories", "tasks", "completions", "settings"] {
            tx.execute(&format!("INSERT INTO main.{t} SELECT * FROM bak.{t}"), [])
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

    for t in TABLES {
        let want: i64 = conn
            .query_row("SELECT count(*) FROM pragma_table_info(?1)", params![t], |r| r.get(0))
            .map_err(e2s)?;
        let got: i64 = probe
            .query_row("SELECT count(*) FROM pragma_table_info(?1)", params![t], |r| r.get(0))
            .map_err(|_| format!("这个文件里读不到 `{t}` 表，不像是工作记录本导出的备份"))?;

        if got == 0 {
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
    use chrono::TimeZone;

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
}
