//! 拖到屏幕边缘自动吸附：左右半屏、左上/右上四分之一、顶部最大化。
//!
//! ## 为什么自己写，不直接用系统的
//!
//! Windows 11 自带 Aero Snap，但只在**有非客户区**的窗口上生效。
//! 本程序 `decorations: false` 自绘标题栏，窗口没有系统标题栏，
//! 系统那套吸附对它不触发；而 `data-tauri-drag-region` 走的是
//! `ReleaseCapture + WM_NCLBUTTONDOWN(HTCAPTION)`，同样触发不了原生吸附。
//! 改成 `decorations: true` 能白拿系统吸附，但会毁掉现在的标题栏
//! （搜索框、「从模板」、「新建工作」都在标题栏里），不划算。
//!
//! 所以照着 Win7 Aero Snap 的逻辑自己判定：拖到边缘 → 松手落位。
//! 没有预览浮层，这是有意取舍 —— 浮层要另开一个置顶窗口，
//! 还得处理多屏、缩放、拖动中闪烁，收益不抵事。
//!
//! ## 与贴边隐藏（`edge.rs`）的关系
//!
//! 两个功能都盯着左右边缘，同一时刻只能有一个说了算：
//! 贴边隐藏开着时吸附自动让位（`enabled()` 里直接返回 false）。
//! 顺序上也要注意——`Moved` 里先判吸附再判贴边，吸附落位后窗口边缘正好压在
//! 屏幕边上，顺序反过来的话会被贴边当成「用户拖到边上」而收走。
//!
//! ## 三条纪律
//!
//! 1. **动自己的窗口也会触发 `Moved`。** 靠 `expect` 里记的坐标认出「这一次是我自己挪的」
//!    并直接返回，否则每次落位都会被当成一次新的用户拖动。
//! 2. **拖动途中 `Moved` 连发。** 松手前不做任何落位，等 `SETTLE` 之后
//!    坐标仍然停在边缘才动手；期间又来一次 `Moved` 就说明用户还在动，作废。
//! 3. **吸附前记住尺寸。** 半屏是临时形态，用户把窗口拖离边缘时必须还原成
//!    吸附前的大小，否则窗口会一直卡在半屏（这是这类功能最常见的坑）。

use crate::edge;
use crate::geom::{px, Rect};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{Manager, PhysicalPosition, PhysicalSize, Position, Runtime, Window};

/// 距屏幕边缘多少算「贴上去」。逻辑像素，随缩放放大。
const EDGE: f64 = 12.0;
/// 松手后的等待时间：拖动过程中每挪一下都会来一个 `Moved`，
/// 停顿这么久才认为用户真的松手了。
const SETTLE: Duration = Duration::from_millis(420);
/// 吸附后这段时间内不让贴边隐藏动手。正常情况下两者互斥，这里是兜底：
/// 万一两个开关在同一瞬间被切过，不至于让窗口刚吸附就被收走。
const GRACE: Duration = Duration::from_millis(1500);

/// 窗口可以停靠的区域。名字对应屏幕上的位置。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    /// 左半屏
    Left,
    /// 右半屏
    Right,
    /// 左上四分之一
    TopLeft,
    /// 右上四分之一
    TopRight,
    /// 顶部 = 最大化
    Top,
}

/// 窗口在屏幕内（不在最大化），且离某条屏幕边不超过 `tol` 物理像素时，
/// 返回它贴住的是哪条边。
///
/// 用「距边的距离」而不是「窗口某条边必须等于屏幕边」：拖动时鼠标抓的往往不是
/// 窗口最外侧，抓点在窗口内部也会让整条边落到屏幕边外面一点，留一点容差更跟手。
///
/// **同时贴到两条边时用窗口中心点分上下半**：贴到左边缘、顶边又正好也齐着屏幕顶的窗口，
/// 既可能是「用户把它拖去了左上角」，也可能是「它就是刚才吸附出来的左半屏」——
/// 几何上这两种情况一模一样，光看边距分不出来。区别在中心：半屏窗口的垂直中心
/// 落在屏幕正中间，四分之一才在上半部分。少了这一句，吸附出来的半屏每次
/// 都会在下次移动时被重新判成四分之一，窗口自己往角落缩。
pub fn hit_zone(win: Rect, mon: Rect, tol: i32) -> Option<Zone> {
    // 比屏幕还宽的窗口同时压着左右两条边，按下面的判据会稳定落进左半屏，
    // 而它其实哪儿都没「贴」——直接放行，别乱吸。
    if win.w >= mon.w {
        return None;
    }
    let left = (win.x - mon.x).abs() <= tol;
    let right = (mon.right() - win.right()).abs() <= tol;
    let top = (win.y - mon.y).abs() <= tol;
    let upper = win.y + win.h / 2 < mon.y + mon.h / 2;
    match (left, right) {
        (true, _) if top && upper => Some(Zone::TopLeft),
        (_, true) if top && upper => Some(Zone::TopRight),
        (true, _) => Some(Zone::Left),
        (_, true) => Some(Zone::Right),
        // 左右都不贴边时，只有顶边还可能算数
        _ if top => Some(Zone::Top),
        _ => None,
    }
}

/// 某个区域在工作区里对应的矩形。
///
/// 宽高为奇数时，左半屏比右半屏少一个像素：宽屏上差一像素看不出来，
/// 重要的是两半之间不能留缝 —— 留一像素的缝会被用户看成「没吸上」。
pub fn zone_rect(zone: Zone, mon: Rect) -> Rect {
    let half_w = mon.w / 2;
    let half_h = mon.h / 2;
    match zone {
        Zone::Left => Rect { x: mon.x, y: mon.y, w: half_w, h: mon.h },
        // 从左半屏的右边界起算，宽度补足剩下的部分：奇数宽时正好接上，不留缝
        Zone::Right => Rect {
            x: mon.x + half_w,
            y: mon.y,
            w: mon.w - half_w,
            h: mon.h,
        },
        Zone::TopLeft => Rect { x: mon.x, y: mon.y, w: half_w, h: half_h },
        Zone::TopRight => Rect {
            x: mon.x + half_w,
            y: mon.y,
            w: mon.w - half_w,
            h: half_h,
        },
        Zone::Top => mon,
    }
}

struct State {
    /// 当前吸附在哪个区域（None = 自由状态）
    zone: Option<Zone>,
    /// 吸附前的尺寸，拖离边缘时还原
    restore: Option<(i32, i32)>,
    /// 下一次 `Moved` 若报这个坐标，就是程序自己挪的，忽略
    expect: Option<(i32, i32)>,
    /// 代际号：拖动中每来一个 `Moved` 加一，落位任务醒来时对不上就作废
    gen: i64,
    /// 最近一次吸附的时刻，贴边隐藏靠它让路
    snapped_at: Option<Instant>,
}

static STATE: Mutex<State> = Mutex::new(State {
    zone: None,
    restore: None,
    expect: None,
    gen: 0,
    snapped_at: None,
});

/// 窗口每移动一次就调一次。返回 `true` 表示本次事件已被吸附逻辑接管，
/// 调用方就不用再往下传给贴边隐藏了。
pub fn on_moved<R: Runtime>(window: &Window<R>) -> bool {
    if !enabled(window) {
        return false;
    }
    // 最大化状态下窗口四条边都贴着屏幕，按「距边距离」判定会误判成贴边。
    // 用户点标题栏的放大按钮是明确意图，不能被吸附改写成半屏。
    if window.is_maximized().unwrap_or(false) {
        return false;
    }
    let Some((win, work, scale)) = geo(window) else { return false };
    let zone = hit_zone(win, work, px(EDGE, scale));

    let action = {
        let Ok(mut st) = STATE.lock() else { return false };
        if st.expect == Some((win.x, win.y)) {
            // 自己挪的那一下。状态已经在 apply() 里写好了，这里什么都不用做。
            st.expect = None;
            return true;
        }
        match (st.zone, zone) {
            // 已经贴在这块区域上，且位置没有超出容差：用户只是在原地蹭，
            // 不该反复重新落位（重新落位会把拖动中的手感打断）
            (Some(z), Some(z2)) if z == z2 => return true,
            // 从吸附区被拖走了：尺寸还原回吸附前的样子
            (Some(_), None) => {
                st.zone = None;
                Next::Restore
            }
            // 拖进某个区域（含半屏 → 四分之一这种换区）：延时落位
            (_, Some(z)) => {
                st.gen += 1;
                Next::Snap { zone: z, gen: st.gen }
            }
            // 自由状态且没贴边：什么都不做
            (None, None) => return false,
        }
    };

    match action {
        Next::Snap { zone, gen } => {
            // 松手判定：等一会儿，坐标没再变（= 没有新的 `Moved` 把代际号加一）
            // 才真的落位。拖动途中开出去的旧任务醒来会发现代际号对不上，直接走人。
            let window = window.clone();
            std::thread::spawn(move || {
                std::thread::sleep(SETTLE);
                let Ok(st) = STATE.lock() else { return };
                if st.gen != gen {
                    return; // 用户还在拖
                }
                drop(st);
                apply(&window, zone);
            });
            true
        }
        Next::Restore => {
            restore_size(window, win, work);
            true
        }
        Next::Ignore => true,
    }
}

enum Next {
    Snap { zone: Zone, gen: i64 },
    Restore,
    #[allow(dead_code)]
    Ignore,
}

/// 真正落位。
///
/// 状态和窗口的改动要一件件做，中间随时可能抛出 `Moved` 回调（tauri 在
/// 同一个线程上同步派发），所以**任何时候都不许握着 `STATE` 的锁去调窗口函数** ——
/// 那会当场死锁。锁只用来做「记一笔」，改完立刻放。
fn apply<R: Runtime>(window: &Window<R>, zone: Zone) {
    let Some((win, work, _)) = geo(window) else { return };
    let maximized = window.is_maximized().unwrap_or(false);
    {
        let Ok(mut st) = STATE.lock() else { return };
        if st.zone == Some(zone) {
            return; // 已经在这一区了
        }
        // 只在「从自由状态吸上去」时记尺寸。半屏 → 四分之一是同一轮吸附的延续，
        // 记进去会把半屏尺寸当成原始尺寸，拖走以后还原不回用户原来的大小。
        if st.zone.is_none() && !maximized {
            st.restore = Some((win.w, win.h));
        }
        st.zone = Some(zone);
    }

    // 顶部 = 最大化，交给系统做：从最大化状态往外拖时，
    // Windows 自己负责还原位置和尺寸，我们硬设会跟它打架。
    if zone == Zone::Top {
        if !maximized {
            let _ = window.maximize();
        }
        set_state(Some(zone), None);
        return;
    }

    let r = zone_rect(zone, work);
    {
        let Ok(mut st) = STATE.lock() else { return };
        st.expect = Some((win.x, win.y)); // 解除最大化会带出一串 Moved，先把当前位置认下来
    }
    if maximized {
        let _ = window.unmaximize();
    }
    // 先尺寸后位置。反过来的话系统会按旧尺寸算新的 x，贴边会差一截。
    let resized = window
        .set_size(PhysicalSize::new(r.w.max(1) as u32, r.h.max(1) as u32))
        .is_ok();
    let moved = resized
        && window
            .set_position(Position::Physical(PhysicalPosition::new(r.x, r.y)))
            .is_ok();
    if moved {
        set_state(Some(zone), Some((r.x, r.y)));
    } else {
        // 没挪成就不许记「已吸附」：状态和实际位置对不上，
        // 下一次拖动会莫名其妙地跳。
        set_state(None, None);
    }
}

/// 落位之后统一补一次状态。中间那些 `Moved` 回调（解除最大化、set_size 都会带出来）
/// 可能已经把 `zone` 改掉了，这里按最终结果收口。
fn set_state(zone: Option<Zone>, expect: Option<(i32, i32)>) {
    let Ok(mut st) = STATE.lock() else { return };
    st.zone = zone;
    st.expect = expect;
    st.snapped_at = if zone.is_some() { Some(Instant::now()) } else { st.snapped_at };
}

/// 拖离吸附区：把尺寸还原回吸附前的样子。
///
/// 位置不动 —— 用户拖到哪儿就停在哪儿，只是把被拉长/压扁的窗口放回原样。
/// 越界的话稍微拉回来一点，保证标题栏不会被拖到屏幕外够不着。
fn restore_size<R: Runtime>(window: &Window<R>, win: Rect, work: Rect) {
    if window.is_maximized().unwrap_or(false) {
        return;
    }
    let (w, h) = STATE
        .lock()
        .ok()
        .and_then(|st| st.restore)
        .unwrap_or((win.w, win.h));
    let x = win.x.clamp(work.x, (work.right() - w).max(work.x));
    let y = win.y.clamp(work.y, (work.bottom() - h).max(work.y));
    {
        let Ok(mut st) = STATE.lock() else { return };
        st.expect = Some((x, y));
    }
    let _ = window.set_size(PhysicalSize::new(w.max(1) as u32, h.max(1) as u32));
    let _ = window.set_position(Position::Physical(PhysicalPosition::new(x, y)));
}

/// 刚刚吸附过吗。贴边隐藏拿它让路。
pub fn recently_snapped() -> bool {
    STATE.lock()
        .map(|st| st.snapped_at.is_some_and(|t| t.elapsed() < GRACE))
        .unwrap_or(false)
}

/// 吸附是否启用。默认启用（这是系统自带的行为，不该要用户先去设置里打开），
/// 但贴边隐藏开着时一律让位。
fn enabled<R: Runtime>(window: &Window<R>) -> bool {
    if edge::is_enabled(window) {
        return false;
    }
    let Some(state) = window.app_handle().try_state::<crate::AppState>() else {
        return false;
    };
    let Ok(conn) = state.db.lock() else { return false };
    crate::db::setting_on(&conn, "snap")
}

/// 窗口矩形、所在显示器的工作区、缩放比。
///
/// 用 `MonitorFromWindow`（而不是 Tauri 的 `current_monitor`）：窗口横跨两块屏幕时，
/// 系统取的是与窗口**重叠面积最大**的那块，这正是 Windows 自己判「拖到哪块屏上了」的规则；
/// Tauri 那个默认的判定规则不一定一致。两边用同一套规则，多屏拖动才不会跳。
fn geo<R: Runtime>(window: &Window<R>) -> Option<(Rect, Rect, f64)> {
    let pos = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    let mon = window.current_monitor().ok().flatten()?;
    let (mx, my) = (mon.position().x, mon.position().y);
    let (mw, mh) = (mon.size().width as i32, mon.size().height as i32);
    let full = Rect { x: mx, y: my, w: mw, h: mh };
    // 工作区拿不到就退回整块显示器：顶多在有任务栏的屏幕上稍微压一点，
    // 比完全不吸附强。
    let work = window
        .hwnd()
        .ok()
        .and_then(crate::win32::work_area_of)
        .unwrap_or(full);
    let scale = window.scale_factor().unwrap_or(1.0);
    let win = Rect {
        x: pos.x,
        y: pos.y,
        w: size.width as i32,
        h: size.height as i32,
    };
    Some((win, work, scale))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MON: Rect = Rect { x: 0, y: 0, w: 1920, h: 1080 };
    const TOL: i32 = 12;

    #[test]
    fn flush_left_hits_left_zone() {
        let win = Rect { x: 0, y: 300, w: 900, h: 700 };
        assert_eq!(hit_zone(win, MON, TOL), Some(Zone::Left));
    }

    #[test]
    fn flush_right_hits_right_zone() {
        // 关键是右边界而不是左边界贴着屏幕右边
        let win = Rect { x: 1020, y: 300, w: 900, h: 700 };
        assert_eq!(hit_zone(win, MON, TOL), Some(Zone::Right));
    }

    #[test]
    fn top_corner_hits_a_quarter() {
        assert_eq!(
            hit_zone(Rect { x: 0, y: 0, w: 900, h: 700 }, MON, TOL),
            Some(Zone::TopLeft)
        );
        assert_eq!(
            hit_zone(Rect { x: 1020, y: 0, w: 900, h: 700 }, MON, TOL),
            Some(Zone::TopRight)
        );
    }

    #[test]
    fn top_middle_hits_maximize() {
        assert_eq!(
            hit_zone(Rect { x: 500, y: 0, w: 900, h: 700 }, MON, TOL),
            Some(Zone::Top)
        );
    }

    /// 停在屏幕中间上方一点不算贴边：容差是给「贴上去」留的余量，不是随便靠一下就吸
    #[test]
    fn near_but_not_flush_does_not_snap() {
        let win = Rect { x: 40, y: 300, w: 900, h: 700 };
        assert_eq!(hit_zone(win, MON, TOL), None);
    }

    /// 副屏坐标不从 0 开始。负坐标（左边的屏幕）也要算对
    #[test]
    fn secondary_monitor_with_negative_origin() {
        let left_screen = Rect { x: -2560, y: 0, w: 2560, h: 1440 };
        let win = Rect { x: -2550, y: 400, w: 1000, h: 700 };
        assert_eq!(hit_zone(win, left_screen, TOL), Some(Zone::Left));
    }

    /// 两块屏拼着的时候坐标是不连续的（一块到 0 为止，另一块从 0 或负数开始）。
    /// 判定只认传进来的那块显示器：站在右边屏上、正好齐着中缝的窗口，
    /// 不能被左边屏的右边界带偏成另一个区域。
    #[test]
    fn a_window_against_the_seam_uses_the_monitor_it_stands_on() {
        let left_screen = Rect { x: -1920, y: 0, w: 1920, h: 1080 };
        let right_screen = Rect { x: 0, y: 0, w: 1920, h: 1080 };

        // 齐着右边屏的左边界
        let on_right = Rect { x: 0, y: 300, w: 1000, h: 700 };
        assert_eq!(hit_zone(on_right, right_screen, TOL), Some(Zone::Left));
        // 同一组坐标若误按左边屏判，它整个飘在那块屏外面，不该产生任何区域
        assert_eq!(hit_zone(on_right, left_screen, TOL), None);

        // 齐着左边屏的右边界（大半截露在屏幕外的那种，用户自己拖出来的）
        let on_left = Rect { x: -1000, y: 300, w: 1000, h: 700 };
        assert_eq!(hit_zone(on_left, left_screen, TOL), Some(Zone::Right));
        assert_eq!(hit_zone(on_left, right_screen, TOL), None);
    }

    /// 落位后的窗口正好等于区域矩形，再判定一次仍应是同一区 ——
    /// 否则自己落位产生的 Moved 会被当成一次新拖动
    ///
    /// 不含 Top：最大化由 `is_maximized()` 提前短路，压根走不到这里。
    #[test]
    fn a_snapped_rect_still_hits_its_own_zone() {
        for zone in [Zone::Left, Zone::Right, Zone::TopLeft, Zone::TopRight] {
            let r = zone_rect(zone, MON);
            assert_eq!(hit_zone(r, MON, TOL), Some(zone), "{zone:?} 落位后判成了别的区");
        }
    }

    /// 吸附出来的左半屏，顶边本来就齐着屏幕顶。不能因为这个就把自己当成
    /// 「拖到了左上角」—— 少了这条，下面那个换区的分支会一直生效，
    /// 窗口停在半屏上也会自己缩成四分之一。
    #[test]
    fn a_full_height_half_is_not_read_as_a_quarter() {
        let half = zone_rect(Zone::Left, MON);
        assert_eq!(half.y, MON.y, "半屏顶边确实齐着屏幕顶");
        assert_eq!(hit_zone(half, MON, TOL), Some(Zone::Left));
        // 只往上挪一点（还在容差内）也还是半屏
        let nudged = Rect { y: 6, ..half };
        assert_eq!(hit_zone(nudged, MON, TOL), Some(Zone::Left));
    }

    /// 但用户真的把窗口拖到角上时，得给四分之一
    #[test]
    fn a_window_dragged_to_the_corner_is_a_quarter() {
        let big = Rect { x: 0, y: 0, w: 1120, h: 720 };
        assert_eq!(hit_zone(big, MON, TOL), Some(Zone::TopLeft));
    }

    #[test]
    fn zones_tile_the_work_area_without_gaps() {
        let l = zone_rect(Zone::Left, MON);
        let r = zone_rect(Zone::Right, MON);
        assert_eq!(l.x, 0);
        assert_eq!(r.right(), 1920);
        assert_eq!(l.right(), r.x, "左右两半之间不能有空隙");
        assert_eq!(l.h, 1080);
        assert_eq!(l.w + r.w, 1920);
    }

    /// 奇数宽的屏幕：不能因为除以 2 少一个像素就漏掉一个像素宽的缝
    #[test]
    fn odd_monitor_width_leaves_no_gap() {
        let odd = Rect { x: 0, y: 0, w: 1367, h: 768 };
        let l = zone_rect(Zone::Left, odd);
        let r = zone_rect(Zone::Right, odd);
        assert_eq!(l.right(), r.x);
        assert_eq!(l.w + r.w, 1367);
        assert_eq!(r.right(), 1367);
    }
    /// 四分之一区的顶边和左边都要真的在屏幕边上
    #[test]
    fn quarters_hug_their_corner() {
        let q = zone_rect(Zone::TopRight, MON);
        assert_eq!((q.x, q.y), (960, 0));
        assert_eq!((q.w, q.h), (960, 540));
    }
}
