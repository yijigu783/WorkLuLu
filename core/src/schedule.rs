//! 周期任务排期引擎
//!
//! 只回答一个问题：按这条重复规则，下一次该在什么时候做。
//! 全部按本地时区计算——「每周五 17:00」在用户脑子里就是墙上的时钟，
//! 换算成 UTC 再算回来只会引入时区偏移的坑。

use crate::model::Rule;
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};

/// 默认提醒时刻：规则没写时间时用它
const DEFAULT_TIME: (u32, u32) = (9, 0);
/// 规则没指定星期时按周五——周报这类最常见
const DEFAULT_WEEKDAY: i64 = 5;
/// 跟界面上的 WEEK 同序：0 = 周日
const WEEKDAY_CN: [&str; 7] = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"];

fn parse_hhmm(s: &str) -> Option<NaiveTime> {
    let mut it = s.trim().split(':');
    let h: u32 = it.next()?.trim().parse().ok()?;
    let m: u32 = it.next().unwrap_or("0").trim().parse().ok()?;
    NaiveTime::from_hms_opt(h, m, 0)
}

fn rule_time(rule: &Rule) -> NaiveTime {
    rule.time
        .as_deref()
        .and_then(parse_hhmm)
        .unwrap_or_else(|| NaiveTime::from_hms_opt(DEFAULT_TIME.0, DEFAULT_TIME.1, 0).expect("常量合法"))
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let first = NaiveDate::from_ymd_opt(year, month, 1).expect("month 已由 Datelike 保证合法");
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .expect("下个月的 1 号一定存在");
    next.signed_duration_since(first).num_days() as u32
}

fn to_local(n: NaiveDateTime) -> Option<DateTime<Local>> {
    Local.from_local_datetime(&n).earliest()
}

/// 规则里指定的「几号」，默认 1 号
fn want_day(rule: &Rule) -> u32 {
    rule.by_day
        .as_ref()
        .and_then(|v| v.first())
        .copied()
        .unwrap_or(1)
        .clamp(1, 31) as u32
}

/// 31 号在小月不存在，夹到当月最后一天（1 月 31 日 → 2 月 28 日），
/// 否则这些月份会被整月跳过
fn clamp_day(want: u32, d: NaiveDate) -> u32 {
    want.min(days_in_month(d.year(), d.month()))
}

/// 季度规则的锚点月，默认 1 月（即 1/4/7/10 月）
fn anchor_month(rule: &Rule) -> i64 {
    rule.by_month
        .as_ref()
        .and_then(|v| v.first())
        .copied()
        .unwrap_or(1)
        .clamp(1, 12)
}

/// 从 `from` 起算的下一次发生时间。
///
/// `inclusive = true` 表示允许取「正好等于 from」的那一刻（新任务排期用），
/// `false` 表示必须严格晚于它（刚做完一次、要算下一次时用）。
pub fn next_occurrence(rule: &Rule, from: NaiveDateTime, inclusive: bool) -> Option<NaiveDateTime> {
    let time = rule_time(rule);
    let hit = |d: NaiveDate| -> Option<NaiveDateTime> {
        let cand = d.and_time(time);
        let ok = if inclusive { cand >= from } else { cand > from };
        ok.then_some(cand)
    };

    match rule.freq.as_str() {
        "daily" => (0..=1).find_map(|off| hit(from.date() + Duration::days(off))),

        "monthly" => {
            let want = want_day(rule);
            (0..=62).find_map(|off| {
                let d = from.date() + Duration::days(off);
                (d.day() == clamp_day(want, d)).then(|| hit(d)).flatten()
            })
        }

        // 每季度：只在锚点月以及它 +3 / +6 / +9 个月的月份上出现。
        // 锚点月由用户选，选 3 月就是 3/6/9/12 月（季末交季报），选 1 月就是年初那张。
        "quarterly" => {
            let want = want_day(rule);
            let anchor = anchor_month(rule);
            // 400 天足够：候选月最多隔 3 个月出现一次，同一天的时刻没过只需往后顺延一轮
            (0..=400).find_map(|off| {
                let d = from.date() + Duration::days(off);
                let step = (d.month() as i64 - anchor).rem_euclid(12);
                (step % 3 == 0 && d.day() == clamp_day(want, d))
                    .then(|| hit(d))
                    .flatten()
            })
        }

        // 默认走 weekly：未指定星期时回落成每周五
        _ => {
            let days = rule
                .by_day
                .clone()
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| vec![DEFAULT_WEEKDAY]);
            (0..=7).find_map(|off| {
                let d = from.date() + Duration::days(off);
                let wd = d.weekday().num_days_from_sunday() as i64;
                days.contains(&wd).then(|| hit(d)).flatten()
            })
        }
    }
}

/// 从某一时刻起（含）的下一次发生时间
pub fn next_from(rule: &Rule, from: DateTime<Local>) -> Option<DateTime<Local>> {
    next_occurrence(rule, from.naive_local(), true).and_then(to_local)
}

/// 严格晚于某一时刻的下一次发生时间
pub fn next_after(rule: &Rule, from: DateTime<Local>) -> Option<DateTime<Local>> {
    next_occurrence(rule, from.naive_local(), false).and_then(to_local)
}

/// 完成或跳过某一次之后，下一次该在什么时候。
///
/// 取「严格晚于原定时间的那一次」与「从此刻起的那一次」中较晚者：
/// - 提前做完（周五的周报周三就交了）→ 本周五那次已完成，指向下周五
/// - 准点做完 → 指向下周同一时刻
/// - 迟到很久才补上 → 直接跳到当前之后的一次，补完旧账不会立刻又显示逾期
/// - 跳过某一次 → 同样越过这一次，规则本身不动
pub fn advance(
    rule: &Rule,
    current_due: Option<DateTime<Local>>,
    now: DateTime<Local>,
) -> Option<DateTime<Local>> {
    let by_due = current_due.and_then(|d| next_after(rule, d));
    let by_now = next_from(rule, now);
    match (by_due, by_now) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}

/// 把重复规则说成人话，给 CSV 导出这类纯文本场景用。
///
/// 文案刻意跟界面上的 `ruleLabel` 对齐：同一件事不该有两种说法。
/// 比如界面上写「每月 15 日」，导出的表格里也得是「每月 15 日」。
/// 传进来的解析不了就返回空串——导出时宁可这格空着，也不要写个 `{freq:...}` 进表格。
pub fn describe_rule(json: &str) -> String {
    let Ok(rule) = serde_json::from_str::<Rule>(json) else {
        return String::new();
    };

    let time = match rule.time.as_deref() {
        Some(t) if !t.trim().is_empty() => format!(" {t}"),
        _ => String::new(),
    };

    // 31 号按「月末」说 —— 排期正是这么算的
    let day_text = || {
        let d = want_day(&rule);
        if d >= 31 {
            "月末".to_string()
        } else {
            format!(" {d} 日")
        }
    };

    match rule.freq.as_str() {
        "daily" => format!("每天{time}"),

        "weekly" => {
            let days = rule
                .by_day
                .clone()
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| vec![DEFAULT_WEEKDAY]);
            let names = days
                .iter()
                .map(|d| WEEKDAY_CN[(*d).rem_euclid(7) as usize])
                .collect::<Vec<_>>()
                .join("、");
            format!("每{names}{time}")
        }

        "monthly" => format!("每月{}{time}", day_text()),

        "quarterly" => {
            // 锚点月往后每 3 个月取一次，跨年要绕回来（锚点 10 月 → 1/4/7/10 月）
            let a = anchor_month(&rule) - 1;
            let mut months: Vec<i64> = (0..4).map(|i| ((a + i * 3) % 12) + 1).collect();
            months.sort_unstable();
            let list = months
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join("/");
            format!("每季度 {list} 月{}{time}", day_text())
        }

        _ => "自定义".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(freq: &str, by_day: Option<Vec<i64>>, time: &str) -> Rule {
        Rule { freq: freq.into(), by_day, by_month: None, time: Some(time.into()) }
    }

    /// 季度规则：锚点月 + 几号 + 时刻
    fn qrule(by_month: i64, by_day: i64, time: &str) -> Rule {
        Rule {
            freq: "quarterly".into(),
            by_day: Some(vec![by_day]),
            by_month: Some(vec![by_month]),
            time: Some(time.into()),
        }
    }

    fn at(y: i32, m: u32, d: u32, h: u32, mi: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(y, m, d, h, mi, 0)
            .single()
            .expect("本地时间唯一")
    }

    #[test]
    fn daily_picks_today_when_time_not_passed() {
        let r = rule("daily", None, "18:00");
        assert_eq!(
            next_from(&r, at(2026, 9, 23, 9, 0)).unwrap(),
            at(2026, 9, 23, 18, 0)
        );
    }

    #[test]
    fn daily_rolls_to_tomorrow_when_time_passed() {
        let r = rule("daily", None, "18:00");
        assert_eq!(
            next_from(&r, at(2026, 9, 23, 19, 0)).unwrap(),
            at(2026, 9, 24, 18, 0)
        );
    }

    /// 2026-09-23 是周三，规则「每周五 17:00」
    #[test]
    fn weekly_from_wednesday_lands_on_friday() {
        let r = rule("weekly", Some(vec![5]), "17:00");
        assert_eq!(
            next_from(&r, at(2026, 9, 23, 10, 0)).unwrap(),
            at(2026, 9, 25, 17, 0)
        );
        // 周五当天 18:00 已过点，顺延到下周
        assert_eq!(
            next_from(&r, at(2026, 9, 25, 18, 0)).unwrap(),
            at(2026, 10, 2, 17, 0)
        );
    }

    #[test]
    fn weekly_supports_multiple_days() {
        let r = rule("weekly", Some(vec![1, 3, 5]), "09:30"); // 一三五
        assert_eq!(
            next_from(&r, at(2026, 9, 23, 10, 0)).unwrap(), // 周三已过 9:30
            at(2026, 9, 25, 9, 30)
        );
    }

    /// 周五的活儿周三就做完了 → 本周五那次已了结，下一次是下周五。
    /// 关键是结果不能停在 9/25（那会让人以为还得再做一遍）。
    #[test]
    fn early_completion_advances_past_the_current_occurrence() {
        let r = rule("weekly", Some(vec![5]), "17:00");
        assert_eq!(
            advance(&r, Some(at(2026, 9, 25, 17, 0)), at(2026, 9, 23, 10, 0)).unwrap(),
            at(2026, 10, 2, 17, 0)
        );
    }

    /// 跳过某一次也一样：越过这一次，规则不动
    #[test]
    fn skip_rolls_to_the_following_occurrence() {
        let r = rule("weekly", Some(vec![5]), "17:00");
        assert_eq!(
            advance(&r, Some(at(2026, 10, 2, 17, 0)), at(2026, 9, 23, 10, 0)).unwrap(),
            at(2026, 10, 9, 17, 0)
        );
    }

    #[test]
    fn advance_skips_stale_backlog() {
        let r = rule("weekly", Some(vec![5]), "17:00");
        // 9 月 25 日那次拖到 10 月 8 日才处理 → 不补旧账，直接指向 10 月 9 日
        assert_eq!(
            advance(&r, Some(at(2026, 9, 25, 17, 0)), at(2026, 10, 8, 12, 0)).unwrap(),
            at(2026, 10, 9, 17, 0)
        );
    }

    /// 每月 31 号在小月夹到最后一天，不能被整月跳过
    #[test]
    fn monthly_clamps_short_months() {
        let r = rule("monthly", Some(vec![31]), "10:00");
        assert_eq!(
            next_from(&r, at(2026, 1, 31, 11, 0)).unwrap(),
            at(2026, 2, 28, 10, 0)
        );
        assert_eq!(
            next_from(&r, at(2026, 2, 28, 11, 0)).unwrap(),
            at(2026, 3, 31, 10, 0)
        );
    }

    #[test]
    fn monthly_keeps_asking_day_for_long_months() {
        let r = rule("monthly", Some(vec![15]), "08:00");
        assert_eq!(
            next_from(&r, at(2026, 9, 20, 0, 0)).unwrap(),
            at(2026, 10, 15, 8, 0)
        );
    }

    /// 锚点 1 月 → 1/4/7/10 月
    #[test]
    fn quarterly_runs_on_anchor_quarter_months() {
        let r = qrule(1, 15, "10:00");
        assert_eq!(
            next_from(&r, at(2026, 9, 23, 10, 0)).unwrap(),
            at(2026, 10, 15, 10, 0)
        );
        // 10 月 15 日当天已过点 → 跨年到次年 1 月 15 日
        assert_eq!(
            next_from(&r, at(2026, 10, 15, 11, 0)).unwrap(),
            at(2027, 1, 15, 10, 0)
        );
    }

    /// 锚点 3 月 → 3/6/9/12 月，季末交季报的典型配置
    #[test]
    fn quarterly_anchor_march_covers_quarter_ends() {
        let r = qrule(3, 25, "17:00");
        assert_eq!(
            next_from(&r, at(2026, 9, 23, 10, 0)).unwrap(),
            at(2026, 9, 25, 17, 0)
        );
        assert_eq!(
            next_from(&r, at(2026, 9, 25, 18, 0)).unwrap(),
            at(2026, 12, 25, 17, 0)
        );
    }

    /// 选 31 号（月末）：季度月里没有 31 号的落到当月最后一天，不能整季跳过
    #[test]
    fn quarterly_clamps_short_months() {
        let r = qrule(3, 31, "09:00");
        assert_eq!(
            next_from(&r, at(2026, 9, 1, 0, 0)).unwrap(),
            at(2026, 9, 30, 9, 0)
        );
        assert_eq!(
            next_from(&r, at(2026, 9, 30, 10, 0)).unwrap(),
            at(2026, 12, 31, 9, 0)
        );
    }

    /// 锚点月不限于 1/4/7/10 这类常规值，照样按「每 3 个月」走
    #[test]
    fn quarterly_anchor_accepts_any_month() {
        let r = qrule(2, 1, "08:00"); // 2/5/8/11 月
        assert_eq!(
            next_from(&r, at(2026, 9, 23, 10, 0)).unwrap(),
            at(2026, 11, 1, 8, 0)
        );
        assert_eq!(
            next_from(&r, at(2026, 11, 1, 9, 0)).unwrap(),
            at(2027, 2, 1, 8, 0)
        );
    }

    /// 前端传过来的 JSON 要能如实还原成规则。字段名对不上会静默丢掉锚点月，
    /// 排期直接算错却没有任何报错——所以这里照着前端的报文盯一遍
    #[test]
    fn rule_parses_frontend_payload_for_quarterly() {
        let r: Rule = serde_json::from_str(r#"{"freq":"quarterly","byDay":[31],"byMonth":[3],"time":"10:00"}"#)
            .expect("前端报文应当能解析");
        assert_eq!(r.freq, "quarterly");
        assert_eq!(r.by_day.as_deref(), Some(&[31][..]));
        assert_eq!(r.by_month.as_deref(), Some(&[3][..]));
    }

    /// 升级前存下的老规则没有 byMonth，不能因此解析失败
    #[test]
    fn rule_parses_legacy_payload_without_by_month() {
        let r: Rule = serde_json::from_str(r#"{"freq":"monthly","byDay":[15],"time":"10:00"}"#)
            .expect("老数据应当能解析");
        assert!(r.by_month.is_none());
        assert_eq!(r.by_day.as_deref(), Some(&[15][..]));
    }

    /// 老数据 / 手填规则里没有锚点月时回落成 1 月，不该崩也不该返回 None
    #[test]
    fn quarterly_defaults_to_january() {
        let r = Rule {
            freq: "quarterly".into(),
            by_day: None,
            by_month: None,
            time: Some("08:00".into()),
        };
        assert_eq!(
            next_from(&r, at(2026, 9, 23, 10, 0)).unwrap(),
            at(2026, 10, 1, 8, 0)
        );
    }

    /// 季度任务提前做完 → 越过这一季，指向下一季而不是原地不动
    #[test]
    fn quarterly_advance_skips_done_quarter() {
        let r = qrule(3, 25, "17:00");
        assert_eq!(
            advance(&r, Some(at(2026, 9, 25, 17, 0)), at(2026, 9, 24, 10, 0)).unwrap(),
            at(2026, 12, 25, 17, 0)
        );
    }

    #[test]
    fn missing_time_falls_back_to_nine() {
        let r = Rule { freq: "daily".into(), by_day: None, by_month: None, time: None };
        assert_eq!(
            next_from(&r, at(2026, 9, 23, 8, 0)).unwrap(),
            at(2026, 9, 23, 9, 0)
        );
    }

    /// 单个数字的小时也要能解析，用户手填 "9:00" 不该失败
    #[test]
    fn accepts_single_digit_hour() {
        assert_eq!(parse_hhmm("9:00"), NaiveTime::from_hms_opt(9, 0, 0));
    }

    /* ---- 规则文案（CSV 导出用，跟界面 ruleLabel 对齐）---- */

    #[test]
    fn describe_covers_every_frequency() {
        assert_eq!(describe_rule(r#"{"freq":"daily","time":"18:00"}"#), "每天 18:00");

        let w = r#"{"freq":"weekly","byDay":[1,3,5],"time":"09:30"}"#;
        assert_eq!(describe_rule(w), "每周一、周三、周五 09:30");

        // 没写星期时跟排期引擎一样回落成周五，不能说成「每」
        assert_eq!(describe_rule(r#"{"freq":"weekly","byDay":[],"time":"08:00"}"#), "每周五 08:00");

        assert_eq!(describe_rule(r#"{"freq":"monthly","byDay":[15],"time":"10:00"}"#), "每月 15 日 10:00");
        // 31 号要说成「月末」，跟排期里夹到当月最后一天的行为对上
        assert_eq!(describe_rule(r#"{"freq":"monthly","byDay":[31],"time":"09:00"}"#), "每月月末 09:00");
    }

    #[test]
    fn describe_quarterly_lists_the_four_months() {
        let r = r#"{"freq":"quarterly","byDay":[25],"byMonth":[3],"time":"17:00"}"#;
        assert_eq!(describe_rule(r), "每季度 3/6/9/12 月 25 日 17:00");
    }

    /// 锚点月靠后时要绕回来并排好序，不能输出 10/13/16/19 这种不存在的月份
    #[test]
    fn describe_quarterly_wraps_past_december() {
        let r = r#"{"freq":"quarterly","byDay":[1],"byMonth":[10],"time":"08:00"}"#;
        assert_eq!(describe_rule(r), "每季度 1/4/7/10 月 1 日 08:00");
    }

    /// 脏数据不该让导出崩，也不能把原始 JSON 漏进表格
    #[test]
    fn describe_survives_broken_json() {
        assert_eq!(describe_rule(""), "");
        assert_eq!(describe_rule("不是 json"), "");
        // 缺 freq 时跟界面 ruleLabel 一样落到「自定义」，而不是瞎猜一个频率
        assert_eq!(describe_rule("{}"), "自定义");
        assert_eq!(describe_rule(r#"{"freq":"unknown"}"#), "自定义");
    }
}
