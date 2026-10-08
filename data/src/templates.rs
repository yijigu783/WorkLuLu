//! 模板：把一套反复要用的工作结构（含各层子任务）存下来，下次一键重建。
//!
//! 关键设计：模板里的日期一律存**相对基准日的偏移天数**，绝不存绝对日期。
//! 存一个「10 月 8 日截止」，下个月调用时就已经是过去时了；
//! 存「第 3 天截止」，调用时挑个基准日就能算出真实日期。

use std::collections::HashMap;

use chrono::{Local, NaiveDate, TimeZone, Timelike};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use worklog_core::schedule;
use worklog_core::Rule;

use crate::model::fetch_task;
use crate::tasks::collect_subtree;
use crate::time::{now_local, parse_dt};
use crate::{e2s, R};

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
pub fn template_from_task(conn: &Connection, task_id: i64, name: &str) -> R<Template> {
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
pub fn all_templates(conn: &Connection) -> R<Vec<Template>> {
    let ids = {
        let mut stmt = conn
            .prepare("SELECT id FROM templates ORDER BY sort, id")
            .map_err(e2s)?;
        let rows = stmt.query_map([], |r| r.get::<_, i64>(0)).map_err(e2s)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(e2s)?
    };
    ids.into_iter().map(|id| read_template(conn, id)).collect()
}

pub fn list_templates(conn: &Connection) -> R<Vec<Template>> {
    all_templates(&conn)
}

pub fn save_template(conn: &Connection, task_id: i64, name: String) -> R<Template> {
    template_from_task(&conn, task_id, &name)
}

pub fn apply_template(conn: &Connection, id: i64, base: String) -> R<i64> {
    apply_template_inner(&conn, id, &base)
}

pub fn delete_template(conn: &Connection, id: i64) -> R<()> {
    conn.execute("DELETE FROM templates WHERE id = ?1", params![id])
        .map_err(e2s)?;
    Ok(())
}
