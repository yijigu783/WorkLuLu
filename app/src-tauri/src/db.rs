use rusqlite::{Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use tauri::Manager;

/// 数据目录用中文名：用户打开资源管理器一眼能认出来是什么软件的数据
const APP_FOLDER: &str = "工作记录本";
/// 早期版本用 identifier 当目录名，首次运行自动搬过来
const LEGACY_FOLDER: &str = "com.local.worklog";
const DB_FILE: &str = "worklog.db";
/// SQLite 的预写日志与共享内存文件，搬家时不能落下
const DB_SIDECARS: [&str; 3] = [DB_FILE, "worklog.db-wal", "worklog.db-shm"];

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
"#;

/// 数据目录：%APPDATA%\工作记录本
/// 不放在 exe 同级，所以 exe 随便挪位置、覆盖升级，数据都不会丢。
pub fn data_dir(app: &tauri::AppHandle) -> PathBuf {
    let roaming = app.path().data_dir().ok();
    let dir = match &roaming {
        Some(base) => base.join(APP_FOLDER),
        // 拿不到 Roaming 就退回 Tauri 的默认位置，至少保证能跑
        None => app
            .path()
            .app_data_dir()
            .unwrap_or_else(|_| PathBuf::from(".")),
    };
    if let Some(base) = &roaming {
        migrate_legacy_dir(base, &dir);
    }
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// 把旧目录里的数据库整体搬到新目录。
/// 只在「新目录还没有数据库」时动手，避免覆盖现有数据。
fn migrate_legacy_dir(roaming: &Path, target: &Path) {
    let old = roaming.join(LEGACY_FOLDER);
    if !old.is_dir() || old == target || target.join(DB_FILE).exists() {
        return;
    }
    if std::fs::create_dir_all(target).is_err() {
        return;
    }
    for name in DB_SIDECARS {
        let src = old.join(name);
        if src.exists() {
            // 同盘符下 rename 是原子操作；失败就算了，旧目录还在，不会丢数据
            let _ = std::fs::rename(&src, target.join(name));
        }
    }
}

pub fn init(app: &tauri::AppHandle) -> rusqlite::Result<Connection> {
    let conn = Connection::open(data_dir(app).join(DB_FILE))?;
    conn.execute_batch(SCHEMA)?;
    migrate(&conn)?;
    seed(&conn)?;
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
