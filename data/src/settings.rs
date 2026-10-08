//! 设置项：一张简单的 key-value 表。
//!
//! 布尔型的读法见 `schema::setting_on` —— 没写过的键**默认是开的**，
//! 和前端 `settingOn()` 的判定必须保持一致。

use std::collections::HashMap;

use rusqlite::{params, Connection};

use crate::{e2s, R};

pub fn get_settings(conn: &Connection) -> R<HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT key, value FROM settings").map_err(e2s)?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(e2s)?;
    let mut map = HashMap::new();
    for r in rows {
        let (k, v) = r.map_err(e2s)?;
        map.insert(k, v);
    }
    Ok(map)
}

/// 设置项落库。一律用 upsert，不存在就建。
const SETTING_UPSERT: &str = "INSERT INTO settings (key, value) VALUES (?1, ?2)
     ON CONFLICT(key) DO UPDATE SET value = excluded.value";

/// 写一个设置项，返回它是不是「开」。
///
/// 收 `&mut Connection` 而不是 `&Connection`，是因为互斥的两行必须落在
/// **同一个事务**里 —— 否则中途失败会留下「一个开了、另一个也开着」的状态，
/// 而贴边隐藏和边缘分屏盯的是同一条屏幕边，两个同时开着的窗口行为是未定义的。
///
/// 返回值是给调用方判断布尔用的，省得它再解析一遍（解析规则要和这里一致，
/// 两处各写一遍迟早会漂）。桌面端拿它去真切地开关开机自启 ——
/// 那属于平台行为，不在本层。
pub fn set_setting(conn: &mut Connection, key: &str, value: &str) -> R<bool> {
    // 「贴边隐藏」和「边缘分屏」盯的是同一条屏幕边：窗口拖到左边到底是摆半屏
    // 还是藏起来，同一时刻只能有一个说了算，所以打开一个必须关掉另一个。
    //
    // 这条互斥由后端执行，界面上的互斥只是提前把结果显示出来。
    // 判据不能只活在前端 —— 以后多一个改设置的入口，就得多记一次「别忘了互斥」，
    // 放在这里是一劳永逸。
    let on = matches!(value, "1" | "true" | "on");
    let counterpart = match key {
        "snap" if on => Some("edge"),
        "edge" if on => Some("snap"),
        _ => None,
    };
    {
        let tx = conn.transaction().map_err(e2s)?;
        tx.execute(SETTING_UPSERT, params![key, value])
            .map_err(e2s)?;
        if let Some(other) = counterpart {
            tx.execute(SETTING_UPSERT, params![other, "0"])
                .map_err(e2s)?;
        }
        tx.commit().map_err(e2s)?;
    }
    Ok(on)
}
