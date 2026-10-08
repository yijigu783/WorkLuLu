//! 附件：贴在一条工作上的图片或文件，用来留痕（把聊天截图直接粘进来）。

use base64::{engine::general_purpose::STANDARD, Engine as _};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::time::now_local;
use crate::{e2s, R};

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
pub(crate) const MAX_ATTACHMENT_BYTES: usize = 20 * 1024 * 1024;

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

pub fn list_attachments(conn: &Connection, task_id: Option<i64>) -> R<Vec<Attachment>> {
    list_attachments_of(&conn, task_id)
}

/// 存一个附件。`data_b64` / `thumb_b64` 都是**不带前缀**的 base64
/// （前端已经剥掉 `data:...;base64,`）。
///
/// 前端先把图片压过一道再送过来，所以这里只做体积与合法性检查，不做二次压缩 ——
/// 后端再编一遍图会让粘贴有明显停顿。
pub fn insert_attachment(
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

pub fn add_attachment(
    conn: &Connection,
    task_id: i64,
    name: String,
    mime: String,
    kind: String,
    data: String,
    thumb: Option<String>,
) -> R<Attachment> {
    insert_attachment(&conn, task_id, &name, &mime, &kind, &data, thumb.as_deref())
}

/// 取附件内容，返回 data URL。
///
/// `thumb = true` 时优先给缩略图：列表里一格一格的小图不该把原图整块拉过来，
/// 几十张截图一起加载会让抽屉卡住。没有缩略图（或不是图片）就退回原图。
pub fn attachment_data_url(conn: &Connection, id: i64, thumb: bool) -> R<String> {
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

pub fn get_attachment(conn: &Connection, id: i64, thumb: Option<bool>) -> R<String> {
    attachment_data_url(&conn, id, thumb.unwrap_or(false))
}

/// 取附件的**原始字节**（文件名 + 内容）。
///
/// 「用系统程序打开附件」两边都要：桌面端写到临时目录再 `open_path`，
/// 安卓端得写进用户选中的 `content://` 目标。两边要的都是原始字节，
/// 而 `attachment_data_url` 交出来的是 base64 包了一层的字符串 ——
/// 拿来再解一遍纯属绕路，所以单开一个入口。
pub fn attachment_raw(conn: &Connection, id: i64) -> R<(String, Vec<u8>)> {
    conn.query_row(
        "SELECT name, data FROM attachments WHERE id = ?1",
        params![id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .map_err(|_| "这个附件已经不在了".to_string())
}

pub(crate) fn remove_attachment(conn: &Connection, id: i64) -> R<()> {
    conn.execute("DELETE FROM attachments WHERE id = ?1", params![id])
        .map_err(e2s)?;
    Ok(())
}

pub fn delete_attachment(conn: &Connection, id: i64) -> R<()> {
    remove_attachment(&conn, id)
}
