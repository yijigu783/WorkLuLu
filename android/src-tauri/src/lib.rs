//! 工作记录本 WorkLuLu —— 安卓版（Tauri mobile）入口。
//!
//! # 为什么必须有这个文件
//!
//! **Tauri 的移动端只认 `lib.rs`。** 官方文档原话：
//! > "Nothing you write in `main.rs` runs on mobile."
//!
//! `main.rs` 只是桌面端的启动壳（本地调试用，能在 Windows 上跑起来看一眼）。
//! 安卓真正加载的是这里导出的 `run()`。
//!
//! # 这个文件里不能有什么
//!
//! 托盘、开机自启、后台轮询线程 —— 这些桌面专属的东西一概没有。真要用，必须
//! `#[cfg(desktop)]` 隔离，否则安卓端直接编不过。
//!
//! # 提醒功能为什么不在这一版
//!
//! 桌面版靠 `std::thread::spawn` + `sleep(30s)` 轮询。安卓上进程会被系统回收，
//! **线程随进程一起消失，而且不报任何错**。要做提醒只能走 WorkManager / 精确闹钟
//! 那套 Kotlin 侧插件，还得申请三个权限、并且仍然会被国产 ROM 的省电策略搞掉。
//! 所以第一版按「查看 + 勾选 + 快速新建」定位，把提醒层抽象好、留待后续。
//!
//! # 目前的状态
//!
//! **骨架阶段**：验证编译链路与共享层在安卓上能跑通。真正的数据层
//! （db.rs / commands.rs 那 2000 行）还没搬过来 —— 见 docs/安卓环境搭建记录.md。

use worklog_core::schedule;

/// 把排期规则翻译成人话（「每月最后一个工作日」这类）。纯计算，不碰任何 IO。
///
/// 这个命令存在的意义是**证明共享层在安卓上是通的** —— 它整条链路都走
/// `worklog_core`，桌面版调的是同一份实现。
#[tauri::command]
fn describe_schedule(rule_json: String) -> String {
    schedule::describe_rule(&rule_json)
}

/// 算出这条规则的下一次执行时间。
///
/// 注意 `from_rfc3339` 是**前端传进来的**，不是在这里取系统时间 ——
/// 共享层不碰系统时钟，这样它才是纯函数、才能单测。
#[tauri::command]
fn next_occurrence(rule_json: String, from_rfc3339: String) -> Result<Option<String>, String> {
    let rule: worklog_core::Rule = serde_json::from_str(&rule_json).map_err(|e| e.to_string())?;
    let from = chrono::DateTime::parse_from_rfc3339(&from_rfc3339)
        .map_err(|e| e.to_string())?
        .with_timezone(&chrono::Local);
    Ok(schedule::next_from(&rule, from).map(|d| d.to_rfc3339()))
}

/// 三端统一的启动入口。
///
/// `#[cfg_attr(mobile, tauri::mobile_entry_point)]` 给安卓/iOS 生成 JNI 那一层的
/// 胶水代码；桌面端编译时这个属性直接消失，所以同一个函数两边都能用。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            describe_schedule,
            next_occurrence
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
