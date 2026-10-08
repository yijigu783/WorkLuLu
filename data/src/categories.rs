//! 分类：用户自定义的工作分类（本职工作 / 副业 / 学习提升 / 生活…）。

use rusqlite::{params, Connection};

use crate::model::Category;
use crate::{e2s, R};

pub fn list_categories(conn: &Connection) -> R<Vec<Category>> {
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

pub fn create_category(conn: &Connection, name: String, color: String) -> R<Category> {
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

pub fn update_category(
    conn: &Connection,
    id: i64,
    name: Option<String>,
    color: Option<String>,
    sort: Option<i64>,
) -> R<()> {
    if let Some(v) = name  { conn.execute("UPDATE categories SET name = ?1 WHERE id = ?2", params![v, id]).map_err(e2s)?; }
    if let Some(v) = color { conn.execute("UPDATE categories SET color = ?1 WHERE id = ?2", params![v, id]).map_err(e2s)?; }
    if let Some(v) = sort  { conn.execute("UPDATE categories SET sort = ?1 WHERE id = ?2", params![v, id]).map_err(e2s)?; }
    Ok(())
}

/// 删除分类：其下工作自动回落到「未分类」，不会被一起删掉
pub fn delete_category(conn: &Connection, id: i64) -> R<()> {
    conn.execute("UPDATE tasks SET category_id = NULL WHERE category_id = ?1", params![id])
        .map_err(e2s)?;
    conn.execute("DELETE FROM categories WHERE id = ?1", params![id])
        .map_err(e2s)?;
    Ok(())
}

pub fn reorder_categories(conn: &Connection, ids: Vec<i64>) -> R<()> {
    for (i, id) in ids.iter().enumerate() {
        conn.execute("UPDATE categories SET sort = ?1 WHERE id = ?2", params![i as i64, id])
            .map_err(e2s)?;
    }
    Ok(())
}
