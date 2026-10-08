//! 建表、迁移与种子数据。
//!
//! 这里是**两端唯一的表结构定义**。桌面版和安卓版都靠它建库 ——
//! 各写一份的话，同一个 `.db` 在两个端上会被建成不同的样子，
//! 而且不报错，等到用户发现数据不对时已经晚了。
//!
//! 加表之前先看两条约定：
//! 1. 新增表要同时更新 `OPTIONAL_TABLES`（各端命令层里）和备份恢复的删/插顺序，
//!    让老备份能继续恢复 —— 否则用户会平白丢掉全部数据。
//! 2. `CREATE TABLE IF NOT EXISTS` 补不了「已有表上新增的列」，
//!    要加列必须同时改 `migrate()`。

use rusqlite::{Connection, OptionalExtension};
use std::path::Path;

pub const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS categories (
  id    INTEGER PRIMARY KEY AUTOINCREMENT,
  name  TEXT    NOT NULL,
  color TEXT    NOT NULL DEFAULT '#4F5BE8',
  sort  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS tasks (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  title        TEXT    NOT NULL,
  note         TEXT    NOT NULL DEFAULT '',
  category_id  INTEGER REFERENCES categories(id) ON DELETE SET NULL,
  pattern      TEXT    NOT NULL DEFAULT 'once',
  status       TEXT    NOT NULL DEFAULT 'todo',
  due_at       TEXT,
  end_at       TEXT,
  remind_at    TEXT,
  rule         TEXT,
  progress     TEXT,
  parent_id    INTEGER REFERENCES tasks(id) ON DELETE CASCADE,
  sort         INTEGER NOT NULL DEFAULT 0,
  created_at   TEXT,
  completed_at TEXT,
  notified_at  TEXT
);

CREATE INDEX IF NOT EXISTS idx_tasks_status   ON tasks(status);
CREATE INDEX IF NOT EXISTS idx_tasks_category ON tasks(category_id);
CREATE INDEX IF NOT EXISTS idx_tasks_due      ON tasks(due_at);

-- 周期任务每次完成都留一条痕。标题/分类是刻意的冗余：
-- task_id 用 SET NULL，删掉周期任务后历史记录依然能显示「哪一周干了什么」
CREATE TABLE IF NOT EXISTS completions (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  task_id     INTEGER REFERENCES tasks(id) ON DELETE SET NULL,
  title       TEXT    NOT NULL,
  category_id INTEGER,
  pattern     TEXT    NOT NULL DEFAULT 'recurring',
  due_at      TEXT,
  done_at     TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_completions_done ON completions(done_at);

CREATE TABLE IF NOT EXISTS settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

-- 模板：把一套反复要用的工作结构（含各层子任务）存下来，下次一键重建。
--
-- 关键设计：日期一律存「相对基准日的偏移天数」，绝不存绝对日期。
-- 存一个「10 月 8 日截止」，下个月调用时就已经是过去时了；
-- 存「第 3 天截止」，调用时挑个基准日（默认今天）就能算出真实日期。
CREATE TABLE IF NOT EXISTS templates (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  name       TEXT    NOT NULL,
  note       TEXT    NOT NULL DEFAULT '',
  sort       INTEGER NOT NULL DEFAULT 0,
  created_at TEXT
);

-- parent_id 自引用 → 模板天然支持多层结构，和 tasks 表一个路子
CREATE TABLE IF NOT EXISTS template_items (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  template_id INTEGER NOT NULL REFERENCES templates(id) ON DELETE CASCADE,
  parent_id   INTEGER REFERENCES template_items(id) ON DELETE CASCADE,
  title       TEXT    NOT NULL,
  note        TEXT    NOT NULL DEFAULT '',
  category_id INTEGER,
  pattern     TEXT    NOT NULL DEFAULT 'once',
  rule        TEXT,
  due_offset  INTEGER,
  end_offset  INTEGER,
  -- 时分单独记：偏移只管「第几天」，几点几分得另存，
  -- 否则「每天 09:30 的晨会」还原出来会变成默认时间
  due_time    TEXT,
  end_time    TEXT,
  sort        INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_tpl_items_tpl ON template_items(template_id);

-- 附件：贴在一条工作上的图片或文件，用来留痕（把聊天截图直接粘进来）。
--
-- 本体直接存进库而不是落成数据目录里的文件，是为了保住「备份 = 一个 .db 拷走就是全部」
-- 这条语义：VACUUM INTO 会把图片一起带走，恢复、便携模式全都不用改。
-- 落成文件的话，备份就不再完整，用户换台机器恢复才发现图没了。
--
-- task_id 用 CASCADE：附件是这条工作的证据，工作删了证据跟着走。
-- （completions 用的是 SET NULL，那是「历史留痕」，性质不同。）
CREATE TABLE IF NOT EXISTS attachments (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  task_id    INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  name       TEXT    NOT NULL DEFAULT '',
  mime       TEXT    NOT NULL DEFAULT '',
  size       INTEGER NOT NULL DEFAULT 0,
  -- image / file。图片在界面里直接看，文件用系统默认程序打开
  kind       TEXT    NOT NULL DEFAULT 'file',
  data       BLOB    NOT NULL,
  -- 列表里只加载这一份小图，不把原图整块搬过来；非图片为空
  thumb      BLOB,
  created_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_attachments_task ON attachments(task_id);
"#;

/// 在指定路径开库，并把表结构、迁移、种子数据都准备好。
///
/// **路径由调用方给** —— 数据放哪儿是各端的事：桌面端有便携模式
/// （exe 旁的 `data`）和标准模式（`%APPDATA%\工作记录本`），
/// 安卓端是 app 私有目录。数据层不猜。
pub fn open_at(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(SCHEMA)?;
    migrate(&conn)?;
    seed(&conn)?;
    Ok(conn)
}

/// 内存库，只给单测用。省得每个用例都去建临时文件。
#[cfg(test)]
pub fn open_in_memory() -> rusqlite::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

/// 老版本数据库的增量迁移。
/// `CREATE TABLE IF NOT EXISTS` 只管建表，补不了「已有表上新增的列」，得单独判断。
fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(tasks)")?;
    let cols: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    if !cols.iter().any(|c| c == "notified_at") {
        conn.execute("ALTER TABLE tasks ADD COLUMN notified_at TEXT", [])?;
    }
    Ok(())
}

/// 读取布尔型设置项。没写过这个键时默认开启——首次打开就该有提醒和托盘。
/// 与前端 `settingOn()` 的判定保持一致。
pub fn setting_on(conn: &Connection, key: &str) -> bool {
    let v: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
        .optional()
        .unwrap_or(None);
    match v.as_deref() {
        None | Some("") => true,
        Some(s) => matches!(s, "1" | "true" | "on"),
    }
}

/// 首次运行写入默认分类，之后用户可自由增删改
fn seed(conn: &Connection) -> rusqlite::Result<()> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0))?;
    if n == 0 {
        let defaults = [
            ("本职工作", "#4F5BE8", 0i64),
            ("副业", "#F59E0B", 1),
            ("学习提升", "#10B981", 2),
            ("生活", "#EC4899", 3),
        ];
        for (name, color, sort) in defaults {
            conn.execute(
                "INSERT INTO categories (name, color, sort) VALUES (?1, ?2, ?3)",
                rusqlite::params![name, color, sort],
            )?;
        }
    }
    Ok(())
}
