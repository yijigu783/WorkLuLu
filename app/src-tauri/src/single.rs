//! 单实例：再点一次 exe，只把已经在跑的窗口请到前台，不再多开一个进程。
//!
//! 不用 `tauri-plugin-single-instance` 的原因：它要引一个新依赖，而这里需要的
//! 几个调用（`CreateMutexW` / `FindWindowW` / `SetForegroundWindow`）本来就在
//! `tao` 依赖的 `windows` crate 里，把特性门打开即可 —— 编译量不涨，交付的 exe 不大。
//!
//! 三个坑，都是这几十行里真正要小心的部分：
//!
//! 1. **判定必须发生在碰任何数据之前。** 数据库是 SQLite 文件，两个进程同时开
//!    同一个文件会互相锁；便携目录的搬迁逻辑也会打架。所以 `claim()` 是 `main`
//!    的第一行，早于建库、早于建窗口。
//! 2. **互斥体名字要带 exe 路径。** 同一个 exe 打开两次才算重复启动；
//!    便携版和装在固定位置的版本本来就是两个独立应用（数据目录都不同），
//!    不该互相顶掉。
//! 3. **老窗口可能正收在屏幕外。** 贴边隐藏开着的时候它会滑出屏幕，
//!    这时 `SetForegroundWindow` 成功了用户也看不见任何东西，
//!    得先把它挪回来（`win32::pull_into_view`）。

use crate::win32;
use std::time::Duration;

/// 等老实例把窗口建出来的时间上限。老实例可能正卡在启动阶段
/// （读库、搬数据），窗口还没注册完，这时候找不到窗口是正常的。
const LOOKUP: Duration = Duration::from_millis(150);
const LOOKUP_TRIES: usize = 12;

/// 本进程是不是「第一个」。
///
/// 返回 `true` 表示该我继续正常启动；返回 `false` 表示已经有实例在跑，
/// 本进程已经把它的窗口请到前台，可以直接退出。
pub fn claim() -> bool {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError};
        use windows::Win32::System::Threading::CreateMutexW;
        use windows::core::PCWSTR;

        let name = wide(&mutex_name(&exe_path()));
        // bInitialOwner = true：创建即持有所有权。句柄故意留着不关——
        // 进程活着，它就得一直占着这个位置。
        let Ok(handle) = (unsafe { CreateMutexW(None, true, PCWSTR(name.as_ptr())) }) else {
            // 拿不到互斥体（被安全软件挡了等）也照样能开：
            // 宁可偶尔多开一个窗口，也不能让程序直接打不开。
            return true;
        };
        // GetLastError 必须紧跟在 CreateMutexW 后面读，
        // 中间插任何一次 Win32 调用都会把上一个错误码冲掉。
        let already_running = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        if already_running {
            unsafe {
                let _ = CloseHandle(handle);
            }
            focus_running();
            return false;
        }
        true
    }
    #[cfg(not(windows))]
    {
        true
    }
}

/// 把已经在跑的那个实例的窗口叫到前台，然后返回。
///
/// 找不到窗口也不当错误：老实例可能正在启动（比我还慢），它自己会把窗口显示出来，
/// 这次点击不算白点。
fn focus_running() {
    for _ in 0..LOOKUP_TRIES {
        if let Some(hwnd) = win32::find_window_by_title(crate::APP_NAME) {
            win32::pull_into_view(hwnd);
            win32::show_and_focus(hwnd);
            return;
        }
        std::thread::sleep(LOOKUP);
    }
}

/// 互斥体名字：`Local\` 前缀 + exe 完整路径的哈希。
///
/// - 用 `Local\` 而不是 `Global\`：会话级就够。同一用户多开远程桌面会话各跑各的，
///   不该互相顶掉。
/// - 带路径哈希：同一份 exe（哪怕是从不同快捷方式、相对路径启动）哈希一样，
///   才会被认成同一个应用；不同副本各跑各的。
fn mutex_name(path: &str) -> String {
    // FNV-1a 64：小、快、散得够开。哈希在这里只用来「对上号」，不是安全边界，
    // 碰撞的后果至多是两个不同的 exe 互相顶掉，不会损坏数据。
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for ch in path.to_lowercase().chars() {
        hash ^= ch as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("Local\\WorkLuLu-{hash:016x}")
}

/// 当前 exe 的完整路径。取不到就退化成空串（所有实例共用一个名字，
/// 即最保守的「单实例」），总比程序打不开强。
fn exe_path() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Rust 字符串 → 以 NUL 结尾的 UTF-16。
#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 同一个 exe 无论从哪个路径形式启动，名字都得一样，否则单实例等于没有
    #[test]
    fn same_path_gives_same_name() {
        let a = mutex_name("C:\\tools\\WorkLuLu.exe");
        let b = mutex_name("C:\\tools\\WorkLuLu.exe");
        assert_eq!(a, b);
    }

    /// Windows 路径大小写不敏感：`C:\Tools` 和 `c:\tools` 是同一个文件
    #[test]
    fn path_case_does_not_change_the_name() {
        assert_eq!(
            mutex_name("C:\\Tools\\WorkLuLu.exe"),
            mutex_name("c:\\tools\\worklulu.exe")
        );
    }

    /// 便携版和安装版是两份数据、两个应用，不该互相顶掉
    #[test]
    fn different_copies_give_different_names() {
        assert_ne!(
            mutex_name("D:\\portable\\WorkLuLu.exe"),
            mutex_name("C:\\Program Files\\WorkLuLu.exe")
        );
    }

    #[test]
    fn name_is_session_scoped_and_prefixed() {
        let name = mutex_name("C:\\tools\\WorkLuLu.exe");
        assert!(name.starts_with("Local\\WorkLuLu-"), "实际是 {name}");
        // 后缀是 16 位十六进制
        assert_eq!(name.len(), "Local\\WorkLuLu-".len() + 16);
        assert!(name["Local\\WorkLuLu-".len()..].chars().all(|c| c.is_ascii_hexdigit()));
    }
}
