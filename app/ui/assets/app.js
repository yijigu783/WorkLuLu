/* 工作记录本 (WorkLuLu) — 前端逻辑 */

/** 软件身份。界面上凡是出现名字、版本、版权的地方都从这里取，
 *  免得改一处漏一处。
 *  版本号以打包进 exe 的包信息为准（后端 app_version 命令），
 *  这里的 FALLBACK 只在浏览器预览模式（没有后端）下兜底。 */
const APP_NAME_CN = '工作记录本';
const APP_NAME_EN = 'WorkLuLu';
const APP_VERSION_FALLBACK = '1.6.0';
const APP_COPYRIGHT = '© 2026 JJAI 制作';

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
  subtasks: [],      // 阶段性工作拆出来的步骤（含子任务的子任务），不进主列表
  completions: [],   // 周期任务的完成记录
  templates: [],     // 模板库：把一套反复要用的结构存下来，下次一键重建
  attachments: [],   // 附件的元信息（不含图片本体）。图片本体按需单独取
  thumbCache: new Map(),  // 附件 id → 缩略图 data URL。抽屉每重绘一次都走一遍 IPC 太慢
  lightbox: null,    // 正在看的大图：{ url, name }
  attSeq: 0,         // 新建弹窗里暂存附件的临时编号（那时还没有数据库 id）
  settings: {},
  dataPath: '',
  dataInfo: null,    // 数据存放位置：{ path, portable, portablePath, standardPath, blocked, note }
  appVersion: APP_VERSION_FALLBACK,
  q: '',
  drawerId: null,
  drawerRendered: null,  // 抽屉此刻渲染的是哪个任务：切走前要先收下没保存的输入
  editing: null,
  catEditing: null,
  catPickerOpen: false,  // 新建时分类是自动带上的，这一行只显示一行；点「更改」才铺开整排
  dayView: null,     // 「这一天还有 N 项」弹窗正看着哪一天（YYYY-MM-DD）
  subAddFor: null,   // 正在给哪个子任务加下级：那一行会展开一个输入框
  ctxMenu: null,     // 日历上右键弹出的菜单：{ date, x, y }
  tplApplying: null, // 「从模板新建」正挑哪一天：{ id, name, items, base }
  tplSaving: null,   // 「存为模板」正起名字：{ taskId, name, steps }
  tplPick: false,    // 主界面「从模板新建」正列着模板让人挑
  reward: false,     // 赞赏码弹窗开着没有
  // 贴边隐藏的状态。判定在后端（它才拿得到鼠标绝对位置和显示器边界），
  // 前端收到通知后只用来给收起时露出那条窄边上色，别的地方不要拿它做判断。
  edgeCollapsed: false,
  edgeSide: '',      // 'left' / 'right' / ''（没贴过边）
};

/* ---------------- API 层（Tauri / 浏览器预览双通道） ---------------- */
const hasTauri = typeof window !== 'undefined' && !!window.__TAURI__;
const inv = (cmd, args) => {
  if (hasTauri && window.__TAURI__.core) return window.__TAURI__.core.invoke(cmd, args);
  return Promise.reject(new Error('no tauri'));
};

/* ---------------- 原生文件对话框 ----------------
   插件自己往 window.__TAURI__.dialog 上挂了一层 JS 包装，优先用它 ——
   它把 buttons 之类的参数转成了后端要的格式，比手写 invoke 少踩坑。
   （本项目前端是原生 JS、没有打包器，装不了 @tauri-apps/plugin-dialog，
   靠的是 tauri.conf.json 里 withGlobalTauri + 插件自带的 api-iife.js。）*/
function dialogApi() {
  return (hasTauri && window.__TAURI__ && window.__TAURI__.dialog) || null;
}

const dlg = (cmd, options) => {
  const d = dialogApi();
  if (d && typeof d[cmd] === 'function') return d[cmd](options);
  if (!hasTauri) return Promise.resolve(null);
  // 后路：包装没挂上就自己 invoke，参数格式跟包装里是同一套
  return window.__TAURI__.core.invoke(`plugin:dialog|${cmd}`, { options });
};

/** 原生确认框，返回用户是不是点了「是」 */
function askDialog(message, title) {
  const d = dialogApi();
  if (d && typeof d.ask === 'function') return d.ask(message, { title, kind: 'Warning' });
  if (!hasTauri) return Promise.resolve(false);
  return window.__TAURI__.core
    .invoke('plugin:dialog|message', { message, title, kind: 'warning', buttons: 'YesNo' })
    .then(r => r === 'Yes');
}

/** 备份文件名带时间戳，同一天里连着备份几次也不会互相覆盖 */
function stampName(ext) {
  const d = new Date(), p = n => String(n).padStart(2, '0');
  return `工作记录本-${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}` +
         `-${p(d.getHours())}${p(d.getMinutes())}.${ext}`;
}

/** 对话框默认落在数据目录，用户回头好找 */
function defaultPathFor(name) {
  return state.dataPath ? `${state.dataPath}\\${name}` : name;
}

const DB_FILTER = [{ name: '工作记录本备份', extensions: ['db'] }];

/** 切换数据存放位置（标准 ↔ 便携）。
 *
 *  这一步真的会搬文件，所以先问一次，而且要把「搬什么、搬到哪」说清楚 ——
 *  用户点了确定之后界面上路径会变，不提前说明会以为是程序出问题了。 */
async function togglePortable() {
  if (!hasTauri) { toast('浏览器预览模式'); return; }
  const i = state.dataInfo || {};
  const toPortable = !i.portable;
  const target = (toPortable ? i.portablePath : i.standardPath) || '（未知位置）';
  const yes = await askDialog(
    (toPortable
      ? '把数据搬到程序旁边的 data 文件夹？\n\n数据会整体搬过去，不会丢。搬完之后，整个文件夹拷到 U 盘就能带走。'
      : '把数据搬回系统的用户目录？\n\n数据会整体搬回去。之后程序文件可以随便挪位置，都不影响数据。')
    + `\n\n目标位置：${target}`
    + '\n\n切换前会自动留一份快照。',
    '切换数据存放位置'
  );
  if (!yes) return;
  try {
    const msg = await inv('set_portable', { on: toPortable });
    toast(msg || '已切换');
    await loadAll();
    renderView();
  } catch (e) {
    console.warn(e);
    toast(String(e) || '切换失败');
  }
}

async function backupNow() {
  if (!hasTauri) { toast('浏览器预览模式'); return; }
  try {
    const dest = await dlg('save', {
      title: '备份到',
      defaultPath: defaultPathFor(stampName('db')),
      filters: DB_FILTER,
    });
    if (!dest) return;                      // 用户按了取消
    await inv('backup_to', { dest });
    toast('备份完成');
  } catch (e) {
    toast(`备份失败：${e}`);
  }
}

async function restoreNow() {
  if (!hasTauri) { toast('浏览器预览模式'); return; }
  try {
    const src = await dlg('open', {
      title: '选择要恢复的备份文件',
      multiple: false,
      directory: false,
      filters: DB_FILTER,
    });
    if (!src) return;

    // 这一步会顶掉全部数据，把话说在前面再让用户点
    const yes = await askDialog(
      '恢复会用这个备份完全替换当前的全部工作、分类和完成记录。\n\n' +
      '恢复之前会自动把现在的数据另存到 backups 文件夹，万一选错了还能切回来。',
      '确定要恢复吗？',
    );
    if (!yes) return;

    const snap = await inv('restore_from', { src });
    await loadAll();
    state.drawerId = null;
    state.view = 'settings';
    renderAll_();
    toast('已恢复');
    console.info('[恢复] 恢复前的数据已另存到', snap);
  } catch (e) {
    toast(`恢复失败：${e}`);
  }
}

async function exportCsv() {
  if (!hasTauri) { toast('浏览器预览模式'); return; }
  try {
    const dest = await dlg('save', {
      title: '导出为 CSV',
      defaultPath: defaultPathFor(stampName('csv')),
      filters: [{ name: 'CSV 表格', extensions: ['csv'] }],
    });
    if (!dest) return;
    const n = await inv('export_csv', { dest });
    toast(`已导出 ${n} 条工作`);
  } catch (e) {
    toast(`导出失败：${e}`);
  }
}

/* 浏览器预览用示例数据 */

/** 预览里假装有一张聊天截图。真实的附件是粘贴进来的位图，这里用 SVG 顶一下，
 *  好让附件区在 `make_preview.js` 生成的静态页里也能看到长什么样。 */
function mockShot() {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="180" height="240">
<rect width="180" height="240" fill="#F2F4F7"/>
<rect x="12" y="14" width="118" height="24" rx="7" fill="#FFFFFF"/>
<rect x="12" y="44" width="146" height="24" rx="7" fill="#FFFFFF"/>
<rect x="48" y="74" width="118" height="24" rx="7" fill="#DCE9FF"/>
<rect x="12" y="104" width="96" height="24" rx="7" fill="#FFFFFF"/>
<rect x="48" y="134" width="118" height="24" rx="7" fill="#DCE9FF"/>
<rect x="12" y="164" width="132" height="24" rx="7" fill="#FFFFFF"/>
</svg>`;
  return 'data:image/svg+xml;utf8,' + encodeURIComponent(svg);
}

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
    // 阶段性工作拆出来的步骤，父任务那条的进度就由它们算。
    // 104 下面还有一层：拆解最深支持三层（工作 → 子任务 → 子子任务）
    subtasks: [
      { id: 101, parentId: 2, title: '首页终稿',             status: 'done', pattern: 'once', note: '' },
      { id: 102, parentId: 2, title: '产品页终稿',           status: 'done', pattern: 'once', note: '' },
      { id: 103, parentId: 2, title: '关于页终稿',           status: 'done', pattern: 'once', note: '' },
      { id: 104, parentId: 2, title: '移动端适配',           status: 'todo', pattern: 'once', note: '' },
      { id: 106, parentId: 104, title: '窄屏（< 900px）走查', status: 'done', pattern: 'once', note: '' },
      { id: 107, parentId: 104, title: '触屏点击区放大',      status: 'todo', pattern: 'once', note: '' },
      { id: 105, parentId: 2, title: '交付设计稿源文件',     status: 'todo', pattern: 'once', note: '' },
    ],
    // 模板库示例。日期全是「相对基准日的天数」，不是具体日期——
    // 存「10 月 8 日截止」的话，下个月调用就过期了
    templates: [
      {
        id: 1, name: '季度复盘流程', note: '', createdAt: at(-20, 10),
        items: [
          { id: 1, parentId: null, title: '整理季度复盘材料', note: '', categoryId: 1, pattern: 'stage',
            rule: null, dueOffset: 0, endOffset: 4, dueTime: '09:00', endTime: '23:59', sort: 0 },
          { id: 2, parentId: 1, title: '对齐各条线数据', note: '', categoryId: 1, pattern: 'once',
            rule: null, dueOffset: 0, endOffset: 1, dueTime: '18:00', endTime: '23:59', sort: 1 },
          { id: 3, parentId: 2, title: '找财务要流水', note: '', categoryId: 1, pattern: 'once',
            rule: null, dueOffset: 0, endOffset: 0, dueTime: '12:00', endTime: '23:59', sort: 2 },
          { id: 4, parentId: 2, title: '找运营要转化数据', note: '', categoryId: 1, pattern: 'once',
            rule: null, dueOffset: 1, endOffset: 1, dueTime: '12:00', endTime: '23:59', sort: 3 },
          { id: 5, parentId: 1, title: '出结论页', note: '', categoryId: 1, pattern: 'once',
            rule: null, dueOffset: 3, endOffset: 4, dueTime: '18:00', endTime: '23:59', sort: 4 },
        ],
      },
      {
        id: 2, name: '每月费用对账', note: '', createdAt: at(-40, 10),
        items: [
          { id: 6, parentId: null, title: '月度费用对账', note: '', categoryId: 2, pattern: 'recurring',
            rule: { freq: 'monthly', byDay: [1], time: '10:00' },
            dueOffset: null, endOffset: null, dueTime: null, endTime: null, sort: 0 },
          { id: 7, parentId: 6, title: '导出上月全部流水', note: '', categoryId: 2, pattern: 'once',
            rule: null, dueOffset: 0, endOffset: 0, dueTime: '18:00', endTime: '23:59', sort: 1 },
          { id: 8, parentId: 6, title: '逐笔核对并标注差异', note: '', categoryId: 2, pattern: 'once',
            rule: null, dueOffset: 1, endOffset: 1, dueTime: '18:00', endTime: '23:59', sort: 2 },
        ],
      },
    ],
    // 附件示例：一张「聊天截图」+ 一个文件。预览里能看到附件区长什么样
    attachments: [
      { id: 1, taskId: 1, name: '和甲方的沟通记录.png', mime: 'image/png', size: 184320,
        kind: 'image', hasThumb: true, createdAt: at(-1, 15), url: mockShot() },
      { id: 2, taskId: 1, name: '报价单.pdf', mime: 'application/pdf', size: 98304,
        kind: 'file', hasThumb: false, createdAt: at(-1, 15) },
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
      const [categories, tasks, settings, dataInfo, completions, subtasks, templates, version, attachments] = await Promise.all([
        inv('list_categories'), inv('list_tasks'), inv('get_settings'),
        inv('data_info'), inv('list_completions', { limit: 1000 }), inv('list_subtasks'),
        inv('list_templates'),
        // 版本号只影响「关于」那几行字，读不到也不该把整次加载带崩
        inv('app_version').catch(() => APP_VERSION_FALLBACK),
        // 附件的元信息很小，一次拿全，列表/看板上的「有附件」标记就不用再逐条问
        inv('list_attachments').catch(() => []),
      ]);
      state.categories = categories;
      state.tasks = tasks;
      state.settings = settings || {};
      state.dataInfo = dataInfo || null;
      state.dataPath = (dataInfo && dataInfo.path) || '';
      state.completions = completions || [];
      state.subtasks = subtasks || [];
      state.templates = templates || [];
      state.attachments = attachments || [];
      state.appVersion = version || APP_VERSION_FALLBACK;
      applySubtaskProgress();
      return;
    } catch (e) { console.warn('后端调用失败，启用预览数据', e); }
  }
  const m = mockData();
  state.categories = m.categories;
  state.tasks = m.tasks;
  state.completions = m.completions;
  state.subtasks = m.subtasks || [];
  state.templates = m.templates || [];
  state.attachments = m.attachments || [];
  state.settings = {};
  state.dataPath = 'C:\\Users\\<你>\\AppData\\Roaming\\工作记录本';
  state.dataInfo = {
    path: state.dataPath,
    portable: false,
    portablePath: 'D:\\WorkLuLu\\data',
    standardPath: state.dataPath,
    blocked: '',
    note: null,
  };
  state.appVersion = APP_VERSION_FALLBACK;
  applySubtaskProgress();
}

/** 直接挂在这一层下面的步骤 */
const subtasksOf = parentId => state.subtasks.filter(s => s.parentId === parentId);

/** 这件工作下面拆出来的**全部**步骤，含子任务的子任务（深度优先展平）。
 *
 *  进度按全部步骤算，不是只算直接子任务 —— 否则「A 底下还分了三小步」时，
 *  A 这一层勾没勾就成了唯一计分项，下面干到哪了一步全被吞掉。 */
function descendantsOf(id) {
  const out = [];
  const walk = pid => subtasksOf(pid).forEach(s => { out.push(s); walk(s.id); });
  walk(id);
  return out;
}

/** 拆解层数上限。顶层工作算第 1 层，与后端 MAX_DEPTH 同一口径。 */
const MAX_DEPTH = 3;

/** 某个子任务在第几层（顶层工作算 1 层） */
function depthOf(id) {
  let d = 1;
  let cur = state.subtasks.find(s => s.id === id);
  while (cur && d < 64) { d++; cur = state.subtasks.find(s => s.id === cur.parentId); }
  return d;
}

/** 还能不能再往下拆：超过上限的那一层，界面上直接不给「+」入口 */
const canNest = id => depthOf(id) < MAX_DEPTH;

/** 阶段性工作的进度由子任务算出来，不给人手填的机会——少一个能填错的地方。
 *  没有子任务时就是 0/0，界面显示「未拆解」。 */
function applySubtaskProgress() {
  state.tasks.forEach(t => {
    if (t.pattern !== 'stage') return;
    const kids = descendantsOf(t.id);
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
      ${navItem('tpl', '模板', '#0EA5E9', state.templates.length || '')}
      ${navItem('stats', '统计看板', '#8B5CF6', '')}
      ${navItem('settings', '设置', '#94A3B8', '')}
      <button class="side-reward" data-act="open-reward"
              title="软件免费使用；赞赏纯属自愿，不影响任何功能">
        <svg class="ic" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M8 13.2S2.6 10 2.6 6.4A2.9 2.9 0 0 1 8 4.9a2.9 2.9 0 0 1 5.4 1.5c0 3.6-5.4 6.8-5.4 6.8z"/>
        </svg>
        <span>赞赏支持</span>
      </button>
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
        <div class="task-meta">${overdue ? `<span class="overdue">${metaLabel(t, cat)}</span>` : metaLabel(t, cat)}${attBadge(t.id)}</div>
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

/** 当前视图隐含的分类。
 *
 *  - 分类页 → 那个分类的 id
 *  - 「未分类」页 → null
 *  - 其它视图（今天 / 全部 / 已完成 / 设置）→ undefined，表示**没有上下文**
 *
 *  区分 null 和 undefined 是有意的：null 是「明确地属于未分类」，
 *  undefined 是「这个视图压根不涉及分类」。新建工作时两者处理方式不同。
 *  分类被删掉（数据从别处改过）时也返回 undefined —— 宁可退回让人选，也不要写进一个不存在的 id。 */
function viewCategoryId() {
  const v = state.view || '';
  if (v === 'cat:none') return null;
  if (v.startsWith('cat:')) {
    const id = Number(v.slice(4));
    return catById(id) ? id : undefined;
  }
  return undefined;
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
        <div class="board-card-meta">${esc(meta)}${attBadge(t.id)}</div>
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

/** 某一天在日历上要显示的条目，按「交期优先」排好。
 *
 *  交期那天的条目 kind 为 'due'；阶段性工作的跨度内其余各天给一条 'span'。
 *  只标交期的话，一个跨三周的大任务在日历上只出现一格，
 *  中间这段时间在忙什么、有多忙，完全看不出来。
 *
 *  `pool` 是已经按当前视图筛过的工作集合；不传就自己算一次。 */
function entriesOn(dateStr, pool) {
  if (!dateStr) return [];
  const key = startOfDay(new Date(`${dateStr}T00:00:00`)).getTime();
  const out = [];
  (pool || scopeTasks()).forEach(t => {
    const dl = deadlineOf(t);
    if (!dl) return;
    const to = startOfDay(new Date(dl)).getTime();
    if (to === key) { out.push({ t, kind: 'due' }); return; }
    if (t.pattern !== 'stage' || !t.dueAt || !t.endAt) return;
    const from = startOfDay(new Date(t.dueAt)).getTime();
    // 跨度超过一年多半是数据有问题，别让循环跑飞
    if ((to - from) / DAY > 400) return;
    if (key >= from && key < to) out.push({ t, kind: 'span' });
  });
  // 交期排在跨度条前面——同一天里那是重点，也是格子里优先露出来的那几条
  return out.sort((a, b) => {
    if (a.kind !== b.kind) return a.kind === 'due' ? -1 : 1;
    return a.t.id - b.t.id;
  });
}

function renderCalendar() {
  const base = state.calMonth || new Date();
  const y = base.getFullYear();
  const m = base.getMonth();
  const first = new Date(y, m, 1);

  const pool = scopeTasks();

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
    const entries = entriesOn(toLocalDate(d), pool);
    const shown = entries.slice(0, 3);
    const more = entries.length - shown.length;

    grid += `
      <div class="cal-cell ${d.getMonth() !== m ? 'is-out' : ''} ${k === todayKey ? 'is-today' : ''}"
           data-act="cal-new" data-date="${toLocalDate(d)}" title="在这一天新建工作">
        <div class="cal-date">${d.getDate()}</div>
        <div class="cal-items">
          ${shown.map(({ t, kind }) => {
            const cat = catById(t.categoryId);
            // 标题单独包一层：flex 容器上直接写 text-overflow 不管用，
            // 文本会被当成匿名 flex 项，省不掉省略号
            return `<div class="cal-item ${kind === 'span' ? 'is-span' : ''} ${t.status === 'done' ? 'is-done' : ''}"
                         data-act="open" data-id="${t.id}"
                         title="${escAttr(t.title)}${kind === 'span' ? '（进行中）' : ''}">
              <span class="cal-dot" style="background:${cat ? cat.color : '#CBD5E1'}"></span>
              <span class="cal-text">${esc(t.title)}</span>
            </div>`;
          }).join('')}
          ${more > 0
            ? `<div class="cal-more" data-act="cal-day" data-date="${toLocalDate(d)}">还有 ${more} 项</div>`
            : ''}
        </div>
      </div>`;
  }

  const now = new Date();
  const offMonth = y !== now.getFullYear() || m !== now.getMonth();

  return `
    <div class="view-head">
      <div class="view-title">日历</div>
      <div class="view-sub">按截止日铺开。点一条看详情，点空白处在那天新建，滚轮翻月</div>
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

/* ---------------- 模板库：把一套结构存下来，下次一键重建 ---------------- */

/** 模板卡片上那行摘要：几件工作、要不要分层、跨度多少天。
 *  模板页和主界面的「从模板新建」共用 —— 两处各写一份，
 *  迟早会出现同一个模板两种说法。 */
function tplMeta(t) {
  const items = t.items || [];
  const top = items.filter(i => i.parentId == null).length;
  const offs = items.map(i => i.dueOffset).filter(v => v != null);
  const span = offs.length ? `跨度 ${Math.min(0, ...offs)} – ${Math.max(0, ...offs)} 天` : '';
  // 最深到第几层：模板里存的下级关系看不出深浅，得算一遍才说得清
  const byId = new Map(items.map(i => [i.id, i]));
  const depth = items.reduce((mx, i) => {
    let d = 1, cur = i;
    while (cur && cur.parentId != null && d < 64) { d++; cur = byId.get(cur.parentId); }
    return Math.max(mx, d);
  }, 0);

  return [
    `${top} 件工作`,
    depth > 1 ? `${depth} 层结构` : '',
    `${items.length} 个条目`,
    span,
  ].filter(Boolean).join(' · ');
}

function renderTemplates() {
  const list = state.templates || [];

  const cards = list.map(t => `
      <div class="tpl-card">
        <div class="tpl-main">
          <div class="tpl-name">${esc(t.name)}</div>
          <div class="tpl-meta">${esc(tplMeta(t))}</div>
        </div>
        <div class="tpl-actions">
          <button class="btn btn-primary" data-act="tpl-apply" data-id="${t.id}">一键新建</button>
          <button class="btn btn-danger-ghost" data-act="tpl-del" data-id="${t.id}">删除</button>
        </div>
      </div>`).join('');

  return `
    <div class="view-head">
      <div class="view-title">模板</div>
      <div class="view-sub">${list.length
        ? `${list.length} 个模板 · 里面存的是相对天数，挑哪天调用就从哪天铺开，不会过期`
        : '把反复要做的一整套结构存下来，下次一键重建'}</div>
    </div>
    ${list.length ? `<div class="tpl-list">${cards}</div>` : `
      <div class="empty">
        还没有模板。<br>
        打开任意一条工作的详情，点「存为模板」，就能把它连同下面的步骤一起存下来。
      </div>`}`;
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
  { key: 'autostart', title: '开机自动启动', desc: '登录 Windows 后在托盘静默启动。默认不开，需要它常驻后台时再打开' },
  { key: 'notify',    title: '到期提醒',     desc: '任务到期时弹出系统通知' },
  { key: 'tray',      title: '关闭窗口时最小化到托盘', desc: '关闭后继续在后台运行，保证提醒准时' },
  { key: 'snap',      title: '拖到屏幕边缘自动分屏', desc: '拖到左/右边缘变半屏，拖到左上/右上角变四分之一，拖到顶边最大化' },
  { key: 'edge',      title: '贴边自动隐藏', desc: '把窗口拖到屏幕左边缘或右边缘，它会滑到一边只留一条窄边；鼠标碰一下再滑回来' },
];

/** 默认是「关」的开关。
 *  用户没动过设置时，其余开关默认开（它们只是程序内部的行为）；
 *  开机自启必须默认关 —— 它要往系统里写启动项，
 *  不该在用户没明确同意的情况下替他做这个决定。
 *  这也是杀软行为引擎最敏感的动作之一，少写一次就少一分被误报的理由。
 *
 *  贴边隐藏同样默认关：拖窗口贴边是很自然的动作，
 *  默认开的话每个人某天都会遇到「窗口不见了」，而且看不出是自己拖出去造成的。 */
const DEFAULT_OFF = { autostart: true, edge: true };

function settingOn(key) {
  const v = state.settings?.[key];
  if (v === undefined || v === null || v === '') return !DEFAULT_OFF[key];
  return v === '1' || v === 'true' || v === 'on' || v === true;
}

/** 开关之间的连带关系当场讲清楚。
 *  提醒是程序自己在后台轮询发出来的 —— 程序不跑就没有提醒。
 *  用户关掉开机自启后，很自然会以为「提醒还开着 = 到点会响」，这里得拦住这个误会。 */
function settingHint(s) {
  const needsBackground = s.key === 'notify' && settingOn('notify') && !settingOn('autostart');
  if (needsBackground) {
    return `<div class="task-meta warn-text">提醒要靠程序在后台跑着才发得出来。没开开机自启，重启电脑后就收不到提醒了。</div>`;
  }
  // 贴边隐藏和边缘分屏抢的是同两条边：窗口拖到左边到底是「摆半屏」还是「藏起来」，
  // 只能有一个说了算。谁被打开，另一个就被关掉，这里把这个连带关系讲明白，
  // 否则用户会看到开关自己跳回去了，还以为程序出 bug。
  if (s.key === 'snap' && settingOn('edge')) {
    return `<div class="task-meta warn-text">和「贴边自动隐藏」只能留一个：它们盯的是同一条屏幕边。打开分屏，贴边隐藏会自动关掉。</div>`;
  }
  if (s.key === 'edge' && settingOn('snap')) {
    return `<div class="task-meta warn-text">和「拖到屏幕边缘自动分屏」只能留一个：它们盯的是同一条屏幕边。打开贴边隐藏，分屏会自动关掉。</div>`;
  }
  return '';
}

/** 「数据存放位置」这张卡片。
 *
 *  便携模式的两条触发（exe 旁的 portable.txt，或一个名为 data 的目录）判定在后端，
 *  这里只负责把话讲明白：现在存哪儿、怎么切、切了会发生什么、以及切不了的理由。
 *
 *  `blocked` 非空时必须显眼 —— exe 放在 Program Files 这类只读位置时给不出便携模式，
 *  不写清楚的话用户只会看到「点了没反应」。 */
function portableCard() {
  const i = state.dataInfo || {};
  const portable = !!i.portable;
  const blocked = i.blocked || '';
  const target = (portable ? i.standardPath : i.portablePath) || '';
  const path = state.dataPath || '（还没读到，点「打开目录」试试）';
  return `
      <div class="task" style="padding:14px">
        <div class="task-main">
          <div class="task-title">数据存放位置${portable ? '<span class="mode-chip">便携</span>' : ''}</div>
          <div class="task-meta path-line">${esc(path)}</div>
          <div class="task-meta">${portable
            ? '数据就在程序旁边的 data 文件夹里，整个文件夹拷到 U 盘就能带走。'
            : '数据存在系统用户目录，程序文件可以随便挪位置、覆盖升级都不影响。'}</div>
          ${blocked ? `<div class="task-meta warn-text">这里切不了便携模式：${esc(blocked)}</div>` : ''}
        </div>
        <div class="task-acts">
          <button class="btn btn-ghost" data-act="open-data-dir">打开目录</button>
          <button class="btn btn-ghost" data-act="toggle-portable" title="${escAttr(target)}">${portable ? '切回标准模式' : '切为便携模式'}</button>
        </div>
      </div>`;
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
            ${settingHint(s)}
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
          <div class="task-title">备份数据</div>
          <div class="task-meta">导出成一个独立的数据库文件，拷到 U 盘或网盘都行</div>
        </div>
        <button class="btn btn-ghost" data-act="backup-now">备份…</button>
      </div>
      <div class="task" style="padding:14px">
        <div class="task-main">
          <div class="task-title">从备份恢复</div>
          <div class="task-meta">会用它替换当前全部数据；替换前自动把现在的数据另存一份，选错了还能切回来</div>
        </div>
        <button class="btn btn-ghost" data-act="restore-now">选择文件…</button>
      </div>
      <div class="task" style="padding:14px">
        <div class="task-main">
          <div class="task-title">导出为表格</div>
          <div class="task-meta">存成 CSV，Excel / WPS 直接打开，中文不乱码</div>
        </div>
        <button class="btn btn-ghost" data-act="export-csv">导出 CSV</button>
      </div>
      ${portableCard()}
    </div>

    <div class="section-head" style="margin-top:22px">
      <span class="section-title">关于</span><span class="section-line"></span>
    </div>
    <div class="task-list">
      <div class="task" style="padding:14px">
        <div class="task-main">
          <div class="task-title">
            ${APP_NAME_CN}<span class="name-en" style="margin-left:7px">${APP_NAME_EN}</span>
          </div>
          <div class="task-meta">版本 ${esc(state.appVersion || APP_VERSION_FALLBACK)} · 单文件绿色版，拷到哪儿都能跑</div>
          <div class="task-meta">${APP_COPYRIGHT}</div>
        </div>
        <button class="btn btn-ghost" data-act="open-reward">赞赏…</button>
      </div>
    </div>`;
}

function renderView() {
  const v = state.view;
  // 这几屏没有「列表 / 看板 / 日历」可选，不挂右上角的开关
  const plain = v === 'stats' || v === 'settings' || v === 'tpl';
  let html;

  if (v === 'stats')                 html = renderStats();
  else if (v === 'settings')         html = renderSettings();
  else if (v === 'tpl')              html = renderTemplates();
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

/* ---------------- 附件：粘贴图片 / 上传文件 ----------------
 *
 *  场景来自使用反馈：把跟别人的聊天截图直接粘进来留痕。
 *
 *  三条约定：
 *  1. 图片本体存进数据库（见 commands.rs 的 attachments 表），所以备份仍然是
 *     「一个 .db 拷走就是全部」，恢复、便携模式都不用改。
 *  2. 列表只取缩略图，原图等点开才取 —— 否则抽屉一打开就要搬几十 MB。
 *  3. 粘贴进来的图片先压一道再送后端：聊天截图动辄好几 MB，原样入库会让库
 *     和备份文件一起失控。压缩在 canvas 里做，不走后端，粘完立刻能看到。
 */

/** 与后端 MAX_ATTACHMENT_BYTES 同一口径 */
const MAX_ATT_BYTES = 20 * 1024 * 1024;
/** 原图最长边上限。够看清聊天记录里的字，又不至于把库撑起来 */
const IMG_MAX_EDGE = 2200;
/** 列表缩略图的最长边 */
const ATT_THUMB_EDGE = 360;

/** 抽屉 / 弹窗此刻能往哪儿加附件。弹窗压在最上面，所以它优先。 */
function currentAttScope() {
  if (state.editing) return 'modal';
  if (state.drawerId) return 'drawer';
  return null;
}

/** 某个作用域此刻该显示哪些附件。
 *  弹窗里编辑一条**已存在**的工作时直接用库里那份，不用另存一份列表。 */
function attScopeList(scope) {
  if (scope === 'modal') {
    const e = state.editing;
    if (!e) return [];
    return e.id ? state.attachments.filter(a => a.taskId === e.id) : (e.atts || []);
  }
  if (!state.drawerId) return [];
  return state.attachments.filter(a => a.taskId === state.drawerId);
}

/** 一个附件在界面上的唯一标识：已入库的用 id，暂存的用临时 key */
const attKey = a => (a.pending ? a.key : String(a.id));

function findAtt(scope, key) {
  return attScopeList(scope).find(a => attKey(a) === key) || null;
}

/** 这条工作挂了几张图 / 几个文件。0 表示没有，界面就不显示这个标记。 */
const attCountOf = taskId => state.attachments.filter(a => a.taskId === taskId).length;

/** 列表和看板上那个「有留痕」的小标记。
 *  没有它的话，用户不逐条打开详情就不知道哪条存了截图 —— 留痕的价值有一半在「找得到」。 */
function attBadge(taskId) {
  const n = attCountOf(taskId);
  if (!n) return '';
  return `<span class="att-badge" title="有 ${n} 个附件（截图 / 文件）">`
       + '<svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.4" '
       + 'stroke-linecap="round" stroke-linejoin="round">'
       + '<path d="M11.4 6.1 7.2 10.3a2.4 2.4 0 0 1-3.4-3.4l4.6-4.6a1.7 1.7 0 0 1 2.4 2.4l-4.6 4.6a.9.9 0 0 1-1.3-1.3l4-4"/></svg>'
       + `${n}</span>`;
}

/** 只把列表行 / 看板卡上那条「有留痕」小标记改掉，不重画整屏。
 *  renderView 会把主区滚回顶部，用户正开着详情抽屉删附件时那一下很讨厌，
 *  而这里真正变的只有那个数字。 */
function refreshAttBadge(taskId) {
  const rows = document.querySelectorAll(
    `.task[data-id="${taskId}"], .board-card[data-id="${taskId}"]`);
  if (!rows || !rows.length) return;
  const want = attCountOf(taskId);
  for (const row of rows) {
    const meta = row.querySelector('.task-meta') || row.querySelector('.board-card-meta');
    if (!meta) continue;
    const old = meta.querySelector('.att-badge');
    if (old) old.remove();
    if (want) meta.insertAdjacentHTML('beforeend', attBadge(taskId));
  }
}

function fmtSize(n) {
  if (!n) return '';
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${Math.round(n / 1024)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

function extOf(name) {
  const m = /\.([A-Za-z0-9]{1,5})$/.exec(name || '');
  return m ? m[1].toUpperCase() : '文件';
}

function attCard(a, scope) {
  const isImg = a.kind === 'image';
  const key = attKey(a);
  // 已入库的图片第一次渲染时缩略图还没取回来，先留空，由 hydrateThumbs 补上
  const cached = a.url || (!a.pending ? state.thumbCache.get(a.id) : '') || '';
  const body = isImg
    ? `<img class="att-thumb" alt=""${cached ? ` src="${cached}"` : ''}${a.pending ? '' : ` data-att-thumb="${a.id}"`}>`
    : `<span class="att-file"><b>${esc(extOf(a.name))}</b></span>`;
  const tip = `${a.name}${a.size ? ' · ' + fmtSize(a.size) : ''}`;
  return `
    <div class="att-card">
      <button class="att-hit" data-act="att-open" data-scope="${scope}"
              data-key="${key}" title="${escAttr(tip)}${isImg ? '（点开看大图）' : '（用系统程序打开）'}">${body}</button>
      <span class="att-name" title="${escAttr(tip)}">${esc(a.name)}</span>
      <button class="att-del" data-act="att-del" data-scope="${scope}" data-key="${key}" title="移除">
        <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M3.5 3.5l7 7M10.5 3.5l-7 7"/></svg>
      </button>
    </div>`;
}

function attFieldInner(scope) {
  const list = attScopeList(scope);
  return `
    <div class="field-label">附件${list.length ? `<span class="sub-progress">${list.length}</span>` : ''}</div>
    <div class="att-zone" data-att-zone="${scope}">
      <div class="att-zone-main">
        <svg class="ic" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
          <path d="M8 10.6V2.4M5.2 5.2 8 2.4l2.8 2.8"/><path d="M2.4 10.4v2.2a1 1 0 0 0 1 1h9.2a1 1 0 0 0 1-1v-2.2"/>
        </svg>
        <span>把聊天截图<kbd>Ctrl</kbd>+<kbd>V</kbd> 直接粘贴进来，或把文件拖到这里</span>
      </div>
      <button class="btn btn-ghost" data-act="att-pick" data-scope="${scope}">选择文件</button>
    </div>
    ${list.length ? `<div class="att-grid">${list.map(a => attCard(a, scope)).join('')}</div>` : ''}`;
}

/** 只重绘附件这一块。整块重绘弹窗会把用户敲了一半的表单冲掉，
 *  抽屉重绘则会把滚动位置带跑，为加一张图不值当。 */
function rerenderAttScope(scope) {
  const el = document.getElementById(scope === 'modal' ? 'att-field-modal' : 'att-field-drawer');
  if (el) el.innerHTML = attFieldInner(scope);
  hydrateThumbs(scope);
}

/** 把还没取回来的缩略图补上。已经取过的走 thumbCache，不重复走 IPC。 */
async function hydrateThumbs(scope) {
  if (!hasTauri) return;
  for (const a of attScopeList(scope)) {
    if (a.pending || !a.hasThumb || state.thumbCache.has(a.id)) continue;
    if (!document.querySelector(`img[data-att-thumb="${a.id}"]`)) continue;
    try {
      const url = await inv('get_attachment', { id: a.id, thumb: true });
      state.thumbCache.set(a.id, url);
      // 取图期间界面可能已经重绘过，重新查一次再赋值，别写进已经作废的节点
      const img = document.querySelector(`img[data-att-thumb="${a.id}"]`);
      if (img) img.src = url;
    } catch (e) { console.warn(e); }
  }
}

/* ---- 采集：粘贴 / 拖拽 / 选文件 ---- */

function pickFiles(scope) {
  if (!hasTauri) { toast('浏览器预览模式'); return; }
  const input = document.createElement('input');
  input.type = 'file';
  input.multiple = true;
  input.style.display = 'none';
  document.body.appendChild(input);
  input.addEventListener('change', () => {
    const files = Array.from(input.files || []);
    input.remove();
    handleFiles(files, scope);
  });
  input.click();
}

async function handleFiles(files, scope) {
  const list = (files || []).filter(Boolean);
  if (!list.length) return;
  let ok = 0;
  for (const f of list) {
    try { await addOneFile(f, scope); ok++; }
    catch (e) { console.warn(e); toast((e && e.message) || '这个文件加不进来'); }
  }
  if (ok) toast(ok === 1 ? '已添加附件' : `已添加 ${ok} 个附件`);
}

async function addOneFile(file, scope) {
  const isImg = (file.type || '').startsWith('image/');
  let payload;
  if (isImg) {
    payload = await prepareImage(file);
  } else {
    if (file.size > MAX_ATT_BYTES) {
      throw new Error(`单个附件不能超过 ${MAX_ATT_BYTES / 1024 / 1024} MB`);
    }
    const dataB64 = await readAsBase64(file);
    if (!dataB64) throw new Error('这个文件是空的');
    payload = {
      name: file.name || '未命名文件',
      mime: file.type || 'application/octet-stream',
      dataB64, thumbB64: null, size: file.size,
    };
  }

  const meta = {
    name: payload.name, mime: payload.mime, size: payload.size,
    kind: isImg ? 'image' : 'file', hasThumb: !!payload.thumbB64,
  };

  // 新建弹窗里这条工作还没建出来，先在内存里放着，保存时再一起入库
  if (scope === 'modal' && !state.editing?.id) {
    state.editing.atts = state.editing.atts || [];
    state.editing.atts.push(Object.assign({}, meta, {
      key: 'p' + (++state.attSeq),
      pending: true,
      url: payload.thumbB64 ? `data:image/jpeg;base64,${payload.thumbB64}` : null,
      dataB64: payload.dataB64,
      thumbB64: payload.thumbB64,
    }));
    rerenderAttScope(scope);
    return;
  }

  const taskId = scope === 'modal' ? state.editing.id : state.drawerId;
  if (!taskId) throw new Error('先保存这条工作，再添加附件');

  // 浏览器预览（make_preview 生成的静态副本）没有后端可存，就地挂在内存里，
  // 好让预览能把整个流程演示出来。和 toggleTask / saveModal 里的预览分支一个路子。
  if (!hasTauri) {
    state.attachments.push(Object.assign({}, meta, {
      id: -(state.attachments.length + 1),
      taskId,
      url: payload.thumbB64 ? `data:image/jpeg;base64,${payload.thumbB64}` : null,
    }));
    rerenderAttScope(scope);
    refreshAttBadge(taskId);
    return;
  }

  const saved = await inv('add_attachment', {
    taskId, name: meta.name, mime: meta.mime, kind: meta.kind,
    data: payload.dataB64, thumb: payload.thumbB64,
  });
  state.attachments.push(saved);
  // 刚生成的缩略图直接进缓存，省一次往返
  if (payload.thumbB64) state.thumbCache.set(saved.id, `data:image/jpeg;base64,${payload.thumbB64}`);
  rerenderAttScope(scope);
  // 列表行上的「有留痕」标记要跟着涨。只改那个标记，不动抽屉 ——
  // 抽屉刚操作过，整块重画会把用户正在写的标题/备注冲掉
  refreshAttBadge(taskId);
}

/** 新建保存后把暂存的附件补进库。失败一个不影响其余，也不影响工作本身已经建好 */
async function flushPendingAttachments(taskId) {
  const pend = ((state.editing && state.editing.atts) || []).filter(a => a.pending);
  for (const a of pend) {
    try {
      const saved = await inv('add_attachment', {
        taskId, name: a.name, mime: a.mime, kind: a.kind, data: a.dataB64, thumb: a.thumbB64,
      });
      state.attachments.push(saved);
      if (a.thumbB64) state.thumbCache.set(saved.id, `data:image/jpeg;base64,${a.thumbB64}`);
    } catch (e) {
      console.warn(e);
      toast(`附件「${a.name}」没能存上，其余照常`);
    }
  }
}

/* ---- 图片压缩：在 canvas 里做，不经过后端 ---- */

function readAsBase64(file) {
  return new Promise((res, rej) => {
    const r = new FileReader();
    r.onload = () => res(String(r.result).split(',')[1] || '');
    r.onerror = () => rej(new Error('读文件失败'));
    r.readAsDataURL(file);
  });
}

function loadBitmap(file) {
  if (typeof createImageBitmap === 'function') return createImageBitmap(file);
  // 兜底：老 WebView 没有 createImageBitmap
  return new Promise((res, rej) => {
    const img = new Image();
    img.onload = () => res(img);
    img.onerror = () => rej(new Error('这张图读不出来'));
    img.src = URL.createObjectURL(file);
  });
}

/** 等比缩到最长边不超过 maxEdge；本来就够小就原样返回，不做无谓的重采样 */
function drawScaled(src, maxEdge) {
  const w = src.width, h = src.height;
  const scale = Math.min(1, maxEdge / Math.max(w, h));
  const c = document.createElement('canvas');
  c.width = Math.max(1, Math.round(w * scale));
  c.height = Math.max(1, Math.round(h * scale));
  const ctx = c.getContext('2d');
  ctx.drawImage(src, 0, 0, c.width, c.height);
  return c;
}

function stampShort() {
  const d = new Date(), p = n => String(n).padStart(2, '0');
  return `${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
}

/** 把粘贴/选中的图片压成「原图 + 缩略图」两份 base64。
 *
 *  原图优先用 PNG：聊天截图里全是文字，JPEG 的块效应会把字糊掉，而留痕最要紧的
 *  就是字还认不认得出。只有 PNG 压完仍然很大（长截图、照片）才退一步用 JPEG。 */
async function prepareImage(file) {
  const bmp = await loadBitmap(file);
  const full = drawScaled(bmp, IMG_MAX_EDGE);

  let mime = 'image/png';
  let dataUrl = full.toDataURL('image/png');
  if (dataUrl.length > 4 * 1024 * 1024) {
    mime = 'image/jpeg';
    dataUrl = full.toDataURL('image/jpeg', 0.9);
  }
  const thumbUrl = drawScaled(bmp, ATT_THUMB_EDGE).toDataURL('image/jpeg', 0.8);
  if (typeof bmp.close === 'function') bmp.close();

  const name = file.name || `粘贴图片-${stampShort()}.${mime === 'image/png' ? 'png' : 'jpg'}`;
  return {
    name,
    mime,
    dataB64: dataUrl.split(',')[1] || '',
    thumbB64: thumbUrl.split(',')[1] || '',
    // 压缩后的大小：base64 每 4 个字符还原 3 个字节，减掉填充误差即可
    size: Math.round((dataUrl.length - dataUrl.indexOf(',') - 1) * 3 / 4),
  };
}

/* ---- 单个附件的操作 ---- */

async function viewAttachment(a) {
  try {
    let url = a.url;
    if (!url) {
      if (a.pending) url = `data:${a.mime};base64,${a.dataB64}`;
      else if (hasTauri) url = await inv('get_attachment', { id: a.id, thumb: false });
    }
    if (!url) { toast('浏览器预览模式'); return; }
    state.lightbox = { url, name: a.name };
    renderLightbox();
  } catch (e) {
    console.warn(e);
    toast('这张图打不开了');
  }
}

/** 点附件：图片铺满整屏看细节，其他文件交给系统默认程序。
 *  两件事共用一个入口（`att-open`），是因为界面上的动作本来就只有一个「点开」。 */
async function openAttachment(a) {
  if (a.kind === 'image') return viewAttachment(a);
  if (a.pending) { toast('保存这条工作后就能打开'); return; }
  if (!hasTauri) { toast('浏览器预览模式'); return; }
  try { await inv('open_attachment', { id: a.id }); }
  catch (e) { console.warn(e); toast('打不开这个文件'); }
}

async function deleteAttachment(a, scope) {
  if (a.pending) {
    const list = state.editing?.atts || [];
    const i = list.findIndex(x => x.key === a.key);
    if (i >= 0) list.splice(i, 1);
    rerenderAttScope(scope);
    return;
  }
  if (hasTauri) {
    try { await inv('delete_attachment', { id: a.id }); }
    catch (e) { console.warn(e); toast('删除失败'); return; }
  }
  state.attachments = state.attachments.filter(x => x.id !== a.id);
  state.thumbCache.delete(a.id);
  rerenderAttScope(scope);
  refreshAttBadge(a.taskId);
}

/* ---- 大图预览 ---- */

function renderLightbox() {
  const el = document.getElementById('lightbox');
  if (!el) return;
  const lb = state.lightbox;
  if (!lb) { el.classList.remove('open'); el.innerHTML = ''; return; }
  el.innerHTML = `
    <div class="lightbox-bar">
      <span class="lightbox-name">${esc(lb.name || '')}</span>
      <button class="icon-btn" data-act="close-lightbox" title="关闭">
        <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M3.5 3.5l7 7M10.5 3.5l-7 7"/></svg>
      </button>
    </div>
    <img class="lightbox-img" src="${lb.url}" alt="">`;
  el.classList.add('open');
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

/** 子任务行，连带它自己的下级一起画。缩进按层级递增，深层用一条竖线连回父级。
 *  层数封顶 MAX_DEPTH，所以这个递归一定会停——不需要额外护栏。 */
function subtaskNode(s, depth) {
  const kids = subtasksOf(s.id);
  const done = kids.filter(k => k.status === 'done').length;
  return `
    <div class="sub-node" data-depth="${depth}">
      <div class="sub-item ${s.status === 'done' ? 'is-done' : ''}">
        <button class="sub-check" data-act="sub-toggle" data-id="${s.id}" aria-label="完成这一步">
          <svg viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M1.5 5.2l2.2 2.2 4.8-4.8"/></svg>
        </button>
        <input class="sub-title" data-sub-title="${s.id}" value="${escAttr(s.title)}" title="点一下就能改">
        ${kids.length ? `<span class="sub-ratio" title="下级完成情况">${done}/${kids.length}</span>` : ''}
        ${canNest(s.id) ? `
          <button class="icon-btn sm sub-nest" data-act="sub-add-for" data-id="${s.id}" title="给这一步再分小步">
            <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M7 3v8M3 7h8"/></svg>
          </button>` : ''}
        <button class="icon-btn sm sub-del" data-act="sub-del" data-id="${s.id}" title="删除这一步">
          <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M3.5 3.5l7 7M10.5 3.5l-7 7"/></svg>
        </button>
      </div>
      ${state.subAddFor === s.id ? `
        <div class="sub-add sub-add-inline">
          <input class="field-input" id="d-sub-input-inline" placeholder="给这一步再分一小步，回车确定">
          <button class="btn btn-ghost" data-act="add-subtask" data-parent="${s.id}">添加</button>
        </div>` : ''}
      ${kids.map(k => subtaskNode(k, depth + 1)).join('')}
    </div>`;
}

/** 阶段性工作的子任务清单：勾选、就地改名、删除、随手追加，还能给「一步」再分小步 */
function subtaskEditor(t) {
  const all = descendantsOf(t.id);
  const done = all.filter(s => s.status === 'done').length;
  return `
    <div class="field">
      <div class="field-label">子任务<span class="sub-progress">${done} / ${all.length}</span></div>
      ${all.length
        ? `<div class="sub-list">${subtasksOf(t.id).map(s => subtaskNode(s, 1)).join('')}</div>`
        : '<div class="hint">还没拆步骤。把这件事分成几步，进度就是自动算的。</div>'}
      <div class="sub-add">
        <input class="field-input" id="d-sub-input" placeholder="加一步，回车确定">
        <button class="btn btn-ghost" data-act="add-subtask">添加</button>
      </div>
      ${all.length && done === all.length
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
      <button class="icon-btn" data-act="edit" title="编辑">
        <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M9.3 2.1l2.6 2.6M2 12l.7-2.9 6.6-6.6 2.6 2.6-6.6 6.6L2 12z"/>
        </svg>
      </button>
      <button class="icon-btn" data-act="duplicate" title="复制一份（连步骤一起）">
        <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
          <rect x="4.8" y="4.8" width="7.7" height="7.7" rx="1.4"/>
          <path d="M9.2 1.5H3.1a1.6 1.6 0 0 0-1.6 1.6v6.1"/>
        </svg>
      </button>
      <button class="icon-btn" data-act="close-drawer" title="关闭">
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
      <div class="field" id="att-field-drawer">${attFieldInner('drawer')}</div>
      ${t.pattern === 'stage' ? subtaskEditor(t) : ''}
      <div class="field">
        <div class="field-label">复用</div>
        <div class="row-between">
          <div class="hint">${(() => {
            const n = descendantsOf(t.id).length;
            return n ? `连同 ${n} 个步骤一起存成模板，下次一键重建` : '存成模板，下次一键重建';
          })()}</div>
          <button class="btn btn-ghost" data-act="save-as-template">存为模板</button>
        </div>
      </div>
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
  hydrateThumbs('drawer');
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

  // 下面三个是辅助小面板（存模板 / 从模板建 / 按日查看），
  // 它们各自只有一个输入框，光标还原交给 restoreModalFocus 就够了
  // 主界面「从模板新建」的挑模板那一屏。挑完立刻接基准日，所以这屏只有一个动作。
  if (state.tplPick) {
    const list = state.templates || [];
    modal.innerHTML = `
      <div class="modal-head">
        <h2>从模板新建</h2>
        <button class="icon-btn" data-act="close-aux-modal">
          <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M3.5 3.5l7 7M10.5 3.5l-7 7"/></svg>
        </button>
      </div>
      <div class="modal-body">
        ${list.length ? `
          <div class="hint">挑一个模板，再选个起算的日子，整套工作和子任务连同日期一次建出来。</div>
          <div class="tpl-pick-list">
            ${list.map(t => `
              <button class="tpl-pick" data-act="tpl-apply" data-id="${t.id}">
                <span class="tpl-pick-main">
                  <span class="tpl-pick-name">${esc(t.name)}</span>
                  <span class="tpl-pick-meta">${esc(tplMeta(t))}</span>
                </span>
                <svg class="ic" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M6 3.5 10.5 8 6 12.5"/></svg>
              </button>`).join('')}
          </div>` : `
          <div class="empty">
            还没有模板。<br>
            打开任意一条工作的详情，点「存为模板」，就能把整套结构连同步骤存下来。
          </div>`}
      </div>`;
    mask.classList.add('open');
    return;
  }

  if (state.tplSaving) {
    const s = state.tplSaving;
    modal.innerHTML = `
      <div class="modal-head">
        <h2>存为模板</h2>
        <button class="icon-btn" data-act="close-aux-modal">
          <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M3.5 3.5l7 7M10.5 3.5l-7 7"/></svg>
        </button>
      </div>
      <div class="modal-body">
        <div class="field">
          <div class="field-label">模板名称</div>
          <input class="field-input" id="t-name" value="${escAttr(s.name)}" placeholder="例如：月度复盘流程">
        </div>
        <div class="field">
          <div class="field-label">会存下什么</div>
          <div class="hint">${s.steps ? `这条工作本身，加上它下面的 ${s.steps} 个步骤。` : '这条工作本身。'}
            存的是「第几天做什么」而不是具体日期，所以哪个月调用都不过期。</div>
        </div>
      </div>
      <div class="modal-foot">
        <button class="btn btn-ghost" data-act="close-aux-modal">取消</button>
        <button class="btn btn-primary" data-act="tpl-do-save">存为模板</button>
      </div>`;
    mask.classList.add('open');
    setTimeout(() => restoreModalFocus(null, 't-name'), 30);
    return;
  }

  if (state.tplApplying) {
    const a = state.tplApplying;
    modal.innerHTML = `
      <div class="modal-head">
        <h2>从模板新建</h2>
        <button class="icon-btn" data-act="close-aux-modal">
          <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M3.5 3.5l7 7M10.5 3.5l-7 7"/></svg>
        </button>
      </div>
      <div class="modal-body">
        <div class="field">
          <div class="field-label">模板</div>
          <div class="hint">${esc(a.name)} · ${a.items} 个条目</div>
        </div>
        <div class="field">
          <div class="field-label">从哪一天开始</div>
          <input class="field-input" type="date" id="t-base" value="${escAttr(a.base)}">
          <div class="hint">模板里存的是相对天数，这一批就按「基准日 + 第几天」铺开</div>
        </div>
      </div>
      <div class="modal-foot">
        <button class="btn btn-ghost" data-act="close-aux-modal">取消</button>
        <button class="btn btn-primary" data-act="tpl-do-apply">创建</button>
      </div>`;
    mask.classList.add('open');
    setTimeout(() => restoreModalFocus(null, 't-base'), 30);
    return;
  }

  if (state.dayView) {
    const entries = entriesOn(state.dayView);
    const d = new Date(`${state.dayView}T00:00:00`);
    modal.innerHTML = `
      <div class="modal-head">
        <h2>${d.getMonth() + 1} 月 ${d.getDate()} 日 · ${entries.length} 项</h2>
        <button class="icon-btn" data-act="close-aux-modal">
          <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M3.5 3.5l7 7M10.5 3.5l-7 7"/></svg>
        </button>
      </div>
      <div class="modal-body">
        <div class="day-list">
          ${entries.map(({ t, kind }) => {
            const cat = catById(t.categoryId);
            return `<div class="day-item ${t.status === 'done' ? 'is-done' : ''}"
                         data-act="day-open" data-id="${t.id}">
              <span class="cal-dot" style="background:${cat ? cat.color : '#CBD5E1'}"></span>
              <span class="day-title">${hl(t.title)}</span>
              ${kind === 'span'
                ? '<span class="chip chip-stage">进行中</span>'
                : t.status === 'done' ? '<span class="chip chip-ok">已完成</span>' : ''}
            </div>`;
          }).join('')}
        </div>
      </div>
      <div class="modal-foot">
        <button class="btn btn-ghost" data-act="close-aux-modal">关闭</button>
        <button class="btn btn-primary" data-act="day-new" data-date="${state.dayView}">在这一天新建</button>
      </div>`;
    mask.classList.add('open');
    return;
  }

  // 赞赏码。入口藏在设置页「关于」的最下面，这里只管把码放大到扫得动的尺寸。
  // 图是微信导出的海报裁出来的（见 tools/prep_reward_qr.py），署名不在图上 ——
  // 海报那行「xxx的赞赏码」和软件署名对不上，留着反而让人怀疑码是不是被换过。
  if (state.reward) {
    modal.innerHTML = `
      <div class="modal-head">
        <h2>赞赏</h2>
        <button class="icon-btn" data-act="close-aux-modal">
          <svg viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M3.5 3.5l7 7M10.5 3.5l-7 7"/></svg>
        </button>
      </div>
      <div class="modal-body">
        <div class="reward">
          <img class="reward-qr" src="assets/reward-qr.png" alt="微信赞赏码" width="260" height="260">
          <div class="reward-cap">微信扫一扫</div>
        </div>
        <div class="hint">工作记录本完全免费开源，赞赏纯属自愿，不影响任何功能。</div>
      </div>
      <div class="modal-foot">
        <button class="btn btn-ghost" data-act="close-aux-modal">关闭</button>
      </div>`;
    mask.classList.add('open');
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

  // 在分类页里新建时，分类已经定了，界面上只占一行、不再让人挑一遍。
  // 点了「更改」（catPickerOpen）或者本来就在「今天 / 全部」这种没有分类上下文的视图里，
  // 才铺开整排分类让人选。
  const ctxCat = (!e.id && !state.catPickerOpen) ? viewCategoryId() : undefined;
  const ctxCatObj = ctxCat === null || ctxCat === undefined ? null : catById(ctxCat);
  const ctxCatName = ctxCat === null ? '未分类' : (ctxCatObj?.name || '未分类');
  const ctxCatColor = ctxCat === null ? '#CBD5E1' : (ctxCatObj?.color || '#CBD5E1');

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
        once: '做一次就结束，适合临时任务。要拆成几步、看进度，就选「阶段性」',
        recurring: '按规则重复出现，适合周报、对账这类固定节奏',
        stage: '持续一段时间的大任务，保存后可以拆成多层子任务（最多三层）',
      }[e.pattern]}</div>
    </div>

    <div class="field">
      <div class="field-label">分类</div>
      ${ctxCat !== undefined ? `
      <div class="row-between">
        <span class="cat-fixed">
          <span class="swatch" style="background:${ctxCatColor}"></span>${esc(ctxCatName)}
        </span>
        <button class="btn btn-ghost" data-act="m-cat-open">更改</button>
      </div>
      <div class="hint">在当前分类里新建，已经替你选好了</div>` : `
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
      </div>`}
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

    <div class="field" id="att-field-modal">${attFieldInner('modal')}</div>
    </div>

    <div class="modal-foot">
      <button class="btn btn-ghost" data-act="close-modal">取消</button>
      <button class="btn btn-primary" data-act="m-save">${e.id ? '保存修改' : '创建工作'}</button>
    </div>`;
  mask.classList.add('open');
  setTimeout(() => restoreModalFocus(keep, 'm-title'), 30);
  hydrateThumbs('modal');
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

/** 某个子任务连同它下面各级的 id。删一个节点时，它的后代也活不成。 */
function subtreeIds(id) {
  const out = [id];
  subtasksOf(id).forEach(k => out.push(...subtreeIds(k)));
  return out;
}

/** 后端拒绝的理由翻译成人话。返回原文会把 SQL/英文抛给用户看。 */
function subErrText(e) {
  const s = String(e);
  if (s.includes('最多拆到')) return `最多拆到第 ${MAX_DEPTH} 层`;
  if (s.includes('阶段性')) return '只有阶段性工作能拆子任务';
  return '添加失败';
}

/** 追加一步。`parent` 给了就是给那个子任务再分小步，否则加在当前工作下面。
 *  连加时输入框留在原地——一次拆五步不用每步都点一遍「+」。 */
async function addSubtask(parent) {
  const nest = Number(parent) || 0;
  const parentId = nest || state.drawerId;
  if (!parentId) return;
  const input = document.getElementById(nest ? 'd-sub-input-inline' : 'd-sub-input');
  const title = (input?.value || '').trim();
  if (!title) { input?.focus(); return; }
  input.value = '';

  let created;
  if (hasTauri) {
    try { created = await inv('create_subtask', { parentId, title }); }
    catch (e) {
      console.warn(e);
      toast(subErrText(e));
      return;
    }
  } else {
    if (nest && !canNest(nest)) { toast(`最多拆到第 ${MAX_DEPTH} 层`); return; }
    created = { id: -(state.subtasks.length + 1), parentId, title, status: 'todo', pattern: 'once', note: '' };
  }
  state.subtasks.push(created);
  applySubtaskProgress();
  renderAfterSubtaskChange();
  document.getElementById(nest ? 'd-sub-input-inline' : 'd-sub-input')?.focus();
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
  const doomed = new Set(subtreeIds(id));
  state.subtasks = state.subtasks.filter(x => !doomed.has(x.id));
  if (doomed.has(state.subAddFor)) state.subAddFor = null;
  if (hasTauri) { try { await inv('delete_task', { id }); } catch (e) { console.warn(e); } }
  applySubtaskProgress();
  renderAfterSubtaskChange();
  // 只想删一步、结果连它的下级一起没了，得说清楚——
  // 不吭声的话用户会以为程序多删了东西
  const extra = doomed.size - 1;
  toast(extra > 0 ? `已删掉这一步，连同它下面的 ${extra} 个小步` : '已删掉这一步');
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

function openEditor(id, preset) {
  if (id) {
    const t = state.tasks.find(x => x.id === id);
    state.editing = JSON.parse(JSON.stringify(t));
  } else {
    // 站在某个分类页里新建，就把这个分类直接带上 —— 用户已经在那页了，
    // 再让他从一排分类里挑一遍是多余的一步。
    // 只有「今天 / 全部」这类没有分类上下文的视图才退回默认第一个分类。
    const ctx = viewCategoryId();
    state.editing = {
      id: 0, title: '', note: '',
      categoryId: ctx === undefined ? (state.categories[0]?.id ?? 1) : ctx,
      pattern: 'once', status: 'todo', dueAt: null, rule: { freq: 'weekly', byDay: [5], time: '17:00' },
      // 新建时还没有数据库 id，粘贴进来的附件先在这儿攒着，保存成功后再入库
      atts: [],
    };
    // 从日历格子点进来的：那一天就是要填的日期。
    // 开始日和交期都先落在那天，用户切成「阶段性」也不用再选一遍
    if (preset) Object.assign(state.editing, preset);
  }
  // 分类是自动带上的时候，界面上只显示一行；用户点了「更改」才铺开整排
  state.catPickerOpen = false;
  state.catEditing = null;
  state.tplApplying = null;
  state.tplSaving = null;
  state.dayView = null;
  state.reward = false;   // 赞赏码弹窗要是还开着，会把编辑器压在下面出不来（Ctrl+N 能触发）
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
      if (e.id) {
        await inv('update_task', { task: e });
      } else {
        const created = await inv('create_task', { task: e });
        // 新建时暂存的附件，要拿到 id 之后才入得了库。
        // 放在 loadAll 之前：loadAll 会用库里的最新数据覆盖 state.attachments
        await flushPendingAttachments(created.id);
      }
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
  // 库那边是 ON DELETE CASCADE，内存里也要跟着清，否则步骤会一直飘着。
  // 必须连子孙一起清：只滤掉直接子任务的话，第三层会变成没人认领的孤儿
  const doomed = new Set([id, ...subtreeIds(id)]);
  state.subtasks = state.subtasks.filter(s => !doomed.has(s.id));
  // 附件挂在工作上，库里靠外键级联删；内存里也得跟着清，
  // 否则缩略图会一直留在缓存里，别的条目还可能把计数算错
  state.attachments = state.attachments.filter(a => !doomed.has(a.taskId));
  if (doomed.has(state.subAddFor)) state.subAddFor = null;
  if (hasTauri) { try { await inv('delete_task', { id }); } catch (e) { console.warn(e); } }
  state.drawerId = null;
  renderAll_();
  toast('已删除');
}

/* ---------------- 复制一份 ---------------- */

/** 复制当前这条，连同下面拆出来的各层步骤。
 *  日期整体平移到今天起算——「这件事我下周还要再做一遍」是最常见的用法，
 *  照着原日期复制出来的东西会落在过去。 */
async function duplicateTask(id) {
  const t = state.tasks.find(x => x.id === id);
  if (!t) return;
  if (!hasTauri) { toast('浏览器预览模式'); return; }
  try {
    const created = await inv('duplicate_task', { id, base: todayLocal() });
    await loadAll();
    state.drawerId = created.id;     // 停在副本上，用户多半是要接着改它
    renderAll_();
    toast('已复制一份，日期从今天起算');
  } catch (e) {
    console.warn(e);
    toast('复制失败');
  }
}

/* ---------------- 模板库 ---------------- */

/** 交期正好落在这天的全部工作。
 *  和 `entriesOn` 的区别：这里**不含**跨天任务的中间日——
 *  「顺延一天」要挪的是「今天该交的活」，把跨度中间的日子也算进去会误伤大任务。 */
function tasksOn(dateStr) {
  if (!dateStr) return [];
  const key = startOfDay(new Date(`${dateStr}T00:00:00`)).getTime();
  return sortTasks(scopeTasks().filter(t => {
    const dl = deadlineOf(t);
    return dl && startOfDay(new Date(dl)).getTime() === key;
  }));
}

/** 「还有 N 项」：日历格子只摆得下 3 条，剩下的从这里看全。
 *  口径跟格子完全一致，不这么做的话，格子说「还有 5 项」、
 *  点开只列出 3 条，用户会以为界面漏了东西。 */
function openDayView(dateStr) {
  if (!entriesOn(dateStr).length) return;
  state.dayView = dateStr;
  state.editing = null;
  state.catEditing = null;
  state.tplApplying = null;
  renderModal();
}

/** 「存为模板」：起个名字，把这条工作连同各层步骤一起存进模板库 */
/** 主界面的「从模板新建」：就地列模板让人挑，不用先跳到模板页。
 *  「这套流程今天要走一遍」的场景下，多一步切视图就是多一次打断。 */
function openTplPicker() {
  state.tplPick = true;
  state.editing = null;
  state.catEditing = null;
  state.tplApplying = null;
  state.tplSaving = null;
  state.dayView = null;
  state.reward = false;
  modalFocus = null;
  renderModal();
}

function openSaveTemplate() {
  const t = state.tasks.find(x => x.id === state.drawerId);
  if (!t) return;
  state.tplSaving = { taskId: t.id, name: t.title, steps: descendantsOf(t.id).length };
  state.editing = null;
  state.catEditing = null;
  state.tplApplying = null;
  modalFocus = null;
  renderModal();
}

async function doSaveTemplate() {
  const s = state.tplSaving; if (!s) return;
  const name = (document.getElementById('t-name')?.value || '').trim() || s.name;
  if (!hasTauri) { toast('浏览器预览模式'); return; }
  try {
    await inv('save_template', { taskId: s.taskId, name });
    await loadAll();
    state.tplSaving = null;
    renderModal();
    renderAll_();
    toast(`已存成模板「${name}」`);
  } catch (e) {
    console.warn(e);
    toast(`存模板失败：${e}`);
  }
}

/** 「从模板新建」：先挑基准日，模板里的相对天数都从这天起算 */
function openApplyTemplate(id) {
  const t = (state.templates || []).find(x => x.id === id);
  if (!t) return;
  state.tplApplying = { id, name: t.name, items: t.items.length, base: todayLocal() };
  state.editing = null;
  state.catEditing = null;
  state.tplSaving = null;
  state.tplPick = false;   // 从模板列表点进来的，那层列表要收掉，否则会盖住基准日这一步
  modalFocus = null;
  renderModal();
}

async function doApplyTemplate() {
  const a = state.tplApplying; if (!a) return;
  const base = document.getElementById('t-base')?.value || a.base;
  if (!hasTauri) { toast('浏览器预览模式'); return; }
  try {
    const n = await inv('apply_template', { id: a.id, base });
    await loadAll();
    state.tplApplying = null;
    renderModal();
    state.view = 'all';        // 建出来的东西得让人看见，直接送到「全部工作」
    renderAll_();
    toast(`已新建 ${n} 件工作，日期从 ${base} 起算`);
  } catch (e) {
    console.warn(e);
    toast(`新建失败：${e}`);
  }
}

async function deleteTemplate(id) {
  const t = (state.templates || []).find(x => x.id === id);
  if (!t) return;
  if (!hasTauri) { toast('浏览器预览模式'); return; }
  // 模板删了不影响已经建出来的工作，这一点要说清楚，否则用户不敢删
  const yes = await askDialog(
    `模板「${t.name}」会被删除，之后不能再从它新建。\n已经建出来的工作不受影响。`,
    '删除模板',
  );
  if (!yes) return;
  try {
    await inv('delete_template', { id });
    state.templates = state.templates.filter(x => x.id !== id);
    renderView();
    toast('模板已删除');
  } catch (e) { console.warn(e); toast('删除失败'); }
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
  state.reward = false;
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

/* ---------------- 日历上的右键菜单 ---------------- */

/** 从日历某天新建时预填的日期。开始日和交期都先落在那天，
 *  用户切成「阶段性」也不用再选一遍——那时两个输入框都已经是对的。 */
const dayPreset = dateStr => dateStr ? {
  dueAt: new Date(`${dateStr}T18:00:00`).toISOString(),
  endAt: new Date(`${dateStr}T23:59:00`).toISOString(),
} : null;

function closeCtxMenu() {
  const el = document.getElementById('ctx-menu');
  if (el && typeof el.remove === 'function') el.remove();
  state.ctxMenu = null;
}

function showCtxMenu(dateStr, x, y) {
  closeCtxMenu();
  state.ctxMenu = { date: dateStr };

  const n = tasksOn(dateStr).length;
  const el = document.createElement('div');
  el.className = 'ctx-menu';
  el.id = 'ctx-menu';
  el.innerHTML = `
    <button data-ctx="new">在这一天新建</button>
    ${n ? `<button data-ctx="day">查看当天 ${n} 项</button>
           <button data-ctx="push">把这一天的工作顺延一天</button>` : ''}
    <button data-ctx="today">回到今天</button>`;
  // 菜单别顶出窗口：靠右下的格子右键时会跑到屏幕外
  el.style.left = `${Math.min(x, Math.max(0, window.innerWidth - 180))}px`;
  el.style.top  = `${Math.min(y, Math.max(0, window.innerHeight - 150))}px`;
  document.body.appendChild(el);

  el.addEventListener('click', ev => {
    const b = ev.target.closest('[data-ctx]');
    if (!b) return;
    const d = state.ctxMenu?.date;
    const k = b.dataset.ctx;
    closeCtxMenu();     // 先收菜单再干活，否则后面的重绘会把菜单留在原地
    if (k === 'new')   openEditor(null, dayPreset(d));
    if (k === 'day')   openDayView(d);
    if (k === 'push')  pushDay(d);
    if (k === 'today') { state.calMonth = null; renderView(); }
  });
}

/** 把这天还没做完的整体往后挪一天。
 *  「今天做不完了，都挪到明天」比一条条改日期快得多，也少一次漏改。
 *  阶段性工作的开始日和交期一起挪，跨度保持不变。 */
async function pushDay(dateStr) {
  const list = tasksOn(dateStr).filter(t => t.status !== 'done');
  if (!list.length) { toast('这一天没有待处理的工作'); return; }
  const iso = s => s ? new Date(new Date(s).getTime() + DAY).toISOString() : null;
  for (const t of list) {
    t.dueAt = iso(t.dueAt);
    t.endAt = iso(t.endAt);
    t.remindAt = iso(t.remindAt);
    if (hasTauri) { try { await inv('update_task', { task: t }); } catch (e) { console.warn(e); } }
  }
  renderAll_();
  toast(`已把 ${list.length} 项顺延一天`);
}

/* ---------------- 事件绑定 ---------------- */
function bind() {
  document.body.addEventListener('click', ev => {
    closeCtxMenu();   // 点哪儿都先把右键菜单收掉
    // data-act 优先于 data-nav：分类行里嵌着编辑按钮，不能点按钮却跳了视图
    const act = ev.target.closest('[data-act]');
    if (act) {
      const a = act.dataset.act;
      const id = Number(act.dataset.id);

      if (a === 'toggle')   { toggleTask(id); return; }
      if (a === 'open')     { state.drawerId = id; renderDrawer(); return; }
      if (a === 'close-drawer') { state.drawerId = null; state.subAddFor = null; renderDrawer(); return; }
      if (a === 'delete')   { deleteTask(state.drawerId); return; }
      if (a === 'edit')     { const x = state.drawerId; state.drawerId = null; renderDrawer(); openEditor(x); return; }
      if (a === 'duplicate')        { duplicateTask(state.drawerId); return; }
      if (a === 'save-as-template') { openSaveTemplate(); return; }
      if (a === 'close-modal') { state.editing = null; modalFocus = null; renderModal(); return; }

      // 附件：抽屉和弹窗共用一套动作，靠 data-scope 区分作用域
      if (a === 'att-pick') { pickFiles(act.dataset.scope); return; }
      if (a === 'att-del') {
        const x = findAtt(act.dataset.scope, act.dataset.key);
        if (x) deleteAttachment(x, act.dataset.scope);
        return;
      }
      if (a === 'att-open') {
        const x = findAtt(act.dataset.scope, act.dataset.key);
        if (x) openAttachment(x);
        return;
      }
      if (a === 'close-lightbox') { state.lightbox = null; renderLightbox(); return; }

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
      // 自动带上的分类想改：把整排分类铺开
      if (a === 'm-cat-open') { syncEditorFields(); state.catPickerOpen = true; renderModal(); return; }
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
      // 点空白格＝在那天新建。这个动作以前完全没有，日历上最自然的操作反而是做不到的
      if (a === 'cal-new') { openEditor(null, dayPreset(act.dataset.date)); return; }
      if (a === 'cal-day') { openDayView(act.dataset.date); return; }

      // 子任务
      if (a === 'sub-toggle')  { toggleSubtask(id); return; }
      if (a === 'sub-del')     { deleteSubtask(id); return; }
      if (a === 'add-subtask') { addSubtask(act.dataset.parent); return; }
      if (a === 'sub-add-for') {
        // 再点一次收起。同一个入口既开又关，省一个「取消」按钮
        state.subAddFor = state.subAddFor === id ? null : id;
        renderDrawer();
        document.getElementById('d-sub-input-inline')?.focus();
        return;
      }

      // 辅助面板：挑模板 / 存模板 / 从模板新建 / 按日查看 / 赞赏码
      if (a === 'open-reward') { state.reward = true; renderModal(); return; }
      if (a === 'close-aux-modal') {
        state.tplPick = false; state.tplApplying = null; state.tplSaving = null; state.dayView = null;
        state.reward = false;
        renderModal(); return;
      }
      if (a === 'tpl-do-save')  { doSaveTemplate();  return; }
      if (a === 'tpl-do-apply') { doApplyTemplate(); return; }
      if (a === 'tpl-apply')    { openApplyTemplate(id); return; }
      if (a === 'tpl-del')      { deleteTemplate(id); return; }
      if (a === 'day-open') {
        state.dayView = null; renderModal();
        state.drawerId = id; renderDrawer();
        return;
      }
      if (a === 'day-new') { state.dayView = null; openEditor(null, dayPreset(act.dataset.date)); return; }

      // 设置项
      if (a === 'toggle-setting') {
        const k = act.dataset.key;
        state.settings = state.settings || {};
        state.settings[k] = settingOn(k) ? '0' : '1';
        // 贴边隐藏和边缘分屏抢同一条屏幕边：打开一个，另一个必须让位。
        // 后端也会改这一行，这里先改是为了界面上立刻看得到结果，
        // 不用等 invoke 回来再刷新 —— 开关自己跳回去会让人以为是 bug。
        if (state.settings[k] === '1' && (k === 'snap' || k === 'edge')) {
          const other = k === 'snap' ? 'edge' : 'snap';
          state.settings[other] = '0';
          if (hasTauri) inv('set_setting', { key: other, value: '0' }).catch(console.warn);
        }
        renderView();
        if (hasTauri) inv('set_setting', { key: k, value: state.settings[k] }).catch(console.warn);
        // 关掉贴边隐藏时窗口可能正滑在屏幕外，得先把它请回来 ——
        // 不然开关是关了，窗口却还挂在边上只露一条窄边，看着像坏了
        if (k === 'edge' && state.settings[k] === '0' && hasTauri) {
          inv('edge_reset').catch(console.warn);
        }
        return;
      }
      if (a === 'open-data-dir') {
        if (!hasTauri) { toast('浏览器预览模式'); return; }
        inv('open_data_dir').catch(err => { console.warn(err); toast('打开目录失败'); });
        return;
      }
      if (a === 'toggle-portable') { togglePortable(); return; }
      if (a === 'backup-now')  { backupNow();  return; }
      if (a === 'restore-now') { restoreNow(); return; }
      if (a === 'export-csv')  { exportCsv();  return; }

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

    if (ev.target.id === 'modal-mask') {
      state.editing = null; state.catEditing = null;
      // 辅助面板（挑模板 / 存模板 / 按日 / 赞赏）也一并关掉 ——
      // 它们和主弹窗是同一个遮罩，点外面只有一部分能关会显得时灵时不灵
      state.tplPick = false; state.tplApplying = null; state.tplSaving = null;
      state.dayView = null; state.reward = false;
      renderModal();
      return;
    }

    // 大图预览：点图片以外的空白处关掉（点图片本身不关，那是要看的地方）
    if (ev.target.id === 'lightbox') { state.lightbox = null; renderLightbox(); return; }

    // 抽屉是并排布局、没有遮罩，「点旁边空白」是用户最自然的关闭动作。
    // 只有角上的 X 和 Esc 能关的话，会被当成「关不掉」——这正是有人反馈的那条。
    // 标题栏除外：那是拖窗口和点窗口按钮的地方，顺手关掉抽屉会很烦人。
    if (state.drawerId && !ev.target.closest('.drawer') && !ev.target.closest('.titlebar')) {
      state.drawerId = null;
      state.subAddFor = null;
      renderDrawer();
    }
  });

  document.getElementById('btn-new').addEventListener('click', () => openEditor(null));
  // 主界面入口：反复要走的那套流程就地重建，不必先切到模板页
  document.getElementById('btn-from-tpl').addEventListener('click', () => openTplPicker());

  /* 贴边隐藏的鼠标进出判定不在这里 —— 收起后整扇窗只剩几像素露在屏幕里，
     那几像素仍然是窗口的一部分，鼠标停上去会一直触发「进入」事件，窗口刚收起
     就被自己叫回来；而且那一瞬间鼠标本来就压在边上，前端分不出「碰一下」和
     「刚拖完还按着」。改由后端轮询采样（窗口矩形 + 鼠标绝对坐标 + 左键状态）
     统一判定，前端只负责下面那个状态监听把「把手」画出来。 */

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

  /* 附件采集：粘贴 / 拖拽 / 选文件。
     粘贴挂在 document 上，因为用户常常是刚点开抽屉就按 Ctrl+V，焦点还在别处；
     挂在某个输入框上会漏掉这种情况。 */
  document.addEventListener('paste', ev => {
    const scope = currentAttScope();
    if (!scope) return;
    const items = ev.clipboardData && ev.clipboardData.items;
    if (!items) return;
    // 按下标取而不是 for...of：DataTransferItemList 不是规范要求的可迭代对象，
    // 靠 Symbol.iterator 遍历在部分内核上会直接抛。下标访问是规范保证的
    const files = [];
    for (let i = 0; i < items.length; i++) {
      const it = items[i];
      if (it && it.kind === 'file') { const f = it.getAsFile(); if (f) files.push(f); }
    }
    // 剪贴板里是纯文字就放行 —— 用户多半在往备注里粘内容，不该被抢走
    if (!files.length) return;
    ev.preventDefault();
    handleFiles(files, scope);
  });

  // 拖拽：只认落在附件投放区里的，别的地方该怎么拖还怎么拖
  document.addEventListener('dragover', ev => {
    const z = ev.target.closest?.('[data-att-zone]');
    if (!z) return;
    ev.preventDefault();
    z.classList.add('drag');
  });
  document.addEventListener('dragleave', ev => {
    ev.target.closest?.('[data-att-zone]')?.classList.remove('drag');
  });
  document.addEventListener('drop', ev => {
    const z = ev.target.closest?.('[data-att-zone]');
    if (!z) return;
    ev.preventDefault();
    z.classList.remove('drag');
    handleFiles(Array.from((ev.dataTransfer && ev.dataTransfer.files) || []), z.dataset.attZone);
  });

  document.addEventListener('keydown', ev => {
    const el = ev.target;
    const typing = el instanceof HTMLElement && /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName);

    if (el?.id === 'd-sub-input' && ev.key === 'Enter') { ev.preventDefault(); addSubtask(); return; }
    if (el?.classList?.contains('sub-title') && ev.key === 'Enter') { ev.preventDefault(); el.blur(); return; }

    if (ev.key === 'Escape') {
      // 大图铺在最上层，Esc 先关它 —— 一次只退一层
      if (state.lightbox) { state.lightbox = null; renderLightbox(); return; }
      // 正在输入时按 Esc 只退出输入，不顺手把整个面板关掉
      if (typing) { el.blur(); return; }
      if (state.ctxMenu) { closeCtxMenu(); }
      else if (state.dayView || state.tplApplying || state.tplSaving || state.tplPick || state.reward) {
        state.dayView = null; state.tplApplying = null; state.tplSaving = null;
        state.tplPick = false;
        state.reward = false;
        renderModal();
      }
      else if (state.catEditing) { state.catEditing = null; renderModal(); }
      else if (state.editing) { state.editing = null; renderModal(); }
      // 「加下级」的输入框先收，再考虑整个抽屉——一次 Esc 只退一层
      else if (state.subAddFor) { state.subAddFor = null; renderDrawer(); }
      else if (state.drawerId) { state.drawerId = null; renderDrawer(); }
    }
    if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === 'n') { ev.preventDefault(); openEditor(null); }
  });

  // 日历上滚轮翻月。监听挂在 document 上，因为日历每次重绘都会换掉整块 DOM，
  // 挂在下层元素上翻一次月就失效了。
  // 250ms 节流是必须的：wheel 一秒能触发几十次，不节流会一口气翻十几月。
  let calWheelAt = 0;
  document.addEventListener('wheel', ev => {
    if (state.mode !== 'calendar') return;
    if (!ev.target.closest?.('.cal-grid')) return;

    // 页面本身还能滚就先让它滚：不判断的话，「往下看点内容」会变成翻月。
    // 只有在滚动已经到头、没有别的去向时，滚轮才接管翻月。
    const view = document.getElementById('view');
    const sh = view?.scrollHeight, ch = view?.clientHeight;
    if (Number.isFinite(sh) && Number.isFinite(ch) && sh > ch + 1) {
      const atTop = view.scrollTop <= 0;
      const atBottom = view.scrollTop + ch >= sh - 1;
      if ((ev.deltaY < 0 && !atTop) || (ev.deltaY > 0 && !atBottom)) return;
    }

    const now = Date.now();
    if (now - calWheelAt < 250) return;
    calWheelAt = now;
    ev.preventDefault?.();

    const base = state.calMonth || new Date();
    state.calMonth = new Date(base.getFullYear(), base.getMonth() + (ev.deltaY > 0 ? 1 : -1), 1);
    renderView();
  }, { passive: false });

  // 日历格子上右键：新建 / 看全当天 / 整体顺延。
  // 必须 preventDefault，否则 WebView 会弹出系统的「刷新、复制」菜单盖在上面。
  document.addEventListener('contextmenu', ev => {
    const cell = ev.target.closest?.('.cal-cell');
    if (cell && state.mode === 'calendar') {
      ev.preventDefault?.();
      showCtxMenu(cell.dataset?.date, ev.clientX || 0, ev.clientY || 0);
      return;
    }
    closeCtxMenu();
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

  // 贴边隐藏：收起 / 展开全由后端轮询线程决定，这里只跟着改样式。
  // 露在外面的那几像素本来就是窗口的边缘，得给个颜色才看得出是「把手」，
  // 不然用户看到的是一条说不清是什么的窄边。
  listen('edge://changed', ev => {
    const p = ev.payload || {};
    state.edgeCollapsed = !!p.collapsed;
    // 展开之后 side 仍然保留：鼠标再移开还要按同一边收回去
    state.edgeSide = p.side || '';
    document.body.classList.toggle('edge-collapsed', state.edgeCollapsed);
    document.body.dataset.edgeSide = state.edgeSide;
  });
}

/* ---------------- 启动 ---------------- */
(async function init() {
  await loadAll();
  bind();
  bindBackendEvents();
  renderAll_();
  // 刚搬过数据就说一声。后端只给一次（读走即清），所以这里不用去重
  const note = state.dataInfo && state.dataInfo.note;
  if (note) toast(note);
})();
