/* 渲染自检：脱离界面把 taskRow / subtaskEditor / renderBoard / renderCalendar 跑一遍，
   确认几件容易改坏的事——备注该显示时显示、子任务进度算得对、
   看板分列不丢件、日历格子和「今天」标记不错位，
   以及「表单里填了一半，点个按钮就没了」这类重绘问题。
   用法：node tools/check_ui.js */
const fs = require('fs');
const path = require('path');
const vm = require('vm');

const src = fs.readFileSync(path.join(__dirname, '..', 'ui', 'assets', 'app.js'), 'utf8');
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

const ctx = {
  console,
  window: {},                     // 没有 __TAURI__ → 走预览数据
  document: {
    activeElement: null,
    addEventListener() {},
    getElementById: dom.get,
    querySelectorAll: () => [],
    body: { addEventListener: (t, fn) => { (bodyHandlers[t] = bodyHandlers[t] || []).push(fn); } },
  },
  requestAnimationFrame: fn => fn(),
  setTimeout, clearTimeout,
};
vm.createContext(ctx);
vm.runInContext(
  body + '\n;globalThis.__api = { state, loadAll, taskRow, subtaskEditor, applySubtaskProgress, subtasksOf, renderDrawer, renderBoard, renderCalendar, laneOf, scopeTasks, bind, openEditor, syncEditorFields, ruleLabel, quarterMonths, dayText, nextOccurrence, defaultRuleFor, FREQS, renderSettings, stampName };',
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
    stage.progress.done === 3 && stage.progress.total === 5, JSON.stringify(stage.progress));
  ok('行上显示 done/total', row(stage.id).includes('3/5'));
  ok('进度条宽度按完成比例', /width:60%/.test(row(stage.id)));
  ok('抽屉里子任务条数与数据一致',
    (api.subtaskEditor(stage).match(/class="sub-item/g) || []).length === 5);

  // 全部勾上 → 行上提示可以收尾
  api.subtasksOf(stage.id).forEach(s => { s.status = 'done'; });
  api.applySubtaskProgress();
  ok('子任务全完成时行上提示「待收尾」', row(stage.id).includes('待收尾'));

  // 一个都没有 → 不显示进度条，也不假装有进度
  api.subtasksOf(stage.id).forEach(s => { s.status = 'todo'; });
  state.subtasks = state.subtasks.filter(s => s.parentId !== stage.id);
  api.applySubtaskProgress();
  ok('没有子任务时进度归零', stage.progress.total === 0 && stage.progress.done === 0);
  ok('没有子任务时不画进度条', !/class="progress"/.test(row(stage.id)));
  ok('没有子任务时提示「未拆解」', row(stage.id).includes('未拆解'));

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

  /* ---- 备份文件名 ---- */
  const bakName = api.stampName('db');
  ok('备份文件名带日期时间，连备份两次不会互相覆盖',
    /^工作记录本-\d{8}-\d{4}\.db$/.test(bakName));
  ok('CSV 用同一个命名规则，只是后缀不同',
    api.stampName('csv').endsWith('.csv') && api.stampName('csv').startsWith('工作记录本-'));

  console.log('---- 渲染自检 ----');
  checks.forEach(([s, n, e]) => console.log(`  ${s}  ${n}${e ? '  → ' + e : ''}`));
  const failed = checks.filter(c => c[0] === 'FAIL').length;
  console.log(failed ? `\n${failed} 项未通过` : '\n全部通过');
  process.exit(failed ? 1 : 0);
})();
