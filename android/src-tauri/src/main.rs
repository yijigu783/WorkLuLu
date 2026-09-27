// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! 桌面端的启动壳，只为本地调试方便。
//!
//! 安卓根本不看这个文件 —— 移动端加载的是 lib.rs 里那个 `run()`。
//! 保留它是为了能在 Windows 上直接跑，省得每次都走真机。

fn main() {
    worklog_android_lib::run()
}
