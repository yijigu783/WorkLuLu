/* 工作记录本 — 前端逻辑 */

const PATTERNS = {
  once:      { label: '一次性', bar: 'p-once',      chip: 'chip-once',      color: '#F59E0B' },
  recurring: { label: '周期性', bar: 'p-recurring', chip: 'chip-recurring', color: '#3B82F6' },
  stage:     { label: '阶段性', bar: 'p-stage',     chip: 'chip-stage',     color: '#8B5CF6' },
};

const PALETTE = ['#4F5BE8','#3B82F6','#0EA5E9','#10B981','#84CC16','#F59E0B',
                 '#F97316','#EF4444','#EC4899','#8B5CF6','#6366F1','#64748B'];

const WEEK = ['周日','周一','周二','周三','周四','周五','周六'];

/** 重复频率。`byDay` 的含义随频率而变（见 src-tauri/src/schedule.rs 的说明）：
 *  每周 = 星期几；每月 / 每季度 = 几号。 */
const FREQS = [
  { key: 'daily',     label: '每天' },
  { key: 'weekly',    label: '每周' },
  { key: 'monthly',   label: '每月' },
  { key: 'quarterly', label: '每季度' },
];

/** 各频率的频率说明，写在选择器下面 */
const FREQ_HINT = {
  daily:     '每天到点提醒一次',
  weekly:    '选好星期几，可以多选（比如每周一、三、五）',
  monthly:   '选每月几号；选 31 号表示月末，小月自动落到当月最后一天',
  quarterly: '每 3 个月一次：先选从哪个月起算，再选几号',
};

/** 星期几的展示顺序：周一打头，符合工作日习惯 */
const WEEK_ORDER = [1, 2, 3, 4, 5, 6, 0];
const MONTHS_OF_YEAR = Array.from({ length: 12 }, (_, i) => i + 1);
const DAYS_OF_MONTH  = Array.from({ length: 31 }, (_, i) => i + 1);

/** 切到某个频率时，给 byDay 一个合理的默认值。
 *  已经在用的值如果对新频率也说得通就留着，避免手一抖把选好的日子清掉。 */
function defaultRuleFor(freq, old) {
  const o = old || {};
  const day = o.byDay?.[0];
  if (freq === 'weekly') {
    const days = (o.byDay || []).filter(i => i >= 0 && i <= 6);
    return { byDay: days.length ? days : [5] };
  }
  if (freq === 'monthly')   return { byDay: [day >= 1 && day <= 31 ? day : 1] };
  if (freq === 'quarterly') {
    const m = o.byMonth?.[0];
    return {
      byDay: [day >= 1 && day <= 31 ? day : 31],
      byMonth: [m >= 1 && m <= 12 ? m : 3],
    };
  }
  return {};
}

/** 季度规则命中的四个月份，如锚点 3 月 → [3, 6, 9, 12] */
function quarterMonths(rule) {
  const a = (Number(rule?.byMonth?.[0]) || 1) - 1;
  return [0, 1, 2, 3].map(i => ((a + i * 3) % 12) + 1).sort((x, y) => x - y);
}

/** 「几号」的人话说法。31 号按月末理解——排期正是这么算的。
 *  数字前留一个空格，拼进「每月…」正好读作「每月 15 日」「每月月末」 */
const dayText = rule => {
  const d = Number(rule?.byDay?.[0]) || 1;
  return d >= 31 ? '月末' : ` ${d} 日`;
};

/* ---------------- 日期工具 ---------------- */
const DAY = 86400000;
const startOfDay = d => { const x = new Date(d); x.setHours(0,0,0,0); return x; };
const dayDiff = (a, b) => Math.round((startOfDay(a) - startOfDay(b)) / DAY);

function relLabel(iso) {
  if (!iso) return '';
  const d = new Date(iso), diff = dayDiff(d, new Date());
  if (diff === 0) return '今天';
  if (diff === 1) return '明天';
  if (diff === -1) return '昨天';
  if (diff > 1 && diff < 7) return WEEK[d.getDay()];
  return `${d.getMonth() + 1}月${d.getDate()}日`;
}
function timeLabel(iso) {
  if (!iso) return '';
  const d = new Date(iso);
  const h = String(d.getHours()).padStart(2,'0'), m = String(d.getMinutes()).padStart(2,'0');
  return (h === '00' && m === '00') ? '' : `${h}:${m}`;
}
function toLocalDate(iso) {
  if (!iso) return '';
  const d = new Date(iso);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}
function toLocalTime(iso, fallback) {
  if (!iso) return fallback || '';
  const d = new Date(iso);
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
}
function todayLocal() { return toLocalDate(new Date()); }
function ruleLabel(t) {
  const r = t.rule; if (!r) return '未设置规则';
  const time = r.time ? ` ${r.time}` : '';
  if (r.freq === 'daily')  return `每天${time}`;
  if (r.freq === 'weekly') {
    const days = (r.byDay || []).length ? r.byDay : [5];
    return `每${days.map(i => WEEK[i]).join('、')}${time}`;
  }
  if (r.freq === 'monthly')   return `每月${dayText(r)}${time}`;
  if (r.freq === 'quarterly') return `每季度 ${quarterMonths(r).join('/')} 月${dayText(r)}${time}`;
  return '自定义';
}
function metaLabel(t, cat) {
  const catName = esc(cat ? cat.name : '未分类');
  if (t.pattern === 'recurring') return `${catName} · ${ruleLabel(t)}`;
  if (t.pattern === 'stage') {
    const pr = t.progress || {};
    const bits = [catName, pr.total ? `子任务 ${pr.done}/${pr.total}` : '还没拆步骤'];
    if (t.dueAt && t.endAt) bits.push(`${relLabel(t.dueAt)} – ${relLabel(t.endAt)}`);
    else if (t.endAt)       bits.push(`截止 ${relLabel(t.endAt)}`);
    else if (t.dueAt)       bits.push(`起 ${relLabel(t.dueAt)}`);
    return bits.join(' · ');
  }
  const d = t.dueAt ? new Date(t.dueAt) : null;
  if (!d) return `${catName} · 未设置时间`;
  const diff = dayDiff(d, new Date());
  const t2 = timeLabel(t.dueAt);
  let when = relLabel(t.dueAt) + (t2 ? ` ${t2}` : '');
  let suffix = '截止';
  if (t.status !== 'done' && diff < 0) suffix = `逾期 ${-diff} 天`;
  return `${catName} · ${when} ${suffix}`;
}

/* ---------------- 状态 ---------------- */
const state = {
  view: 'today',
  mode: 'list',      // 排布方式：list 列表 / board 看板 / calendar 日历
  calMonth: null,    // 日历正翻到哪个月（该月 1 号）；null = 跟着今天走
  categories: [],
  tasks: [],
  subtasks: [],      // 阶段性工作拆出来的步骤，不进主列表
  completions: [],   // 周期任务的完成记录
  settings: {},
  dataPath: '',
  q: '',
  drawerId: null,
  drawerRendered: null,  // 抽屉此刻渲染的是哪个任务：切走前要先收下没保存的输入
  editing: null,
  catEditing: null,
};

/* ---------------- API 层（Tauri / 浏览器预览双通道） ---------------- */
const hasTauri = typeof window !== 'undefined' && !!window.__TAURI__;
const inv = (cmd, args) => {
  if (hasTauri && window.__TAURI__.core) return window.__TAURI__.core.invoke(cmd, args);
  return Promise.reject(new Error('no tauri'));
};

/* 浏览器预览用示例数据 */
function mockData() {
  const now = new Date();
  const at = (dOffset, h, m) => {
    const d = new Date(now); d.setDate(d.getDate() + dOffset); d.setHours(h, m || 0, 0, 0); return d.toISOString();
  };
  return {
    categories: [
      { id: 1, name: '本职工作', color: '#4F5BE8', sort: 0 },
      { id: 2, name: '副业',     color: '#F59E0B', sort: 1 },
      { id: 3, name: '学习提升', color: '#10B981', sort: 2 },
      { id: 4, name: '生活',     color: '#EC4899', sort: 3 },
    ],
    tasks: [
      { id: 1, title: '整理季度复盘材料', categoryId: 1, pattern: 'once', status: 'todo',
        dueAt: at(0, 18), remindAt: at(0, 17), note: '把 7-9 月的数据和结论汇总成一页纸。' },
      { id: 2, title: '完成官网改版视觉终稿', categoryId: 1, pattern: 'stage', status: 'todo',
        dueAt: at(0, 9), endAt: at(11, 23), progress: { done: 3, total: 5 },
        note: '首页 / 产品页 / 关于页 三张终稿。' },
      { id: 3, title: '回复供应商报价邮件', categoryId: 1, pattern: 'once', status: 'todo',
        dueAt: at(-1, 12), note: '对比三家报价后给结论。' },
      { id: 4, title: '提交项目周报', categoryId: 1, pattern: 'recurring', status: 'todo',
        rule: { freq: 'weekly', byDay: [5], time: '17:00' }, dueAt: at(2, 17), remindAt: at(2, 16) },
      { id: 5, title: '月度费用对账', categoryId: 2, pattern: 'recurring', status: 'todo',
        rule: { freq: 'monthly', byDay: [1], time: '10:00' }, dueAt: at(8, 10) },
      { id: 6, title: '读完《设计心理学》第 3 章', categoryId: 3, pattern: 'once', status: 'todo',
        dueAt: at(1, 21) },
      { id: 7, title: '预约牙医', categoryId: 4, pattern: 'once', status: 'todo', dueAt: at(5, 9) },
      { id: 8, title: '整理会议纪要并发群', categoryId: 1, pattern: 'once', status: 'done',
        dueAt: at(-2, 18), completedAt: at(-2, 17) },
      { id: 9, title: '每周晨会同步', categoryId: 1, pattern: 'recurring', status: 'todo',
        rule: { freq: 'weekly', byDay: [1], time: '09:30' }, dueAt: at(5, 9) },
      { id: 10, title: '整理 8 月报销单', categoryId: 1, pattern: 'once', status: 'done',
        dueAt: at(-16, 18), completedAt: at(-16, 17) },
      { id: 11, title: '读完《影响力》前两章', categoryId: 3, pattern: 'once', status: 'done',
        dueAt: at(-9, 21), completedAt: at(-10, 22) },   // 提前完成
      { id: 12, title: '给客户回访电话', categoryId: 1, pattern: 'once', status: 'done',
        dueAt: at(-30, 12), completedAt: at(-29, 10) },  // 迟了一天
      // 季度规则：锚点 3 月 → 3/6/9/12 月，25 号交材料
      { id: 13, title: '季度复盘会材料', categoryId: 1, pattern: 'recurring', status: 'todo',
        rule: { freq: 'quarterly', byDay: [25], byMonth: [3], time: '15:00' }, dueAt: at(9, 15) },
    ],
    // 阶段性工作拆出来的步骤。父任务那条的进度就由它们算
    subtasks: [
      { id: 101, parentId: 2, title: '首页终稿',           status: 'done', pattern: 'once', note: '' },
      { id: 102, parentId: 2, title: '产品页终稿',         status: 'done', pattern: 'once', note: '' },
      { id: 103, parentId: 2, title: '关于页终稿',         status: 'done', pattern: 'once', note: '' },
      { id: 104, parentId: 2, title: '移动端适配',         status: 'todo', pattern: 'once', note: '' },
      { id: 105, parentId: 2, title: '交付设计稿源文件',   status: 'todo', pattern: 'once', note: '' },
    ],
    // 半年的周期完成记录，让热力图和趋势图在预览里有东西可看
    completions: (() => {
      const src = [
        { t: '提交项目周报',   taskId: 4, c: 1, h: 17 },   // 周五
        { t: '每周晨会同步',   taskId: 9, c: 1, h: 9  },   // 周一
        { t: '月度费用对账',   taskId: 5, c: 2, h: 10 },
        { t: '健身打卡',       taskId: 0, c: 4, h: 20 },
      ];
      const out = [];
      let id = 0;
      for (let i = 180; i >= 0; i--) {
        const d = new Date(now); d.setDate(d.getDate() - i);
        const dow = d.getDay();
        // 周五交周报、周一晨会、周三健身，偶尔冒出一件杂事
        const pick = dow === 5 ? src[0] : dow === 1 ? src[1] : dow === 3 ? src[3] : (i % 17 === 0 ? src[2] : null);
        if (!pick) continue;
        const late = (i % 13 === 0 || i % 29 === 0) && dow !== 3;   // 偶尔拖一天
        const done = new Date(d);
        done.setDate(done.getDate() + (late ? 1 : 0));
        done.setHours(pick.h + (i % 3), (i * 7) % 60, 0, 0);
        const due = new Date(d); due.setHours(pick.h, 0, 0, 0);
        out.push({
          id: ++id, taskId: pick.taskId || null, title: pick.t,
          categoryId: pick.c, pattern: 'recurring',
          dueAt: due.toISOString(), doneAt: done.toISOString(),
        });
      }
      // 与后端 list_completions 保持一致：按完成时间倒序
      return out.sort((a, b) => new Date(b.doneAt) - new Date(a.doneAt));
    })(),
  };
}

async function loadAll() {
  if (hasTauri) {
    try {
      const [categories, tasks, settings, dataPath, completions, subtasks] = await Promise.all([
        inv('list_categories'), inv('list_tasks'), inv('get_settings'),
        inv('data_dir'), inv('list_completions', { limit: 1000 }), inv('list_subtasks'),
      ]);
      state.categories = categories;
      state.tasks = tasks;
      state.settings = settings || {};
      state.dataPath = dataPath;
      state.completions = completions || [];
      state.subtasks = subtasks || [];
      applySubtaskProgress();
      return;
    } catch (e) { console.warn('后端调用失败，启用预览数据', e); }
  }
  const m = mockData();
  state.categories = m.categories;
  state.tasks = m.tasks;
  state.completions = m.completions;
  state.subtasks = m.subtasks || [];
  state.settings = {};
  state.dataPath = 'C:\\Users\\<你>\\AppData\\Roaming\\工作记录本';
  applySubtaskProgress();
}

const subtasksOf = parentId => state.subtasks.filter(s => s.parentId === parentId);

/** 阶段性工作的进度由子任务算出来，不给人手填的机会——少一个能填错的地方。
 *  没有子任务时就是 0/0，界面显示「未拆解」。 */
function applySubtaskProgress() {
  state.tasks.forEach(t => {
    if (t.pattern !== 'stage') return;
    const kids = subtasksOf(t.id);
    t.progress = { done: kids.filter(s => s.status === 'done').length, total: kids.length };
  });
}

/** 周期完成记录单独取，撤销/完成一次之后只刷这一块 */
async function refreshCompletions() {
  if (!hasTauri) return;
  try { state.completions = await inv('list_completions', { limit: 1000 }); }
  catch (e) { console.warn(e); }
}

/* ---------------- 排期（浏览器预览用，规则与后端 schedule.rs 一致） ---------------- */
function nextOccurrence(rule, iso, inclusive) {
  if (!rule) return iso || null;
  const [hh, mm] = String(rule.time || '09:00').split(':').map(Number);
  const from = iso ? new Date(iso) : new Date();
  const hit = d => {
    const c = new Date(d); c.setHours(hh, mm || 0, 0, 0);
    return (inclusive ? c >= from : c > from) ? c : null;
  };
  const dayAt = off => {
    const d = new Date(from); d.setDate(d.getDate() + off); d.setHours(0, 0, 0, 0); return d;
  };

  // 31 号在小月不存在，夹到当月最后一天——与后端 clamp_day 同一口径
  const lastDayOf = d => new Date(d.getFullYear(), d.getMonth() + 1, 0).getDate();

  if (rule.freq === 'daily') {
    for (let i = 0; i <= 1; i++) { const c = hit(dayAt(i)); if (c) return c.toISOString(); }
  } else if (rule.freq === 'monthly' || rule.freq === 'quarterly') {
    const want = Number(rule.byDay?.[0]) || 1;
    // 季度：只在锚点月以及它 +3 / +6 / +9 的月份上命中
    const anchor = ((((Number(rule.byMonth?.[0]) || 1) - 1) % 12) + 12) % 12 + 1;
    const span = rule.freq === 'quarterly' ? 400 : 62;
    for (let i = 0; i <= span; i++) {
      const d = dayAt(i);
      if (rule.freq === 'quarterly') {
        const step = ((((d.getMonth() + 1) - anchor) % 12) + 12) % 12;
        if (step % 3 !== 0) continue;
      }
      if (d.getDate() === Math.min(want, lastDayOf(d))) { const c = hit(d); if (c) return c.toISOString(); }
    }
  } else {
    const days = rule.byDay?.length ? rule.byDay : [5];
    for (let i = 0; i <= 7; i++) {
      const d = dayAt(i);
      if (days.includes(d.getDay())) { const c = hit(d); if (c) return c.toISOString(); }
    }
  }
  return iso || null;
}

/** 完成/跳过某一次之后的下一次：取「晚于原定」与「从此刻起」中较晚者 */
function advanceOccurrence(rule, dueIso) {
  const byDue = nextOccurrence(rule, dueIso, false);
  const byNow = nextOccurrence(rule, null, true);
  if (!byDue) return byNow;
  if (!byNow) return byDue;
  return new Date(byDue) > new Date(byNow) ? byDue : byNow;
}

/* ---------------- 查询 ---------------- */
const catById = id => state.categories.find(c => c.id === id);

/** 「这件工作什么时候算到期」。
 *  阶段性工作是一段跨度，dueAt 是开始日、endAt 才是交期——
 *  拿开始日判逾期会把刚起步的大任务一路标红。 */
const deadlineOf = t => (t.pattern === 'stage' && t.endAt) ? t.endAt : t.dueAt;
const isOverdue = t => t.status !== 'done' && !!deadlineOf(t)
                   && dayDiff(new Date(deadlineOf(t)), new Date()) < 0;

const dueBy = t => (t.dueAt ? dayDiff(new Date(t.dueAt), new Date()) : null);

/** 「今天该处理」：到点了或已经过的非阶段性工作。
 *  阶段性工作是跨天的容器，按「今天到期」算没有意义，它单独一段列着。
 *  侧栏角标和主区列表共用这一个口径，免得两处数字对不上。 */
const needsAttention = t => t.status !== 'done' && t.pattern !== 'stage'
                        && dueBy(t) !== null && dueBy(t) <= 0;
const sortTasks = arr => arr.slice().sort((a, b) => {
  if (a.status !== b.status) return a.status === 'done' ? 1 : -1;
  if (!!a.dueAt !== !!b.dueAt) return a.dueAt ? -1 : 1;
  if (a.dueAt && b.dueAt) return new Date(a.dueAt) - new Date(b.dueAt);
  return a.id - b.id;
});
function matchQ(t) {
  if (!state.q) return true;
  const s = state.q.toLowerCase();
  const cat = catById(t.categoryId);
  return (t.title || '').toLowerCase().includes(s)
      || (t.note || '').toLowerCase().includes(s)
      || (cat ? cat.name.toLowerCase().includes(s) : false);
}

/* ---------------- 渲染：侧栏 ---------------- */
function renderSidebar() {
  const open = state.tasks.filter(t => t.status !== 'done');
  const all  = state.tasks;
  const done = state.tasks.filter(t => t.status === 'done');

  const navItem = (key, label, color, count, filled = true) => `
    <div class="nav-item ${state.view === key ? 'active' : ''}" data-nav="${key}">
      <span class="nav-dot" style="background:${color}"></span>
      <span class="nav-label">${label}</span>
      <span class="nav-count">${count}</span>
    </div>`;

  const catItem = c => {
    const n = all.filter(t => t.categoryId === c.id && t.status !== 'done').length;
    return `
    <div class="nav-item ${state.view === 'cat:' + c.id ? 'active' : ''}"
         data-nav="cat:${c.id}" data-catdrag="${c.id}" draggable="true">
      <span class="nav-dot" style="background:${c.color}"></span>
      <span class="nav-label">${esc(c.name)}</span>
      <button class="nav-edit" data-act="edit-cat" data-cat="${c.id}" title="编辑分类">
        <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
          <path d="M9.3 2.1l2.6 2.6M2 12l.7-2.9 6.6-6.6 2.6 2.6-6.6 6.6L2 12z"/>
        </svg>
      </button>
      <span class="nav-count">${n}</span>
    </div>`;
  };

  const uncat = all.filter(t => t.categoryId == null);
  const uncatItem = uncat.length ? `
    <div class="nav-item ${state.view === 'cat:none' ? 'active' : ''}" data-nav="cat:none">
      <span class="nav-dot" style="background:#CBD5E1"></span>
      <span class="nav-label">未分类</span>
      <span class="nav-count">${uncat.filter(t => t.status !== 'done').length}</span>
    </div>` : '';

  document.getElementById('sidebar').innerHTML = `
    <div class="nav-group">
      ${navItem('today', '今天', '#4F5BE8', open.filter(needsAttention).length)}
      ${navItem('all', '全部工作', '#94A3B8', open.length)}
      ${navItem('done', '已完成', '#10B981', done.length)}
    </div>
    <div class="nav-sep"></div>
    <div class="nav-heading">
      <span>分类</span>
      <button data-act="new-category" title="新建分类">
        <svg viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M6 2v8M2 6h8"/></svg>
      </button>
    </div>
    <div class="nav-group">${state.categories.map(catItem).join('')}${uncatItem}</div>
    <div class="sidebar-foot">
      <div class="nav-sep"></div>
      ${navItem('stats', '统计看板', '#8B5CF6', '')}
      ${navItem('settings', '设置', '#94A3B8', '')}
    </div>`;
}

/* ---------------- 渲染：主区 ---------------- */
function taskRow(t) {
  const cat = catById(t.categoryId);
  const p = PATTERNS[t.pattern];
  const overdue = isOverdue(t);
  let chip = '';
  if (t.status === 'done')                     chip = `<span class="chip chip-ok">已完成</span>`;
  else if (overdue)                            chip = `<span class="chip chip-danger">已逾期</span>`;
  else if (t.pattern === 'recurring')          chip = `<span class="chip ${p.chip}">${relLabel(t.dueAt) || '周期'}</span>`;
  else if (t.pattern === 'stage') {
    const pr = t.progress || {};
    const all = pr.total > 0 && pr.done === pr.total;
    chip = `<span class="chip ${all ? 'chip-ok' : p.chip}">`
         + `${all ? '待收尾' : pr.total ? `${pr.done}/${pr.total}` : '未拆解'}</span>`;
  }
  else if (t.dueAt)                            chip = `<span class="chip ${p.chip}">${relLabel(t.dueAt)}</span>`;

  const prog = t.pattern === 'stage' && t.progress?.total
    ? `<div class="progress"><i style="width:${t.progress.done / t.progress.total * 100}%"></i></div>` : '';

  // 周期任务才有「跳过这一次」——让这一次过去，规则本身不动
  const skip = t.pattern === 'recurring' && t.status !== 'done' ? `
      <button class="icon-btn sm" data-act="skip" data-id="${t.id}" title="跳过这一次，不打断节奏">
        <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M3 2.6l5.6 4.4L3 11.4z"/><path d="M10.6 2.6v8.8"/>
        </svg>
      </button>` : '';

  // 备注跟在元信息下面，左侧一道细线把它和「分类 · 时间」区分开。
  // 最多两行，超出部分裁剪；title 属性让鼠标悬停能看到全文。
  const note = (t.note || '').trim();

  return `
    <div class="task ${overdue ? 'is-overdue' : ''} ${t.status === 'done' ? 'is-done' : ''}" data-id="${t.id}">
      <button class="check" data-act="toggle" data-id="${t.id}">
        <svg viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M1.5 5.2l2.2 2.2 4.8-4.8"/></svg>
      </button>
      <span class="pattern-bar ${p.bar}"></span>
      <div class="task-main" data-act="open" data-id="${t.id}">
        <div class="task-title">${hl(t.title)}</div>
        <div class="task-meta">${overdue ? `<span class="overdue">${metaLabel(t, cat)}</span>` : metaLabel(t, cat)}</div>
        ${note ? `<div class="task-note" title="${escAttr(note)}">${hl(note)}</div>` : ''}
      </div>
      ${prog}
      <div class="task-actions">${skip}</div>
      ${chip}
    </div>`;
}

function section(title, note, rows) {
  if (!rows.length) return '';
  return `
    <div class="section-head">
      <span class="section-title">${title}</span>
      <span class="section-line"></span>
      <span class="section-note">${note}</span>
    </div>
    <div class="task-list">${rows.map(taskRow).join('')}</div>`;
}

function renderToday() {
  const now = new Date();
  const openTasks = state.tasks.filter(t => t.status !== 'done' && matchQ(t));
  const doneTasks = state.tasks.filter(t => t.status === 'done');

  // 今天该做的：一次性 + 周期性的到期日已到或已过。
  // 周期任务排期后 dueAt 就是「下一次」，所以到点的那次会自动出现在这里。
  const todayDue = sortTasks(openTasks.filter(needsAttention));

  // 阶段性工作是容器，带进度条，不按「今天到期」算
  const stageOnes = sortTasks(openTasks.filter(t => t.pattern === 'stage'));

  // 还没到点的周期节奏：一眼看到接下来什么在等着
  const upcoming = sortTasks(openTasks.filter(t =>
    t.pattern === 'recurring' && dueBy(t) !== null && dueBy(t) > 0));

  const soon = sortTasks(openTasks.filter(t =>
    t.pattern === 'once' && dueBy(t) !== null && dueBy(t) > 0));

  const weekDone = doneTasks.filter(t => t.completedAt && new Date(t.completedAt) - now > -7 * DAY).length;
  const weekRecurring = state.completions.filter(c => new Date(c.doneAt) - now > -7 * DAY).length;

  return `
    <div class="view-head">
      <div class="view-title">今天</div>
      <div class="view-sub">${now.getMonth() + 1} 月 ${now.getDate()} 日 ${WEEK[now.getDay()]} · 共有 ${openTasks.length} 项待办</div>
    </div>

    <div class="stat-row">
      <div class="stat"><div class="stat-label">今天到期</div><div class="stat-value">${todayDue.length}</div></div>
      <div class="stat is-ok"><div class="stat-label">近 7 天完成</div><div class="stat-value">${weekDone + weekRecurring}</div></div>
      <div class="stat ${openTasks.filter(isOverdue).length ? 'is-danger' : ''}">
        <div class="stat-label">已逾期</div>
        <div class="stat-value">${openTasks.filter(isOverdue).length}</div>
      </div>
    </div>

    ${section('今天到期', todayDue.length + ' 项', todayDue)}
    ${section('进行中的阶段性工作', stageOnes.length + ' 项', stageOnes)}
    ${section('周期性节奏', '自动排期', upcoming)}
    ${section('接下来', soon.length + ' 项', soon.slice(0, 6))}
    ${!todayDue.length && !upcoming.length && !stageOnes.length && !soon.length
      ? '<div class="empty">今天没有待办，享受一下</div>' : ''}`;
}

function renderAll() {
  const list = sortTasks(state.tasks.filter(matchQ));
  return `
    <div class="view-head">
      <div class="view-title">全部工作</div>
      <div class="view-sub">共 ${list.length} 项</div>
    </div>
    ${list.length ? `<div class="task-list">${list.map(taskRow).join('')}</div>` : '<div class="empty">还没有任何记录</div>'}`;
}

/** 一条周期完成记录：周期任务不进「已完成」，它每次做完都记在这里 */
function completionRow(c) {
  const cat = catById(c.categoryId);
  const d = new Date(c.doneAt);
  const when = `${d.getMonth() + 1} 月 ${d.getDate()} 日${timeLabel(c.doneAt) ? ' ' + timeLabel(c.doneAt) : ''}`;
  const late = c.dueAt ? dayDiff(d, new Date(c.dueAt)) : 0;
  const lateNote = late > 0 ? ` · 晚 ${late} 天` : '';
  return `
    <div class="task is-done" data-id="c${c.id}">
      <span class="check static" aria-hidden="true">
        <svg viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M1.5 5.2l2.2 2.2 4.8-4.8"/></svg>
      </span>
      <span class="pattern-bar p-recurring"></span>
      <div class="task-main">
        <div class="task-title">${esc(c.title)}</div>
        <div class="task-meta">${cat ? esc(cat.name) : '未分类'} · 完成于 ${when}${lateNote}</div>
      </div>
      <div class="task-actions">
        <button class="icon-btn sm" data-act="undo-completion" data-id="${c.id}" title="撤销这次完成记录">
          <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
            <path d="M2.6 6.4h5.6a3 3 0 0 1 0 6H5.6"/><path d="M5 3.4L2 6.4l3 3"/>
          </svg>
        </button>
      </div>
    </div>`;
}

function renderDone() {
  const list = sortTasks(state.tasks.filter(t => t.status === 'done' && matchQ));
  const recs = state.completions.filter(c => {
    if (!state.q) return true;
    const s = state.q.toLowerCase();
    const cat = catById(c.categoryId);
    return (c.title || '').toLowerCase().includes(s)
        || (cat ? cat.name.toLowerCase().includes(s) : false);
  });

  return `
    <div class="view-head">
      <div class="view-title">已完成</div>
      <div class="view-sub">${list.length} 项工作 · ${recs.length} 次周期记录</div>
    </div>
    ${list.length ? `<div class="task-list">${list.map(taskRow).join('')}</div>` : '<div class="empty">还没有完成的工作</div>'}
    ${recs.length ? `
      <div class="section-head" style="margin-top:24px">
        <span class="section-title">周期性完成记录</span>
        <span class="section-line"></span>
        <span class="section-note">共 ${recs.length} 次</span>
      </div>
      <div class="task-list">${recs.map(completionRow).join('')}</div>` : ''}`;
}

function renderCategory(id) {
  const c = catById(id);
  if (!c) return '<div class="empty">分类不存在</div>';
  const list = sortTasks(state.tasks.filter(t => t.categoryId === id && matchQ));
  const open = list.filter(t => t.status !== 'done');
  return `
    <div class="view-head">
      <div class="view-title" style="display:flex;align-items:center;gap:9px">
        <span style="width:9px;height:9px;border-radius:50%;background:${c.color}"></span>${esc(c.name)}
      </div>
      <div class="view-sub">${open.length} 项待办 · 共 ${list.length} 项</div>
    </div>
    ${list.length ? `<div class="task-list">${list.map(taskRow).join('')}</div>` : '<div class="empty">这个分类下还没有工作</div>'}`;
}

function renderUncategorized() {
  const list = sortTasks(state.tasks.filter(t => t.categoryId == null && matchQ));
  const open = list.filter(t => t.status !== 'done');
  return `
    <div class="view-head">
      <div class="view-title" style="display:flex;align-items:center;gap:9px">
        <span style="width:9px;height:9px;border-radius:50%;background:#CBD5E1"></span>未分类
      </div>
      <div class="view-sub">${open.length} 项待办 · 共 ${list.length} 项</div>
    </div>
    ${list.length ? `<div class="task-list">${list.map(taskRow).join('')}</div>` : '<div class="empty">这里没有工作</div>'}`;
}

/* ---------------- 排布方式：列表 / 看板 / 日历 ---------------- */

const MODES = [
  ['list', '列表'],
  ['board', '看板'],
  ['calendar', '日历'],
];

/** 三种排布共用同一个「当前范围」：侧栏选中的那一屏 + 顶部搜索词。
 *  把范围抽出来，看板和日历才不会各算各的。 */
function scopeTasks() {
  const v = state.view;
  let list = state.tasks.filter(matchQ);
  if (v === 'today')         list = list.filter(t => t.status !== 'done');
  else if (v === 'done')     list = list.filter(t => t.status === 'done');
  else if (v === 'cat:none') list = list.filter(t => t.categoryId == null);
  else if (v.startsWith('cat:')) {
    const id = Number(v.slice(4));
    list = list.filter(t => t.categoryId === id);
  }
  return list;   // 'all' 就是全部，不再筛
}

/** 主区右上角的「列表 / 看板 / 日历」开关。
 *  用 DOM 挂上去而不是拼进字符串：各视图的页头结构不完全一样，
 *  硬要插进去就得挨个改 render 函数，不值当。 */
function mountViewSwitch() {
  const head = document.querySelector('#view .view-head');
  if (!head) return;
  const el = document.createElement('div');
  el.className = 'view-switch';
  el.innerHTML = MODES.map(([k, label]) =>
    `<button data-act="set-mode" data-mode="${k}" class="${state.mode === k ? 'on' : ''}">${label}</button>`
  ).join('');
  head.appendChild(el);
}

/* ---------------- 看板：按「什么时候要」分列 ---------------- */

const LANES = [
  { key: 'overdue', title: '已逾期',      color: 'var(--danger)' },
  { key: 'today',   title: '今天',        color: 'var(--primary)' },
  { key: 'week',    title: '接下来 7 天', color: 'var(--p-recurring)' },
  { key: 'later',   title: '更晚',   color: 'var(--text-3)' },
  { key: 'none',    title: '没排期', color: 'var(--text-3)' },
  { key: 'done',    title: '已完成', color: 'var(--ok)' },
];

/** 一条工作落在看板的哪一列。用 deadlineOf：阶段性工作看交期，不看开始日，
 *  否则刚起步的大任务会一路挂在「已逾期」里。 */
function laneOf(t) {
  if (t.status === 'done') return 'done';
  const dl = deadlineOf(t);
  if (!dl) return 'none';
  const d = dayDiff(new Date(dl), new Date());
  if (d < 0) return 'overdue';
  if (d === 0) return 'today';
  if (d <= 7) return 'week';
  return 'later';
}

function boardCard(t) {
  const cat = catById(t.categoryId);
  const p = PATTERNS[t.pattern];
  const note = (t.note || '').trim();
  const prog = t.pattern === 'stage' && t.progress?.total
    ? `<div class="progress"><i style="width:${t.progress.done / t.progress.total * 100}%"></i></div>` : '';
  const meta = [cat ? cat.name : '未分类',
                deadlineOf(t) ? relLabel(deadlineOf(t)) : '',
                t.pattern === 'stage' && t.progress?.total
                  ? `子任务 ${t.progress.done}/${t.progress.total}` : '']
    .filter(Boolean).join(' · ');

  return `
    <div class="board-card ${isOverdue(t) ? 'is-overdue' : ''}" data-act="open" data-id="${t.id}">
      <span class="pattern-bar ${p.bar}"></span>
      <div class="board-card-body">
        <div class="board-card-title">${hl(t.title)}</div>
        <div class="board-card-meta">${esc(meta)}</div>
        ${note ? `<div class="board-card-note" title="${escAttr(note)}">${hl(note)}</div>` : ''}
        ${prog}
      </div>
    </div>`;
}

function renderBoard() {
  const list = scopeTasks();
  const buckets = {};
  LANES.forEach(l => { buckets[l.key] = []; });
  list.forEach(t => buckets[laneOf(t)].push(t));

  // 「已完成」这一列在只看待办的范围里必然空着，摆一列空的反而占地儿
  const lanes = LANES.filter(l => l.key !== 'done' || buckets.done.length);

  const cols = lanes.map(l => {
    const items = sortTasks(buckets[l.key]);
    return `
      <div class="board-col">
        <div class="board-col-head">
          <span class="board-col-dot" style="background:${l.color}"></span>
          <span class="board-col-title">${l.title}</span>
          <span class="board-col-count">${items.length}</span>
        </div>
        <div class="board-col-body">
          ${items.map(boardCard).join('') || '<div class="board-empty">—</div>'}
        </div>
      </div>`;
  }).join('');

  return `
    <div class="view-head">
      <div class="view-title">看板</div>
      <div class="view-sub">${list.length} 项 · 按「什么时候要」分成几列，横向拖动可看全</div>
    </div>
    <div class="board">${cols}</div>`;
}

/* ---------------- 日历：把截止日铺到月历上 ---------------- */

function renderCalendar() {
  const base = state.calMonth || new Date();
  const y = base.getFullYear();
  const m = base.getMonth();
  const first = new Date(y, m, 1);

  // 按天归堆。只认截止日——一件工作占一个格子，跨天的阶段性工作落在交期那天
  const byDay = new Map();
  scopeTasks().forEach(t => {
    const dl = deadlineOf(t);
    if (!dl) return;
    const k = startOfDay(new Date(dl)).getTime();
    if (!byDay.has(k)) byDay.set(k, []);
    byDay.get(k).push(t);
  });

  // 周一起头：起点回退到 1 号所在周的周一
  const gridStart = startOfWeek(first);
  const lead = (first.getDay() + 6) % 7;
  const daysInMonth = new Date(y, m + 1, 0).getDate();
  const cells = Math.ceil((lead + daysInMonth) / 7) * 7;

  const todayKey = startOfDay(new Date()).getTime();
  let grid = '';
  for (let i = 0; i < cells; i++) {
    const d = new Date(gridStart);
    d.setDate(d.getDate() + i);
    const k = d.getTime();
    const items = sortTasks(byDay.get(k) || []);
    const shown = items.slice(0, 3);
    const more = items.length - shown.length;

    grid += `
      <div class="cal-cell ${d.getMonth() !== m ? 'is-out' : ''} ${k === todayKey ? 'is-today' : ''}">
        <div class="cal-date">${d.getDate()}</div>
        <div class="cal-items">
          ${shown.map(t => {
            const cat = catById(t.categoryId);
            // 标题单独包一层：flex 容器上直接写 text-overflow 不管用，
            // 文本会被当成匿名 flex 项，省不掉省略号
            return `<div class="cal-item ${t.status === 'done' ? 'is-done' : ''}"
                         data-act="open" data-id="${t.id}" title="${escAttr(t.title)}">
              <span class="cal-dot" style="background:${cat ? cat.color : '#CBD5E1'}"></span>
              <span class="cal-text">${esc(t.title)}</span>
            </div>`;
          }).join('')}
          ${more > 0 ? `<div class="cal-more">还有 ${more} 项</div>` : ''}
        </div>
      </div>`;
  }

  const now = new Date();
  const offMonth = y !== now.getFullYear() || m !== now.getMonth();

  return `
    <div class="view-head">
      <div class="view-title">日历</div>
      <div class="view-sub">按截止日铺开，点一条可以打开详情</div>
    </div>
    <div class="cal-bar">
      <button class="icon-btn" data-act="cal-prev" title="上个月">
        <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M8.6 3.2L4.8 7l3.8 3.8"/></svg>
      </button>
      <span class="cal-month">${y} 年 ${m + 1} 月</span>
      <button class="icon-btn" data-act="cal-next" title="下个月">
        <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M5.4 3.2L9.2 7l-3.8 3.8"/></svg>
      </button>
      ${offMonth ? '<button class="btn btn-ghost" data-act="cal-today">回到今天</button>' : ''}
    </div>
    <div class="cal-week">
      ${['一', '二', '三', '四', '五', '六', '日'].map(w => `<div class="cal-weekday">${w}</div>`).join('')}
    </div>
    <div class="cal-grid">${grid}</div>`;
}

/* ---------------- 统计看板 ---------------- */

const r1 = v => Math.round(v * 10) / 10;

/** 一周从周一起算 —— 按国内日历习惯，而不是 JS 默认的周日 */
function startOfWeek(d) {
  const x = startOfDay(d);
  x.setDate(x.getDate() - ((x.getDay() + 6) % 7));
  return x;
}

/** 把「完成了一次」的所有事件汇成一条时间线。
 *
 *  一次性 / 阶段性工作看 completedAt；周期工作看 completions 表（每完成一次留一条）。
 *  两者必须合一 —— 否则天天在做的周期工作（周报、晨会）在统计里等于不存在，
 *  而这恰恰是「这个月到底干了多少事」最主要的来源。 */
function completionEvents() {
  const out = [];
  state.tasks.forEach(t => {
    if (t.status === 'done' && t.completedAt) {
      out.push({ at: t.completedAt, dueAt: t.dueAt, pattern: t.pattern,
                 categoryId: t.categoryId, title: t.title });
    }
  });
  state.completions.forEach(c => {
    out.push({ at: c.doneAt, dueAt: c.dueAt, pattern: 'recurring',
               categoryId: c.categoryId, title: c.title });
  });
  return out
    .filter(e => e.at && !isNaN(new Date(e.at)))
    .sort((a, b) => new Date(b.at) - new Date(a.at));
}

/** 只把上面两个角做圆：底部要跟下一段严丝合缝拼起来，圆角会漏出底色 */
function barPath(x, y, w, h, r) {
  const rr = Math.min(r, w / 2, Math.max(h, 0));
  const x2 = x + w, y2 = y + h;
  return `M${r1(x)} ${r1(y2)}L${r1(x)} ${r1(y + rr)}`
       + `Q${r1(x)} ${r1(y)} ${r1(x + rr)} ${r1(y)}`
       + `L${r1(x2 - rr)} ${r1(y)}Q${r1(x2)} ${r1(y)} ${r1(x2)} ${r1(y + rr)}`
       + `L${r1(x2)} ${r1(y2)}Z`;
}

/** 近 30 天完成趋势：周期性垫在下面（蓝），一次性叠在上面（琥珀）。
 *  两段分开看，就知道这段时间是「固定节奏在推」还是「临时活儿多」。 */
function trendChart(events) {
  // W 是画布宽，PLOT 是柱子区宽 —— 右侧留 30px 给刻度数字，
  // 否则最后一天（也就是今天）的高亮带会和刻度叠在一起。
  const W = 800, PLOT = 770, H = 184, TOP = 18, BASE = 150, N = 30;
  const today = startOfDay(new Date());
  const slots = [];
  for (let i = N - 1; i >= 0; i--) {
    const d = new Date(today); d.setDate(d.getDate() - i);
    slots.push({ d, once: 0, rec: 0 });
  }
  const idx = new Map(slots.map((s, i) => [toLocalDate(s.d), i]));
  events.forEach(e => {
    const i = idx.get(toLocalDate(e.at));
    if (i === undefined) return;
    if (e.pattern === 'recurring') slots[i].rec++; else slots[i].once++;
  });

  const max = Math.max(1, ...slots.map(s => s.once + s.rec));
  const slot = PLOT / N;
  const bw = Math.max(7, slot - 9);
  const h = v => (v / max) * (BASE - TOP);

  // max 为 1 时不画中线：两条线会重叠，刻度也都标成 1
  const fracs = max >= 2 ? [1, .5] : [1];
  const grid = fracs.map(f => {
    const y = BASE - h(max * f);
    return `<line x1="0" y1="${r1(y)}" x2="${PLOT}" y2="${r1(y)}" stroke="#F0F1F4" stroke-width="1"/>`
         + `<text x="${W}" y="${r1(y + 3.5)}" text-anchor="end" font-size="10" fill="#9AA4B2">${Math.round(max * f)}</text>`;
  }).join('')
  + `<line x1="0" y1="${BASE}" x2="${PLOT}" y2="${BASE}" stroke="#E8EAED" stroke-width="1"/>`;

  // 今天那一列淡提一笔，一眼能找到「现在」
  const bandX = (N - 1) * slot;
  const band = `<rect x="${r1(bandX)}" y="${TOP - 8}" width="${r1(slot)}" height="${r1(BASE - TOP + 8)}" rx="4" fill="#EEF0FE"/>`;

  const bars = slots.map((s, i) => {
    const x = i * slot + (slot - bw) / 2;
    const total = s.once + s.rec;
    const tip = `<title>${s.d.getMonth() + 1} 月 ${s.d.getDate()} 日 · 完成 ${total} 次</title>`;
    if (!total) {
      return `<g>${tip}<rect x="${r1(x)}" y="${BASE - 2}" width="${r1(bw)}" height="2" rx="1" fill="#E8EAED"/></g>`;
    }
    const hr = h(s.rec), ho = h(s.once);
    let y = BASE, out = '';
    if (hr > 0) { out += `<path d="${barPath(x, y - hr, bw, hr, ho > 0 ? 0 : 3)}" fill="#3B82F6"/>`; y -= hr; }
    if (ho > 0) { out += `<path d="${barPath(x, y - ho, bw, ho, 3)}" fill="#F59E0B"/>`; }
    return `<g>${tip}${out}</g>`;
  }).join('');

  const labels = slots.map((s, i) => {
    const last = i === N - 1;
    if (!last && s.d.getDay() !== 1) return '';
    const x = i * slot + slot / 2;
    const txt = last ? '今天' : `${s.d.getMonth() + 1}/${s.d.getDate()}`;
    return `<text x="${r1(x)}" y="${H - 8}" text-anchor="${last ? 'end' : 'middle'}" font-size="10" fill="#9AA4B2">${txt}</text>`;
  }).join('');

  return `<svg viewBox="0 0 ${W} ${H}" width="100%" style="display:block">${band}${grid}${bars}${labels}</svg>`;
}

/** 近 26 周（约半年）的完成热力图。单元格自带原生 tooltip，悬停即可看当天次数。 */
function heatmap(events) {
  const COLS = 26, ROWS = 7, PITCH = 28, CELL = 21, R = 5;
  const LEFT = 28, TOP = 22;
  const W = LEFT + COLS * PITCH, H = TOP + ROWS * PITCH, HH = H + 26;

  const today = startOfDay(new Date());
  const firstMon = startOfWeek(today);
  firstMon.setDate(firstMon.getDate() - (COLS - 1) * 7);

  const counts = new Map();
  events.forEach(e => {
    const k = toLocalDate(e.at);
    counts.set(k, (counts.get(k) || 0) + 1);
  });

  const lvl = n => n === 0 ? '#EEF0F4' : n === 1 ? '#C9CFFA'
                  : n === 2 ? '#98A2F3' : n <= 4 ? '#6D79EA' : '#4F5BE8';

  let cells = '', months = '', lastMonth = -1;
  for (let c = 0; c < COLS; c++) {
    const mon = new Date(firstMon); mon.setDate(mon.getDate() + c * 7);
    if (mon.getMonth() !== lastMonth) {
      lastMonth = mon.getMonth();
      months += `<text x="${r1(LEFT + c * PITCH)}" y="${TOP - 7}" font-size="10" fill="#9AA4B2">${lastMonth + 1} 月</text>`;
    }
    for (let rw = 0; rw < ROWS; rw++) {
      const d = new Date(firstMon); d.setDate(d.getDate() + c * 7 + rw);
      if (d > today) continue;
      const n = counts.get(toLocalDate(d)) || 0;
      const tip = `${d.getMonth() + 1} 月 ${d.getDate()} 日 · ${n ? `完成 ${n} 次` : '没有记录'}`;
      cells += `<rect x="${LEFT + c * PITCH}" y="${TOP + rw * PITCH}" width="${CELL}" height="${CELL}"`
             + ` rx="${R}" fill="${lvl(n)}"><title>${tip}</title></rect>`;
    }
  }

  const wd = ['一', '二', '三', '四', '五', '六', '日'];
  const wdLabels = [0, 2, 4].map(rw =>
    `<text x="${LEFT - 9}" y="${r1(TOP + rw * PITCH + CELL * 0.74)}" text-anchor="end" font-size="10" fill="#9AA4B2">${wd[rw]}</text>`
  ).join('');

  const lx = LEFT, ly = H + 6;
  const legend = `<text x="${lx}" y="${ly + 11}" font-size="10" fill="#9AA4B2">少</text>`
    + [0, 1, 2, 3, 5].map((n, i) =>
        `<rect x="${lx + 18 + i * 25}" y="${ly}" width="15" height="15" rx="4" fill="${lvl(n)}"/>`).join('')
    + `<text x="${lx + 18 + 5 * 25 + 4}" y="${ly + 11}" font-size="10" fill="#9AA4B2">多</text>`;

  return `<svg viewBox="0 0 ${W} ${HH}" width="100%" style="display:block">${months}${wdLabels}${cells}${legend}</svg>`;
}

/** 一行分类统计：长条按完成次数，右侧补一句「进行中」 */
function catStatRow(name, color, done, open, maxDone) {
  return `
    <div class="cat-stat">
      <span class="cat-stat-dot" style="background:${color}"></span>
      <span class="cat-stat-name">${esc(name)}</span>
      <span class="cat-stat-bar"><i style="width:${done / maxDone * 100}%;background:${color}"></i></span>
      <span class="cat-stat-num">${done}</span>
      <span class="cat-stat-sub">进行中 ${open}</span>
    </div>`;
}

function renderStats() {
  const events = completionEvents();
  const now = new Date();
  const openTasks = state.tasks.filter(t => t.status !== 'done');
  const overdue = openTasks.filter(isOverdue).length;

  const weekN  = events.filter(e => new Date(e.at) >= startOfWeek(now)).length;
  const monthN = events.filter(e => new Date(e.at) >= new Date(now.getFullYear(), now.getMonth(), 1)).length;

  const from30 = startOfDay(new Date(now.getTime() - 29 * DAY));
  const recent = events.filter(e => new Date(e.at) >= from30);
  const activeDays = new Set(recent.map(e => toLocalDate(e.at))).size;

  // 按时 = 完成日不晚于到期日。按天比而不是按分钟，
  // 因为「今天做的」不该因为截止写的是 18:00、实际 20:00 交就算迟到。
  const withDue = events.filter(e => e.dueAt);
  const onTime  = withDue.filter(e => dayDiff(new Date(e.at), new Date(e.dueAt)) <= 0).length;
  const rate = withDue.length ? Math.round(onTime / withDue.length * 100) : null;
  const rateCls = rate === null ? '' : rate >= 80 ? 'is-ok' : rate < 60 ? 'is-danger' : '';

  // 分类：以「完成次数」为主 —— 这样才看得出时间实际花在哪，而不是录了多少条
  const catStats = state.categories.map(c => ({
    name: c.name, color: c.color,
    done: events.filter(e => e.categoryId === c.id).length,
    open: openTasks.filter(t => t.categoryId === c.id).length,
  }));
  const uDone = events.filter(e => e.categoryId == null).length;
  const uOpen = openTasks.filter(t => t.categoryId == null).length;
  if (uDone || uOpen) catStats.push({ name: '未分类', color: '#CBD5E1', done: uDone, open: uOpen });
  catStats.sort((a, b) => b.done - a.done || b.open - a.open);
  const maxDone = Math.max(1, ...catStats.map(s => s.done));

  const byPat = ['once', 'recurring', 'stage'].map(k => ({
    ...PATTERNS[k], k,
    n: state.tasks.filter(t => t.pattern === k).length,
    done: events.filter(e => e.pattern === k).length,
  }));
  const patTotal = Math.max(1, byPat.reduce((s, x) => s + x.n, 0));

  const empty = '<div class="empty">还没有完成记录。<br>打完第一个勾，这里就开始长出你的工作轨迹。</div>';

  return `
    <div class="view-head">
      <div class="view-title">统计看板</div>
      <div class="view-sub">累计完成 ${events.length} 次 · 近 30 天 ${recent.length} 次 · ${openTasks.length} 项待办${overdue ? `（逾期 ${overdue}）` : ''}</div>
    </div>

    <div class="stat-row four">
      <div class="stat">
        <div class="stat-label">本周完成</div>
        <div class="stat-value">${weekN}</div>
        <div class="stat-sub">周一至今</div>
      </div>
      <div class="stat">
        <div class="stat-label">本月完成</div>
        <div class="stat-value">${monthN}</div>
        <div class="stat-sub">${now.getMonth() + 1} 月</div>
      </div>
      <div class="stat">
        <div class="stat-label">活跃天数</div>
        <div class="stat-value">${activeDays}</div>
        <div class="stat-sub">近 30 天中</div>
      </div>
      <div class="stat ${rateCls}">
        <div class="stat-label">按时完成率</div>
        <div class="stat-value">${rate === null ? '—' : rate + '%'}</div>
        <div class="stat-sub">${withDue.length ? `${onTime} / ${withDue.length} 次有明确时间` : '暂无带时间的记录'}</div>
      </div>
    </div>

    ${events.length ? `
    <div class="chart-card">
      <div class="chart-head">
        <span class="chart-title">完成趋势</span>
        <span class="chart-note">近 30 天</span>
      </div>
      ${trendChart(events)}
      <div class="chart-legend">
        <span><i style="background:#3B82F6"></i>周期性</span>
        <span><i style="background:#F59E0B"></i>一次性 / 阶段性</span>
      </div>
    </div>

    <div class="chart-card">
      <div class="chart-head">
        <span class="chart-title">活跃热力图</span>
        <span class="chart-note">近 26 周 · 共 ${events.length} 次完成</span>
      </div>
      ${heatmap(events)}
    </div>` : empty}

    <div class="section-head" style="margin-top:22px">
      <span class="section-title">分类分布</span>
      <span class="section-line"></span>
      <span class="section-note">按完成次数</span>
    </div>
    ${catStats.length ? `
    <div class="card"><div class="stat-list">
      ${catStats.map(s => catStatRow(s.name, s.color, s.done, s.open, maxDone)).join('')}
    </div></div>` : '<div class="empty">先在侧栏建一个分类</div>'}

    <div class="section-head" style="margin-top:22px">
      <span class="section-title">节奏类型</span>
      <span class="section-line"></span>
      <span class="section-note">按工作条数</span>
    </div>
    <div class="card">
      <div class="stack-bar">
        ${byPat.filter(p => p.n).map(p =>
          `<i style="width:${p.n / patTotal * 100}%;background:${p.color}"></i>`).join('')}
      </div>
      <div class="stat-list" style="margin-top:10px">
        ${byPat.map(p => `
          <div class="cat-stat">
            <span class="cat-stat-dot" style="background:${p.color}"></span>
            <span class="cat-stat-name">${p.label}</span>
            <span class="cat-stat-bar"><i style="width:${p.n / patTotal * 100}%;background:${p.color}"></i></span>
            <span class="cat-stat-num">${p.n}</span>
            <span class="cat-stat-sub">完成 ${p.done} 次</span>
          </div>`).join('')}
      </div>
    </div>

    <div class="hint" style="margin-top:14px">
      口径说明：「完成一次」= 一次性或阶段性工作打勾 + 周期性工作的每一次完成记录。
      阶段性工作的子任务是「步骤」，不单独计入统计，免得一步被算成一件工作。
      删除工作不会删掉它的历史记录。
    </div>`;
}

const SETTING_ITEMS = [
  { key: 'autostart', title: '开机自动启动', desc: '登录 Windows 后在托盘静默启动' },
  { key: 'notify',    title: '到期提醒',     desc: '任务到期时弹出系统通知' },
  { key: 'tray',      title: '关闭窗口时最小化到托盘', desc: '关闭后继续在后台运行，保证提醒准时' },
];

function settingOn(key) {
  const v = state.settings?.[key];
  if (v === undefined || v === null || v === '') return true;   // 未设置时默认开启
  return v === '1' || v === 'true' || v === 'on' || v === true;
}

function renderSettings() {
  const on = settingOn;
  return `
    <div class="view-head">
      <div class="view-title">设置</div>
      <div class="view-sub">数据存放在本机，不会上传</div>
    </div>
    <div class="task-list">
      ${SETTING_ITEMS.map(s => `
        <div class="task" style="padding:14px">
          <div class="task-main">
            <div class="task-title">${s.title}</div>
            <div class="task-meta">${s.desc}</div>
          </div>
          <button class="switch ${on(s.key) ? 'on' : ''}" data-act="toggle-setting" data-key="${s.key}" aria-label="${s.title}"><i></i></button>
        </div>`).join('')}
    </div>

    <div class="section-head" style="margin-top:22px">
      <span class="section-title">数据</span><span class="section-line"></span>
    </div>
    <div class="task-list">
      <div class="task" style="padding:14px">
        <div class="task-main">
          <div class="task-title">数据存放位置</div>
          <div class="task-meta">${esc(state.dataPath || '本机用户目录 / worklog.db')}</div>
        </div>
        <button class="btn btn-ghost" data-act="open-data-dir">打开目录</button>
      </div>
    </div>`;
}

function renderView() {
  const v = state.view;
  const plain = v === 'stats' || v === 'settings';   // 这两屏没有排布方式可选
  let html;

  if (v === 'stats')                 html = renderStats();
  else if (v === 'settings')         html = renderSettings();
  else if (state.mode === 'board')   html = renderBoard();
  else if (state.mode === 'calendar')html = renderCalendar();
  else if (v === 'today')            html = renderToday();
  else if (v === 'all')              html = renderAll();
  else if (v === 'done')             html = renderDone();
  else if (v === 'cat:none')         html = renderUncategorized();
  else if (v.startsWith('cat:'))     html = renderCategory(Number(v.slice(4)));
  else html = '<div class="empty">未实现</div>';

  const el = document.getElementById('view');
  el.innerHTML = html;
  if (!plain) mountViewSwitch();
  el.scrollTop = 0;
}

/* ---------------- 渲染：抽屉 ---------------- */

/** 把抽屉输入框里的内容收进内存里的任务对象（不落库）。
 *  抽屉每次重绘都会重建 DOM，不收一下的话，用户写在标题/备注里的字会被抹掉。
 *  真正写库仍然只在点「保存」时发生。 */
function flushDrawerDom() {
  const id = state.drawerRendered;
  if (!id) return;
  const t = state.tasks.find(x => x.id === id);
  if (!t) return;
  const titleEl = document.getElementById('d-title');
  if (titleEl && titleEl.value.trim()) t.title = titleEl.value.trim();
  const noteEl = document.getElementById('d-note');
  if (noteEl) t.note = noteEl.value;
  const d = document.getElementById('d-date')?.value;
  const tm = document.getElementById('d-time')?.value;
  if (d) t.dueAt = new Date(`${d}T${tm || '18:00'}:00`).toISOString();
  const endEl = document.getElementById('d-end');
  if (endEl) t.endAt = endEl.value ? new Date(`${endEl.value}T23:59:00`).toISOString() : null;
}

/** 阶段性工作的子任务清单：勾选、就地改名、删除、随手追加 */
function subtaskEditor(t) {
  const kids = subtasksOf(t.id);
  const done = kids.filter(s => s.status === 'done').length;
  return `
    <div class="field">
      <div class="field-label">子任务<span class="sub-progress">${done} / ${kids.length}</span></div>
      ${kids.length ? `
        <div class="sub-list">
          ${kids.map(s => `
            <div class="sub-item ${s.status === 'done' ? 'is-done' : ''}">
              <button class="sub-check" data-act="sub-toggle" data-id="${s.id}" aria-label="完成这一步">
                <svg viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M1.5 5.2l2.2 2.2 4.8-4.8"/></svg>
              </button>
              <input class="sub-title" data-sub-title="${s.id}" value="${escAttr(s.title)}" title="点一下就能改">
              <button class="icon-btn sm sub-del" data-act="sub-del" data-id="${s.id}" title="删除这一步">
                <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M3.5 3.5l7 7M10.5 3.5l-7 7"/></svg>
              </button>
            </div>`).join('')}
        </div>` : '<div class="hint">还没拆步骤。把这件事分成几步，进度就是自动算的。</div>'}
      <div class="sub-add">
        <input class="field-input" id="d-sub-input" placeholder="加一步，回车确定">
        <button class="btn btn-ghost" data-act="add-subtask">添加</button>
      </div>
      ${kids.length && done === kids.length
        ? '<div class="hint">子任务都完成了，可以给这件工作收尾</div>' : ''}
    </div>`;
}

function renderDrawer() {
  flushDrawerDom();

  const el = document.getElementById('drawer');
  if (!state.drawerId) { el.classList.remove('open'); el.innerHTML = ''; state.drawerRendered = null; return; }
  const t = state.tasks.find(x => x.id === state.drawerId);
  if (!t) { el.classList.remove('open'); state.drawerRendered = null; return; }
  const cat = catById(t.categoryId);
  const p = PATTERNS[t.pattern];

  el.innerHTML = `
    <div class="drawer-head">
      <span class="drawer-title">工作详情</span>
      <button class="icon-btn" data-act="edit">
        <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M9.3 2.1l2.6 2.6M2 12l.7-2.9 6.6-6.6 2.6 2.6-6.6 6.6L2 12z"/>
        </svg>
      </button>
      <button class="icon-btn" data-act="close-drawer">
        <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M3.5 3.5l7 7M10.5 3.5l-7 7"/></svg>
      </button>
    </div>
    <div class="drawer-body">
      <div class="field">
        <div class="field-label">标题</div>
        <input class="field-input" id="d-title" value="${escAttr(t.title)}">
      </div>
      <div class="field">
        <div class="field-label">节奏类型</div>
        <div class="seg">
          ${Object.entries(PATTERNS).map(([k, v]) => `
            <button class="${t.pattern === k ? 'on' : ''}" data-act="set-pattern" data-pattern="${k}">
              <span class="dot" style="background:${v.color}"></span>${v.label}
            </button>`).join('')}
        </div>
      </div>
      <div class="field">
        <div class="field-label">分类</div>
        <div class="cat-grid">
          ${state.categories.map(c => `
            <button class="cat-pill ${t.categoryId === c.id ? 'on' : ''}"
                    style="color:${t.categoryId === c.id ? c.color : ''}"
                    data-act="set-cat" data-cat="${c.id}">
              <span class="swatch" style="background:${c.color}"></span>${esc(c.name)}
            </button>`).join('')}
          <button class="cat-pill ${t.categoryId == null ? 'on' : ''}"
                  data-act="set-cat" data-cat="">
            <span class="swatch" style="background:#CBD5E1"></span>未分类
          </button>
        </div>
      </div>
      ${t.pattern === 'recurring' ? `
      <div class="field">
        <div class="field-label">重复规则</div>
        <input class="field-input" value="${escAttr(ruleLabel(t))}" readonly>
        <div class="hint">规则在编辑面板中修改</div>
      </div>
      <div class="field">
        <div class="field-label">下一次</div>
        <div class="row-between">
          <div class="hint">${t.dueAt ? `${relLabel(t.dueAt)}${timeLabel(t.dueAt) ? ' ' + timeLabel(t.dueAt) : ''}` : '尚未排期'}</div>
          <button class="btn btn-ghost" data-act="skip" data-id="${t.id}">跳过这一次</button>
        </div>
      </div>` : t.pattern === 'stage' ? `
      <div class="field">
        <div class="field-label">开始日期</div>
        <div class="time-row">
          <input class="field-input" type="date" id="d-date" value="${toLocalDate(t.dueAt)}">
          <input class="field-input" type="time" id="d-time" value="${toLocalTime(t.dueAt, '09:00')}">
        </div>
      </div>
      <div class="field">
        <div class="field-label">结束日期</div>
        <input class="field-input" type="date" id="d-end" value="${toLocalDate(t.endAt)}">
        <div class="hint">逾期与否看结束日期，不是开始日期</div>
      </div>` : `
      <div class="field">
        <div class="field-label">截止时间</div>
        <div class="time-row">
          <input class="field-input" type="date" id="d-date" value="${toLocalDate(t.dueAt)}">
          <input class="field-input" type="time" id="d-time" value="${toLocalTime(t.dueAt, '18:00')}">
        </div>
      </div>`}
      <div class="field">
        <div class="field-label">备注</div>
        <textarea class="field-textarea" id="d-note" placeholder="补充说明…">${esc(t.note || '')}</textarea>
        <div class="hint">备注会显示在列表里，最多两行</div>
      </div>
      ${t.pattern === 'stage' ? subtaskEditor(t) : ''}
      <div class="field">
        <div class="field-label">创建时间</div>
        <div class="hint">${t.createdAt ? new Date(t.createdAt).toLocaleString('zh-CN') : '—'}</div>
      </div>
    </div>
    <div class="drawer-foot">
      <button class="btn btn-danger-ghost" data-act="delete">删除</button>
      ${t.pattern === 'recurring' && t.status !== 'done'
        ? '<button class="btn btn-ghost" data-act="complete-occurrence">完成这一次</button>' : ''}
      <button class="btn btn-primary" data-act="save">保存</button>
    </div>`;
  el.classList.add('open');
  state.drawerRendered = t.id;
}

/* ---------------- 新建 / 编辑 弹窗 ---------------- */
function catModalHtml() {
  const e = state.catEditing;
  const used = state.tasks.filter(t => t.categoryId === e.id).length;
  return `
    <div class="modal-head">
      <h2>${e.id ? '编辑分类' : '新建分类'}</h2>
      <button class="icon-btn" data-act="close-cat-modal">
        <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M3.5 3.5l7 7M10.5 3.5l-7 7"/></svg>
      </button>
    </div>

    <div class="modal-body">
    <div class="field">
      <div class="field-label">分类名称</div>
      <input class="field-input" id="c-name" placeholder="例如：本职工作" value="${escAttr(e.name || '')}" autofocus>
    </div>

    <div class="field">
      <div class="field-label">标记颜色</div>
      <div class="swatches">
        ${PALETTE.map(c => `
          <button class="swatch-btn ${e.color === c ? 'on' : ''}" style="background:${c}"
                  data-act="c-color" data-color="${c}" title="${c}"></button>`).join('')}
      </div>
    </div>

    ${e.id ? `
    <div class="field">
      <div class="field-label">使用情况</div>
      <div class="hint">${used} 项工作在这个分类下。删除分类不会删除工作，它们会移到「未分类」。</div>
    </div>` : `
    <div class="hint" style="margin-top:-4px">创建后可拖动侧栏分类调整顺序</div>`}
    </div>

    <div class="modal-foot">
      ${e.id ? `<button class="btn btn-danger-ghost" data-act="c-delete">${e.confirmDelete ? '确认删除？' : '删除分类'}</button>` : ''}
      <button class="btn btn-ghost" data-act="close-cat-modal">取消</button>
      <button class="btn btn-primary" data-act="c-save">${e.id ? '保存' : '创建'}</button>
    </div>`;
}

/* 弹窗和抽屉里的内容都是「整块重绘」——换个分类就得重画下半部分的表单。
   重绘会把输入框整个换掉，所以两件事必须做：
   1. 重绘前把用户敲的内容收回 state（syncXxxFields），否则新 HTML 用的还是旧值；
   2. 记住焦点和光标位置，画完还回原处，别让人重新点一遍。
   这两条是配套的，少哪一条都会「输一半就没了」。 */

/** 最近一次聚焦的表单控件；由下面的 focusin 监听维护 */
let modalFocus = null;

function rememberModalFocus(el) {
  const id = el && el.id;
  if (!id || !/^[mcd]-/.test(id)) return;
  modalFocus = { id, start: el.selectionStart ?? null, end: el.selectionEnd ?? null };
}

/** 重绘后把焦点和光标放回原处；没记录就退回 `fallback` 指定的控件 */
function restoreModalFocus(keep, fallback) {
  const saved = keep && document.getElementById(keep.id);
  const target = saved || (fallback ? document.getElementById(fallback) : null);
  if (!target || typeof target.focus !== 'function') return;
  target.focus();
  // time / date 这类输入框不支持选区，赋了会抛
  if (saved && keep.start != null) {
    try { target.setSelectionRange(keep.start, keep.end ?? keep.start); } catch (_) {}
  }
}

/** 把弹窗里已经敲进去的内容收回 state（不落库） */
function syncEditorFields() {
  const e = state.editing; if (!e) return;
  const title = document.getElementById('m-title');
  if (title) e.title = title.value;
  const note = document.getElementById('m-note');
  if (note) e.note = note.value;
}

function renderModal() {
  const mask = document.getElementById('modal-mask');
  const modal = document.getElementById('modal');
  const keep = modalFocus;   // 先存着，下面重绘会把它用掉
  modalFocus = null;

  if (state.catEditing) {
    modal.innerHTML = catModalHtml();
    mask.classList.add('open');
    setTimeout(() => restoreModalFocus(keep, 'c-name'), 30);
    return;
  }

  const e = state.editing;

  if (!e) { mask.classList.remove('open'); modal.innerHTML = ''; return; }

  // 规则里的值在模板里反复要用，先取出来，省得每处都写一遍默认值
  const rule    = e.rule || {};
  const freq    = rule.freq || 'weekly';
  const days    = (rule.byDay || []).length ? rule.byDay : [5];
  const dom     = Number(rule.byDay?.[0]) || 1;
  const byMonth = Number(rule.byMonth?.[0]) || 1;

  const today = todayLocal();

  modal.innerHTML = `
    <div class="modal-head">
      <h2>${e.id ? '编辑工作' : '新建工作'}</h2>
      <button class="icon-btn" data-act="close-modal">
        <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M3.5 3.5l7 7M10.5 3.5l-7 7"/></svg>
      </button>
    </div>

    <div class="modal-body">
    <div class="field">
      <div class="field-label">要做什么</div>
      <input class="field-input" id="m-title" placeholder="例如：整理季度复盘材料" value="${escAttr(e.title || '')}" autofocus>
    </div>

    <div class="field">
      <div class="field-label">节奏类型</div>
      <div class="seg">
        ${Object.entries(PATTERNS).map(([k, v]) => `
          <button class="${e.pattern === k ? 'on' : ''}" data-act="m-pattern" data-pattern="${k}">
            <span class="dot" style="background:${v.color}"></span>${v.label}
          </button>`).join('')}
      </div>
      <div class="hint">${{
        once: '做一次就结束，适合临时任务',
        recurring: '按规则重复出现，适合周报、对账这类固定节奏',
        stage: '持续一段时间的大任务，可以拆成多个子任务',
      }[e.pattern]}</div>
    </div>

    <div class="field">
      <div class="field-label">分类</div>
      <div class="cat-grid">
        ${state.categories.map(c => `
          <button class="cat-pill ${e.categoryId === c.id ? 'on' : ''}"
                  style="color:${e.categoryId === c.id ? c.color : ''}"
                  data-act="m-cat" data-cat="${c.id}">
            <span class="swatch" style="background:${c.color}"></span>${esc(c.name)}
          </button>`).join('')}
        <button class="cat-pill ${e.categoryId == null ? 'on' : ''}" data-act="m-cat" data-cat="">
          <span class="swatch" style="background:#CBD5E1"></span>未分类
        </button>
      </div>
    </div>

    ${e.pattern === 'recurring' ? `
      <div class="field">
        <div class="field-label">重复频率</div>
        <div class="seg">
          ${FREQS.map(f => `
            <button class="${freq === f.key ? 'on' : ''}" data-act="m-freq" data-freq="${f.key}">${f.label}</button>`).join('')}
        </div>
        <div class="hint">${FREQ_HINT[freq]}</div>
      </div>

      ${freq === 'weekly' ? `
      <div class="field">
        <div class="field-label">星期几</div>
        <div class="chip-grid">
          ${WEEK_ORDER.map(i => `
            <button class="chip ${days.includes(i) ? 'on' : ''}" data-act="m-day" data-day="${i}">${WEEK[i]}</button>`).join('')}
        </div>
      </div>` : ''}

      ${freq === 'monthly' || freq === 'quarterly' ? `
      ${freq === 'quarterly' ? `
      <div class="field">
        <div class="field-label">从哪个月起算</div>
        <div class="chip-grid cols-6">
          ${MONTHS_OF_YEAR.map(m => `
            <button class="chip ${byMonth === m ? 'on' : ''}" data-act="m-month" data-month="${m}">${m} 月</button>`).join('')}
        </div>
        <div class="hint">每 3 个月一次，实际会排在 ${quarterMonths(e.rule).join('、')} 月</div>
      </div>` : ''}
      <div class="field">
        <div class="field-label">${freq === 'quarterly' ? '几号' : '每月几号'}</div>
        <div class="chip-grid">
          ${DAYS_OF_MONTH.map(d => `
            <button class="chip ${dom === d ? 'on' : ''}" data-act="m-dom" data-dom="${d}">${d}</button>`).join('')}
        </div>
        ${dom >= 31 ? '<div class="hint">31 号表示月末：2 月这类没有 31 号的月份会自动落到当月最后一天</div>' : ''}
      </div>` : ''}

      <div class="field">
        <div class="field-label">提醒时间</div>
        <input class="field-input" type="time" id="m-time" value="${e.rule?.time || '17:00'}">
      </div>
    ` : `
      <div class="field">
        <div class="field-label">${e.pattern === 'stage' ? '开始日期' : '截止时间'}</div>
        <div class="time-row">
          <input class="field-input" type="date" id="m-date" value="${toLocalDate(e.dueAt) || today}">
          <input class="field-input" type="time" id="m-time2" value="${toLocalTime(e.dueAt, '18:00')}">
        </div>
      </div>
      ${e.pattern === 'stage' ? `
      <div class="field">
        <div class="field-label">结束日期</div>
        <input class="field-input" type="date" id="m-end" value="${toLocalDate(e.endAt) || today}">
      </div>` : ''}
    `}

    <div class="field">
      <div class="field-label">备注（可选）</div>
      <textarea class="field-textarea" id="m-note" placeholder="补充说明、链接、要点…">${esc(e.note || '')}</textarea>
    </div>
    </div>

    <div class="modal-foot">
      <button class="btn btn-ghost" data-act="close-modal">取消</button>
      <button class="btn btn-primary" data-act="m-save">${e.id ? '保存修改' : '创建工作'}</button>
    </div>`;
  mask.classList.add('open');
  setTimeout(() => restoreModalFocus(keep, 'm-title'), 30);
}

/* ---------------- 交互 ---------------- */
async function toggleTask(id) {
  const t = state.tasks.find(x => x.id === id); if (!t) return;

  // 周期任务打勾 = 做完这一次。后端会留一条完成记录并滚到下一次，
  // 条目不离开待办列表，所以不能像普通任务那样直接改本地状态。
  if (t.pattern === 'recurring' && t.status !== 'done') {
    if (hasTauri) {
      let updated;
      try { updated = await inv('set_task_status', { id, status: 'done' }); }
      catch (e) { console.warn(e); toast('操作失败'); return; }
      Object.assign(t, updated);
      await refreshCompletions();
    } else {
      state.completions.unshift({
        id: -(state.completions.length + 1), taskId: id, title: t.title,
        categoryId: t.categoryId, pattern: 'recurring',
        dueAt: t.dueAt, doneAt: new Date().toISOString(),
      });
      t.dueAt = advanceOccurrence(t.rule, t.dueAt);
    }
    renderAll_();
    toast(`已完成这一次 · 下次 ${relLabel(t.dueAt) || '待定'}`);
    return;
  }

  const next = t.status === 'done' ? 'todo' : 'done';
  t.status = next;
  t.completedAt = next === 'done' ? new Date().toISOString() : null;
  if (hasTauri) { try { await inv('set_task_status', { id, status: next }); } catch (e) { console.warn(e); } }
  renderAll_();
  toast(next === 'done' ? '已完成' : '已恢复');
}

/** 跳过这一次：只让当前这次过去，重复规则本身不动 */
async function skipOccurrence(id) {
  const t = state.tasks.find(x => x.id === id); if (!t) return;
  if (hasTauri) {
    let updated;
    try { updated = await inv('skip_occurrence', { id }); }
    catch (e) { console.warn(e); toast('跳过失败'); return; }
    Object.assign(t, updated);
  } else {
    t.dueAt = advanceOccurrence(t.rule, t.dueAt);
  }
  renderAll_();
  toast(`已跳过这一次 · 下次 ${relLabel(t.dueAt) || '待定'}`);
}

/** 撤销一条周期完成记录，并把该工作退回那一次 */
async function undoCompletion(recId) {
  const i = state.completions.findIndex(c => c.id === recId);
  if (i < 0) return;
  const c = state.completions[i];
  if (hasTauri) {
    try { await inv('undo_completion', { id: recId }); await loadAll(); }
    catch (e) { console.warn(e); toast('撤销失败'); return; }
  } else {
    state.completions.splice(i, 1);
    const t = c.taskId ? state.tasks.find(x => x.id === c.taskId) : null;
    if (t && c.dueAt) t.dueAt = c.dueAt;
  }
  renderAll_();
  toast('已撤销这次完成记录');
}

/** 抽屉里点「完成这一次」：先把抽屉里的改动落盘，再记完成 */
async function completeOccurrence() {
  const t = state.tasks.find(x => x.id === state.drawerId);
  if (!t) return;
  await saveDrawerFields(t);
  state.drawerId = null;
  await toggleTask(t.id);
}

/* ---------------- 子任务 ---------------- */

/** 子任务变动后只刷主区和抽屉：侧栏的计数算的是「一件工作」，不受步骤影响 */
function renderAfterSubtaskChange() { renderView(); renderDrawer(); }

async function addSubtask() {
  const parentId = state.drawerId;
  if (!parentId) return;
  const input = document.getElementById('d-sub-input');
  const title = (input?.value || '').trim();
  if (!title) { input?.focus(); return; }
  input.value = '';

  let created;
  if (hasTauri) {
    try { created = await inv('create_subtask', { parentId, title }); }
    catch (e) {
      console.warn(e);
      toast(String(e).includes('阶段性') ? '只有阶段性工作能拆子任务' : '添加失败');
      return;
    }
  } else {
    created = { id: -(state.subtasks.length + 1), parentId, title, status: 'todo', pattern: 'once', note: '' };
  }
  state.subtasks.push(created);
  applySubtaskProgress();
  renderAfterSubtaskChange();
  document.getElementById('d-sub-input')?.focus();   // 连着加几条不用每次点回来
}

async function toggleSubtask(id) {
  const s = state.subtasks.find(x => x.id === id); if (!s) return;
  const next = s.status === 'done' ? 'todo' : 'done';
  s.status = next;
  s.completedAt = next === 'done' ? new Date().toISOString() : null;
  if (hasTauri) { try { await inv('set_task_status', { id, status: next }); } catch (e) { console.warn(e); } }
  applySubtaskProgress();
  renderAfterSubtaskChange();
}

async function deleteSubtask(id) {
  const i = state.subtasks.findIndex(x => x.id === id); if (i < 0) return;
  state.subtasks.splice(i, 1);
  if (hasTauri) { try { await inv('delete_task', { id }); } catch (e) { console.warn(e); } }
  applySubtaskProgress();
  renderAfterSubtaskChange();
  toast('已删掉这一步');
}

/** 改名落库。名字是空的就退回原名，不留一条没名字的步骤在库里。 */
async function renameSubtask(id, title) {
  const s = state.subtasks.find(x => x.id === id); if (!s) return;
  const v = (title || '').trim();
  if (!v) { renderAfterSubtaskChange(); return; }
  if (v === s.title) return;
  s.title = v;
  if (hasTauri) { try { await inv('rename_subtask', { id, title: v }); } catch (e) { console.warn(e); } }
  renderAfterSubtaskChange();
}

function openEditor(id) {
  if (id) {
    const t = state.tasks.find(x => x.id === id);
    state.editing = JSON.parse(JSON.stringify(t));
  } else {
    state.editing = {
      id: 0, title: '', note: '', categoryId: state.categories[0]?.id || 1,
      pattern: 'once', status: 'todo', dueAt: null, rule: { freq: 'weekly', byDay: [5], time: '17:00' },
    };
  }
  state.catEditing = null;
  modalFocus = null;          // 新开的面板，别把上一条工作的光标位置带过来
  renderModal();
}

async function saveModal() {
  const e = state.editing; if (!e) return;
  const title = document.getElementById('m-title').value.trim();
  if (!title) { toast('请填写要做什么'); return; }
  e.title = title;
  e.note = document.getElementById('m-note').value.trim();

  if (e.pattern === 'recurring') {
    const time = document.getElementById('m-time')?.value || '17:00';
    e.rule = Object.assign({}, e.rule, { freq: e.rule?.freq || 'weekly', time });
    // 兜底收敛：点选时值就写进 state 了，这里防的是「每周却一天没选」
    // 「几号 = 0」这类存进去就再也排不出来的坏数据
    const clamp = (v, lo, hi) => Math.min(Math.max(Number(v) || lo, lo), hi);
    if (e.rule.freq === 'weekly') {
      const days = (e.rule.byDay || []).filter(i => i >= 0 && i <= 6);
      e.rule.byDay = days.length ? days : [5];
    } else if (e.rule.freq === 'monthly') {
      e.rule.byDay = [clamp(e.rule.byDay?.[0], 1, 31)];
    } else if (e.rule.freq === 'quarterly') {
      e.rule.byDay   = [clamp(e.rule.byDay?.[0], 1, 31)];
      e.rule.byMonth = [clamp(e.rule.byMonth?.[0], 1, 12)];
    }
    e.dueAt = null;
  } else {
    const d = document.getElementById('m-date')?.value;
    const t2 = document.getElementById('m-time2')?.value || '18:00';
    e.dueAt = d ? new Date(`${d}T${t2}:00`).toISOString() : null;
    if (e.pattern === 'stage') {
      const ed = document.getElementById('m-end')?.value;
      e.endAt = ed ? new Date(`${ed}T23:59:00`).toISOString() : null;
    }
  }

  if (hasTauri) {
    try {
      e.id ? await inv('update_task', { task: e }) : await inv('create_task', { task: e });
      await loadAll();
    } catch (err) { console.warn(err); }
  } else {
    if (e.id) {
      const i = state.tasks.findIndex(x => x.id === e.id);
      state.tasks[i] = Object.assign({}, state.tasks[i], e);
    } else {
      e.id = Math.max(0, ...state.tasks.map(x => x.id)) + 1;
      e.createdAt = new Date().toISOString();
      state.tasks.push(e);
    }
  }
  state.editing = null;
  renderModal();
  renderAll_();
  toast('已保存');
}

async function deleteTask(id) {
  const i = state.tasks.findIndex(x => x.id === id); if (i < 0) return;
  state.tasks.splice(i, 1);
  // 库那边是 ON DELETE CASCADE，内存里也要跟着清，否则步骤会一直飘着
  state.subtasks = state.subtasks.filter(s => s.parentId !== id);
  if (hasTauri) { try { await inv('delete_task', { id }); } catch (e) { console.warn(e); } }
  state.drawerId = null;
  renderAll_();
  toast('已删除');
}

/* ---------------- 分类管理 ---------------- */
function openCatEditor(id) {
  if (id) {
    const c = state.categories.find(x => x.id === id);
    if (!c) return;
    state.catEditing = JSON.parse(JSON.stringify(c));
  } else {
    state.catEditing = { id: 0, name: '', color: PALETTE[state.categories.length % PALETTE.length] };
  }
  state.editing = null;
  modalFocus = null;
  renderModal();
}

async function saveCategory() {
  const e = state.catEditing; if (!e) return;
  const isNew = !e.id;
  const name = document.getElementById('c-name').value.trim();
  if (!name) { toast('请填写分类名称'); return; }
  if (state.categories.some(c => c.id !== e.id && c.name === name)) { toast('已存在同名分类'); return; }
  e.name = name;

  if (hasTauri) {
    try {
      if (isNew) await inv('create_category', { name, color: e.color });
      else       await inv('update_category', { id: e.id, name, color: e.color });
      await loadAll();
    } catch (err) { console.warn(err); toast('保存失败'); return; }
  } else if (isNew) {
    const nid = Math.max(0, ...state.categories.map(c => c.id)) + 1;
    state.categories.push({ id: nid, name, color: e.color, sort: state.categories.length });
  } else {
    const i = state.categories.findIndex(c => c.id === e.id);
    state.categories[i] = Object.assign({}, state.categories[i], { name, color: e.color });
  }

  state.catEditing = null;
  renderModal();
  renderAll_();
  toast(isNew ? '分类已创建' : '分类已更新');
}

/** 删除分类：其下工作自动回落到「未分类」，不会一起删掉 */
async function deleteCategory(id) {
  const c = state.categories.find(x => x.id === id);
  if (!c) return;
  if (!state.catEditing.confirmDelete) {
    state.catEditing.confirmDelete = true;   // 二次确认，避免误删
    renderModal();
    return;
  }
  const moved = state.tasks.filter(t => t.categoryId === id).length;

  if (hasTauri) {
    try { await inv('delete_category', { id }); await loadAll(); }
    catch (err) { console.warn(err); toast('删除失败'); return; }
  } else {
    state.categories = state.categories.filter(x => x.id !== id);
    state.tasks.forEach(t => { if (t.categoryId === id) t.categoryId = null; });
  }

  if (state.view === 'cat:' + id) state.view = 'all';
  state.catEditing = null;
  renderModal();
  renderAll_();
  toast(moved ? `分类已删除，${moved} 项工作移到「未分类」` : '分类已删除');
}

/* ---------------- 分类拖拽排序 ---------------- */
let dragCatId = null;

function clearDropHint() {
  document.querySelectorAll('.nav-item.drop-before, .nav-item.drop-after')
    .forEach(el => el.classList.remove('drop-before', 'drop-after'));
}

async function moveCategory(fromId, toId, after) {
  if (fromId === toId) return;
  const list = state.categories;
  const from = list.findIndex(c => c.id === fromId);
  if (from < 0) return;
  const [moved] = list.splice(from, 1);
  const to = list.findIndex(c => c.id === toId);
  if (to < 0) { list.splice(from, 0, moved); return; }
  list.splice(after ? to + 1 : to, 0, moved);
  list.forEach((c, i) => { c.sort = i; });
  renderSidebar();
  if (hasTauri) {
    try { await inv('reorder_categories', { ids: list.map(c => c.id) }); }
    catch (e) { console.warn(e); return; }
  }
  toast('顺序已保存');
}

function bindCatDnD() {
  const sb = document.getElementById('sidebar');

  sb.addEventListener('dragstart', ev => {
    const el = ev.target.closest('[data-catdrag]');
    if (!el) return;
    dragCatId = Number(el.dataset.catdrag);
    el.classList.add('dragging');
    if (ev.dataTransfer) {
      ev.dataTransfer.effectAllowed = 'move';
      try { ev.dataTransfer.setData('text/plain', String(dragCatId)); } catch (e) { /* 忽略 */ }
    }
  });

  sb.addEventListener('dragover', ev => {
    if (dragCatId == null) return;
    const el = ev.target.closest('[data-catdrag]');
    if (!el || Number(el.dataset.catdrag) === dragCatId) { clearDropHint(); return; }
    ev.preventDefault();
    if (ev.dataTransfer) ev.dataTransfer.dropEffect = 'move';
    const r = el.getBoundingClientRect();
    const after = ev.clientY > r.top + r.height / 2;
    clearDropHint();
    el.classList.add(after ? 'drop-after' : 'drop-before');
    el.dataset.dropAfter = after ? '1' : '0';
  });

  sb.addEventListener('drop', ev => {
    const el = ev.target.closest('[data-catdrag]');
    if (!el || dragCatId == null) return;
    ev.preventDefault();
    const fromId = dragCatId;
    dragCatId = null;
    moveCategory(fromId, Number(el.dataset.catdrag), el.dataset.dropAfter === '1');
    clearDropHint();
  });

  sb.addEventListener('dragend', () => {
    dragCatId = null;
    clearDropHint();
    document.querySelectorAll('.nav-item.dragging').forEach(el => el.classList.remove('dragging'));
  });
}

/* ---------------- 帮助函数 ---------------- */
function esc(s) {
  return String(s ?? '').replace(/[&<>"']/g, m => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[m]));
}
const escAttr = esc;

/** 把搜索命中的字标出来。备注也显示在列表里之后，
 *  一眼就能看出是标题匹配还是备注匹配，不用挨个打开详情。 */
function hl(text) {
  const s = esc(text);
  if (!state.q) return s;
  const q = esc(state.q).replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  if (!q) return s;
  try { return s.replace(new RegExp(q, 'gi'), m => `<mark>${m}</mark>`); }
  catch { return s; }
}

let toastTimer;
function toast(msg) {
  let el = document.querySelector('.toast');
  if (!el) { el = document.createElement('div'); el.className = 'toast'; document.body.appendChild(el); }
  el.textContent = msg;
  requestAnimationFrame(() => el.classList.add('show'));
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => el.classList.remove('show'), 1600);
}

function renderAll_() { renderSidebar(); renderView(); renderDrawer(); }

/* ---------------- 事件绑定 ---------------- */
function bind() {
  document.body.addEventListener('click', ev => {
    // data-act 优先于 data-nav：分类行里嵌着编辑按钮，不能点按钮却跳了视图
    const act = ev.target.closest('[data-act]');
    if (act) {
      const a = act.dataset.act;
      const id = Number(act.dataset.id);

      if (a === 'toggle')   { toggleTask(id); return; }
      if (a === 'open')     { state.drawerId = id; renderDrawer(); return; }
      if (a === 'close-drawer') { state.drawerId = null; renderDrawer(); return; }
      if (a === 'delete')   { deleteTask(state.drawerId); return; }
      if (a === 'edit')     { const x = state.drawerId; state.drawerId = null; renderDrawer(); openEditor(x); return; }
      if (a === 'close-modal') { state.editing = null; modalFocus = null; renderModal(); return; }

      // 下面几个控件都会让弹窗整块重绘，所以动手前必须先把已输入的内容收回来，
      // 否则新画出来的表单拿的还是旧 state，用户刚敲的标题就没了
      if (a === 'm-pattern') {
        syncEditorFields();
        const p = act.dataset.pattern;
        const e = state.editing;
        if (e) {
          if (p === 'recurring' && !e.rule) e.rule = { freq: 'weekly', byDay: [5], time: '17:00' };
          e.pattern = p;
        }
        renderModal(); return;
      }
      if (a === 'm-cat') {
        syncEditorFields();
        const v = act.dataset.cat;
        if (state.editing) state.editing.categoryId = v === '' ? null : Number(v);
        renderModal(); return;
      }
      if (a === 'm-freq') {
        syncEditorFields();
        const e = state.editing;
        const freq = act.dataset.freq;
        // 频率一变，byDay 的含义就变了（星期几 ⇄ 几号），给它一个说得通的默认值
        if (e) e.rule = Object.assign({}, e.rule, { freq }, defaultRuleFor(freq, e.rule));
        renderModal(); return;
      }
      if (a === 'm-day') {
        syncEditorFields();
        const day = Number(act.dataset.day);
        const cur = state.editing?.rule?.byDay || [];
        const next = cur.includes(day) ? cur.filter(x => x !== day) : cur.concat(day);
        if (state.editing) {
          state.editing.rule = Object.assign({}, state.editing.rule, { byDay: next.length ? next : [day] });
        }
        renderModal(); return;
      }
      if (a === 'm-month' || a === 'm-dom') {
        syncEditorFields();
        const e = state.editing;
        if (e) {
          if (a === 'm-month') e.rule = Object.assign({}, e.rule, { byMonth: [Number(act.dataset.month)] });
          else                 e.rule = Object.assign({}, e.rule, { byDay: [Number(act.dataset.dom)] });
        }
        renderModal(); return;
      }
      if (a === 'm-save')   { saveModal(); return; }
      if (a === 'save')     { saveDrawer(); return; }
      if (a === 'skip')     { skipOccurrence(id); return; }
      if (a === 'undo-completion')     { undoCompletion(id); return; }
      if (a === 'complete-occurrence') { completeOccurrence(); return; }

      // 排布方式（列表 / 看板 / 日历）
      if (a === 'set-mode') { state.mode = act.dataset.mode; renderView(); return; }
      if (a === 'cal-prev' || a === 'cal-next') {
        const base = state.calMonth || new Date();
        state.calMonth = new Date(
          base.getFullYear(),
          base.getMonth() + (a === 'cal-next' ? 1 : -1),
          1,
        );
        renderView();
        return;
      }
      if (a === 'cal-today') { state.calMonth = null; renderView(); return; }

      // 子任务
      if (a === 'sub-toggle')  { toggleSubtask(id); return; }
      if (a === 'sub-del')     { deleteSubtask(id); return; }
      if (a === 'add-subtask') { addSubtask(); return; }

      // 设置项
      if (a === 'toggle-setting') {
        const k = act.dataset.key;
        state.settings = state.settings || {};
        state.settings[k] = settingOn(k) ? '0' : '1';
        renderView();
        if (hasTauri) inv('set_setting', { key: k, value: state.settings[k] }).catch(console.warn);
        return;
      }
      if (a === 'open-data-dir') {
        if (!hasTauri) { toast('浏览器预览模式'); return; }
        inv('open_data_dir').catch(err => { console.warn(err); toast('打开目录失败'); });
        return;
      }

      // 抽屉里的即时修改（点了立刻反映在界面上，保存时一并落库）
      if (a === 'set-pattern') {
        const t = state.tasks.find(x => x.id === state.drawerId);
        if (t) { t.pattern = act.dataset.pattern; renderDrawer(); }
        return;
      }
      if (a === 'set-cat') {
        const t = state.tasks.find(x => x.id === state.drawerId);
        if (t) { const v = act.dataset.cat; t.categoryId = v === '' ? null : Number(v); renderDrawer(); }
        return;
      }

      // 分类管理
      if (a === 'new-category')    { openCatEditor(null); return; }
      if (a === 'edit-cat')        { openCatEditor(Number(act.dataset.cat)); return; }
      if (a === 'c-color')         { state.catEditing.color = act.dataset.color; renderModal(); return; }
      if (a === 'c-save')          { saveCategory(); return; }
      if (a === 'c-delete')        { deleteCategory(state.catEditing.id); return; }
      if (a === 'close-cat-modal') { state.catEditing = null; renderModal(); return; }
    }

    const nav = ev.target.closest('[data-nav]');
    if (nav) { state.view = nav.dataset.nav; renderAll_(); return; }

    if (ev.target.id === 'modal-mask') { state.editing = null; state.catEditing = null; renderModal(); return; }
  });

  document.getElementById('btn-new').addEventListener('click', () => openEditor(null));

  // 记住弹窗里最近聚焦的那个输入框。控件一点就整块重绘，重绘后要照着这条记录
  // 把光标还回去（点击按钮本身会让按钮成为 activeElement，靠它找不回输入框）
  document.addEventListener('focusin', ev => rememberModalFocus(ev.target));

  const s = document.getElementById('search');
  s.addEventListener('input', () => { state.q = s.value.trim(); renderView(); });

  // 子任务改名：输入框失焦或回车时落库。
  // 挂在下层元素上不行——抽屉每次重绘都会重建 DOM，监听器跟着没了。
  document.body.addEventListener('change', ev => {
    const el = ev.target.closest?.('.sub-title');
    if (el) renameSubtask(Number(el.dataset.subTitle), el.value);
  });

  document.addEventListener('keydown', ev => {
    const el = ev.target;
    const typing = el instanceof HTMLElement && /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName);

    if (el?.id === 'd-sub-input' && ev.key === 'Enter') { ev.preventDefault(); addSubtask(); return; }
    if (el?.classList?.contains('sub-title') && ev.key === 'Enter') { ev.preventDefault(); el.blur(); return; }

    if (ev.key === 'Escape') {
      // 正在输入时按 Esc 只退出输入，不顺手把整个面板关掉
      if (typing) { el.blur(); return; }
      if (state.catEditing) { state.catEditing = null; renderModal(); }
      else if (state.editing) { state.editing = null; renderModal(); }
      else if (state.drawerId) { state.drawerId = null; renderDrawer(); }
    }
    if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === 'n') { ev.preventDefault(); openEditor(null); }
  });

  bindCatDnD();

  // 窗口控制
  document.querySelectorAll('[data-win]').forEach(b => {
    b.addEventListener('click', async () => {
      if (!hasTauri) { toast('浏览器预览模式'); return; }
      const w = window.__TAURI__.window.getCurrentWindow();
      const k = b.dataset.win;
      if (k === 'min')   await w.minimize();
      if (k === 'max')   await w.toggleMaximize();
      if (k === 'close') await w.close();
    });
  });
}

/** 把抽屉里的输入写进 task 对象并落库（周期任务没有日期输入框，就不会碰 dueAt） */
async function saveDrawerFields(t) {
  const titleEl = document.getElementById('d-title');
  if (titleEl) t.title = titleEl.value.trim() || t.title;
  const noteEl = document.getElementById('d-note');
  if (noteEl) t.note = noteEl.value;
  const d = document.getElementById('d-date')?.value;
  const tm = document.getElementById('d-time')?.value || '18:00';
  if (d) t.dueAt = new Date(`${d}T${tm}:00`).toISOString();
  const endEl = document.getElementById('d-end');
  if (endEl) t.endAt = endEl.value ? new Date(`${endEl.value}T23:59:00`).toISOString() : null;
  if (hasTauri) { try { await inv('update_task', { task: t }); } catch (e) { console.warn(e); } }
}

async function saveDrawer() {
  const t = state.tasks.find(x => x.id === state.drawerId); if (!t) return;
  await saveDrawerFields(t);
  state.drawerId = null;
  renderAll_();
  toast('已保存');
}

/* ---------------- 后端事件 ---------------- */
/** 后台巡检发出到期通知后会推这个事件，界面上的逾期标记跟着刷新 */
function bindBackendEvents() {
  if (!hasTauri || !window.__TAURI__.event) return;
  const listen = (name, fn) => window.__TAURI__.event.listen(name, fn).catch(console.warn);

  listen('tasks://changed', () => {
    // 正在编辑就先别打断，否则用户敲了一半的标题会被后端数据覆盖
    if (state.drawerId || state.editing || state.catEditing) return;
    loadAll().then(renderAll_).catch(console.warn);
  });
  listen('tray://new-task', () => openEditor(null));

  // 点托盘图标、或托盘菜单里的「今天待办」：窗口已经被后端拎到前面了，这里负责切屏
  listen('tray://open', () => {
    state.view = 'today';
    // 抽屉/弹窗开着就先别重载数据，免得把用户敲了一半的内容冲掉
    if (state.drawerId || state.editing || state.catEditing) { renderAll_(); return; }
    loadAll().then(renderAll_).catch(console.warn);
  });

  // 用户点了系统通知：直接开在那一条工作上，不必自己再去列表里找
  listen('notify://open-task', ev => {
    const id = ev.payload;
    // 通知多半是在窗口收进托盘时点的，先拉一次最新数据再定位
    loadAll()
      .catch(console.warn)
      .then(() => { state.view = 'today'; state.drawerId = id; renderAll_(); });
  });

  // 在托盘上暂停/恢复了提醒，设置页那个开关得跟着变，否则同一件事两处显示相反
  listen('settings://notify', () => {
    inv('get_settings')
      .then(s => {
        state.settings = s || {};
        if (state.view === 'settings') renderView();
      })
      .catch(console.warn);
  });
}

/* ---------------- 启动 ---------------- */
(async function init() {
  await loadAll();
  bind();
  bindBackendEvents();
  renderAll_();
})();
