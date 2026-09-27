//! 到期提醒的系统通知。
//!
//! 为什么不用 `tauri-plugin-notification` 的 builder：它是「发完即忘」的，
//! 拿不到「用户点开了这条通知」的回调，而提醒的意义恰恰在于点进去看是哪个活儿。
//! 底层的 winrt `Toast` 暴露 `on_activated`，代价是要自己安排两件事：
//!   1. AUMID —— 绿色 exe 没被安装过，系统不认得它，得自己在 HKCU 注册；
//!   2. 线程 —— 激活回调是 COM 事件，只有在跑消息循环的线程上注册才会送达。

#![cfg(windows)]

use tauri::{AppHandle, Emitter, Manager};
use tauri_winrt_notification::{Duration, Toast};
use windows::core::PCWSTR;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_WRITE,
    REG_OPTION_NON_VOLATILE, REG_SZ,
};

/// 通知归属的应用标识。显示名由 `ensure_app_id` 写进注册表。
pub const APP_ID: &str = "com.local.worklog";

/// Windows 的宽字符串：UTF-16，末尾一个 0
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 在 HKCU 下注册 AUMID，让通知顶着软件名而不是 exe 文件名。
///
/// 绿色 exe 没进过「程序和功能」，系统不会替我们登记这个身份，
/// 不登记的话 `CreateToastNotifierWithId` 拿不到归属，通知根本弹不出来。
///
/// 走系统 API 而不是 `reg.exe`：调外部进程又慢又容易被安全软件记一笔。
/// 幂等，且**失败不致命**——注册不上就当没有这回事，至少别把启动流程带崩。
pub fn ensure_app_id() {
    let Ok(exe) = std::env::current_exe() else { return };
    let exe = exe.display().to_string();
    let sub = wide(&format!(r"Software\Classes\AppUserModelId\{APP_ID}"));

    unsafe {
        let mut key = HKEY::default();
        if RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(sub.as_ptr()),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut key,
            None,
        )
        .0 != 0
        {
            return;
        }

        // DisplayName 是「设置 → 通知」里显示的应用名，用中英双名，
        // 免得用户在那一长串名单里只认得出中文、邮件里又只写英文。
        for (name, value) in [("DisplayName", crate::APP_NAME), ("IconUri", exe.as_str())] {
            let n = wide(name);
            let v = wide(value);
            let bytes = std::slice::from_raw_parts(v.as_ptr().cast::<u8>(), v.len() * 2);
            let _ = RegSetValueExW(key, PCWSTR(n.as_ptr()), None, REG_SZ, Some(bytes));
        }

        let _ = RegCloseKey(key);
    }
}

/// 弹一条「该做了」的提醒；用户点它 → 打开主窗口并定位到这一条工作。
///
/// **必须从主线程调用。** winrt 的激活回调是 COM 事件，只有注册它的那个
/// 线程正在跑消息循环时才会真的送达到 `on_activated`——丢到后台线程里
/// 注册的话，通知照样弹得出来（`show()` 返回 Ok），但点了没任何反应。
pub fn show_task_toast(app: &AppHandle, task_id: i64, title: &str) {
    let app_on_click = app.clone();
    let line = format!("该做了：{title}");

    let toast = Toast::new(APP_ID)
        .title("工作记录本")
        .text1(&line)
        .text2("点这里打开这一条")
        .duration(Duration::Short)
        .on_activated(move |_| {
            focus_task(&app_on_click, task_id);
            Ok(())
        });

    if let Err(e) = toast.show() {
        eprintln!("[reminder] toast 发送失败: {e:?}");
    }
}

/// 把窗口拎到最前面，并让前端把抽屉开在那条工作上
fn focus_task(app: &AppHandle, task_id: i64) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    let _ = app.emit("notify://open-task", task_id);
}
