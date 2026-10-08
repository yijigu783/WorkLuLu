use crate::AppState;
use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::State;

// 排期引擎与时间工具都从共享层进来 —— 桌面端和安卓端用的是同一份实现。
// 时钟工具放在数据层而不是各端各写一个：备份文件名、快照文件名都靠它，
// 两端的命名规则必须一致，否则「恢复前-xxx.db」这类文件会各叫各的。
use worklog_core::schedule;
use worklog_data::time::{now_local, parse_dt};

/* ---------------- 数据模型 ----------------
   全部来自共享数据层 `worklog-data`。这里只做重导出，让 `commands::Task`
   这类老路径继续可用 —— 和当初把 `Rule` 搬进 core 时用的是同一个手法，
   调用方（main.rs、各端前端契约检查）一行都不用改。
   `Progress` / `TemplateItem` 只作为 `Task` / `Template` 的字段类型存在，
   本 crate 里没有一处按名字提到它们，所以不重导出（重导出了也会被判 unused）。 */
pub use worklog_data::{Attachment, Category, Completion, Rule, Task, Template};

/// 命令层的统一错误类型：错误信息直接给用户看，所以用 String。
/// 与 `worklog_data::R` 同形状，数据层的返回值可以直接 `?` 上来。
type R<T> = Result<T, String>;

fn e2s<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/* ================================================================
   以下全部是薄包装：取锁 → 交给 `worklog_data`。
   **逻辑一行都不在这里** —— 桌面版和安卓版调的是同一份实现，
   否则同一个周期任务在两端会滚出不同的日期，而且极难定位。
   ================================================================ */

/* ---------------- 分类 ---------------- */

#[tauri::command]
pub fn list_categories(state: State<'_, AppState>) -> R<Vec<Category>> {
    let conn = state.db.lock().map_err(e2s)?;
    worklog_data::categories::list_categories(&conn)
}

#[tauri::command]
pub fn create_category(state: State<'_, AppState>, name: String, color: String) -> R<Category> {
    let conn = state.db.lock().map_err(e2s)?;
    worklog_data::categories::create_category(&conn, name, color)
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
    worklog_data::categories::update_category(&conn, id, name, color, sort)
}

#[tauri::command]
pub fn delete_category(state: State<'_, AppState>, id: i64) -> R<()> {
    let conn = state.db.lock().map_err(e2s)?;
    worklog_data::categories::delete_category(&conn, id)
}

#[tauri::command]
pub fn reorder_categories(state: State<'_, AppState>, ids: Vec<i64>) -> R<()> {
    let conn = state.db.lock().map_err(e2s)?;
    worklog_data::categories::reorder_categories(&conn, ids)
}

/* ---------------- 工作项 ---------------- */

#[tauri::command]
pub fn list_tasks(state: State<'_, AppState>) -> R<Vec<Task>> {
    let conn = state.db.lock().map_err(e2s)?;
    worklog_data::tasks::list_tasks(&conn)
}

#[tauri::command]
pub fn list_subtasks(state: State<'_, AppState>) -> R<Vec<Task>> {
    let conn = state.db.lock().map_err(e2s)?;
    worklog_data::tasks::list_subtasks(&conn)
}

#[tauri::command]
pub fn create_task(state: State<'_, AppState>, task: Task) -> R<Task> {
    let conn = state.db.lock().map_err(e2s)?;
    worklog_data::tasks::create_task(&conn, task)
}

#[tauri::command]
pub fn update_task(state: State<'_, AppState>, task: Task) -> R<Task> {
    let conn = state.db.lock().map_err(e2s)?;
    worklog_data::tasks::update_task(&conn, task)
}

#[tauri::command]
pub fn set_task_status(state: State<'_, AppState>, id: i64, status: String) -> R<Task> {
    let conn = state.db.lock().map_err(e2s)?;
    worklog_data::tasks::set_task_status(&conn, id, status)
}

#[tauri::command]
pub fn skip_occurrence(state: State<'_, AppState>, id: i64) -> R<Task> {
    let conn = state.db.lock().map_err(e2s)?;
    worklog_data::tasks::skip_occurrence(&conn, id)
}

#[tauri::command]
pub fn delete_task(state: State<'_, AppState>, id: i64) -> R<()> {
    let conn = state.db.lock().map_err(e2s)?;
    worklog_data::tasks::delete_task(&conn, id)
}

#[tauri::command]
pub fn duplicate_task(state: State<'_, AppState>, id: i64, base: Option<String>) -> R<Task> {
    let conn = state.db.lock().map_err(e2s)?;
    worklog_data::tasks::duplicate_task(&conn, id, base)
}

/* ---------------- 完成记录 ---------------- */

#[tauri::command]
pub fn list_completions(state: State<'_, AppState>, limit: Option<i64>) -> R<Vec<Completion>> {
    let conn = state.db.lock().map_err(e2s)?;
    worklog_data::tasks::list_completions(&conn, limit)
}

#[tauri::command]
pub fn undo_completion(state: State<'_, AppState>, id: i64) -> R<()> {
    let conn = state.db.lock().map_err(e2s)?;
    worklog_data::tasks::undo_completion(&conn, id)
}

/* ---------------- 子任务 ---------------- */

#[tauri::command]
pub fn create_subtask(state: State<'_, AppState>, parent_id: i64, title: String) -> R<Task> {
    let conn = state.db.lock().map_err(e2s)?;
    worklog_data::tasks::create_subtask(&conn, parent_id, title)
}

#[tauri::command]
pub fn rename_subtask(state: State<'_, AppState>, id: i64, title: String) -> R<Task> {
    let conn = state.db.lock().map_err(e2s)?;
    worklog_data::tasks::rename_subtask(&conn, id, title)
}

/* ---------------- 模板 ---------------- */

#[tauri::command]
pub fn list_templates(state: State<'_, AppState>) -> R<Vec<Template>> {
    let conn = state.db.lock().map_err(e2s)?;
    worklog_data::templates::list_templates(&conn)
}

#[tauri::command]
pub fn save_template(state: State<'_, AppState>, task_id: i64, name: String) -> R<Template> {
    let conn = state.db.lock().map_err(e2s)?;
    worklog_data::templates::save_template(&conn, task_id, name)
}

#[tauri::command]
pub fn apply_template(state: State<'_, AppState>, id: i64, base: String) -> R<i64> {
    let conn = state.db.lock().map_err(e2s)?;
    worklog_data::templates::apply_template(&conn, id, base)
}

#[tauri::command]
pub fn delete_template(state: State<'_, AppState>, id: i64) -> R<()> {
    let conn = state.db.lock().map_err(e2s)?;
    worklog_data::templates::delete_template(&conn, id)
}

/* ---------------- 设置 ---------------- */

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> R<std::collections::HashMap<String, String>> {
    let conn = state.db.lock().map_err(e2s)?;
    worklog_data::settings::get_settings(&conn)
}

/// 写设置项。落库交给共享层，**开机自启留在本地** ——
/// 那要真去写系统的启动项，是 Windows 专属行为，安卓上没有对应物。
#[tauri::command]
pub fn set_setting(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> R<()> {
    // 锁的生存期必须收在块里：后面 `autolaunch()` 可能阻塞，
    // 攥着数据库锁去等系统调用，会把整个界面卡住。
    let on = {
        let mut conn = state.db.lock().map_err(e2s)?;
        worklog_data::settings::set_setting(&mut conn, &key, &value)?
    };

    // 开机自启是要真切生效的系统行为，不能只写个开关就当设过了
    if key == "autostart" {
        use tauri_plugin_autostart::ManagerExt;
        let mgr = app.autolaunch();
        let done = if on { mgr.enable() } else { mgr.disable() };
        done.map_err(|e| format!("写入开机自启失败：{e}"))?;
    }
    Ok(())
}

/* ---------------- 附件 ---------------- */

#[tauri::command]
pub fn list_attachments(state: State<'_, AppState>, task_id: Option<i64>) -> R<Vec<Attachment>> {
    let conn = state.db.lock().map_err(e2s)?;
    worklog_data::attachments::list_attachments(&conn, task_id)
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
    worklog_data::attachments::add_attachment(&conn, task_id, name, mime, kind, data, thumb)
}

#[tauri::command]
pub fn get_attachment(state: State<'_, AppState>, id: i64, thumb: Option<bool>) -> R<String> {
    let conn = state.db.lock().map_err(e2s)?;
    worklog_data::attachments::get_attachment(&conn, id, thumb)
}

#[tauri::command]
pub fn delete_attachment(state: State<'_, AppState>, id: i64) -> R<()> {
    let conn = state.db.lock().map_err(e2s)?;
    worklog_data::attachments::delete_attachment(&conn, id)
}

/* ================================================================
   以下是**桌面专属**的部分，安卓端没有对应物，所以留在本层不往共享层下沉。
   ================================================================ */


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
    let (name, data) = {
        let conn = state.db.lock().map_err(e2s)?;
        worklog_data::attachments::attachment_raw(&conn, id)?
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
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    // 备份/恢复要验「模板和附件有没有一起带走」，所以得先造出这些东西。
    // 这几个辅助函数住在共享数据层里 —— 它们本来就是数据层的内部构件，
    // 这里只是拿来铺测试数据。
    use worklog_data::attachments::{attachment_data_url, insert_attachment};
    use worklog_data::model::fetch_task;
    use worklog_data::tasks::insert_subtask;
    use worklog_data::templates::{all_templates, template_from_task};

    /// 内存库 + 一个分类。tasks.category_id 有外键约束，
    /// 测试里要能真的把工作挂到分类上，否则插不进去。
    fn db() -> Connection {
        let c = Connection::open_in_memory().expect("内存库");
        c.execute_batch(worklog_data::schema::SCHEMA).expect("建表");
        c.execute(
            "INSERT INTO categories (name, color, sort) VALUES ('本职工作', '#4F5BE8', 0)",
            [],
        )
        .expect("建分类");
        c
    }

    /// 造一个阶段性工作（能挂子任务的那种）。
    fn stage(conn: &Connection) -> Task {
        conn.execute(
            "INSERT INTO tasks (title, note, category_id, pattern, status, sort)
             VALUES ('完成官网改版', '', 1, 'stage', 'todo', 0)",
            [],
        )
        .expect("插入阶段性工作");
        fetch_task(conn, conn.last_insert_rowid()).expect("取回")
    }

    /// 把字节编成前端传附件时用的那种 base64 文本。
    fn b64(bytes: &[u8]) -> String {
        STANDARD.encode(bytes)
    }

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
