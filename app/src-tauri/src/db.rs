use rusqlite::Connection;
use std::path::{Path, PathBuf};
use tauri::Manager;

/// 数据目录用中文名：用户打开资源管理器一眼能认出来是什么软件的数据
const APP_FOLDER: &str = "工作记录本";
/// 早期版本用 identifier 当目录名，首次运行自动搬过来
const LEGACY_FOLDER: &str = "com.local.worklog";
pub const DB_FILE: &str = "worklog.db";
/// SQLite 的预写日志与共享内存文件，搬家时不能落下
pub const DB_SIDECARS: [&str; 3] = [DB_FILE, "worklog.db-wal", "worklog.db-shm"];

/* ---------------- 便携模式 ----------------
   来自使用反馈：数据位置能不能自己定，想做成便携版，整个文件夹拷到 U 盘就能带走。

   ⚠️ 触发必须是**显式的**，绝不能做成「检测到 exe 目录可写就自动便携」：
   老用户的数据在 AppData，某天从别人手里拿到一个 exe 丢到 D 盘双击 —— 自动便携会让
   它打开一个空库，界面上一条记录都没有。用户第一反应是「数据被这软件弄丢了」，
   而且完全看不出原因。

   两种触发都认：
     · exe 同级有 `portable.txt` —— 设置页那个按钮建的就是它
     · exe 同级有名为 `data` 的目录 —— 手建一个文件夹更直观
   数据落点固定是 `<exe目录>\data\`。 */
pub const PORTABLE_MARKER: &str = "portable.txt";
pub const PORTABLE_SUBDIR: &str = "data";

/// 设置项的读取来自共享数据层 —— 桌面端和安卓端**同一份**判定规则。
/// 这里只转发名字，让 `db::setting_on` 这些老调用点一行都不用改。
/// （`SCHEMA` 不转发：它只在测试里用得到，直接写 `worklog_data::schema::SCHEMA` 更清楚。）
pub use worklog_data::schema::setting_on;

/// exe 所在目录。拿不到（极罕见）返回 None，调用方退回 AppData。
pub fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
}

/// 便携模式判定。纯函数，便于单测。
///
/// 叫 `data` 的**文件**不算 —— 否则一个无关的同名文件就能把数据位置换掉。
pub fn is_portable(exe_dir: &Path) -> bool {
    exe_dir.join(PORTABLE_MARKER).is_file() || exe_dir.join(PORTABLE_SUBDIR).is_dir()
}

/// 便携模式下数据放哪儿
pub fn portable_data_dir(exe_dir: &Path) -> PathBuf {
    exe_dir.join(PORTABLE_SUBDIR)
}

/// 纯决策：用便携目录还是标准目录。
/// 抽成纯函数是为了能单测 —— `AppHandle` 在测试里造不出来。
fn pick_data_dir(standard: &Path, exe_dir: Option<&Path>) -> (PathBuf, bool) {
    if let Some(exe) = exe_dir {
        if is_portable(exe) {
            return (exe.join(PORTABLE_SUBDIR), true);
        }
    }
    (standard.to_path_buf(), false)
}

/// 标准（非便携）数据目录：%APPDATA%\工作记录本
/// 不放在 exe 同级，所以 exe 随便挪位置、覆盖升级，数据都不会丢。
pub fn standard_data_dir(app: &tauri::AppHandle) -> PathBuf {
    match app.path().data_dir() {
        Ok(base) => base.join(APP_FOLDER),
        // 拿不到 Roaming 就退回 Tauri 的默认位置，至少保证能跑
        Err(_) => app
            .path()
            .app_data_dir()
            .unwrap_or_else(|_| PathBuf::from(".")),
    }
}

/// 数据目录。便携模式开着就用 exe 旁边的 `data`，否则用 `%APPDATA%\工作记录本`。
pub fn data_dir(app: &tauri::AppHandle) -> PathBuf {
    let (dir, portable) = pick_data_dir(&standard_data_dir(app), exe_dir().as_deref());
    // 老目录搬迁只在标准模式下做 —— 便携目录是新开的，没有历史包袱要迁
    if !portable {
        if let Ok(base) = app.path().data_dir() {
            migrate_legacy_dir(&base, &dir);
        }
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

/* ---------------- 数据搬家 ---------------- */

/// 单个文件的搬移。同盘符下 `rename` 是原子的；跨盘符（AppData 在 C:，程序在 U 盘）
/// `rename` 会直接失败，得退回复制 + 删源。
fn move_file(src: &Path, dst: &Path) -> std::io::Result<()> {
    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    std::fs::copy(src, dst)?;
    std::fs::remove_file(src)
}

/// 把一个目录里的文件搬进另一个目录。同名的不覆盖，搬空了才删源目录。
/// `backups/` 用得上：用户可能已经积了几十份快照，直接 rename 整个目录跨盘符会失败。
fn move_dir_files(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !src.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        if !entry.path().is_file() {
            continue;
        }
        let target = dst.join(entry.file_name());
        if target.exists() {
            continue;
        }
        move_file(&entry.path(), &target)?;
    }
    // 空了才删得掉；还有剩的就留着，不硬删用户的东西
    let _ = std::fs::remove_dir(src);
    Ok(())
}

/// 数据目录整体搬家：数据库本体 + 它的 sidecar + `backups/` 子目录，别的一概不动。
/// 两边都有数据库时直接拒绝 —— 绝不覆盖用户手上任何一份数据。
///
/// 注：连接关掉之后 SQLite 会把 `-wal` 并回主文件并删掉它，所以 sidecar 常常已经不在了。
/// 这里对「文件不存在」是容忍的，不是漏搬。
pub fn move_data(from: &Path, to: &Path) -> Result<(), String> {
    if from == to {
        return Ok(());
    }
    std::fs::create_dir_all(to).map_err(|e| format!("没法在目标位置建目录：{e}"))?;
    if from.join(DB_FILE).exists() && to.join(DB_FILE).exists() {
        return Err("目标位置已经有一份数据库了，不覆盖。先把那份改名或挪走再来切。".into());
    }
    for name in DB_SIDECARS {
        let src = from.join(name);
        if src.exists() {
            move_file(&src, &to.join(name)).map_err(|e| format!("搬 {name} 时出错：{e}"))?;
        }
    }
    move_dir_files(&from.join("backups"), &to.join("backups"))
        .map_err(|e| format!("搬 backups 时出错：{e}"))?;
    Ok(())
}

/// 在目标目录真写一个文件试试，写完删掉。
///
/// 便携模式最常踩的坑是 exe 放在只读位置（Program Files、只读 U 盘、光驱）。
/// 那种情况必须当场把原因说清楚，**不能静默退回 AppData** —— 静默退回的话用户
/// 以为已经便携了，实际数据还在 C 盘，下次只拷 U 盘就懵了。
pub fn ensure_writable(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("没法在「{}」建目录，可能是只读位置：{e}", dir.display()))?;
    probe_writable(dir)
}

/// 只测「这个已存在的目录能不能写」，**不创建任何目录**。
///
/// 预检查必须用它：拿 `ensure_writable` 去试 `<exe>\data`，会把 data 目录真的建出来，
/// 而 data 目录本身就是便携模式的触发器 —— 结果就是「只是问了一下能不能写，
/// 下次启动数据就被搬走了」。
pub fn probe_writable(dir: &Path) -> Result<(), String> {
    if !dir.is_dir() {
        return Err(format!("「{}」不是个目录", dir.display()));
    }
    let probe = dir.join(".wl-write-probe");
    std::fs::write(&probe, b"1")
        .map_err(|e| format!("「{}」写不进去，可能是只读位置：{e}", dir.display()))?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

/// 启动时：便携模式开着、exe 旁边却还没有库，而 AppData 里有 —— 说明用户刚把标记放上去
/// （或者刚手建了 `data` 目录），把数据搬过去。
///
/// 必须在 `init()` 之前调用：那时候数据库文件还没被任何连接占着，搬得动。
/// 返回一句给用户看的话；没搬动就返回 None。
pub fn migrate_into_portable(app: &tauri::AppHandle) -> Option<String> {
    let exe = exe_dir()?;
    if !is_portable(&exe) {
        return None;
    }
    let target = portable_data_dir(&exe);
    if target.join(DB_FILE).exists() {
        return None;
    }
    let src = standard_data_dir(app);
    if !src.join(DB_FILE).exists() {
        return None;
    }
    match move_data(&src, &target) {
        Ok(()) => Some(format!(
            "已切到便携模式，数据搬到了程序旁的 {} 文件夹，整个文件夹拷走就能带走。",
            PORTABLE_SUBDIR
        )),
        Err(e) => Some(format!("想切便携模式，但数据没搬成：{e}")),
    }
}

/// 打开数据库并把表结构、迁移、种子数据都准备好。
///
/// 失败时**由调用方**（`main.rs` 的 setup）负责把原因讲给用户听 ——
/// 交付的是 GUI 子系统，这里只能拿到 `rusqlite::Error`，
/// 不带上数据目录的路径，用户报上来也没法查。
pub fn init(app: &tauri::AppHandle) -> rusqlite::Result<Connection> {
    worklog_data::schema::open_at(&data_dir(app).join(DB_FILE))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每个用例一个独立目录。带上进程号，cargo test 并行跑的时候不打架。
    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("wl-db-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn empty_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("wl-db-{name}-{}-target", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /* ---- 便携模式的判定 ---- */

    #[test]
    fn a_plain_folder_is_not_portable() {
        assert!(!is_portable(&tmp("plain")));
    }

    #[test]
    fn the_marker_file_turns_portable_on() {
        let dir = tmp("marker");
        std::fs::write(dir.join(PORTABLE_MARKER), b"").unwrap();
        assert!(is_portable(&dir));
    }

    #[test]
    fn a_data_folder_also_turns_portable_on() {
        let dir = tmp("datafolder");
        std::fs::create_dir_all(dir.join(PORTABLE_SUBDIR)).unwrap();
        assert!(is_portable(&dir));
    }

    #[test]
    fn a_file_named_data_is_not_enough() {
        // 叫 data 的**文件**不算。不然一个无关的同名文件就能把数据位置换掉，
        // 那就等于把「自动检测」的坑从后门放了进来
        let dir = tmp("datafile");
        std::fs::write(dir.join(PORTABLE_SUBDIR), b"x").unwrap();
        assert!(!is_portable(&dir));
    }

    /* ---- 目录选择 ---- */

    #[test]
    fn pick_prefers_the_folder_next_to_the_exe() {
        let standard = PathBuf::from(r"C:\Users\x\AppData\Roaming\工作记录本");
        let exe = tmp("pick-portable");
        std::fs::write(exe.join(PORTABLE_MARKER), b"").unwrap();
        let (dir, portable) = pick_data_dir(&standard, Some(&exe));
        assert!(portable);
        assert_eq!(dir, exe.join(PORTABLE_SUBDIR));
    }

    #[test]
    fn pick_falls_back_to_appdata_without_a_marker() {
        let standard = PathBuf::from(r"C:\Users\x\AppData\Roaming\工作记录本");
        let (dir, portable) = pick_data_dir(&standard, Some(&tmp("pick-standard")));
        assert!(!portable);
        assert_eq!(dir, standard);
    }

    #[test]
    fn pick_survives_an_unknown_exe_dir() {
        // current_exe 万一拿不到，也得能跑起来 —— 宁可回到 AppData，别崩
        let standard = PathBuf::from(r"C:\Users\x\AppData\Roaming\工作记录本");
        let (dir, portable) = pick_data_dir(&standard, None);
        assert!(!portable);
        assert_eq!(dir, standard);
    }

    /* ---- 数据搬家 ---- */

    #[test]
    fn move_carries_the_database_sidecars_and_backups() {
        let from = tmp("move-from");
        let to = empty_dir("move-to");
        std::fs::write(from.join(DB_FILE), b"db").unwrap();
        std::fs::write(from.join("worklog.db-wal"), b"wal").unwrap();
        std::fs::create_dir_all(from.join("backups")).unwrap();
        std::fs::write(from.join("backups").join("a.db"), b"bak").unwrap();

        move_data(&from, &to).unwrap();

        assert_eq!(std::fs::read(to.join(DB_FILE)).unwrap(), b"db");
        assert_eq!(std::fs::read(to.join("worklog.db-wal")).unwrap(), b"wal");
        assert_eq!(
            std::fs::read(to.join("backups").join("a.db")).unwrap(),
            b"bak"
        );
        // 搬完源目录里不该还留着库
        assert!(!from.join(DB_FILE).exists());
    }

    #[test]
    fn move_refuses_to_overwrite_an_existing_database() {
        let from = tmp("clash-from");
        let to = tmp("clash-to");
        std::fs::write(from.join(DB_FILE), b"new").unwrap();
        std::fs::write(to.join(DB_FILE), b"old").unwrap();

        assert!(move_data(&from, &to).is_err());
        // 拒绝之后两边都完好，谁也没被动过
        assert_eq!(std::fs::read(to.join(DB_FILE)).unwrap(), b"old");
        assert_eq!(std::fs::read(from.join(DB_FILE)).unwrap(), b"new");
    }

    #[test]
    fn move_keeps_an_existing_backup_of_the_same_name() {
        // 两边的 backups 里都有同名文件时，以目标那份为准，不覆盖
        let from = tmp("bk-from");
        let to = tmp("bk-to");
        std::fs::create_dir_all(from.join("backups")).unwrap();
        std::fs::create_dir_all(to.join("backups")).unwrap();
        std::fs::write(from.join("backups").join("same.db"), b"new").unwrap();
        std::fs::write(to.join("backups").join("same.db"), b"old").unwrap();

        move_data(&from, &to).unwrap();

        assert_eq!(
            std::fs::read(to.join("backups").join("same.db")).unwrap(),
            b"old"
        );
    }

    #[test]
    fn move_is_a_no_op_when_both_sides_are_the_same() {
        let dir = tmp("same");
        std::fs::write(dir.join(DB_FILE), b"db").unwrap();
        move_data(&dir, &dir).unwrap();
        assert!(dir.join(DB_FILE).exists());
    }

    /* ---- 可写探测 ---- */

    #[test]
    fn probe_never_creates_the_directory_it_is_asked_about() {
        // 这条是便携模式的关键护栏：预检查绝不能把 <exe>\data 建出来，
        // 因为 data 目录本身就是触发器 —— 建出来 = 下次启动数据就被搬走了
        let exe = tmp("probe");
        let target = exe.join(PORTABLE_SUBDIR);
        assert!(probe_writable(&target).is_err());   // 还不存在，报错
        assert!(!target.exists());                   // 而且没被顺手建出来
        assert!(!is_portable(&exe));                 // 因此也没变成便携模式
    }

    #[test]
    fn probe_passes_on_a_writable_directory() {
        assert!(probe_writable(&tmp("probe-ok")).is_ok());
    }

    #[test]
    fn ensure_writable_does_create_the_directory() {
        // 真要切过去的时候才建目录，这一点和 probe 的分工必须清楚
        let dir = tmp("ensure").join(PORTABLE_SUBDIR);
        ensure_writable(&dir).unwrap();
        assert!(dir.is_dir());
    }
}
