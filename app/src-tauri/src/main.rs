#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod db;
mod edge;
mod geom;
mod notify;
mod single;
mod snap;
mod win32;

use rusqlite::{params, OptionalExtension};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, Runtime, WindowEvent};
// 排期引擎在共享层（core）。原来的 `mod schedule;` 已删。
use worklog_core::schedule;

/// 对外的软件名：中文主名 + 英文副名。
/// 窗口标题、托盘提示、系统通知的归属名都用它——只用中文，英文用户认不出；
/// 只用英文，中文用户在一堆 exe 里认不出来。
pub const APP_NAME: &str = "工作记录本 WorkLuLu";

/// 巡检间隔。30 秒足够准时，又不会白耗电。
const TICK: Duration = Duration::from_secs(30);
/// 一次最多弹几条，避免积压了半个月的任务开机瞬间糊满屏幕
const MAX_NOTIFY_PER_TICK: usize = 5;
/// 托盘图标在 Tauri 里的注册名
const TRAY_ID: &str = "main";

/// 上一次写进托盘菜单的 (待办数, 提醒开关)。
/// 菜单文案会随这两项变，但重建菜单会把用户正打开的右键菜单关掉，
/// 所以只在真的变了的时候重建。初值 -1 保证第一次必然构建。
static TRAY_SIGNATURE: AtomicI64 = AtomicI64::new(-1);

pub struct AppState {
    pub db: Mutex<rusqlite::Connection>,
    /// 启动时若刚把数据搬进/搬出便携目录，这里留一句话给界面显示一次。
    /// 不用事件是因为 setup 里发事件时前端多半还没开始监听，消息会直接丢掉。
    pub portable_note: Mutex<Option<String>>,
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        // 贴边隐藏开着的时候，窗口可能整个滑在屏幕外：show + 焦点都给了，
        // 用户眼前还是什么都没有。顺手把它请回屏幕内。
        if let Ok(hwnd) = w.as_ref().window().hwnd() {
            win32::pull_into_view(hwnd);
        }
    }
}

/// 托盘菜单上的那个数字：口径要和前端侧栏「今天」一致，否则两处数字对不上，
/// 用户只会觉得哪儿都不准。
///
/// 逐条用 chrono 比而不是交给 SQL 的 `date()`：due_at 存的是带时区偏移的
/// RFC3339，让 SQLite 去解释时区容易和 Rust 侧的理解错开一天。
fn pending_count(conn: &rusqlite::Connection) -> i64 {
    let today = chrono::Local::now().date_naive();
    let Ok(mut stmt) = conn.prepare(
        "SELECT due_at FROM tasks
         WHERE status <> 'done' AND pattern <> 'stage' AND due_at IS NOT NULL",
    ) else {
        return 0;
    };
    let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) else {
        return 0;
    };
    rows.flatten()
        .filter(|iso| {
            chrono::DateTime::parse_from_rfc3339(iso)
                .map(|d| d.with_timezone(&chrono::Local).date_naive() <= today)
                .unwrap_or(false)
        })
        .count() as i64
}

/// 组装托盘右键菜单。待办数和提醒开关状态都直接写在文案里，
/// 用户不用打开窗口就知道现在什么情况。
fn build_tray_menu<R: Runtime>(
    app: &tauri::AppHandle<R>,
    pending: i64,
    remind_on: bool,
) -> tauri::Result<Menu<R>> {
    let open = MenuItem::with_id(app, "show", "打开主窗口", true, None::<&str>)?;
    let today = MenuItem::with_id(
        app,
        "today",
        if pending > 0 {
            format!("今天待办（{pending}）")
        } else {
            "今天待办".to_string()
        },
        true,
        None::<&str>,
    )?;
    let new = MenuItem::with_id(app, "new", "新建工作", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let remind = MenuItem::with_id(
        app,
        "toggle-remind",
        if remind_on { "暂停到期提醒" } else { "恢复到期提醒" },
        true,
        None::<&str>,
    )?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    Menu::with_items(
        app,
        &[&open, &today, &new, &sep1, &remind, &sep2, &quit],
    )
}

/// 按当前数据把菜单和 tooltip 刷成最新的。
/// 文案变了只能重建菜单（Tauri 没给「改菜单项文字」的接口），
/// 但重建会把用户正打开的右键菜单关掉，所以先比对签名，没变就不动。
fn refresh_tray(app: &tauri::AppHandle) {
    let (pending, remind_on) = {
        let state = app.state::<AppState>();
        let Ok(conn) = state.db.lock() else { return };
        (pending_count(&conn), db::setting_on(&conn, "notify"))
    };

    let signature = (pending << 1) | i64::from(remind_on);
    if TRAY_SIGNATURE.swap(signature, Ordering::Relaxed) == signature {
        return;
    }

    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    if let Ok(menu) = build_tray_menu(app, pending, remind_on) {
        let _ = tray.set_menu(Some(menu));
    }
    let tip = if pending > 0 {
        format!("{APP_NAME} · {pending} 项待办")
    } else {
        APP_NAME.to_string()
    };
    let _ = tray.set_tooltip(Some(tip.as_str()));
}

/// 从托盘直接暂停/恢复到期提醒，省得为这么一件事专门开一趟设置页
fn toggle_remind(app: &tauri::AppHandle) {
    {
        let state = app.state::<AppState>();
        let Ok(conn) = state.db.lock() else { return };
        let next = if db::setting_on(&conn, "notify") { "0" } else { "1" };
        let _ = conn.execute(
            "INSERT INTO settings (key, value) VALUES ('notify', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![next],
        );
    }
    refresh_tray(app);
    // 设置页里的那个开关得跟着动，否则同一件事在两处显示相反的状态
    let _ = app.emit("settings://notify", ());
}

/// 自愈：周期任务如果丢了排期（老版本留下的数据、或有人直接改了库），
/// 它会永远不出现在「今天」里且毫无提示——所以巡检时顺手补上。返回是否有改动。
fn heal_schedules(conn: &rusqlite::Connection) -> Result<bool, String> {
    let todo: Vec<(i64, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, rule FROM tasks
                 WHERE pattern = 'recurring' AND status <> 'done'
                   AND due_at IS NULL AND rule IS NOT NULL",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;

        let mut out = Vec::new();
        for row in rows {
            let (id, rule_json) = row.map_err(|e| e.to_string())?;
            let Ok(rule) = serde_json::from_str::<commands::Rule>(&rule_json) else { continue };
            if let Some(next) = schedule::next_from(&rule, chrono::Local::now()) {
                out.push((id, next.to_rfc3339()));
            }
        }
        out
    };

    for (id, iso) in &todo {
        conn.execute(
            "UPDATE tasks SET due_at = ?1 WHERE id = ?2",
            params![iso, id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(!todo.is_empty())
}

/// 捞出「到点了但还没提醒过」的任务
fn collect_due(conn: &rusqlite::Connection, now: &str) -> Result<Vec<i64>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id FROM tasks
             WHERE status <> 'done'
               AND notified_at IS NULL
               AND COALESCE(remind_at, due_at) IS NOT NULL
               AND COALESCE(remind_at, due_at) <= ?1
             ORDER BY COALESCE(remind_at, due_at)
             LIMIT ?2",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![now, MAX_NOTIFY_PER_TICK as i64], |r| r.get::<_, i64>(0))
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| e.to_string())
}

/// 到期巡检：补排期 → 捞出该提醒的任务 → 弹系统通知并记下时刻。
/// 跑在后台线程里，所以窗口收进托盘之后提醒照样准时。
fn tick(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let now = chrono::Local::now().to_rfc3339();

    let (healed, pending) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let healed = heal_schedules(&conn)?;
        // 关掉提醒就不查通知，但自愈照跑——数据一致性不该受开关影响
        let pending = if db::setting_on(&conn, "notify") {
            collect_due(&conn, &now)?
        } else {
            Vec::new()
        };
        (healed, pending)
    };

    if pending.is_empty() {
        if healed {
            let _ = app.emit("tasks://changed", ());
        }
        refresh_tray(app);
        return Ok(());
    }

    for id in &pending {
        let title: Option<String> = {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            conn.query_row("SELECT title FROM tasks WHERE id = ?1", params![id], |r| r.get(0))
                .optional()
                .map_err(|e| e.to_string())?
        };
        let Some(title) = title else { continue };

        // 派回主线程去弹：winrt 的点击回调是 COM 事件，只有在跑消息循环的线程上
        // 注册才会送达。丢在后台线程里，通知照样弹得出来，但用户点了毫无反应。
        let app_for_toast = app.clone();
        let task_id = *id;
        let _ = app.run_on_main_thread(move || {
            notify::show_task_toast(&app_for_toast, task_id, &title);
        });
    }

    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        for id in &pending {
            let _ = conn.execute(
                "UPDATE tasks SET notified_at = ?1 WHERE id = ?2 AND notified_at IS NULL",
                params![now, id],
            );
        }
    }

    // 让界面上的「已逾期」标记和托盘上的待办数跟着变
    let _ = app.emit("tasks://changed", ());
    refresh_tray(app);
    Ok(())
}

fn spawn_reminder_loop(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(TICK);
        if let Err(e) = tick(&app) {
            eprintln!("[reminder] {e}");
        }
    });
}

fn main() {
    // 单实例：已经有实例在跑，就把它请到前台，本次进程直接结束。
    // 必须排在最前面 —— 数据库是 SQLite 文件，两个进程同时开它会互相锁，
    // 便携目录的搬迁也会打架。
    if !single::claim() {
        return;
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_opener::init())
        // 备份 / 恢复要用原生文件对话框。前端直接 invoke `plugin:dialog|save` 等，
        // 不需要 npm 侧的 @tauri-apps/plugin-dialog。
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // 便携模式的搬迁必须在开库之前做 —— 那时候数据库文件还没被连接占着，搬得动。
            // 用户可能是在设置里点的「切为便携模式」，也可能只是手建了个 data 目录。
            let portable_note = db::migrate_into_portable(app.handle());

            // 开库失败不让它 panic。交付的是 GUI 子系统（没有控制台），
            // panic=abort 的 release 里这一下会静默结束进程 ——
            // 用户看到的就是「双击了，什么都没发生」，连报错都没处看。
            let conn = match db::init(app.handle()) {
                Ok(conn) => conn,
                Err(e) => {
                    let dir = db::data_dir(app.handle());
                    win32::error_box(
                        APP_NAME,
                        &format!(
                            "打不开数据文件，程序没法启动。\n\n\
                             位置：{}\n\
                             原因：{e}\n\n\
                             常见原因是这个位置没有写入权限（比如程序被放在 \
                             Program Files、只读的 U 盘或光盘里）。\n\
                             把程序换到桌面或自己建的文件夹再试；也可以检查一下\
                             是不是杀毒软件把数据目录拦住了。",
                            dir.display()
                        ),
                    );
                    std::process::exit(1);
                }
            };
            app.manage(AppState {
                db: Mutex::new(conn),
                portable_note: Mutex::new(portable_note),
            });

            spawn_reminder_loop(app.handle().clone());
            // 贴边隐藏靠后台轮询采样判定（鼠标位置和左键状态没有事件可听）
            edge::spawn_watch(app.handle().clone());

            /* ---- 系统通知的身份 ---- */
            // 绿色 exe 没被安装过，系统不知道「工作记录本」是谁，
            // 通知就会顶着 exe 文件名出来。自己往 HKCU 注册一个显示名。
            notify::ensure_app_id();

            /* ---- 系统托盘 ---- */
            let menu = build_tray_menu(app.handle(), 0, true)?;

            let mut tray = TrayIconBuilder::with_id(TRAY_ID)
                .tooltip(APP_NAME)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, ev| match ev.id().as_ref() {
                    "quit" => app.exit(0),
                    "show" => show_main(app),
                    // 「今天待办」比「打开主窗口」多一步：让前端切到今天那一屏
                    "today" => {
                        show_main(app);
                        let _ = app.emit("tray://open", "today");
                    }
                    "new" => {
                        show_main(app);
                        let _ = app.emit("tray://new-task", ());
                    }
                    "toggle-remind" => toggle_remind(app),
                    _ => {}
                })
                .on_tray_icon_event(|tray, ev| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = ev
                    {
                        // 点图标就是想看看有什么要做的，顺手切到「今天」
                        show_main(tray.app_handle());
                        let _ = tray.app_handle().emit("tray://open", "today");
                    }
                });

            if let Some(icon) = app.default_window_icon().cloned() {
                tray = tray.icon(icon);
            }
            tray.build(app)?;

            // 开局把菜单文案对齐真实数据（首次构建时传的是占位值）
            TRAY_SIGNATURE.store(-1, Ordering::Relaxed);
            refresh_tray(app.handle());

            Ok(())
        })
        /* 关闭窗口 = 收进托盘继续跑，保证到期提醒不丢；
           但用户在设置里关掉了托盘常驻，就按普通的关窗处理 */
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                let keep_running = {
                    let state = window.app_handle().state::<AppState>();
                    // 绑成局部变量：直接写在块尾的话，MutexGuard 会活过 state 的借用期
                    let on = match state.db.lock() {
                        Ok(conn) => db::setting_on(&conn, "tray"),
                        Err(_) => true,
                    };
                    on
                };
                if keep_running {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            // 拖动窗口。吸附挂在这里；贴边隐藏不挂 —— 它的判定要用鼠标的绝对位置
            // 和左键状态，那些没有事件可听，改由 `edge::spawn_watch` 轮询。
            // 吸附本身在贴边隐藏开着时就自我禁用（`snap::enabled`），两者不会打架。
            WindowEvent::Moved(_) => {
                snap::on_moved(window);
            }
            // 从任务栏或 Alt+Tab 回到窗口：要是它正滑在屏幕外，得先滑回来
            WindowEvent::Focused(true) => edge::on_focused(window),
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_categories,
            commands::create_category,
            commands::update_category,
            commands::delete_category,
            commands::reorder_categories,
            commands::list_tasks,
            commands::create_task,
            commands::update_task,
            commands::set_task_status,
            commands::delete_task,
            commands::list_subtasks,
            commands::create_subtask,
            commands::rename_subtask,
            commands::duplicate_task,
            commands::list_templates,
            commands::save_template,
            commands::apply_template,
            commands::delete_template,
            commands::list_attachments,
            commands::add_attachment,
            commands::get_attachment,
            commands::delete_attachment,
            commands::open_attachment,
            commands::skip_occurrence,
            commands::list_completions,
            commands::undo_completion,
            commands::get_settings,
            commands::set_setting,
            commands::data_info,
            commands::set_portable,
            commands::app_version,
            commands::open_data_dir,
            commands::edge_reset,
            commands::backup_to,
            commands::restore_from,
            commands::export_csv,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> rusqlite::Connection {
        let c = rusqlite::Connection::open_in_memory().expect("内存库");
        c.execute_batch(db::SCHEMA).expect("建表");
        c
    }

    #[test]
    fn heal_fills_a_missing_schedule() {
        let c = db();
        c.execute(
            r#"INSERT INTO tasks (title, note, pattern, status, rule, sort)
               VALUES ('提交项目周报', '', 'recurring', 'todo',
                       '{"freq":"weekly","byDay":[5],"time":"17:00"}', 0)"#,
            [],
        )
        .unwrap();

        assert!(heal_schedules(&c).unwrap(), "应补上排期");

        let due: Option<String> =
            c.query_row("SELECT due_at FROM tasks WHERE id = 1", [], |r| r.get(0)).unwrap();
        assert!(due.is_some(), "周期任务不能没有 due_at，否则永远不会出现在「今天」里");
        assert!(!heal_schedules(&c).unwrap(), "已经排过期的不该反复改写");
    }

    #[test]
    fn heal_leaves_existing_schedules_alone() {
        let c = db();
        c.execute(
            r#"INSERT INTO tasks (title, note, pattern, status, due_at, rule, sort)
               VALUES ('周报', '', 'recurring', 'todo', '2026-01-01T17:00:00+08:00',
                       '{"freq":"daily","time":"17:00"}', 0)"#,
            [],
        )
        .unwrap();

        assert!(!heal_schedules(&c).unwrap());
        let due: String =
            c.query_row("SELECT due_at FROM tasks WHERE id = 1", [], |r| r.get(0)).unwrap();
        assert_eq!(due, "2026-01-01T17:00:00+08:00", "已有排期不能被覆盖");
    }

    #[test]
    fn heal_ignores_non_recurring_and_done_tasks() {
        let c = db();
        c.execute(
            r#"INSERT INTO tasks (title, note, pattern, status, rule, sort)
               VALUES ('普通任务', '', 'once', 'todo', '{"freq":"daily"}', 0)"#,
            [],
        )
        .unwrap();
        c.execute(
            r#"INSERT INTO tasks (title, note, pattern, status, rule, sort)
               VALUES ('已完成的周期任务', '', 'recurring', 'done', '{"freq":"daily"}', 0)"#,
            [],
        )
        .unwrap();
        assert!(!heal_schedules(&c).unwrap(), "只该处理进行中的周期任务");
    }

    #[test]
    fn collect_due_picks_only_unnotified_past_due() {
        let c = db();
        // 1 已到期、没提醒过 → 该弹
        c.execute(
            "INSERT INTO tasks (title, note, pattern, status, due_at, sort)
             VALUES ('已到期', '', 'once', 'todo', '2026-01-01T09:00:00+08:00', 0)",
            [],
        )
        .unwrap();
        // 2 提醒过了 → 不重复弹
        c.execute(
            "INSERT INTO tasks (title, note, pattern, status, due_at, notified_at, sort)
             VALUES ('已提醒过', '', 'once', 'todo', '2026-01-01T09:00:00+08:00',
                     '2026-01-01T09:00:01+08:00', 0)",
            [],
        )
        .unwrap();
        // 3 还没到点 → 不弹
        c.execute(
            "INSERT INTO tasks (title, note, pattern, status, due_at, sort)
             VALUES ('还没到点', '', 'once', 'todo', '2099-01-01T09:00:00+08:00', 0)",
            [],
        )
        .unwrap();
        // 4 已经完成 → 不弹
        c.execute(
            "INSERT INTO tasks (title, note, pattern, status, due_at, sort)
             VALUES ('已完成', '', 'once', 'done', '2026-01-01T09:00:00+08:00', 0)",
            [],
        )
        .unwrap();

        let ids = collect_due(&c, "2026-06-01T00:00:00+08:00").unwrap();
        assert_eq!(ids, vec![1]);
    }

    /// 托盘上的待办数得和侧栏「今天」一个口径，不然用户会看到两个不一样的数字
    #[test]
    fn pending_count_only_counts_due_today_or_earlier() {
        use chrono::{Datelike, TimeZone};

        let c = db();
        let today = chrono::Local::now().date_naive();
        let at = |d: chrono::NaiveDate| {
            chrono::Local
                .with_ymd_and_hms(d.year(), d.month(), d.day(), 18, 0, 0)
                .unwrap()
                .to_rfc3339()
        };
        let add = |title: &str, pattern: &str, status: &str, due: Option<String>| {
            c.execute(
                "INSERT INTO tasks (title, note, pattern, status, due_at, sort)
                 VALUES (?1, '', ?2, ?3, ?4, 0)",
                params![title, pattern, status, due],
            )
            .unwrap();
        };

        add("昨天到期", "once", "todo", Some(at(today - chrono::Duration::days(1))));
        add("今天到期", "once", "todo", Some(at(today)));
        add("明天到期", "once", "todo", Some(at(today + chrono::Duration::days(1))));
        // 阶段性工作是跨天的容器，拿「今天到期」框它没有意义
        add("阶段容器", "stage", "todo", Some(at(today)));
        add("已完成", "once", "done", Some(at(today - chrono::Duration::days(3))));
        add("没日期", "once", "todo", None);

        assert_eq!(
            pending_count(&c),
            2,
            "只数今天及以前到期、且非阶段性的未完成工作"
        );
    }

    /// 提醒时间优先于到期时间：设了提醒就按提醒那一刻推
    #[test]
    fn remind_at_takes_precedence_over_due_at() {
        let c = db();
        c.execute(
            "INSERT INTO tasks (title, note, pattern, status, due_at, remind_at, sort)
             VALUES ('提前一小时提醒', '', 'once', 'todo', '2026-06-02T09:00:00+08:00',
                     '2026-06-01T08:00:00+08:00', 0)",
            [],
        )
        .unwrap();
        assert_eq!(collect_due(&c, "2026-06-01T09:00:00+08:00").unwrap(), vec![1]);
    }
}
