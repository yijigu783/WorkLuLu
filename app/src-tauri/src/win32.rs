//! 必须直接问 Windows 才能办的几件事。
//!
//! 集中放在一个文件里有两个理由：一是不想让 Win32 的 `unsafe` 散落到业务逻辑中；
//! 二是这几件事有共同点 —— Tauri / tao 抽象不出来，或者抽象了也不准
//! （哪块屏幕算「工作区」、哪个线程允许抢焦点，只有系统知道）。
//!
//! 非 Windows 平台上一律退化成「不知道 / 不做」，主流程照常往下走：
//! 缺了工作区就退回显示器整块，缺了窗口句柄就什么也不动。
//!
//! 这里用到的 windows API 全部来自 `tao` 本来就依赖的 `windows` crate，
//! 只是把用到的几个特性门在 Cargo.toml 里打开，不引入任何新依赖。

use crate::geom::Rect;

/// 窗口所在显示器的工作区（物理像素）：扣掉任务栏、停靠栏之后，
/// 真正能摆下窗口的那块。
///
/// 为什么不用显示器整块：吸附把窗口摆成半屏时若按整块算，
/// 下半屏会正好压在任务栏上面，鼠标够不着那条窗口边。
#[cfg(windows)]
pub fn work_area_of(hwnd: windows::Win32::Foundation::HWND) -> Option<Rect> {
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    unsafe {
        // DEFAULTTONEAREST：窗口整个跑到屏幕外时，仍然取最近的那块屏幕，
        // 而不是像 DEFAULTTOPRIMARY 那样一律跳回主屏 —— 多屏用户会被闪到另一块屏上。
        let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(mon, &mut info).as_bool() {
            let rc = info.rcWork;
            Some(Rect {
                x: rc.left,
                y: rc.top,
                w: rc.right - rc.left,
                h: rc.bottom - rc.top,
            })
        } else {
            None
        }
    }
}

#[cfg(not(windows))]
pub fn work_area_of<T>(_window: T) -> Option<Rect> {
    None
}

/// 按窗口标题找到已存在的窗口句柄。
///
/// 标题取 `APP_NAME`（窗口标题就是它），同名窗口在实践中只有本程序自己。
#[cfg(windows)]
pub fn find_window_by_title(title: &str) -> Option<windows::Win32::Foundation::HWND> {
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
    unsafe {
        let w = wide(title);
        let hwnd = FindWindowW(windows::core::PCWSTR(w.as_ptr()), None).ok()?;
        // HWND(0) 和「没找到」在 windows-rs 里长得一样，得自己分一下
        if hwnd.0 == std::ptr::null_mut() {
            None
        } else {
            Some(hwnd)
        }
    }
}

#[cfg(not(windows))]
pub fn find_window_by_title(_title: &str) -> Option<()> {
    None
}

/// 把窗口请到前台：先解除最小化，再抢焦点。
///
/// 抢焦点这一步非做不可：第二次点 exe 时新进程直接退出，
/// 若不把老窗口请到前面，用户会觉得「双击没反应」。
#[cfg(windows)]
pub fn show_and_focus(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId, IsIconic, SetForegroundWindow, ShowWindow,
        SW_RESTORE, SW_SHOW,
    };
    // AttachThreadInput 在 windows crate 里归 System::Threading，不在 UI 那边
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    unsafe {
        // 最小化状态下 SetForegroundWindow 只把「最小化的窗口」摆到前面，还是看不见。
        // 要先 ShowWindow 解除最小化。
        let cmd = if IsIconic(hwnd).as_bool() { SW_RESTORE } else { SW_SHOW };
        let _ = ShowWindow(hwnd, cmd);

        // 系统有个「前台锁定」：不允许后台进程抢焦点，SetForegroundWindow 会被直接拒绝。
        // 老办法是把自己的输入队列临时挂到当前前台线程的队列上，
        // 让系统把两个当成同一个进程处理；设完立刻摘掉。
        let fg = GetForegroundWindow();
        let current = GetCurrentThreadId();
        let target = GetWindowThreadProcessId(fg, None);
        let attached = target != 0 && AttachThreadInput(current, target, true).as_bool();
        let _ = SetForegroundWindow(hwnd);
        if attached {
            let _ = AttachThreadInput(current, target, false);
        }
    }
}

#[cfg(not(windows))]
pub fn show_and_focus<T>(_window: T) {}

/// 窗口大半截在屏幕外时，把它挪回工作区。
///
/// 贴边隐藏会把窗口整个滑到屏幕外、只留 5 像素。这时候别的进程来「请它出来」，
/// 光调 SetForegroundWindow 没用 —— 焦点确实给到了，但用户眼前什么都没有，
/// 只会觉得程序卡死。
///
/// 用 SetWindowPos + NOACTIVATE：只挪位置，不抢焦点、不动尺寸、不动层级。
/// 被挪动会让 Tauri 抛一次 Moved，`edge.rs` 里「窗口管理器挪回来的，让它复位」
/// 那条分支正好接住，贴边的状态会自愈，不需要这里再发通知。
#[cfg(windows)]
pub fn pull_into_view(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, SetWindowPos, SET_WINDOW_POS_FLAGS, SWP_NOACTIVATE, SWP_NOSIZE,
        SWP_NOZORDER,
    };
    unsafe {
        let mut rc = Default::default();
        if GetWindowRect(hwnd, &mut rc).is_err() {
            return;
        }
        let cur = Rect {
            x: rc.left,
            y: rc.top,
            w: rc.right - rc.left,
            h: rc.bottom - rc.top,
        };
        let Some(work) = work_area_of(hwnd) else { return };
        if overlap_width(cur, work) * 2 >= cur.w {
            return; // 还在屏幕里，别乱动
        }
        let x = cur.x.clamp(work.x, (work.right() - cur.w).max(work.x));
        let y = cur.y.clamp(work.y, (work.bottom() - cur.h).max(work.y));
        let flags = SET_WINDOW_POS_FLAGS(SWP_NOSIZE.0 | SWP_NOZORDER.0 | SWP_NOACTIVATE.0);
        let _ = SetWindowPos(hwnd, None, x, y, 0, 0, flags);
    }
}

#[cfg(not(windows))]
pub fn pull_into_view<T>(_window: T) {}

/// 鼠标当前在哪（物理像素，和 `outer_position` 同一坐标系）。
///
/// 贴边隐藏的交互全靠它：收起之后窗口只剩几像素，靠「鼠标有没有回到屏幕上」
/// 决定滑不滑回来，比监听鼠标进出窗口可靠 —— 露边本身就是窗口，
/// 鼠标停在那上面时窗口根本收不到「进入」事件。
#[cfg(windows)]
pub fn cursor_pos() -> Option<(i32, i32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p).ok()? };
    Some((p.x, p.y))
}

#[cfg(not(windows))]
pub fn cursor_pos() -> Option<(i32, i32)> {
    None
}

/// 左键按着没有。
///
/// 拖动中不能收窗口 —— 手还没松就把窗口滑走，窗口会跟着手一起往外蹭。
#[cfg(windows)]
pub fn left_button_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
    // 只认最高位「当前按着」；低位是「上次按过」，拿来判断会把一次点击当成一直按着
    unsafe { (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0 }
}

#[cfg(not(windows))]
pub fn left_button_down() -> bool {
    false
}

/// 弹一个错误框。
///
/// 交付的是 GUI 子系统（`windows_subsystem = "windows"`），**没有控制台**：
/// 启动阶段出事只写 stderr 的话，用户看到的就是「双击了，什么都没发生」，
/// 连报错都无从报起。这种时候只有一个系统弹框能把原因说清楚。
#[cfg(windows)]
pub fn error_box(title: &str, text: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    let (t, m) = (wide(title), wide(text));
    unsafe {
        MessageBoxW(
            None,
            windows::core::PCWSTR(m.as_ptr()),
            windows::core::PCWSTR(t.as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[cfg(not(windows))]
pub fn error_box(title: &str, text: &str) {
    eprintln!("[{title}] {text}");
}

/// 两个矩形横向重叠的宽度，不重叠时为 0。
///
/// 「还在屏幕里」的判据。取可见宽度的一半做门槛：留 5 像素的贴边状态
/// 恰好被算成在屏幕外（该请回来），而正常停在边缘的窗口（完整可见）不会被误挪。
pub fn overlap_width(a: Rect, b: Rect) -> i32 {
    let start = a.x.max(b.x);
    let end = a.right().min(b.right());
    (end - start).max(0)
}

/// Rust 字符串 → 以 NUL 结尾的 UTF-16，Win32 的宽字符 API 要这个形状。
#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_rects_measure_the_shared_span() {
        let a = Rect { x: 0, y: 0, w: 100, h: 100 };
        let b = Rect { x: 60, y: 0, w: 100, h: 100 };
        assert_eq!(overlap_width(a, b), 40);
    }

    #[test]
    fn separate_rects_do_not_overlap() {
        let a = Rect { x: 0, y: 0, w: 100, h: 100 };
        let b = Rect { x: 200, y: 0, w: 100, h: 100 };
        assert_eq!(overlap_width(a, b), 0);
    }

    /// 贴边隐藏后窗口只剩 5 像素露在外面，必须判定为「在屏幕外」才会被请回来
    #[test]
    fn a_narrow_strip_off_the_edge_counts_as_offscreen() {
        let screen = Rect { x: 0, y: 0, w: 1920, h: 1080 };
        let collapsed = Rect { x: -915, y: 200, w: 920, h: 700 };
        assert!(overlap_width(collapsed, screen) * 2 < collapsed.w);
    }

    /// 完整停在屏幕边缘的窗口不能被当成「跑到屏幕外」，否则每次切前台都会被乱挪
    #[test]
    fn a_window_flush_against_the_edge_stays_put() {
        let screen = Rect { x: 0, y: 0, w: 1920, h: 1080 };
        let flush = Rect { x: 0, y: 200, w: 920, h: 700 };
        assert!(!(overlap_width(flush, screen) * 2 < flush.w));
    }
}
