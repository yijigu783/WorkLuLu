/* 渲染自检：脱离界面把 taskRow / subtaskEditor / renderBoard / renderCalendar 跑一遍，
   确认几件容易改坏的事——备注该显示时显示、子任务进度算得对、
   看板分列不丢件、日历格子和「今天」标记不错位，
   以及「表单里填了一半，点个按钮就没了」这类重绘问题。
   用法：node tools/check_ui.js */
const fs = require('fs');
const path = require('path');
const vm = require('vm');

const src = fs.readFileSync(path.join(__dirname, '..', 'ui', 'assets', 'app.js'), 'utf8');
const rawHtml = fs.readFileSync(path.join(__dirname, '..', 'ui', 'index.html'), 'utf8');
// 版本号有四个地方要同步：Cargo.toml、tauri.conf.json、前端兜底常量、exe 文件名。
// 这里把它们对起来，省得改一处漏一处——漏了的话用户看到的版本和 exe 属性对不上。
const cargoToml = fs.readFileSync(path.join(__dirname, '..', 'src-tauri', 'Cargo.toml'), 'utf8');
const cargoVer = (cargoToml.match(/^version\s*=\s*"([^"]+)"/m) || [])[1];
const confVer = JSON.parse(
  fs.readFileSync(path.join(__dirname, '..', 'src-tauri', 'tauri.conf.json'), 'utf8'),
).version;
// 掐掉末尾的启动 IIFE（它会去碰 DOM），只留纯逻辑
const body = src.split('/* ---------------- 启动 ---------------- */')[0];

/* ---- 迷你 DOM ----
   只为验证「控件一点就把填好的内容冲掉」这一类问题。
   真实浏览器里重绘 = 重新解析一遍 innerHTML：输入框的值回到 HTML 上写的默认值。
   这里照着这个语义做——给 modal 的 innerHTML 装个 setter，赋值时把每个表单控件的
   值按新 HTML 重算，这样才分得清「内容被保住了」还是「恰好又读到了旧值」。
   注：不做实体反转义，用例里只用不含 & < > 的纯文本。 */
function makeDom() {
  const els = new Map();
  const make = id => ({
    id, value: '', innerHTML: '', className: '', dataset: {},
    style: {}, selectionStart: null, selectionEnd: null, scrollTop: 0,
    classList: { add() {}, remove() {}, contains: () => false },
    focus() {}, blur() {}, addEventListener() {},
    setSelectionRange(a, b) { this.selectionStart = a; this.selectionEnd = b; },
    getBoundingClientRect: () => ({ top: 0, height: 0 }),
  });
  const get = id => { if (!els.has(id)) els.set(id, make(id)); return els.get(id); };

  const modal = get('modal');
  let html = '';
  Object.defineProperty(modal, 'innerHTML', {
    get: () => html,
    set(v) {
      html = String(v);
      const alive = new Set();
      for (const m of html.matchAll(/id="([A-Za-z0-9-]+)"/g)) {
        const id = m[1];
        if (!/^[mcd]-/.test(id)) continue;      // 只看表单控件
        alive.add(id);
        const gt = html.indexOf('>', m.index);
        const tag = html.slice(html.lastIndexOf('<', m.index), gt + 1);
        let val = (tag.match(/\bvalue="([^"]*)"/) || [])[1];
        if (val === undefined && /^<textarea/i.test(tag)) {
          val = html.slice(gt + 1, html.indexOf('</textarea>', gt));  // 内容在标签之间
        }
        get(id).value = val === undefined ? '' : val;
      }
      // 这一版没画出来的控件值清空——真实 DOM 里它已经不存在了
      for (const [id, el] of els) if (!alive.has(id) && /^[mcd]-/.test(id)) el.value = '';
    },
  });

  return { get, modal };
}

const dom = makeDom();
const bodyHandlers = {};
const docHandlers = {};   // keydown / wheel 是挂在 document 上的，和 body 上的分开收

const ctx = {
  console,
  window: {},                     // 没有 __TAURI__ → 走预览数据
  document: {
    activeElement: null,
    addEventListener: (t, fn) => { (docHandlers[t] = docHandlers[t] || []).push(fn); },
    getElementById: dom.get,
    querySelectorAll: () => [],
    body: { addEventListener: (t, fn) => { (bodyHandlers[t] = bodyHandlers[t] || []).push(fn); } },
  },
  requestAnimationFrame: fn => fn(),
  setTimeout, clearTimeout,
  // keydown 处理器用 `el instanceof HTMLElement` 判「焦点是不是落在输入框上」。
  // 放一个空类在这里：模拟出来的 target 都不是它的实例，等价于「焦点不在输入框」。
  HTMLElement: class HTMLElement {},
};
vm.createContext(ctx);
vm.runInContext(
  body + '\n;globalThis.__api = { state, loadAll, taskRow, subtaskEditor, subtasksOf, descendantsOf, depthOf, canNest, subtreeIds, applySubtaskProgress, renderDrawer, renderBoard, renderCalendar, renderTemplates, renderSidebar, renderModal, renderSettings, openDayView, entriesOn, tasksOn, dayPreset, toLocalDate, laneOf, scopeTasks, bind, openEditor, syncEditorFields, ruleLabel, quarterMonths, dayText, nextOccurrence, defaultRuleFor, FREQS, stampName, MAX_DEPTH, APP_NAME_CN, APP_NAME_EN, APP_VERSION_FALLBACK, APP_COPYRIGHT };',
  ctx,
);

(async () => {
  const api = ctx.__api;
  await api.loadAll();
  const { state } = api;
  const row = id => api.taskRow(state.tasks.find(t => t.id === id));
  const stage = state.tasks.find(t => t.pattern === 'stage');

  const checks = [];
  const ok = (name, cond, extra = '') => checks.push([cond ? 'PASS' : 'FAIL', name, extra]);

  /* ---- 备注 ---- */
  ok('有备注的行渲染备注块', /class="task-note"/.test(row(3)));
  ok('备注文字出现在行里', row(3).includes('对比三家报价后给结论'));
  ok('备注同时挂 title，长了能悬停看全文', /class="task-note" title="[^"]+"/.test(row(3)));
  ok('没有备注的行不留空块', !/class="task-note"/.test(row(4)), '任务 4 没有备注');
  ok('所有行的备注块数量与有备注的任务数一致', (() => {
    const withNote = state.tasks.filter(t => (t.note || '').trim().length);
    const rendered = state.tasks.filter(t => /class="task-note"/.test(row(t.id))).length;
    return withNote.length === rendered;
  })(), `${state.tasks.filter(t => (t.note || '').trim()).length} 条有备注`);

  /* ---- 搜索高亮 ---- */
  state.q = '报价';
  ok('命中处加了 <mark>', row(3).includes('<mark>报价</mark>'));
  state.q = '<img src=x>';
  ok('搜索词里的尖括号不会注入标签', !row(3).includes('<img'));
  state.q = '';

  /* ---- 子任务与进度 ---- */
  ok('阶段性工作的进度由子任务算出',
    stage.progress.done === 4 && stage.progress.total === 7, JSON.stringify(stage.progress));
  ok('行上显示 done/total', row(stage.id).includes('4/7'));
  ok('进度条宽度按完成比例', /width:57\.14/.test(row(stage.id)));
  ok('抽屉里子任务条数与数据一致',
    (api.subtaskEditor(stage).match(/class="sub-item/g) || []).length === 7);

  // 全部勾上 → 行上提示可以收尾。必须连第二层一起勾，
  // 进度算的是整棵子树，漏掉一层就永远凑不齐
  api.descendantsOf(stage.id).forEach(s => { s.status = 'done'; });
  api.applySubtaskProgress();
  ok('子任务全完成时行上提示「待收尾」', row(stage.id).includes('待收尾'));

  // 一个都没有 → 不显示进度条，也不假装有进度
  api.descendantsOf(stage.id).forEach(s => { s.status = 'todo'; });
  state.subtasks = state.subtasks.filter(s => s.parentId !== stage.id);
  api.applySubtaskProgress();
  ok('没有子任务时进度归零', stage.progress.total === 0 && stage.progress.done === 0);
  ok('没有子任务时不画进度条', !/class="progress"/.test(row(stage.id)));
  ok('没有子任务时提示「未拆解」', row(stage.id).includes('未拆解'));

  /* ---- 三层拆解 ----
     工作 → 子任务 → 子子任务，第三层封顶。
     界面上越界的那一层直接不给「+」入口，而不是让人点了才被告知不行。 */
  await api.loadAll();
  const deep = api.subtasksOf(stage.id).find(s => s.title === '移动端适配');
  const grand = state.subtasks.find(s => s.title === '触屏点击区放大');
  ok('第二层的步骤还能再往下拆', api.canNest(deep.id), `第 ${api.depthOf(deep.id)} 层`);
  ok('第三层的步骤不能再拆', !api.canNest(grand.id), `第 ${api.depthOf(grand.id)} 层`);
  ok('层数上限就是三层', api.MAX_DEPTH === 3);

  const editor = api.subtaskEditor(stage);
  ok('第二层的下级被渲染出来', editor.includes('窄屏（&lt; 900px）走查') || editor.includes('窄屏（< 900px）走查'));
  ok('第二层带自己的下级完成标记', /class="sub-ratio"[^>]*>1\/2</.test(editor), '移动端适配 1/2');
  ok('第三层的行缩进一层', /data-depth="2"/.test(editor));
  ok('删一步会连它的下级一起算',
    api.subtreeIds(deep.id).length === 3, `连它自己共 ${api.subtreeIds(deep.id).length} 个`);

  /* ---- 看板 ---- */
  // 上面的用例把子任务删干净了，这里先把数据复位，否则量到的是被改动过的状态
  await api.loadAll();
  const board = api.renderBoard();
  // 每一条都得有地方落，漏掉的话看板上会凭空少东西而且没人发现
  const bucketSum = (() => {
    const n = {};
    state.tasks.forEach(t => { const k = api.laneOf(t); n[k] = (n[k] || 0) + 1; });
    return n;
  })();
  ok('看板每条工作都归到了某一列',
    Object.values(bucketSum).reduce((a, b) => a + b, 0) === state.tasks.length,
    JSON.stringify(bucketSum));
  ok('看板列头数字之和等于卡片数',
    (() => {
      const heads = [...board.matchAll(/board-col-count">(\d+)</g)].map(m => Number(m[1]));
      const cards = (board.match(/class="board-card[\s"]/g) || []).length;
      return heads.reduce((a, b) => a + b, 0) === cards;
    })(), (() => {
      const heads = [...board.matchAll(/board-col-count">(\d+)</g)].map(m => Number(m[1]));
      return `列头 ${JSON.stringify(heads)} vs 卡片 ${(board.match(/class="board-card[\s"]/g) || []).length}`;
    })());
  ok('卡片数与当前范围一致',
    (board.match(/class="board-card[\s"]/g) || []).length === api.scopeTasks().length,
    `${api.scopeTasks().length} 条在范围内`);
  ok('阶段卡片把子任务进度写进元信息', /子任务 \d+\/\d+/.test(board));
  ok('逾期卡片带 is-overdue 标记', /board-card is-overdue/.test(board));
  ok('今天视图下不摆空的「已完成」列', !/board-col-title">已完成/.test(board));

  /* ---- 日历 ---- */
  const cal = api.renderCalendar();
  const cells = (cal.match(/class="cal-cell/g) || []).length;
  ok('日历表头正好 7 列', (cal.match(/class="cal-weekday"/g) || []).length === 7);
  ok('日历格子数是 7 的整数倍', cells % 7 === 0, `${cells} 格`);
  ok('日历只标一个「今天」', (cal.match(/is-today/g) || []).length === 1);
  // 长标题必须用一个能收缩的 span 包着，否则 flex 容器上省略号不生效、整排会被撑宽
  ok('日历条目标题包在可收缩的 span 里', /class="cal-text"/.test(cal));

  /* ---- 日历：空白格可点、还有 N 项可展开 ----
     用户反馈「日历上双击/右击都没反应，浪费了」。真正的硬伤其实是
     连单击都没有——「我想在某天加一件事」这个最自然的动作在日历上做不到。 */
  ok('日历空白格可点，点了在那天新建',
    /data-act="cal-new" data-date="\d{4}-\d{2}-\d{2}"/.test(cal));
  ok('每一格都带着自己的日期',
    (cal.match(/data-act="cal-new"/g) || []).length === cells,
    `${(cal.match(/data-act="cal-new"/g) || []).length} 个入口 / ${cells} 个格子`);

  // 造一天 4 项，把「还有 N 项」逼出来（格子最多只摆 3 条）
  const nowD = new Date();
  const busyIso = new Date(nowD.getFullYear(), nowD.getMonth(), nowD.getDate(), 18).toISOString();
  state.tasks.push(...[1, 2, 3, 4].map(i => ({
    id: 9000 + i, title: `临时项 ${i}`, note: '', categoryId: 1,
    pattern: 'once', status: 'todo', dueAt: busyIso,
  })));
  const calBusy = api.renderCalendar();
  ok('同一天超过 3 项时给出「还有 N 项」入口',
    /class="cal-more" data-act="cal-day" data-date="[\d-]+"/.test(calBusy));
  const busyDay = (calBusy.match(/data-act="cal-day" data-date="([\d-]+)"/) || [])[1];
  ok('「还有 N 项」带上了那天的日期', !!busyDay);
  ok('按日期能取回当天全部工作', api.entriesOn(busyDay).length >= 4,
    `${api.entriesOn(busyDay).length} 项`);

  api.openDayView(busyDay);
  const dayHtml = dom.modal.innerHTML;
  ok('按日弹窗把当天每条都列出来',
    (dayHtml.match(/class="day-item/g) || []).length === api.entriesOn(busyDay).length,
    `${(dayHtml.match(/class="day-item/g) || []).length} 条`);
  ok('按日弹窗里能直接在这一天新建', /data-act="day-new" data-date="/.test(dayHtml));
  state.dayView = null;
  api.renderModal();
  state.tasks = state.tasks.filter(t => t.id < 9000);

  ok('从日历某天新建时预填那天',
    new Date(api.dayPreset('2026-10-05').dueAt).getDate() === 5);
  ok('预填的日子连交期一起给上（切成阶段性就不用再选）',
    new Date(api.dayPreset('2026-10-05').endAt).getDate() === 5);
  ok('没给日期就不预填', api.dayPreset('') === null);

  /* ---- 跨天任务在日历上显示整段跨度 ----
     只标交期的话，一个跨三周的大任务在日历上只出现一格，
     中间这段时间在忙什么完全看不出来。 */
  {
    const s = state.tasks.find(t => t.pattern === 'stage');
    const bak = { due: s.dueAt, end: s.endAt };
    const from = new Date(); from.setDate(from.getDate() + 1); from.setHours(9, 0, 0, 0);
    const to = new Date(); to.setDate(to.getDate() + 5); to.setHours(23, 59, 0, 0);
    s.dueAt = from.toISOString();
    s.endAt = to.toISOString();

    const mid = new Date(); mid.setDate(mid.getDate() + 3);
    const midStr = api.toLocalDate(mid);
    const midEntries = api.entriesOn(midStr);

    ok('跨天工作在跨度中间那天也露出来',
      midEntries.some(e => e.t.id === s.id && e.kind === 'span'),
      JSON.stringify(midEntries.map(e => e.kind)));
    ok('交期那天算「due」，不算「span」',
      api.entriesOn(api.toLocalDate(to)).some(e => e.t.id === s.id && e.kind === 'due'));
    ok('日历上能看出跨度条是淡的', /cal-item is-span/.test(api.renderCalendar()));
    // 同一天里既有别的任务的交期、又有这条的跨度条时，交期要排在前面
    const other = state.tasks.find(t => t.pattern !== 'stage' && t.status !== 'done');
    const bakOther = other.dueAt;
    other.dueAt = mid.toISOString();
    const mixed = api.entriesOn(midStr);
    ok('同一天里交期排在跨度条前面',
      mixed.some(e => e.kind === 'span') &&
      mixed.findIndex(x => x.kind === 'due') < mixed.findIndex(x => x.kind === 'span'),
      JSON.stringify(mixed.map(e => e.kind)));
    other.dueAt = bakOther;
    // 「顺延一天」挪的是「今天该交的活」，把跨度中间的日子也算进去会误伤大任务
    ok('顺延只认交期，不认跨度中间日',
      !api.tasksOn(midStr).some(t => t.id === s.id));

    s.dueAt = bak.due; s.endAt = bak.end;
  }

  /* ---- 表单重绘不丢内容 ----
     用户报过的问题：新建工作里填好标题，再去选分类或换节奏类型，标题就没了。
     根因是弹窗整块重绘，但重绘前没把已输入的内容收回 state。
     这里走真实的事件处理器（而不是直接调 renderModal），漏了 sync 就会被抓出来。 */
  ctx.__api.bind();
  const click = act => (bodyHandlers.click || []).forEach(fn => fn({
    target: { id: '', closest: sel => (sel === '[data-act]' ? act : null) },
  }));
  const read = id => dom.get(id).value;
  const TITLE = '整理季度复盘材料';
  const NOTE = '先对齐三个部门的数字';

  api.openEditor(null);
  ok('新开面板时标题是空的', read('m-title') === '');
  dom.get('m-title').value = TITLE;       // 模拟用户输入
  dom.get('m-note').value = NOTE;

  click({ dataset: { act: 'm-cat', cat: '2' } });
  ok('选分类后标题还在', read('m-title') === TITLE, `实际「${read('m-title')}」`);
  ok('选分类后备注还在', read('m-note') === NOTE);

  click({ dataset: { act: 'm-pattern', pattern: 'recurring' } });
  ok('切节奏类型后标题还在', read('m-title') === TITLE);
  ok('切到周期性会补一条默认规则', state.editing.rule?.freq === 'weekly');

  click({ dataset: { act: 'm-freq', freq: 'monthly' } });
  ok('切频率后标题还在', read('m-title') === TITLE);
  ok('每月只画日号网格，不画星期几',
    /data-act="m-dom"/.test(dom.modal.innerHTML) && !/data-act="m-day"/.test(dom.modal.innerHTML));

  click({ dataset: { act: 'm-dom', dom: '15' } });
  ok('点 15 号写进规则', state.editing.rule.byDay[0] === 15);
  ok('选完日号标题依然在', read('m-title') === TITLE);

  click({ dataset: { act: 'm-freq', freq: 'quarterly' } });
  ok('每季度同时画起始月与日号两个网格',
    /data-act="m-month"/.test(dom.modal.innerHTML) && /data-act="m-dom"/.test(dom.modal.innerHTML));
  click({ dataset: { act: 'm-month', month: '3' } });
  click({ dataset: { act: 'm-dom', dom: '31' } });
  ok('季度规则记下锚点月与日号',
    state.editing.rule.byMonth[0] === 3 && state.editing.rule.byDay[0] === 31,
    JSON.stringify(state.editing.rule));
  ok('连点五个控件后标题没丢', read('m-title') === TITLE, `实际「${read('m-title')}」`);
  ok('连点五个控件后备注没丢', read('m-note') === NOTE);

  click({ dataset: { act: 'm-freq', freq: 'weekly' } });
  ok('切回每星期把 15 号换成合理的星期',
    state.editing.rule.byDay.every(i => i >= 0 && i <= 6) && state.editing.rule.byDay.length > 0,
    JSON.stringify(state.editing.rule.byDay));
  ok('切回每星期后标题还在', read('m-title') === TITLE);

  /* ---- 周期规则文案 ---- */
  const label = r => api.ruleLabel({ pattern: 'recurring', rule: r });
  ok('每天', label({ freq: 'daily', time: '09:00' }) === '每天 09:00');
  ok('每周可多选', label({ freq: 'weekly', byDay: [1, 3, 5], time: '09:30' }) === '每周一、周三、周五 09:30');
  ok('每月带日号', label({ freq: 'monthly', byDay: [15], time: '10:00' }) === '每月 15 日 10:00');
  ok('每月 31 号读作月末', label({ freq: 'monthly', byDay: [31], time: '10:00' }) === '每月月末 10:00');
  ok('季度列出命中的四个月',
    label({ freq: 'quarterly', byDay: [25], byMonth: [3], time: '17:00' }) === '每季度 3/6/9/12 月 25 日 17:00');
  ok('锚点不是常规月时四个月份照样升序',
    api.quarterMonths({ byMonth: [11] }).join(',') === '2,5,8,11');

  /* ---- 预览引擎与后端算法对齐（对应 schedule.rs 的单测）---- */
  const ymd = iso => {
    const d = new Date(iso);
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
  };
  const nextOn = (rule, from, inclusive = true) => ymd(api.nextOccurrence(rule, from, inclusive));
  const Q = (m, d) => ({ freq: 'quarterly', byDay: [d], byMonth: [m], time: '09:00' });
  ok('季度锚点 1 月 → 落在 10 月',
    nextOn(Q(1, 15), '2026-09-23T10:00:00') === '2026-10-15');
  ok('季度锚点 3 月 → 落在 9 月',
    nextOn(Q(3, 25), '2026-09-23T10:00:00') === '2026-09-25');
  ok('季度跨年：12 月之后回到次年 3 月',
    nextOn(Q(3, 25), '2026-12-25T18:00:00', false) === '2027-03-25');
  ok('季度选 31 号时小月落到月末',
    nextOn({ ...Q(3, 31), time: '09:00' }, '2026-09-01T00:00:00') === '2026-09-30');
  ok('每月 31 号在 2 月落到 28 号',
    nextOn({ freq: 'monthly', byDay: [31], time: '10:00' }, '2026-01-31T11:00:00', false) === '2026-02-28');
  ok('每周五（与后端用例同一组数据）',
    nextOn({ freq: 'weekly', byDay: [5], time: '17:00' }, '2026-09-23T10:00:00') === '2026-09-25');

  /* ---- 设置页的数据管理（备份 / 恢复 / 导出）----
     这几个按钮点下去是不可逆的（恢复会顶掉全部数据），
     所以既要有入口，也得把后果写在旁边。 */
  const settings = api.renderSettings();
  for (const [label, act] of [
    ['备份数据', 'backup-now'],
    ['从备份恢复', 'restore-now'],
    ['导出为表格', 'export-csv'],
    ['打开目录', 'open-data-dir'],
  ]) {
    ok(`设置页有「${label}」入口`, settings.includes(`data-act="${act}"`));
  }
  ok('恢复那条把「会替换全部数据」说明白了',
    /替换当前全部数据/.test(settings));
  ok('恢复那条讲了自动留存快照，选错能切回来',
    /另存一份/.test(settings) && /切回来/.test(settings));
  ok('设置页仍然标着数据存放位置', /数据存放位置/.test(settings));
  ok('给用户看的不是数据库内部术语', !/VACUUM|ATTACH/.test(settings));

  /* ---- 关于 / 版权 ---- */
  ok('设置页有「关于」区块', /section-title">关于</.test(settings));
  ok('关于里同时挂着中英双名',
    settings.includes(api.APP_NAME_CN) && settings.includes(api.APP_NAME_EN));
  ok('关于里写了版权署名', settings.includes('JJAI'), api.APP_COPYRIGHT);
  ok('关于里的版本号就是打包版本',
    settings.includes(`版本 ${api.APP_VERSION_FALLBACK}`), `界面写着 ${api.APP_VERSION_FALLBACK}`);
  // 版本号散在四处，这里把它们串起来对一遍
  ok('Cargo.toml 与 tauri.conf.json 的版本号一致',
    !!cargoVer && cargoVer === confVer, `Cargo ${cargoVer} / conf ${confVer}`);
  ok('前端兜底版本号跟得上打包版本',
    api.APP_VERSION_FALLBACK === cargoVer, `前端 ${api.APP_VERSION_FALLBACK} / Cargo ${cargoVer}`);
  ok('标题栏同时挂着中英双名',
    rawHtml.includes(`>${api.APP_NAME_CN}<`) && rawHtml.includes(`>${api.APP_NAME_EN}<`));

  /* ---- 开关的默认值 ----
     开机自启必须默认关：它要往系统里写启动项，不能替用户做这个决定，
     而且那正是杀软行为引擎最敏感的动作。其余开关只是程序内部行为，默认开。 */
  const switchOn = key => {
    const m = settings.match(new RegExp(`class="switch([^"]*)"[^>]*data-key="${key}"`));
    return !!m && /(^|\s)on(\s|$)/.test(m[1]);
  };
  ok('开机自启默认是关的', !switchOn('autostart'));
  ok('到期提醒默认开着', switchOn('notify'));
  ok('最小化到托盘默认开着', switchOn('tray'));
  ok('提醒开着、自启关着时，把「重启后收不到提醒」讲明白',
    /重启电脑后就收不到提醒/.test(settings));
  state.settings.autostart = '1';
  ok('打开自启后那条提醒就消失了',
    !/重启电脑后就收不到提醒/.test(api.renderSettings()));
  delete state.settings.autostart;

  /* ---- 备份文件名 ---- */
  const bakName = api.stampName('db');
  ok('备份文件名带日期时间，连备份两次不会互相覆盖',
    /^工作记录本-\d{8}-\d{4}\.db$/.test(bakName));
  ok('CSV 用同一个命名规则，只是后缀不同',
    api.stampName('csv').endsWith('.csv') && api.stampName('csv').startsWith('工作记录本-'));

  /* ---- 模板库 ----
     反馈里最有价值的一条：「同一套 20 个任务和子任务反复手建，费时又烦」。
     模板要解决的是这个重复劳动，不是层级不够深。 */
  const tplSrc = api.renderTemplates();
  ok('模板页列出全部模板',
    (tplSrc.match(/class="tpl-card"/g) || []).length === state.templates.length,
    `${state.templates.length} 个`);
  ok('模板卡片写清有几件工作、几个条目', /件工作/.test(tplSrc) && /个条目/.test(tplSrc));
  ok('多层结构标出层数', /\d 层结构/.test(tplSrc));
  ok('模板卡片有「新建一批」入口', /data-act="tpl-apply"/.test(tplSrc));
  ok('模板卡片有删除入口', /data-act="tpl-del"/.test(tplSrc));

  // 这是整个模板设计的关键：存绝对日期的话，下个月调用就是一堆过期任务
  ok('模板里存的是相对天数，不是绝对日期',
    state.templates.every(t => t.items.every(i => !('dueAt' in i) && !('endAt' in i))));
  ok('模板条目的偏移能表达跨天跨度',
    state.templates[0].items.some(i => (i.endOffset ?? 0) > (i.dueOffset ?? 0)));

  // 空模板库要说清怎么建，而不是只留一片空白
  {
    const bak = state.templates;
    state.templates = [];
    const empty = api.renderTemplates();
    state.templates = bak;
    ok('没有模板时给出「怎么建」的指引', /存为模板/.test(empty));
    ok('空模板库里不摆卡片', !/class="tpl-card"/.test(empty));
  }

  /* ---- 侧栏的模板入口 ---- */
  api.renderSidebar();
  const side = dom.get('sidebar').innerHTML;
  ok('侧栏有「模板」入口', /data-nav="tpl"/.test(side));
  ok('模板入口带模板数量', /data-nav="tpl"[\s\S]{0,200}?nav-count">\d+</.test(side));

  /* ---- 抽屉里的复制与存模板 ----
     放在最后跑：renderDrawer 会先把输入框里的内容收回 state，
     这里没有真输入框（值都是空的），日期字段会被清掉，不适合再往下测数据。 */
  state.drawerId = stage.id;
  api.renderDrawer();
  const drawer = dom.get('drawer').innerHTML;
  ok('抽屉里有「复制一份」入口', /data-act="duplicate"/.test(drawer));
  ok('抽屉里有「存为模板」入口', /data-act="save-as-template"/.test(drawer));
  ok('抽屉说明了模板会连步骤一起存', /步骤一起存成模板/.test(drawer));
  state.drawerId = null;

  /* ---- 赞赏码 ----
     入口刻意做得不起眼：藏在设置页「关于」最下面，和版本、版权信息并排。
     但几条底线得盯住 —— 图真打进去了、码够大扫得动、文案把性质说清楚了。
     另外原始海报上那行「xxx的赞赏码」不能出现：那个昵称和软件署名对不上，
     留在界面里会让用户怀疑码是不是被人换过。 */
  ok('设置页「关于」里有赞赏入口', settings.includes('data-act="open-reward"'));

  const qrPath = path.join(__dirname, '..', 'ui', 'assets', 'reward-qr.png');
  ok('赞赏码图片打进了前端资源', fs.existsSync(qrPath));
  if (fs.existsSync(qrPath)) {
    const qb = fs.readFileSync(qrPath);
    const qw = qb.readUInt32BE(16), qh = qb.readUInt32BE(20);
    ok('赞赏码是正方形', qw === qh, `${qw}x${qh}`);
    // 微信实测小于 200px 就不容易扫出来；卡 400 是给 2x 屏留的余量
    ok('赞赏码边长够扫（≥400px）', qw >= 400, `${qw}px`);
    ok('赞赏码用 PNG 无损，有损压缩会糊掉码点',
      qb.slice(1, 4).toString('ascii') === 'PNG');
  }

  const clickMask = () => (bodyHandlers.click || []).forEach(fn => fn({
    target: { id: 'modal-mask', closest: () => null },
  }));
  const pressKey = k => [...(docHandlers.keydown || []), ...(bodyHandlers.keydown || [])]
    .forEach(fn => fn({
      key: k, target: null, ctrlKey: false, metaKey: false, preventDefault() {},
    }));

  state.reward = true;
  api.renderModal();
  const rewardHtml = dom.modal.innerHTML;
  ok('赞赏弹窗里挂着二维码图片', /assets\/reward-qr\.png/.test(rewardHtml));
  ok('赞赏弹窗讲明「免费开源 · 赞赏自愿 · 不影响功能」',
    /完全免费开源/.test(rewardHtml) && /自愿/.test(rewardHtml) && /不影响任何功能/.test(rewardHtml));
  ok('赞赏弹窗不出现和软件署名对不上的个人昵称', !/大胡子俊杰/.test(rewardHtml));

  // 三条关闭路径都得通：用户之前就报过「点外面关不掉」
  clickMask();
  ok('点弹窗外面能关掉赞赏码', state.reward === false);

  state.reward = true; api.renderModal();
  click({ dataset: { act: 'close-aux-modal' } });
  ok('弹窗上的关闭按钮能关掉赞赏码', state.reward === false);

  state.reward = true; api.renderModal();
  pressKey('Escape');
  ok('按 Esc 能关掉赞赏码', state.reward === false);

  // Ctrl+N 在弹窗开着时也能触发（监听挂在 document 上）。不把 reward 清掉的话，
  // 编辑器会被赞赏码盖在后面，用户看到的是「快捷键没反应」。
  state.reward = true;
  api.renderModal();
  api.openEditor(null);
  ok('赞赏码开着时新建工作，编辑器不会被挡在后面',
    state.reward === false && !!state.editing);
  state.editing = null;
  api.renderModal();

  console.log('---- 渲染自检 ----');
  checks.forEach(([s, n, e]) => console.log(`  ${s}  ${n}${e ? '  → ' + e : ''}`));
  const failed = checks.filter(c => c[0] === 'FAIL').length;
  console.log(failed ? `\n${failed} 项未通过` : '\n全部通过');
  process.exit(failed ? 1 : 0);
})();
