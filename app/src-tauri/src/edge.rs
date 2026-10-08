//! 贴边自动隐藏。
//!
//! 打开后：把窗口拖到屏幕左/右边缘松手，它会滑出屏幕、只留一条窄边；
//! 鼠标碰到那条边就滑回来，鼠标离开窗口一会儿再收回去。
//!
//! ## 为什么改成后端轮询，不用窗口事件
//!
//! 上一版把「鼠标碰到边」交给前端的 mouseenter/mouseleave，把「拖到边上了」
//! 交给 `WindowEvent::Moved`。真机实测两处都不成立：
//!
//! - 拖动结束时鼠标正压在露出来的那几像素上（那条窄边本身就是窗口的一部分），
//!   于是「鼠标进入窗口」立刻成立，窗口刚收起就被自己叫回来。
//! - 而 `Moved` 只在窗口真的移动时发。用户拖窗口抓的是标题栏中间，鼠标顶到
//!   屏幕边松手时窗口外边缘早就滑出屏幕一百多像素，那时的判据却要求
//!   「边缘正好落在屏幕边上 ±8 像素」—— 这种拖动永远等不到，功能看起来是完全失效。
//!
//! 所以改成后端每 120 毫秒自己采一次样（窗口矩形 + 显示器 + 鼠标位置 + 左键状态），
//! 判定写成纯函数 `decide()`，喂假数据就能单测。前端只负责画那条「把手」。
//!
//! ## 三个必须守住的点
//!
//! 1. **默认关，而且不能靠 `db::setting_on` 判**。那个函数的约定是「没写过这个键＝开」，
//!    适合提醒、托盘这类「本来就该有」的功能；贴边隐藏会改掉窗口的位置行为，
//!    用户没主动打开就该当它是关的。
//! 2. **收起是「把窗口移到屏幕外」，不是「把窗口缩窄」**。窗口有 minWidth(900)，
//!    缩到几像素会被系统直接挡回来。
//! 3. **判据是「越过屏幕边缘」，不是「正好贴住」**。理由见上。
//!
//! ## 与边缘吸附（`snap.rs`）的关系
//!
//! 两个功能都认左右边缘，同一时刻只能有一个说了算：贴边隐藏开着时吸附让位
//! （`snap::enabled` 里直接返回 false）。这里再放一道 `snap::recently_snapped`
//! 兜底，防的是开关在同一瞬间被切过去的那点空档。

use crate::geom::{px, Rect};
use rusqlite::OptionalExtension;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager, PhysicalPosition, Position, Runtime, Window};

/// 收起后露在外面的宽度（逻辑像素）。太窄鼠标够不着，太宽就不像收起来了。
const PEEK: f64 = 5.0;
/// 判定「压到了屏幕边缘」的容差（逻辑像素）
const HIT: f64 = 8.0;
/// 展开时往屏幕里让开的距离。
/// **必须大于 HIT** —— 否则展开的瞬间又会被判成贴边，窗口当场自己收回去。
const INSET: f64 = HIT + 8.0;
/// 鼠标离屏幕边这么近就算「碰到那条把手了」（逻辑像素）。
/// 比起伏的几像素大一点，手感才不至于要求用户瞄准。
const HOT_BAND: f64 = 6.0;
/// 窗口至少要有这么多留在屏幕里才值得收（逻辑像素）。
/// 用户自己把窗口摆成只露一角时不收 —— 那是他自己要的位置。
const MIN_VISIBLE: f64 = 120.0;
/// 位置安静这么久才算「松手了」。拖动过程中坐标是连续变的，刚停下时不能立刻动手。
const SETTLE: Duration = Duration::from_millis(350);
/// 鼠标离开窗口多久之后收回
const AWAY_DELAY: Duration = Duration::from_millis(520);
/// 采样间隔
const POLL: Duration = Duration::from_millis(120);
/// 开关状态每多少次采样复查一次。120ms 就去锁一次数据库没必要；
/// 改完设置最多 1.2 秒生效，用户感觉不出来。
const ENABLED_EVERY: u64 = 10;
/// 主窗口的 label
const MAIN_WINDOW: &str = "main";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    /// 贴在屏幕左边（收起后往左滑出去）
    Left,
    /// 贴在屏幕右边
    Right,
}

impl Side {
    fn as_str(self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Right => "right",
        }
    }
}

/// 窗口压到了哪一边（完全在屏幕里就是 None）。
///
/// 判据是「窗口边缘**越过**了屏幕边缘」，不是「正好压在屏幕边缘上」。
/// 后者是上一版的写法，也是这个功能实测无效的根因：用户抓标题栏中间拖到屏幕边，
/// 松手时窗口外边缘已经滑出屏幕一百多像素，`|win.x - mon.x| <= 8` 永远不成立。
pub fn docked(win: Rect, mon: Rect, tol: i32) -> Option<Side> {
    // 窗口比屏幕还宽时它必然同时压着左右两条边。这时候无论收去哪一边都很荒唐，
    // 直接当它「没贴边」。
    if win.w >= mon.w {
        return None;
    }
    if win.x <= mon.x + tol {
        return Some(Side::Left);
    }
    if win.right() >= mon.right() - tol {
        return Some(Side::Right);
    }
    None
}

/// 收起后窗口应该停在的 x。
/// 左贴边：窗口整体移到屏幕左侧之外，只留最右边 `peek` 像素可见；
/// 右贴边：反过来，只留最左边 `peek` 像素。
pub fn collapsed_x(side: Side, win: Rect, mon: Rect, peek: i32) -> i32 {
    match side {
        Side::Left => mon.x - win.w + peek,
        Side::Right => mon.right() - peek,
    }
}

/// 展开后窗口应该停在的 x。
/// 特意往里让开 `inset`，让窗口一露头就不满足 `docked`，
/// 不会刚滑出来又被自己收回去。
pub fn expanded_x(side: Side, win: Rect, mon: Rect, inset: i32) -> i32 {
    match side {
        Side::Left => mon.x + inset,
        Side::Right => mon.right() - win.w - inset,
    }
}

/// 窗口露在显示器范围内的宽度。
/// 收起时只剩 `peek` 那么一点；一旦明显变大，说明它被人弄回屏幕里了。
pub fn visible_w(win: Rect, mon: Rect) -> i32 {
    let left = win.x.max(mon.x);
    let right = win.right().min(mon.right());
    (right - left).max(0)
}

/// 鼠标是不是贴在屏幕某一边那条窄边上。
///
/// 上下也要落在显示器范围内 —— 多屏上下排列时，鼠标在另一块屏的同一 x 上不算数。
/// 左右方向要求光标在这块显示器里：左边还有一块屏的话，鼠标跑过去不能把它叫出来。
fn in_hot_band(side: Side, mon: Rect, cur: (i32, i32), band: i32) -> bool {
    if cur.1 < mon.y || cur.1 >= mon.bottom() {
        return false;
    }
    match side {
        Side::Left => cur.0 >= mon.x && cur.0 <= mon.x + band,
        // 右边界是不含的：光标最多只能到 right - 1
        Side::Right => cur.0 <= mon.right() - 1 && cur.0 >= mon.right() - 1 - band,
    }
}

/// 一次采样拿到的全部事实。判定只看它，所以喂假数据就能测。
#[derive(Clone, Copy, Debug)]
struct Snapshot {
    win: Rect,
    mon: Rect,
    /// 鼠标位置（物理像素，和窗口坐标同一套）
    cur: (i32, i32),
    /// 左键按着没有
    lbutton: bool,
    scale: f64,
    now: Instant,
}

/// 采样之后该做的一件窗口操作。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Action {
    None,
    /// 收进屏幕外，把窗口移到 x
    Collapse { side: Side, x: i32 },
    /// 滑回屏幕里
    Expand { side: Side, x: i32 },
}

/// 跨采样保留的那点记忆。全是可拷贝的标量 / 小结构，`decide` 前存一份就能回滚。
#[derive(Clone, Copy, Debug, Default)]
struct State {
    /// 现在收在屏幕外
    collapsed: bool,
    /// 贴的是哪一边。**展开之后也保留** —— 展开位置按设计就不贴边，
    /// 光看坐标判断不出鼠标走开之后该往哪边收。
    side: Option<Side>,
    /// 这一次「展开」是程序自己做的，鼠标走开就该收回去。
    /// 用户把窗口拖走之后这里会失效（见 `placed`），不能再自作主张地收。
    placed: Option<(i32, i32)>,
    /// 上一次程序自己把窗口移到的 x，用来认出「这一下是我们自己挪的」
    expect: Option<i32>,
    /// 上一 tick 的窗口矩形，用来判断窗口动了没有
    last: Option<Rect>,
    /// 启动 / 开启以来窗口动过没有。**没动过就不收** ——
    /// 否则用户刚把开关打开，本来就停在屏幕边上的窗口会当场自己滑走，
    /// 而他什么都没做，只会觉得见了鬼。
    moved: bool,
    /// 位置最后一次变化的时刻，松手判定用
    moved_at: Option<Instant>,
    /// 鼠标从窗口（连同那条窄边）走开的起始时刻
    away_since: Option<Instant>,
    /// 「碰一下滑回来」有没有布防。
    ///
    /// **这条不能省。** 用户把窗口拖到屏幕左边松手时，鼠标就停在屏幕上那条边上；
    /// 窗口刚收起来，下一 tick 就发现「鼠标贴边」→ 当场又弹回去，
    /// 表现出来就是「拖过去抖一下又回来了」，等于功能没做成。
    /// 所以收起之后必须先看到鼠标离开那条边，再回来才算一次「碰」。
    armed: bool,
}

static STATE: Mutex<State> = Mutex::new(State {
    collapsed: false,
    side: None,
    placed: None,
    expect: None,
    last: None,
    moved: false,
    moved_at: None,
    away_since: None,
    armed: false,
});

/// 采样 → 该做什么。
///
/// 纯函数：不碰窗口、不碰全局状态，只有 `st` 这一点记忆。窗口几何、鼠标位置、
/// 左键状态、时刻都在 `s` 里。这样每一条判定规则都能用假数据钉住 ——
/// 上一版最要命的问题正是「只能在真机上试，试的方法还错了」。
fn decide(st: &mut State, s: &Snapshot) -> Action {
    let tol = px(HIT, s.scale);
    let peek = px(PEEK, s.scale);
    let band = px(HOT_BAND, s.scale);
    let pad = px(8.0, s.scale);
    let inset = px(INSET, s.scale);
    let min_visible = px(MIN_VISIBLE, s.scale);

    /* ---- 1. 这一 tick 窗口动了没有 ---- */
    let prev = st.last;
    st.last = Some(s.win);
    // 第一次采样时 prev 是 None：那是「刚知道它在哪」，不算动过
    if prev.is_some_and(|p| p != s.win) {
        st.moved = true;
        st.moved_at = Some(s.now);
    }
    // 位置安静够久了才算真的松手
    let settled = st.moved_at.map_or(true, |t| s.now.duration_since(t) >= SETTLE);

    // 鼠标不在那条窄边上 → 布防，下次贴上去才算「碰一下」
    if st.side.map_or(true, |sd| !in_hot_band(sd, s.mon, s.cur, band)) {
        st.armed = true;
    }

    /* ---- 2. 这一次位置变化是不是程序自己造成的 ---- */
    if st.expect == Some(s.win.x) {
        // 是我们刚才那次 set_position，认下来，下次变化才归用户
        st.expect = None;
    } else if st.placed.is_some_and(|(x, y)| {
        // 留两像素余量：系统有时会把落点微调一下
        (s.win.x - x).abs() > 2 || (s.win.y - y).abs() > 2
    }) {
        // 窗口已经不在我们放的那个位置上了 → 用户自己把它拖走了。
        // 之前那次「展开」到此为止：不能再自作主张地收回去，贴边记录也清掉，
        // 等它再贴到某一边时重新判定。
        st.placed = None;
        st.side = None;
        st.away_since = None;
    }

    /* ---- 3. 已经收在屏幕外 ---- */
    if st.collapsed {
        // 3a) 鼠标碰到那条露边 → 滑回来。
        //     前提是已经「布防」：鼠标先离开过那条边（见 `State::armed`）。
        if let Some(side) = st.side {
            if st.armed && in_hot_band(side, s.mon, s.cur, band) {
                let x = expanded_x(side, s.win, s.mon, inset);
                st.collapsed = false;
                st.expect = Some(x);
                st.placed = Some((x, s.win.y));
                st.away_since = None;
                return Action::Expand { side, x };
            }
        }
        // 3b) 窗口被别的东西弄回屏幕里了（换了分辨率、被窗口管理器挪动）。
        //     不复位的话「鼠标走开就收回」会**静默**失效：窗口明明在屏幕里，
        //     表现却像还贴在边上，用户完全看不出原因。
        if visible_w(s.win, s.mon) > peek * 3 {
            st.collapsed = false;
            st.side = None;
            st.placed = None;
        }
        return Action::None;
    }

    /* ---- 4. 没在收起状态 ---- */
    let at = docked(s.win, s.mon, tol);

    // 4a) 窗口压到屏幕边上了 → 等它停稳就收走
    if let Some(side) = at {
        st.side = Some(side);
        // 四个前提缺一不可。`moved` 那条见 `State::moved` 的注释；
        // `min_visible` 那条防的是「用户自己把窗口摆成只露一角，又被我们收一次」。
        if !s.lbutton
            && st.moved
            && settled
            && visible_w(s.win, s.mon) >= min_visible
            && !crate::snap::recently_snapped()
        {
            let x = collapsed_x(side, s.win, s.mon, peek);
            st.collapsed = true;
            st.expect = Some(x);
            st.placed = Some((x, s.win.y));
            st.away_since = None;
            // 松手时鼠标还压在屏幕边上，先撤掉布防 —— 否则下一 tick 就自己弹回来
            st.armed = false;
            return Action::Collapse { side, x };
        }
    } else if st.placed.is_none() {
        // 既没贴边、又不是我们摆的位置：这条贴边记录作废
        st.side = None;
        st.away_since = None;
    }

    // 4b) 是我们把窗口展开出来的 → 鼠标走开够久就收回去。
    //     只有 `placed` 对得上（窗口还在我们放的位置）才管，
    //     用户自己拖走的窗口不归这条管。
    if let Some(side) = st.side {
        if st.placed.is_some() {
            let staying = s.win.holds(s.cur.0, s.cur.1, pad)
                || in_hot_band(side, s.mon, s.cur, band);
            if staying || s.lbutton {
                st.away_since = None;
            } else {
                let since = *st.away_since.get_or_insert(s.now);
                if s.now.duration_since(since) >= AWAY_DELAY {
                    let x = collapsed_x(side, s.win, s.mon, peek);
                    st.collapsed = true;
                    st.expect = Some(x);
                    st.placed = Some((x, s.win.y));
                    st.away_since = None;
                    st.armed = false;
                    return Action::Collapse { side, x };
                }
            }
        }
    }

    Action::None
}

/// 贴边隐藏是否开着。
///
/// 不复用 `db::setting_on`：那个函数把「没写过这个键」当成开，
/// 这里必须当成关（见文件头第 1 条）。
///
/// `pub` 是给 `snap::enabled` 用的 —— 吸附要知道贴边隐藏开了没有，好给它让路。
pub fn is_enabled<R: Runtime>(window: &Window<R>) -> bool {
    let Some(state) = window.app_handle().try_state::<crate::AppState>() else {
        return false;
    };
    let Ok(conn) = state.db.lock() else { return false };
    let v: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = 'edge'", [], |r| r.get(0))
        .optional()
        .unwrap_or(None);
    matches!(v.as_deref(), Some("1" | "true" | "on"))
}

/// 窗口当前的位置与尺寸、所在显示器、缩放比例。
///
/// 最大化 / 最小化 / 隐藏时返回 None —— 铺满屏幕的窗口不该有贴边行为，
/// 收进托盘的窗口更是连碰都不该碰。
fn geo<R: Runtime>(window: &Window<R>) -> Option<(Rect, Rect, f64)> {
    if !window.is_visible().unwrap_or(true) {
        return None;
    }
    if window.is_minimized().unwrap_or(false) {
        return None;
    }
    if window.is_maximized().unwrap_or(false) {
        return None;
    }
    let p = window.outer_position().ok()?;
    let s = window.outer_size().ok()?;
    let m = window.current_monitor().ok().flatten()?;
    let mp = m.position();
    let ms = m.size();
    Some((
        Rect { x: p.x, y: p.y, w: s.width as i32, h: s.height as i32 },
        Rect { x: mp.x, y: mp.y, w: ms.width as i32, h: ms.height as i32 },
        window.scale_factor().unwrap_or(1.0),
    ))
}

fn emit<R: Runtime>(window: &Window<R>, collapsed: bool, side: Option<Side>) {
    let payload = serde_json::json!({
        "collapsed": collapsed,
        "side": side.map(Side::as_str),
    });
    // 写成一行：tools/check_contract.js 按「emit 后紧跟字符串字面量」的形状
    // 对账前后端事件名，拆行会让它认不出来。
    let _ = window.app_handle().emit("edge://changed", payload);
}

/// 把窗口从屏幕外请回屏幕里。不在收起状态就什么也不做，返回有没有真的动过。
fn pull_back<R: Runtime>(window: &Window<R>, keep_out_on_away: bool) -> Option<Side> {
    let (win, mon, scale) = geo(window)?;
    let side = {
        let st = STATE.lock().ok()?;
        if !st.collapsed {
            return None;
        }
        st.side?
    };
    let x = expanded_x(side, win, mon, px(INSET, scale));
    if window
        .set_position(Position::Physical(PhysicalPosition::new(x, win.y)))
        .is_err()
    {
        // 挪不动就什么也别记 —— 状态说「已经出来了」而窗口还在屏幕外，
        // 用户会以为程序坏了，而且再没有第二次机会把它拉回来。
        return None;
    }
    if let Ok(mut st) = STATE.lock() {
        st.collapsed = false;
        st.expect = Some(x);
        // 「鼠标走开就自动收回」要不要跟着生效，看调用方：
        // 用户自己点任务栏把窗口叫回来时鼠标还在任务栏上，这时候挂上自动收回，
        // 半秒后窗口又自己滑走了。所以只有前端主动展开才挂。
        st.placed = if keep_out_on_away { Some((x, win.y)) } else { None };
        st.away_since = None;
    }
    Some(side)
}

/// 关掉开关时调用：窗口可能正滑在屏幕外，先请回来，再把记录清干净。
pub fn reset<R: Runtime>(window: &Window<R>) {
    // 先动窗口再清状态；顺序反了的话 `pull_back` 会以为窗口没收起，白跑一趟
    let side = pull_back(window, false);
    if let Ok(mut st) = STATE.lock() {
        *st = State::default();
    }
    if let Some(side) = side {
        emit(window, false, Some(side));
    }
}

/// 窗口被激活（任务栏点回来、Alt+Tab）。
/// 收起状态下的窗口在屏幕外，不给它滑回来用户会以为什么都没发生。
pub fn on_focused<R: Runtime>(window: &Window<R>) {
    if !is_enabled(window) {
        return;
    }
    // 不挂「鼠标走开自动收回」：从任务栏点回来时鼠标还在任务栏上，
    // 挂上就会半秒后自己收回去，比不收还烦人。
    if let Some(side) = pull_back(window, false) {
        emit(window, false, Some(side));
    }
}

/// 起一个后台线程盯着窗口。
///
/// 开关关着的时候也照跑，但只做一次数据库查询，不做几何采样 ——
/// 省得每 120 毫秒白问一次窗口位置。
pub fn spawn_watch(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut tick: u64 = 0;
        let mut enabled = false;
        // 上一次通知给前端的 (收起, 哪一边)。状态真的变了才发事件，
        // 免得每 120 毫秒都往 WebView 里塞一条。
        let mut emitted: Option<(bool, Option<Side>)> = None;

        loop {
            std::thread::sleep(POLL);
            tick += 1;

            // `get_webview_window` 是 `Manager` 上现成的那个；
            // 判定要的是 `Window`，取一层再 clone 出来（`Window` 本身就是个句柄壳）。
            let Some(webview) = app.get_webview_window(MAIN_WINDOW) else { continue };
            let window = webview.as_ref().window().clone();
            if tick == 1 || tick % ENABLED_EVERY == 0 {
                enabled = is_enabled(&window);
            }

            if !enabled {
                if emitted != Some((false, None)) {
                    // 正常路径是前端在关开关时调 `edge_reset`。这里兜底：
                    // 万一那一次调用丢了，窗口还挂在屏幕外，用户就再也找不回来了。
                    reset(&window);
                    emitted = Some((false, None));
                }
                continue;
            }

            let Some((win, mon, scale)) = geo(&window) else {
                // 最大化 / 最小化 / 收在托盘里：把贴边记录清掉。
                // 不清的话，还原窗口之后鼠标一移开它会莫名其妙地自己收走。
                if let Ok(mut st) = STATE.lock() {
                    st.collapsed = false;
                    st.side = None;
                    st.placed = None;
                    st.expect = None;
                    st.last = None;
                }
                if emitted != Some((false, None)) {
                    emitted = Some((false, None));
                    emit(&window, false, None);
                }
                continue;
            };

            let snapshot = Snapshot {
                win,
                mon,
                // 拿不到鼠标位置时给一个够远的假坐标：所有「贴边 / 在窗口里」
                // 的判定都会落空，等价于「鼠标不在这附近」，不会误触发。
                cur: crate::win32::cursor_pos().unwrap_or((-100_000, -100_000)),
                lbutton: crate::win32::left_button_down(),
                scale,
                now: Instant::now(),
            };

            let action = {
                let Ok(mut st) = STATE.lock() else { continue };
                let before = *st;
                let action = decide(&mut st, &snapshot);
                if action == Action::None {
                    continue;
                }
                (before, action)
            };

            let (before, action) = action;
            let x = match action {
                Action::Collapse { x, .. } | Action::Expand { x, .. } => x,
                Action::None => unreachable!(),
            };
            if window
                .set_position(Position::Physical(PhysicalPosition::new(x, win.y)))
                .is_err()
            {
                // 挪不动就把记忆退回去。留着「已收起」而窗口其实还在原地，
                // 界面和实际就对不上了；展开失败更糟 —— 状态说它在屏幕里，
                // 窗口却真的挂在屏幕外，再没有第二次机会拉回来。
                if let Ok(mut st) = STATE.lock() {
                    *st = before;
                }
                continue;
            }

            let now = STATE
                .lock()
                .map(|st| (st.collapsed, st.side))
                .unwrap_or((false, None));
            if emitted != Some(now) {
                emitted = Some(now);
                emit(&window, now.0, now.1);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const MON: Rect = Rect { x: 0, y: 0, w: 1920, h: 1080 };
    const WIN: Rect = Rect { x: 0, y: 100, w: 1120, h: 720 };

    /// 手动推进的时钟。判定里到处是「多久之后」，不造个假时钟就只能靠 sleep，
    /// 测试会又慢又不稳。
    struct Clock(Instant);
    impl Clock {
        fn new() -> Self {
            Clock(Instant::now())
        }
        /// 前进 `ms` 毫秒，返回当前时刻
        fn tick(&mut self, ms: u64) -> Instant {
            self.0 += Duration::from_millis(ms);
            self.0
        }
    }

    fn snap(win: Rect, cur: (i32, i32), lbutton: bool, now: Instant) -> Snapshot {
        Snapshot { win, mon: MON, cur, lbutton, scale: 1.0, now }
    }

    /* ---------------- 几何 ---------------- */

    #[test]
    fn detects_both_edges() {
        let left = Rect { x: 0, ..WIN };
        let right = Rect { x: 1920 - 1120, ..WIN };
        let mid = Rect { x: 300, ..WIN };

        assert_eq!(docked(left, MON, 8), Some(Side::Left));
        assert_eq!(docked(right, MON, 8), Some(Side::Right));
        assert_eq!(docked(mid, MON, 8), None);
    }

    /// 这一条就是用户报的那个 bug：真实拖动松手时窗口边缘早就滑出屏幕一截，
    /// 旧的「|win.x - mon.x| <= 容差」永远等不到。
    #[test]
    fn a_window_pushed_over_the_edge_counts_as_docked() {
        let over = Rect { x: -119, ..WIN };
        assert_eq!(docked(over, MON, 8), Some(Side::Left));

        let over_right = Rect { x: 1920 - 1120 + 90, ..WIN };
        assert_eq!(docked(over_right, MON, 8), Some(Side::Right));
    }

    /// 完全离开屏幕边缘就什么也不算 —— 容差是给「压上去」留的余量，
    /// 不是随便靠近一下就算。
    #[test]
    fn a_window_away_from_the_edge_is_not_docked() {
        let free = Rect { x: 40, ..WIN };
        assert_eq!(docked(free, MON, 8), None);
    }

    /// 窗口比屏幕还宽时必然同时压着左右两条边，收去哪边都荒唐
    #[test]
    fn a_window_wider_than_the_screen_is_never_docked() {
        let wide = Rect { x: -200, y: 0, w: 2200, h: 700 };
        assert_eq!(docked(wide, MON, 8), None);
    }

    #[test]
    fn collapsed_keeps_only_a_peek_outside() {
        assert_eq!(collapsed_x(Side::Left, WIN, MON, 5), -1120 + 5);
        assert_eq!(collapsed_x(Side::Right, WIN, MON, 5), 1920 - 5);
    }

    /// 这条是整个贴边功能最容易翻车的地方：
    /// 展开的位置要是仍在贴边范围内，窗口会「滑出来 → 立刻收回去」无限循环。
    #[test]
    fn the_expanded_position_is_never_docked_again() {
        for side in [Side::Left, Side::Right] {
            let x = expanded_x(side, WIN, MON, 16);
            assert_eq!(
                docked(Rect { x, ..WIN }, MON, 8),
                None,
                "{side:?} 展开后不该再被判定成贴边"
            );
        }
    }

    /// 收起后的位置必须被 `visible_w` 认出来 —— 收起判定的「已经收好了，
    /// 别再收一次」靠的就是它。
    #[test]
    fn visible_width_tells_collapsed_from_dragged_back() {
        let collapsed = Rect { x: collapsed_x(Side::Left, WIN, MON, 5), ..WIN };
        assert_eq!(visible_w(collapsed, MON), 5, "收起时只该露出 peek 那几像素");

        let back = Rect { x: 400, ..WIN };
        assert_eq!(visible_w(back, MON), 1120, "回到屏幕里就是完整宽度");

        let off = Rect { x: -3000, ..WIN };
        assert_eq!(visible_w(off, MON), 0, "彻底在屏幕外按 0 算，不能出负数");
    }

    /// 副屏在主屏左边时坐标是负数，几何算式必须照样成立
    #[test]
    fn works_on_a_monitor_left_of_the_primary_one() {
        let mon = Rect { x: -1920, y: 0, w: 1920, h: 1080 };
        let win = Rect { x: -1920, y: 60, w: 1120, h: 720 };

        assert_eq!(docked(win, mon, 8), Some(Side::Left));
        assert_eq!(collapsed_x(Side::Left, win, mon, 5), -1920 - 1120 + 5);

        let x = expanded_x(Side::Right, win, mon, 16);
        assert_eq!(docked(Rect { x, ..win }, mon, 8), None);

        // 光标贴在左边那块屏的左缘也能把窗口叫回来
        assert!(in_hot_band(Side::Left, mon, (-1918, 400), 6));
        assert!(!in_hot_band(Side::Left, mon, (5, 400), 6), "另一块屏上的同 x 不算");
    }

    /* ---------------- 判定 ---------------- */

    /// 用户报的那条路径，从头到尾走一遍：
    /// 拖到屏幕左边、松手 → 收进屏幕外。
    #[test]
    fn a_window_dragged_over_the_edge_collapses() {
        let mut st = State::default();
        let mut clock = Clock::new();

        // 一开始窗口在屏幕中间
        let mid = Rect { x: 400, y: 100, w: 1120, h: 720 };
        assert_eq!(decide(&mut st, &snap(mid, (900, 300), false, clock.tick(120))), Action::None);

        // 开始拖，中途的采样
        let dragging = Rect { x: 120, y: 100, w: 1120, h: 720 };
        assert_eq!(
            decide(&mut st, &snap(dragging, (300, 250), true, clock.tick(120))),
            Action::None,
            "手还按着的时候不能动窗口 —— 窗口会跟着手一起往外蹭"
        );

        // 拖到屏幕左边松手：左边缘已经滑出屏幕 119 像素
        let over = Rect { x: -119, y: 100, w: 1120, h: 720 };
        assert_eq!(
            decide(&mut st, &snap(over, (0, 250), true, clock.tick(120))),
            Action::None
        );
        assert_eq!(
            decide(&mut st, &snap(over, (0, 250), false, clock.tick(120))),
            Action::None,
            "刚松手，得再确认一下不是还在拖"
        );

        // 停稳了 → 收
        assert_eq!(
            decide(&mut st, &snap(over, (0, 250), false, clock.tick(400))),
            Action::Collapse { side: Side::Left, x: -1120 + 5 }
        );
        assert!(st.collapsed);
    }

    /// 刚把开关打开时窗口本来就贴在屏幕边上：它没动过，不能替用户做决定
    #[test]
    fn a_window_that_never_moved_is_left_alone() {
        let mut st = State::default();
        let mut clock = Clock::new();
        let flush = Rect { x: 0, y: 100, w: 1120, h: 720 };

        for _ in 0..10 {
            assert_eq!(
                decide(&mut st, &snap(flush, (800, 400), false, clock.tick(120))),
                Action::None
            );
        }
        assert!(!st.collapsed, "窗口压根没动过，不该替用户把它收走");
    }

    /// 收起之后鼠标不在这条边上，就不该把它叫回来 ——
    /// 上一版正是栽在这里：露在外面的那几像素本身就是窗口，
    /// 松手时鼠标正压在上面，窗口刚收起就被自己叫了回来。
    #[test]
    fn a_freshly_collapsed_window_does_not_pop_back_out() {
        let mut st = State::default();
        let mut clock = Clock::new();
        let win = Rect { x: -1115, y: 100, w: 1120, h: 720 };
        st.collapsed = true;
        st.side = Some(Side::Left);
        st.moved = true;

        for _ in 0..10 {
            assert_eq!(
                decide(&mut st, &snap(win, (800, 400), false, clock.tick(120))),
                Action::None
            );
        }
        assert!(st.collapsed);
    }

    /// **这条决定「拖到边上」这个动作能不能真正成立。**
    ///
    /// 用户把窗口拖到屏幕左边松手时，鼠标就停在屏幕边上 ——
    /// 正是那条窄边所在的位置。要是收起之后立刻就认「鼠标贴边」，
    /// 窗口会在下一 tick 自己弹回来，用户看到的只是「抖了一下，什么都没发生」。
    /// 所以收起之后必须先看到鼠标离开，再回来才算一次「碰」。
    #[test]
    fn a_collapse_landing_with_the_cursor_on_the_edge_does_not_pop_straight_back() {
        let mut st = State::default();
        let mut clock = Clock::new();
        let win = Rect { x: -1115, y: 100, w: 1120, h: 720 };
        st.collapsed = true;
        st.side = Some(Side::Left);
        st.moved = true;
        st.armed = false; // 刚收起来，鼠标还压在边上

        for _ in 0..10 {
            assert_eq!(
                decide(&mut st, &snap(win, (0, 250), false, clock.tick(120))),
                Action::None,
                "鼠标还停在松手的位置上，不能立刻弹回来"
            );
        }
        assert!(st.collapsed);

        // 鼠标挪开 → 布防
        assert_eq!(
            decide(&mut st, &snap(win, (600, 400), false, clock.tick(120))),
            Action::None
        );
        assert!(st.armed, "鼠标离开那条边之后就该布防");

        // 再贴回屏幕左缘 → 这次才是真的「碰一下」
        assert_eq!(
            decide(&mut st, &snap(win, (1, 400), false, clock.tick(120))),
            Action::Expand { side: Side::Left, x: 16 }
        );
    }

    #[test]
    fn touching_the_edge_brings_it_back() {
        let mut st = State::default();
        let mut clock = Clock::new();
        let win = Rect { x: -1115, y: 100, w: 1120, h: 720 };
        st.collapsed = true;
        st.side = Some(Side::Left);
        st.moved = true;

        // 鼠标在屏幕中间乱晃（这一步也是在布防）
        assert_eq!(decide(&mut st, &snap(win, (800, 400), false, clock.tick(120))), Action::None);
        assert_eq!(decide(&mut st, &snap(win, (400, 700), false, clock.tick(120))), Action::None);

        // 鼠标贴到屏幕左缘 → 滑回来，并且往屏幕里让开 INSET
        assert_eq!(
            decide(&mut st, &snap(win, (2, 400), false, clock.tick(120))),
            Action::Expand { side: Side::Left, x: 16 }
        );
        assert!(!st.collapsed);
    }

    /// 贴右边时，鼠标得跑到屏幕最右边才叫得出来
    #[test]
    fn the_right_side_uses_the_right_hot_band() {
        let mut st = State::default();
        let mut clock = Clock::new();
        let win = Rect { x: 1920 - 5, y: 100, w: 1120, h: 720 };
        st.collapsed = true;
        st.side = Some(Side::Right);
        st.moved = true;

        assert_eq!(decide(&mut st, &snap(win, (300, 400), false, clock.tick(120))), Action::None);
        assert!(st.armed);
        assert_eq!(
            decide(&mut st, &snap(win, (1919, 400), false, clock.tick(120))),
            Action::Expand { side: Side::Right, x: 1920 - 1120 - 16 }
        );
    }

    /// 展开出来之后，鼠标走开一会儿要自动收回去
    #[test]
    fn leaving_the_expanded_window_collapses_it_again() {
        let mut st = State::default();
        let mut clock = Clock::new();
        let win = Rect { x: 16, y: 100, w: 1120, h: 720 };
        st.collapsed = false;
        st.side = Some(Side::Left);
        st.placed = Some((16, 100));
        st.moved = true;

        // 鼠标在窗口里 → 保持展开
        assert_eq!(decide(&mut st, &snap(win, (500, 400), false, clock.tick(120))), Action::None);
        // 走开，但还没走够时间
        assert_eq!(decide(&mut st, &snap(win, (1500, 400), false, clock.tick(120))), Action::None);
        // 走开够久了 → 收回
        assert_eq!(
            decide(&mut st, &snap(win, (1500, 400), false, clock.tick(600))),
            Action::Collapse { side: Side::Left, x: -1120 + 5 }
        );
    }

    /// 鼠标停在屏幕边上不动（也就是停在窗口让开的那条窄边上），窗口要一直待着 ——
    /// 用户正打算用它，这时候自己收回去是最招人烦的。
    #[test]
    fn parking_on_the_edge_keeps_the_window_out() {
        let mut st = State::default();
        let mut clock = Clock::new();
        let win = Rect { x: 16, y: 100, w: 1120, h: 720 };
        st.side = Some(Side::Left);
        st.placed = Some((16, 100));
        st.moved = true;

        for _ in 0..20 {
            assert_eq!(
                decide(&mut st, &snap(win, (3, 400), false, clock.tick(120))),
                Action::None
            );
        }
        assert!(!st.collapsed);
    }

    /// 用户自己把展开出来的窗口拖走了：之后鼠标爱怎么动都别再收它 ——
    /// 窗口已经不在我们放的位置上了，再自作主张就变成了「抢用户的操作」。
    #[test]
    fn a_window_the_user_dragged_away_is_left_alone() {
        let mut st = State::default();
        let mut clock = Clock::new();
        st.side = Some(Side::Left);
        st.placed = Some((16, 100));
        st.moved = true;

        let moved = Rect { x: 500, y: 100, w: 1120, h: 720 };
        assert_eq!(decide(&mut st, &snap(moved, (1500, 400), false, clock.tick(120))), Action::None);
        assert_eq!(st.side, None, "用户自己挪走的窗口不该再挂在那一边");

        for _ in 0..10 {
            assert_eq!(
                decide(&mut st, &snap(moved, (1500, 400), false, clock.tick(120))),
                Action::None
            );
        }
    }

    /// 用户自己把窗口摆成只露一角：那是他要的位置，不许再收一次
    #[test]
    fn a_window_mostly_off_screen_is_not_collapsed() {
        let mut st = State::default();
        let mut clock = Clock::new();
        st.moved = true;
        // 屏幕里只剩 60 像素
        let win = Rect { x: -1060, y: 100, w: 1120, h: 720 };
        assert_eq!(decide(&mut st, &snap(win, (30, 400), false, clock.tick(120))), Action::None);
        assert!(!st.collapsed);
    }

    /// 收起之后位置又被别的东西改了（换分辨率、被窗口管理器挪回来）：
    /// 必须复位，否则「鼠标走开就收回」会静默失效 —— 窗口在屏幕里，
    /// 行为却还像贴在边上，用户完全看不出原因。
    #[test]
    fn a_collapsed_window_that_comes_back_is_reset() {
        let mut st = State::default();
        let mut clock = Clock::new();
        st.collapsed = true;
        st.side = Some(Side::Left);
        st.moved = true;

        let back = Rect { x: 400, y: 100, w: 1120, h: 720 };
        assert_eq!(decide(&mut st, &snap(back, (800, 400), false, clock.tick(120))), Action::None);
        assert!(!st.collapsed, "窗口已经回到屏幕里了，收起状态必须复位");
        assert_eq!(st.side, None);
    }

    /// 一路收起、展开、再收起：状态机每轮都要能干干净净回到起点。
    /// 现实里这就是「用户来回贴了几次边」，任何一次没复位都会变成偶发 bug。
    #[test]
    fn collapse_and_expand_can_alternate() {
        let mut st = State::default();
        let mut clock = Clock::new();
        st.moved = true;

        const TUCKED: i32 = -1120 + 5;
        const OUT: i32 = 16;
        // 用户手抓的位置还落在窗口里，所以窗口左上角照着这个值走
        let mut win = Rect { x: -119, y: 100, w: 1120, h: 720 };

        for round in 0..3 {
            /* ① 用户把窗口从边上拉回屏幕中间。
                  上一轮结尾窗口是收在屏幕外的，这一步同时验证「拉回来要复位」。 */
            win.x = 400;
            assert_eq!(
                decide(&mut st, &snap(win, (800, 400), false, clock.tick(120))),
                Action::None
            );
            assert!(!st.collapsed, "第 {round} 轮拉回屏幕里之后收起状态没复位");

            /* ② 再推到屏幕左边，停稳 → 收进屏幕外 */
            win.x = -119;
            assert_eq!(
                decide(&mut st, &snap(win, (800, 400), false, clock.tick(120))),
                Action::None,
                "第 {round} 轮刚推到边上，得先确认一下不是还在拖"
            );
            assert_eq!(
                decide(&mut st, &snap(win, (800, 400), false, clock.tick(400))),
                Action::Collapse { side: Side::Left, x: TUCKED },
                "第 {round} 轮没收起来"
            );
            win.x = TUCKED;

            /* ③ 松手时鼠标就压在屏幕边上，这时候绝不许弹回来 */
            assert_eq!(
                decide(&mut st, &snap(win, (0, 250), false, clock.tick(120))),
                Action::None,
                "第 {round} 轮刚收起就自己弹了回来"
            );

            /* ④ 鼠标挪开（这一步在布防） */
            assert_eq!(
                decide(&mut st, &snap(win, (600, 400), false, clock.tick(120))),
                Action::None
            );
            assert!(st.armed);

            /* ⑤ 鼠标碰一下屏幕左缘 → 滑出来 */
            assert_eq!(
                decide(&mut st, &snap(win, (2, 400), false, clock.tick(120))),
                Action::Expand { side: Side::Left, x: OUT },
                "第 {round} 轮展不开"
            );
            win.x = OUT;

            /* ⑥ 鼠标在窗口里待着 → 不许收 */
            assert_eq!(
                decide(&mut st, &snap(win, (500, 400), false, clock.tick(120))),
                Action::None
            );

            /* ⑦ 鼠标走开够久 → 收回去 */
            let _ = decide(&mut st, &snap(win, (1500, 400), false, clock.tick(120)));
            assert_eq!(
                decide(&mut st, &snap(win, (1500, 400), false, clock.tick(600))),
                Action::Collapse { side: Side::Left, x: TUCKED },
                "第 {round} 轮收不回去"
            );
            win.x = TUCKED;
        }
    }
}
