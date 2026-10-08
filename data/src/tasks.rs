//! 工作项：三类工作（一次性 / 周期性 / 阶段性）与它们的子任务、完成记录。
//!
//! 这是数据层最大的一块，也是**周期排期行为唯一落地的地方** ——
//! 「完成后滚到下一次」「跳过这一次」「撤销完成」都在这。
//! 两端共用本模块，所以同一个周期任务在手机和电脑上滚出来的日期必然一致。
//!
//! 子任务和父任务存在同一张 `tasks` 表里，靠 `parent_id` 挂上去。
//! 主列表、待办计数、统计都只看顶层 —— 那里算的是「一件工作」，不是「一步」。

use std::collections::HashMap;

use chrono::{DateTime, Local, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};

use worklog_core::schedule;
use worklog_core::Rule;

use crate::model::{fetch_task, row_to_task, Completion, Task};
use crate::time::{now_local, parse_dt};
use crate::{e2s, R};

/* ==================== 工作项 ==================== */

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

pub fn list_tasks(conn: &Connection) -> R<Vec<Task>> {
    top_level_tasks(&conn)
}

/// 一次把全部子任务取回来，前端按 parentId 分组。
/// 阶段性工作数量不多，分次请求反而更慢也更啰嗦。
pub fn list_subtasks(conn: &Connection) -> R<Vec<Task>> {
    subtask_rows(&conn)
}

pub fn create_task(conn: &Connection, mut task: Task) -> R<Task> {
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

pub fn update_task(conn: &Connection, mut task: Task) -> R<Task> {
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
pub fn set_task_status(conn: &Connection, id: i64, status: String) -> R<Task> {
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
pub fn skip_occurrence(conn: &Connection, id: i64) -> R<Task> {
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

/* ==================== 完成记录 ==================== */

pub fn list_completions(conn: &Connection, limit: Option<i64>) -> R<Vec<Completion>> {
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
pub fn undo_completion(conn: &Connection, id: i64) -> R<()> {
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

pub fn delete_task(conn: &Connection, id: i64) -> R<()> {
    conn.execute("DELETE FROM tasks WHERE id = ?1", params![id])
        .map_err(e2s)?;
    Ok(())
}

/* ==================== 子任务 ==================== */

/// 拆解层数上限。顶层工作算第 1 层，往下最多再拆 2 层。
///
/// 为什么封顶：工作记录的场景里「这件事分几步、每步再分几步」就到头了。
/// 再深对使用者没有实际意义，却会让缩进、折叠、进度统计一起复杂化——
/// 深度没有上界的话，递归渲染也少了一道最关键的护栏。
pub(crate) const MAX_DEPTH: i64 = 3;

/// 一个节点在第几层。顶层工作（parent_id 为空）返回 1。
/// 顺带兜住环：万一数据被外部工具改出 A→B→A 这种环，也不会把程序挂死。
pub(crate) fn depth_of(conn: &Connection, id: i64) -> R<i64> {
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
pub fn insert_subtask(conn: &Connection, parent_id: i64, title: &str) -> R<Task> {
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

pub fn create_subtask(conn: &Connection, parent_id: i64, title: String) -> R<Task> {
    insert_subtask(&conn, parent_id, &title)
}

/// 改名。子任务在抽屉里是可直接编辑的输入框，改完即存。
pub fn rename_subtask(conn: &Connection, id: i64, title: String) -> R<Task> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("子任务不能没有名字".into());
    }
    conn.execute(
        "UPDATE tasks SET title = ?1 WHERE id = ?2 AND parent_id IS NOT NULL",
        params![title, id],
    )
    .map_err(e2s)?;
    fetch_task(&conn, id)
}

/* ==================== 复制一份 ==================== */

/// 按「父先于子」的顺序展开一棵子树（不含根）。
/// 复制和存模板都要照这个顺序落库，父的 id 映射才能先就位。
pub(crate) fn collect_subtree(conn: &Connection, root: i64, out: &mut Vec<Task>) -> R<()> {
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
pub fn duplicate_task(conn: &Connection, id: i64, base: Option<String>) -> R<Task> {
    duplicate_task_inner(&conn, id, base.as_deref())
}
