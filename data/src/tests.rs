//! 数据层的单测。
//!
//! 这些用例原先在桌面端的 `commands.rs` 里。它们测的本来就是数据层的行为
//! （周期任务完成后滚到哪一天、子任务挂在哪一层、模板的偏移怎么算、
//! 附件怎么存），所以跟着代码一起搬过来了。
//!
//! 放这里还有个好处：**安卓端不用真机就能验数据层** ——
//! `cargo test -p worklog-data` 在 Windows 上跑，验的是安卓端将要依赖的
//! 同一份实现。桌面专属的那些（备份、恢复、CSV 导出、文件名清洗）
//! 仍然留在 `app/src-tauri` 里。

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::{Local, NaiveDate, TimeZone};
use rusqlite::{params, Connection};

use crate::attachments::*;
use crate::model::{fetch_task, Task};
use crate::tasks::*;
use crate::templates::*;
use crate::time::parse_dt;

#[cfg(test)]
mod tests {
    use super::*;

    /// 数一张表现在有几行。用例里到处在断言「删干净了没有」。
    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .expect("计数")
    }
    /// 往库里塞一条最简工作。附件类的用例只关心「挂在哪条工作上」。
    fn task(conn: &Connection, title: &str) {
        conn.execute(
            "INSERT INTO tasks (title, note, pattern, status, sort) VALUES (?1, '', 'once', 'todo', 0)",
            params![title],
        )
        .expect("建任务");
    }

    fn db() -> Connection {
            let c = Connection::open_in_memory().expect("内存库");
            c.execute_batch(crate::schema::SCHEMA).expect("建表");
            // 建一个分类：tasks.category_id 有外键约束，测试里要能真的挂上去
            c.execute(
                "INSERT INTO categories (name, color, sort) VALUES ('本职工作', '#4F5BE8', 0)",
                [],
            )
            .expect("建分类");
            c
        }

        /// 造一个阶段性工作（能挂子任务的那种）
        fn stage(conn: &Connection) -> Task {
            conn.execute(
                "INSERT INTO tasks (title, note, category_id, pattern, status, sort)
                 VALUES ('完成官网改版', '', 1, 'stage', 'todo', 0)",
                [],
            )
            .expect("插入阶段性工作");
            fetch_task(conn, conn.last_insert_rowid()).expect("取回")
        }

        /// 造一个「每周五 17:00」的周期任务，到期时间由参数指定
        fn recurring(conn: &Connection, due: &str) -> Task {
            conn.execute(
                r#"INSERT INTO tasks (title, note, pattern, status, due_at, rule, sort)
                   VALUES ('提交项目周报', '', 'recurring', 'todo', ?1,
                           '{"freq":"weekly","byDay":[5],"time":"17:00"}', 0)"#,
                params![due],
            )
            .expect("插入周期任务");
            fetch_task(conn, conn.last_insert_rowid()).expect("取回")
        }

        #[test]
        fn completing_a_recurring_task_records_it_and_rolls_forward() {
            let conn = db();
            let task = recurring(&conn, "2026-09-25T17:00:00+08:00");
            let now = Local.with_ymd_and_hms(2026, 9, 25, 17, 30, 0).unwrap();

            let next = record_occurrence(&conn, &task, now).expect("记一次完成");

            // 准点完成 → 下一次是下周五
            let next_dt = parse_dt(&next).expect("可解析");
            assert_eq!(next_dt.date_naive().to_string(), "2026-10-02");

            // 历史记录里的标题与分类是冗余存的，任务被删掉后仍然可读
            let (title, cat, n): (String, Option<i64>, i64) = (
                conn.query_row("SELECT title FROM completions", [], |r| r.get(0)).unwrap(),
                conn.query_row("SELECT category_id FROM completions", [], |r| r.get(0)).unwrap(),
                conn.query_row("SELECT COUNT(*) FROM completions", [], |r| r.get(0)).unwrap(),
            );
            assert_eq!(title, "提交项目周报");
            assert_eq!(cat, None);
            assert_eq!(n, 1);
        }

        #[test]
        fn early_completion_does_not_stop_the_rhythm() {
            let conn = db();
            let task = recurring(&conn, "2026-09-25T17:00:00+08:00");
            // 周五的活儿周三就交了
            let now = Local.with_ymd_and_hms(2026, 9, 23, 10, 0, 0).unwrap();
            let next = record_occurrence(&conn, &task, now).unwrap();
            assert_eq!(
                parse_dt(&next).unwrap().date_naive().to_string(),
                "2026-10-02",
                "本周五那次已了结，下一次应是下周五"
            );
        }

        #[test]
        fn deleting_the_task_keeps_its_history() {
            let conn = db();
            let task = recurring(&conn, "2026-09-25T17:00:00+08:00");
            record_occurrence(&conn, &task, Local::now()).unwrap();

            conn.execute("DELETE FROM tasks WHERE id = ?1", params![task.id]).unwrap();

            let n: i64 = conn.query_row("SELECT COUNT(*) FROM completions", [], |r| r.get(0)).unwrap();
            let tid: Option<i64> =
                conn.query_row("SELECT task_id FROM completions", [], |r| r.get(0)).unwrap();
            assert_eq!(n, 1, "删掉周期任务不该把历史一起删掉");
            assert_eq!(tid, None, "task_id 置空而不是级联删除");
        }

        #[test]
        fn a_task_without_a_usable_rule_keeps_its_original_time() {
            let conn = db();
            conn.execute(
                r#"INSERT INTO tasks (title, note, pattern, status, due_at, sort)
                   VALUES ('没有规则的周期任务', '', 'recurring', 'todo', '2026-09-25T17:00:00+08:00', 0)"#,
                [],
            )
            .unwrap();
            let task = fetch_task(&conn, 1).unwrap();
            let next = record_occurrence(&conn, &task, Local::now()).unwrap();
            assert_eq!(
                parse_dt(&next).unwrap().date_naive().to_string(),
                "2026-09-25",
                "算不出下一次时应保留原时间，而不是留下空值让任务消失"
            );
        }

        /* ---------------- 子任务 ---------------- */

        #[test]
        fn subtasks_hang_under_their_parent_not_in_the_main_list() {
            let conn = db();
            let parent = stage(&conn);
            insert_subtask(&conn, parent.id, "首页终稿").unwrap();
            insert_subtask(&conn, parent.id, "产品页终稿").unwrap();

            let top = top_level_tasks(&conn).unwrap();
            let subs = subtask_rows(&conn).unwrap();
            assert_eq!(top.len(), 1, "主列表只该看到阶段性工作本身");
            assert_eq!(top[0].id, parent.id);
            assert_eq!(subs.len(), 2, "两个子任务要能取回来");
            assert!(subs.iter().all(|s| s.parent_id == Some(parent.id)));
        }

        #[test]
        fn subtask_inherits_the_parent_category() {
            let conn = db();
            let parent = stage(&conn);
            let sub = insert_subtask(&conn, parent.id, "首页终稿").unwrap();
            assert_eq!(
                sub.category_id,
                Some(1),
                "子任务单独挂分类没意义，应跟随父任务"
            );
        }

        #[test]
        fn deleting_a_stage_task_takes_its_subtasks_with_it() {
            let conn = db();
            let parent = stage(&conn);
            insert_subtask(&conn, parent.id, "首页终稿").unwrap();

            conn.execute("DELETE FROM tasks WHERE id = ?1", params![parent.id]).unwrap();

            let left: i64 = conn
                .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))
                .unwrap();
            assert_eq!(left, 0, "父任务删掉后不该留下无主的子任务");
        }

        #[test]
        fn only_stage_tasks_can_hold_subtasks() {
            let conn = db();
            conn.execute(
                "INSERT INTO tasks (title, note, pattern, status, sort)
                 VALUES ('回一封邮件', '', 'once', 'todo', 0)",
                [],
            )
            .unwrap();
            let err = match insert_subtask(&conn, 1, "拆一步") {
                Err(e) => e,
                Ok(_) => panic!("一次性工作不该能拆子任务"),
            };
            assert!(err.contains("阶段性"), "报错要说清是哪种工作才能拆：{err}");
        }

        #[test]
        fn orphan_subtasks_are_not_handed_to_the_ui() {
            let conn = db();
            let parent = stage(&conn);
            insert_subtask(&conn, parent.id, "首页终稿").unwrap();

            // 模拟绕过外键的改库：留下一条挂不到任何父任务的子任务
            conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
            conn.execute("DELETE FROM tasks WHERE id = ?1", params![parent.id]).unwrap();
            conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();

            assert_eq!(subtask_rows(&conn).unwrap().len(), 0, "孤儿不该出现在界面上");
        }

        #[test]
        fn subtask_title_cannot_be_blank() {
            let conn = db();
            let parent = stage(&conn);
            assert!(insert_subtask(&conn, parent.id, "   ").is_err());
            assert_eq!(subtask_rows(&conn).unwrap().len(), 0);
        }

        #[test]
        fn subtasks_nest_three_levels_but_no_deeper() {
            let conn = db();
            let l1 = stage(&conn);
            let l2 = insert_subtask(&conn, l1.id, "移动端适配").unwrap();
            let l3 = insert_subtask(&conn, l2.id, "窄屏走查").expect("第二层下面还能再拆一层");

            assert_eq!(l3.parent_id, Some(l2.id));
            assert_eq!(subtask_rows(&conn).unwrap().len(), 2);

            // 第三层是底，再往下就该被挡住
            let err = match insert_subtask(&conn, l3.id, "再分一层") {
                Err(e) => e,
                Ok(_) => panic!("第四层不该允许"),
            };
            assert!(err.contains("最多"), "报错要说清是层数到了：{err}");
            assert_eq!(subtask_rows(&conn).unwrap().len(), 2, "被拒的那条不该落库");
        }

        #[test]
        fn a_nested_step_can_hold_children_even_though_it_is_not_stage() {
            let conn = db();
            let l1 = stage(&conn);
            let l2 = insert_subtask(&conn, l1.id, "移动端适配").unwrap();

            // 第二层的步骤本身是 once，但它已经是「一步」了，
            // 给它再分小步是合理的——「只有阶段性才能拆」这条只对顶层成立
            let l3 = insert_subtask(&conn, l2.id, "窄屏走查").unwrap();
            assert_eq!(l3.pattern, "once");
            assert_eq!(l3.category_id, Some(1), "隔了一层也要继承最上面那个分类");
        }

        #[test]
        fn depth_guard_survives_a_cycle_in_the_data() {
            let conn = db();
            let a = stage(&conn);
            let b = insert_subtask(&conn, a.id, "第一步").unwrap();
            // 人为造一个环：把父任务挂到它自己的子任务下面。
            // 外部工具改库可能出现这种数据，遍历必须能自己停下来
            conn.execute("UPDATE tasks SET parent_id = ?1 WHERE id = ?2", params![b.id, a.id])
                .unwrap();
            assert!(depth_of(&conn, b.id).is_err(), "有环时必须报错，而不是转不出来");
        }

        /* ---------------- 复制一份 ---------------- */

        /// 把某个任务的开始 / 交期固定下来，方便对着算天数
        fn set_span(conn: &Connection, id: i64, due: Option<&str>, end: Option<&str>) {
            conn.execute(
                "UPDATE tasks SET due_at = ?1, end_at = ?2 WHERE id = ?3",
                params![due, end, id],
            )
            .expect("设置日期");
        }

        #[test]
        fn duplicating_a_task_brings_its_steps_and_shifts_the_dates() {
            let conn = db();
            let parent = stage(&conn);
            set_span(&conn, parent.id, Some("2026-10-01T09:00:00+08:00"), Some("2026-10-10T23:59:00+08:00"));
            insert_subtask(&conn, parent.id, "第一步").unwrap();
            insert_subtask(&conn, parent.id, "第二步").unwrap();

            let copy = duplicate_task_inner(&conn, parent.id, Some("2026-11-01")).expect("复制");

            assert_ne!(copy.id, parent.id);
            assert!(copy.title.ends_with("（副本）"), "顶层要一眼看出是复制来的：{}", copy.title);
            assert_eq!(copy.status, "todo", "副本从没做过开始");

            // 整体平移：10/1 → 11/1 是 +31 天，内部跨度（9 天）不变
            let due = parse_dt(copy.due_at.as_deref().unwrap()).unwrap();
            let end = parse_dt(copy.end_at.as_deref().unwrap()).unwrap();
            assert_eq!(due.date_naive(), NaiveDate::from_ymd_opt(2026, 11, 1).unwrap());
            assert_eq!(due.time().to_string(), "09:00:00", "时分不该被平移改掉");
            assert_eq!((end.date_naive() - due.date_naive()).num_days(), 9, "跨度不能被改");

            let kids: Vec<Task> = subtask_rows(&conn)
                .unwrap()
                .into_iter()
                .filter(|s| s.parent_id == Some(copy.id))
                .collect();
            assert_eq!(kids.len(), 2, "子任务要跟着复制");
            assert!(
                kids.iter().all(|k| !k.title.contains("副本")),
                "只有顶层加「副本」字样，否则一棵树上全是这几个字"
            );
        }

        #[test]
        fn duplicating_keeps_the_whole_nested_tree() {
            let conn = db();
            let l1 = stage(&conn);
            let l2 = insert_subtask(&conn, l1.id, "移动端适配").unwrap();
            insert_subtask(&conn, l2.id, "窄屏走查").unwrap();

            let copy = duplicate_task_inner(&conn, l1.id, None).expect("复制");
            let all = subtask_rows(&conn).unwrap();
            let mine: Vec<&Task> = all.iter().filter(|s| s.parent_id == Some(copy.id)).collect();
            assert_eq!(mine.len(), 1, "第二层要在");
            let grand: Vec<&Task> =
                all.iter().filter(|s| s.parent_id == Some(mine[0].id)).collect();
            assert_eq!(grand.len(), 1, "第三层也要跟着过来，否则复制出来的是棵断树");
        }

        #[test]
        fn duplicating_without_a_base_keeps_the_original_dates() {
            let conn = db();
            let parent = stage(&conn);
            set_span(&conn, parent.id, Some("2026-10-01T09:00:00+08:00"), None);

            let copy = duplicate_task_inner(&conn, parent.id, None).expect("复制");
            // 取回最新的一条再比：parent 是设日期之前拿的快照，此时它的 due_at 还是空的
            let fresh = fetch_task(&conn, parent.id).unwrap();
            assert_eq!(copy.due_at, fresh.due_at, "没给基准日就原地复制，日期不动");
        }

        #[test]
        fn duplicating_a_recurring_task_does_not_copy_a_stale_date() {
            let conn = db();
            let task = recurring(&conn, "2020-01-03T17:00:00+08:00");   // 一个早就过去的周五

            let copy = duplicate_task_inner(&conn, task.id, None).expect("复制");
            let due = parse_dt(copy.due_at.as_deref().unwrap()).unwrap();
            assert!(
                due > Local.with_ymd_and_hms(2020, 6, 1, 0, 0, 0).unwrap(),
                "周期任务照搬旧时间的话，复制出来一出生就是逾期的：{due}"
            );
        }

        /* ---------------- 模板 ---------------- */

        #[test]
        fn a_template_stores_offsets_not_absolute_dates() {
            let conn = db();
            let parent = stage(&conn);
            set_span(&conn, parent.id, Some("2026-10-01T09:00:00+08:00"), Some("2026-10-05T23:59:00+08:00"));
            let sub = insert_subtask(&conn, parent.id, "第一步").unwrap();
            set_span(&conn, sub.id, Some("2026-10-02T18:00:00+08:00"), None);

            let tpl = template_from_task(&conn, parent.id, "季度复盘").expect("存模板");
            assert_eq!(tpl.items.len(), 2);
            assert_eq!(tpl.name, "季度复盘");

            let root = tpl.items.iter().find(|i| i.parent_id.is_none()).unwrap();
            assert_eq!(root.due_offset, Some(0), "根任务的开始日就是基准日");
            assert_eq!(root.end_offset, Some(4));
            assert_eq!(root.due_time.as_deref(), Some("09:00"), "时分要单独存，光留天数不够");
            assert_eq!(root.pattern, "stage");

            let kid = tpl.items.iter().find(|i| i.parent_id.is_some()).unwrap();
            assert_eq!(kid.due_offset, Some(1), "第二步相对基准日是第 1 天");
            assert_eq!(kid.due_time.as_deref(), Some("18:00"), "第二层的时间也要留住");

            // 库里绝不能出现绝对日期：存了「10 月 8 日截止」，下个月调用就过期了
            let mut stmt = conn.prepare("PRAGMA table_info(template_items)").unwrap();
            let cols: Vec<String> = stmt
                .query_map([], |r| r.get::<_, String>(1))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();
            assert!(
                !cols.iter().any(|c| c == "due_at" || c == "end_at"),
                "模板表里不该有绝对日期字段：{cols:?}"
            );
        }

        #[test]
        fn applying_a_template_lays_the_tree_onto_the_base_date() {
            let conn = db();
            let parent = stage(&conn);
            set_span(&conn, parent.id, Some("2026-10-01T09:00:00+08:00"), None);
            let sub = insert_subtask(&conn, parent.id, "第一步").unwrap();
            set_span(&conn, sub.id, Some("2026-10-03T15:00:00+08:00"), None);
            let l3 = insert_subtask(&conn, sub.id, "再分一小步").unwrap();
            set_span(&conn, l3.id, None, None);

            let tpl = template_from_task(&conn, parent.id, "季度复盘").unwrap();
            // 原件清干净，只留模板——这样测的才是模板本身
            conn.execute("DELETE FROM tasks", []).unwrap();

            let top = apply_template_inner(&conn, tpl.id, "2026-12-01").expect("应用模板");
            assert_eq!(top, 1, "模板里只有 1 件顶层工作");

            let all = subtask_rows(&conn).unwrap();
            assert_eq!(all.len(), 2, "两层步骤都要建出来");

            let root = &top_level_tasks(&conn).unwrap()[0];
            let due = parse_dt(root.due_at.as_deref().unwrap()).unwrap();
            assert_eq!(due.date_naive(), NaiveDate::from_ymd_opt(2026, 12, 1).unwrap());
            assert_eq!(due.time().to_string(), "09:00:00", "时分从模板里还原");

            let step2 = all.iter().find(|s| s.title == "第一步").unwrap();
            let d2 = parse_dt(step2.due_at.as_deref().unwrap()).unwrap();
            assert_eq!(d2.date_naive(), NaiveDate::from_ymd_opt(2026, 12, 3).unwrap(), "第 2 天");
            assert_eq!(d2.time().to_string(), "15:00:00");

            let deep = all.iter().find(|s| s.title == "再分一小步").unwrap();
            assert!(deep.due_at.is_none(), "模板里没有日期的步骤，建出来也不该凭空多一个日期");
            assert_eq!(deep.parent_id, Some(step2.id), "第三层要挂在第二层下面");
        }

        #[test]
        fn a_template_keeps_working_after_its_category_is_deleted() {
            let conn = db();
            let parent = stage(&conn);   // 挂在分类 1 上
            let tpl = template_from_task(&conn, parent.id, "带分类的模板").unwrap();
            conn.execute("DELETE FROM categories WHERE id = 1", []).unwrap();

            let top = apply_template_inner(&conn, tpl.id, "2026-12-01").expect("分类没了也要能建");
            assert_eq!(top, 1);
            assert_eq!(
                top_level_tasks(&conn).unwrap()[0].category_id,
                None,
                "分类没了就落到「未分类」，不能为这个拦下整批"
            );
        }

        #[test]
        fn a_template_needs_a_name_and_some_content() {
            let conn = db();
            let parent = stage(&conn);
            assert!(template_from_task(&conn, parent.id, "   ").is_err());
            assert_eq!(all_templates(&conn).unwrap().len(), 0, "没名字的模板不该留下");

            // 空模板（删掉条目后）应用时要给出说得清的话，而不是默默建出零条
            let tpl = template_from_task(&conn, parent.id, "空壳").unwrap();
            conn.execute("DELETE FROM template_items WHERE template_id = ?1", params![tpl.id])
                .unwrap();
            assert!(apply_template_inner(&conn, tpl.id, "2026-12-01").is_err());
        }

        #[test]
        fn deleting_a_template_leaves_its_tasks_alone() {
            let conn = db();
            let parent = stage(&conn);
            let tpl = template_from_task(&conn, parent.id, "随手存的").unwrap();
            apply_template_inner(&conn, tpl.id, "2026-12-01").unwrap();

            let before = top_level_tasks(&conn).unwrap().len();
            conn.execute("DELETE FROM templates WHERE id = ?1", params![tpl.id]).unwrap();

            assert_eq!(top_level_tasks(&conn).unwrap().len(), before, "删模板不能连累已建出来的工作");
            assert_eq!(all_templates(&conn).unwrap().len(), 0);
            // 条目靠外键级联清掉，不留垃圾
            let left: i64 = conn
                .query_row("SELECT COUNT(*) FROM template_items", [], |r| r.get(0))
                .unwrap();
            assert_eq!(left, 0, "模板删了，它的条目也该跟着走");
        }

        /* ---- 备份 / 恢复 ---- */

        /// 每个测试用独立的临时目录，避免并行跑的时候互相踩
        fn b64(bytes: &[u8]) -> String {
            STANDARD.encode(bytes)
        }

        /// 存进去 → 列出来 → 取内容，三处口径必须一致
        #[test]
        fn attachment_round_trips_through_the_database() {
            let conn = db();
            task(&conn, "和甲方的沟通留痕");
            let tid: i64 = conn.last_insert_rowid();

            let saved = insert_attachment(
                &conn,
                tid,
                "聊天记录.png",
                "image/png",
                "image",
                &b64(b"PNGDATA"),
                Some(&b64(b"THUMB")),
            )
            .expect("存附件");

            assert_eq!(saved.task_id, tid);
            assert_eq!(saved.kind, "image");
            assert_eq!(saved.size, 7);
            assert!(saved.has_thumb);

            let list = list_attachments_of(&conn, Some(tid)).unwrap();
            assert_eq!(list.len(), 1);
            assert_eq!(list[0].name, "聊天记录.png");

            // 原图与缩略图各取各的
            assert_eq!(attachment_data_url(&conn, saved.id, false).unwrap(), "data:image/png;base64,UE5HREFUQQ==");
            assert_eq!(attachment_data_url(&conn, saved.id, true).unwrap(), "data:image/jpeg;base64,VEhVTUI=");

            remove_attachment(&conn, saved.id).unwrap();
            assert!(list_attachments_of(&conn, Some(tid)).unwrap().is_empty());
        }

        /// 列表里**不能**带图片本体 —— 带上就意味着抽屉一打开要搬几十 MB
        #[test]
        fn attachment_metadata_is_selectable_without_the_body() {
            let conn = db();
            task(&conn, "带图的工作");
            let tid: i64 = conn.last_insert_rowid();
            // 非图片没有缩略图，has_thumb 要是 false，列表才不会去取不存在的小图
            insert_attachment(&conn, tid, "合同.pdf", "application/pdf", "file", &b64(b"PDF"), None).unwrap();

            let list = list_attachments_of(&conn, None).unwrap();
            assert_eq!(list.len(), 1);
            assert!(!list[0].has_thumb);
            assert_eq!(list[0].kind, "file");
            // 没有缩略图时退回原图，而不是返回空
            assert_eq!(
                attachment_data_url(&conn, list[0].id, true).unwrap(),
                "data:application/pdf;base64,UERG"
            );
        }

        /// 只取某一条工作的附件，别的不能混进来
        #[test]
        fn attachments_are_filtered_by_task() {
            let conn = db();
            task(&conn, "甲");
            let a: i64 = conn.last_insert_rowid();
            task(&conn, "乙");
            let b: i64 = conn.last_insert_rowid();

            insert_attachment(&conn, a, "a.png", "image/png", "image", &b64(b"A"), None).unwrap();
            insert_attachment(&conn, b, "b.png", "image/png", "image", &b64(b"B"), None).unwrap();

            assert_eq!(list_attachments_of(&conn, Some(a)).unwrap().len(), 1);
            assert_eq!(list_attachments_of(&conn, Some(b)).unwrap()[0].name, "b.png");
            assert_eq!(list_attachments_of(&conn, None).unwrap().len(), 2);
        }

        /// 附件是这条工作的证据，工作删了证据跟着走（外键 CASCADE）
        #[test]
        fn deleting_the_task_takes_its_attachments_with_it() {
            let conn = db();
            task(&conn, "要被删掉的工作");
            let tid: i64 = conn.last_insert_rowid();
            insert_attachment(&conn, tid, "截图.png", "image/png", "image", &b64(b"X"), None).unwrap();

            conn.execute("DELETE FROM tasks WHERE id = ?1", params![tid]).unwrap();

            assert_eq!(count(&conn, "attachments"), 0, "工作没了，挂在它上面的附件不该留下来");
        }

        /// 脏数据不能进库：坏 base64、空文件、超限的都要拦下来
        #[test]
        fn attachment_rejects_bad_input() {
            let conn = db();
            task(&conn, "目标");
            let tid: i64 = conn.last_insert_rowid();

            assert!(insert_attachment(&conn, tid, "x", "image/png", "image", "这不是 base64!!", None).is_err());
            assert!(insert_attachment(&conn, tid, "x", "image/png", "image", "", None).is_err());

            let huge = vec![0u8; MAX_ATTACHMENT_BYTES + 1];
            let err = insert_attachment(&conn, tid, "巨无霸", "image/png", "image", &b64(&huge), None).unwrap_err();
            assert!(err.contains("MB"), "超限要给人话，不是 SQL 报错：{err}");

            // 上面全部失败之后库里不该留下任何一条
            assert_eq!(count(&conn, "attachments"), 0);
        }

        /// 挂到不存在的工作上要拦住，并给一句人话（外键报的是 SQL 错误，用户看不懂）
        #[test]
        fn attachment_needs_an_existing_task() {
            let conn = db();
            let err = insert_attachment(&conn, 999, "x", "image/png", "image", &b64(b"X"), None).unwrap_err();
            assert_eq!(err, "这条工作已经不在了");
        }

        /// 缩略图坏了不该让整条附件存不进去 —— 顶多列表多取一次原图
        #[test]
        fn a_broken_thumbnail_does_not_lose_the_attachment() {
            let conn = db();
            task(&conn, "目标");
            let tid: i64 = conn.last_insert_rowid();

            let saved =
                insert_attachment(&conn, tid, "x.png", "image/png", "image", &b64(b"FULL"), Some("坏缩略图"))
                    .expect("仍应存进去");
            assert!(!saved.has_thumb);
            assert_eq!(
                attachment_data_url(&conn, saved.id, true).unwrap(),
                "data:image/png;base64,RlVMTA=="
            );
        }

}
