/* 冒烟测试：把界面放进真浏览器里跑一遍，看它是不是真的画出来了、点得动。
 *
 * 和 check_ui.js 的分工 —— 两个都要跑，谁也替不了谁：
 *   check_ui.js  在 Node 里用一个假 DOM 校验「渲染函数吐出来的字符串」。快、能覆盖
 *                边界情况，但它证明不了 CSS 生效、事件真的接上了、控制台没报错。
 *   smoke.js     补的就是后者：调本机装好的 Edge / Chrome，开一个无头标签页，
 *                真点、真粘、真按 Esc。
 *
 * 用法：
 *   node tools/smoke.js              无头跑一遍，输出通过 / 失败清单
 *   node tools/smoke.js --headed     开着窗口跑，出问题时方便肉眼看
 *   node tools/smoke.js --keep       跑完不关浏览器（配 --headed 排查用）
 *   node tools/smoke.js --only=paste 只跑某个场景
 *
 * 依赖：本机装了 Edge 或 Chrome。只用 Node 内置能力（fetch + WebSocket + http），
 *      不需要 npm 装任何包 —— 这也是当初没选 agent-browser 的原因，
 *      为了跑一次冒烟去下几百兆 Chromium 不值当。
 */
const fs = require('fs');
const os = require('os');
const path = require('path');
const http = require('http');
const { spawn, spawnSync } = require('child_process');
const { build } = require('./make_preview.js');

const APP = path.join(__dirname, '..');
const OUT = path.join(APP, 'out');
const SMOKE_DIR = path.join(OUT, '_smoke');
const headed = process.argv.includes('--headed');
const keep = process.argv.includes('--keep');
const onlyArg = process.argv.find(a => a.startsWith('--only='));
const only = onlyArg ? onlyArg.split('=')[1] : null;

const sleep = ms => new Promise(r => setTimeout(r, ms));

/* ---------------- 结果记录 ---------------- */

let passed = 0;
const failures = [];
const notes = [];

function check(name, cond, detail) {
  if (cond) { passed++; console.log(`  PASS  ${name}${detail ? '  → ' + detail : ''}`); }
  else { failures.push(name); console.log(`  FAIL  ${name}${detail ? '  → ' + detail : ''}`); }
  return !!cond;
}

/** 冒烟测试里有些东西是「尽力而为」的：比如无头浏览器里剪贴板构造受限。
 *  这类只记一行说明，不算失败 —— 免得报一堆假红让人不再信这份结果。 */
function note(text) { notes.push(text); console.log(`  note  ${text}`); }

/* ---------------- 1. 找浏览器 ---------------- */

const BROWSERS = [
  process.env.SMOKE_BROWSER,
  'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
  'C:/Program Files/Microsoft/Edge/Application/msedge.exe',
  'C:/Program Files/Google/Chrome/Application/chrome.exe',
  'C:/Program Files (x86)/Google/Chrome/Application/chrome.exe',
].filter(Boolean);

const browser = BROWSERS.find(p => fs.existsSync(p));
if (!browser) {
  console.error('没找到 Edge 或 Chrome，冒烟测试跑不了。');
  console.error('装了的话可以用 SMOKE_BROWSER=<可执行文件路径> 指定。');
  process.exit(2);
}
console.log(`浏览器：${browser}`);

/* ---------------- 2. 生成各场景的静态副本 ---------------- */

/** 每个场景都是 make_preview 生成的一份静态页。这样冒烟测试不需要编译整个
 *  Tauri 程序，也不需要等 cargo —— 改一行前端就能立刻验。 */
const SCENARIOS = [
  {
    key: 'drawer',
    title: '详情抽屉：附件区能看能点',
    preview: { view: 'all', drawer: 1 },
    async run(t) {
      check('抽屉真的画出来了', await t.eval(`!!document.querySelector('#drawer .drawer-head')`));
      const zone = await t.eval(`!!document.querySelector('#drawer .att-zone')`);
      check('附件投放区在抽屉里', zone);
      check('投放区提示了 Ctrl+V 粘贴',
        await t.eval(`/Ctrl/.test(document.querySelector('#drawer .att-zone')?.textContent || '')`));
      const n = await t.eval(`document.querySelectorAll('#drawer .att-card').length`);
      check('示例附件画成了卡片', n === 2, `${n} 张`);
      check('图片附件渲染成缩略图',
        await t.eval(`!!document.querySelector('#drawer .att-card img.att-thumb')`));
      check('非图片附件显示扩展名',
        await t.eval(`(document.querySelector('#drawer .att-file')?.textContent || '').includes('PDF')`));

      // 缩略图是真的解码出来了，不是个空 img（CSS 生效、data URL 没被 CSP 拦掉）
      const decoded = await t.eval(`(async () => {
        const img = document.querySelector('#drawer .att-card img.att-thumb');
        if (!img) return 'no-img';
        if (img.complete && img.naturalWidth) return 'ok';
        await new Promise(r => { img.onload = r; img.onerror = r; setTimeout(r, 2000); });
        return img.naturalWidth ? 'ok' : 'broken:' + img.naturalWidth;
      })()`);
      check('缩略图真的解码出来了', decoded === 'ok', String(decoded));

      // 点缩略图 → 灯箱
      await t.eval(`document.querySelector('#drawer .att-hit').click()`);
      check('点图片打开了大图预览', await t.waitFor(`!!document.querySelector('#lightbox.open')`));
      check('灯箱里挂着那张图',
        await t.eval(`!!document.querySelector('#lightbox .lightbox-img')`));
      check('灯箱铺在了最上层', await t.eval(`(function(){
        const z = getComputedStyle(document.getElementById('lightbox')).zIndex;
        return Number(z) >= 70;
      })()`), await t.eval(`getComputedStyle(document.getElementById('lightbox')).zIndex`));

      // Esc 关掉
      await t.eval(`document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))`);
      check('按 Esc 能关掉大图', await t.waitFor(`!document.querySelector('#lightbox.open')`));
      check('关掉后灯箱内容清干净了',
        await t.eval(`document.getElementById('lightbox').innerHTML === ''`));
    },
  },

  {
    key: 'paste',
    title: '粘贴：Ctrl+V 把截图存成留痕',
    preview: { view: 'all', drawer: 1 },
    async run(t) {
      // 列表行上那个「有留痕」数字，粘之前先记下来。
      // mock 数据本来就给任务 1 挂了附件，所以「标记存在」是恒真的 ——
      // 只有比数字才说明是这一下粘贴把它加上去的。
      const badgeOf = `(document.querySelector('#view .task[data-id="1"] .att-badge')?.textContent || '').trim()`;
      const badgeBefore = await t.eval(badgeOf);

      const r = await t.eval(`(async () => {
        // 造一张真 PNG（canvas 出的），走和真实剪贴板同一条路
        const c = document.createElement('canvas');
        c.width = 60; c.height = 60;
        const g = c.getContext('2d');
        g.fillStyle = '#4F5BE8'; g.fillRect(0, 0, 60, 60);
        g.fillStyle = '#fff'; g.font = '22px sans-serif'; g.fillText('图', 16, 42);
        const blob = await new Promise(res => c.toBlob(res, 'image/png'));

        const dt = new DataTransfer();
        dt.items.add(new File([blob], '和客户的聊天记录.png', { type: 'image/png' }));
        const before = state.attachments.filter(a => a.taskId === 1).length;
        const ev = new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true });
        document.dispatchEvent(ev);
        for (let i = 0; i < 80; i++) {
          await new Promise(res => setTimeout(res, 50));
          const now = state.attachments.filter(a => a.taskId === 1).length;
          if (now > before) return { before, after: now, prevented: ev.defaultPrevented };
        }
        return { before, after: before, timedOut: true };
      })()`);

      if (!r || r.timedOut) {
        check('粘贴事件被接住并新增了附件', false,
          r && r.before === 0 ? '一条都没进来' : `条数没变（${r && r.before}）`);
      } else {
        check('粘贴事件被接住并新增了附件', r.after === r.before + 1,
          `${r.before} → ${r.after}`);
        check('粘贴时压掉了浏览器默认行为', r.prevented === true);
      }
      check('新附件立刻出现在抽屉里',
        await t.waitFor(`document.querySelectorAll('#drawer .att-card').length >= 3`),
        `${await t.eval(`document.querySelectorAll('#drawer .att-card').length`)} 张`);
      check('新附件带上了原始文件名',
        await t.eval(`[...document.querySelectorAll('#drawer .att-name')]
          .some(e => e.textContent.includes('和客户的聊天记录'))`));
      const badgeAfter = await t.eval(badgeOf);
      check('列表行上的「有留痕」数字跟着涨了',
        Number(badgeAfter) === Number(badgeBefore) + 1,
        `${badgeBefore || '(无)'} → ${badgeAfter || '(无)'}`);

      // 删掉刚粘进来的那张。这条走的是 deleteAttachment 里「只改标记、不重画整屏」
      // 那个分支 —— 顺手把列表滚动位置也验了，重画整屏会把它拉回顶部。
      const del = await t.eval(`(async () => {
        const v = document.getElementById('view');
        v.scrollTop = 140;
        await new Promise(r => requestAnimationFrame(r));
        const setTop = v.scrollTop;
        const card = [...document.querySelectorAll('#drawer .att-card')]
          .find(c => (c.querySelector('.att-name')?.textContent || '').includes('和客户的聊天记录'));
        if (!card) return { error: '抽屉里找不到刚粘的那张' };
        card.querySelector('.att-del').click();
        await new Promise(r => setTimeout(r, 350));
        return {
          setTop, after: v.scrollTop,
          badge: (document.querySelector('#view .task[data-id="1"] .att-badge')?.textContent || '').trim(),
          left: document.querySelectorAll('#drawer .att-card').length,
        };
      })()`);
      if (!del || del.error) {
        check('能删掉刚粘进来的附件', false, del && del.error);
      } else {
        check('能删掉刚粘进来的附件', true);
        check('删掉后标记数字退回去了', Number(del.badge) === Number(badgeBefore),
          `${badgeAfter} → ${del.badge}`);
        check('抽屉里那张卡片也没了', del.left === 2, `${del.left} 张`);
        if (del.setTop) {
          check('删附件时列表没被拉回顶部', del.after === del.setTop,
            `scrollTop ${del.setTop} → ${del.after}`);
        } else {
          note('列表内容不够长、滚不动，「删附件不跳回顶部」这条这次没验到');
        }
      }

      // 备注里没内容时不应该被粘进来；只认文件
      const noFile = await t.eval(`(async () => {
        const before = state.attachments.length;
        const dt = new DataTransfer();
        dt.setData('text/plain', '只是段文字，不是文件');
        document.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true }));
        await new Promise(r => setTimeout(r, 300));
        return state.attachments.length === before;
      })()`);
      check('只粘文字时不会凭空多出附件', noFile);
    },
  },

  {
    key: 'catnew',
    title: '在分类页新建：分类自动带好',
    // 故意挑第二个分类（副业）。挑第一个分类的话，即使自动关联坏掉、
    // 退回「默认选第一个」，断言照样会绿 —— 那样这条测试就是白写的。
    preview: { view: 'cat:2', mode: 'list' },
    async run(t) {
      check('进到了「副业」这一页',
        await t.eval(`state.view === 'cat:2' && /副业/.test(document.getElementById('view').textContent)`));

      await t.eval(`document.getElementById('btn-new').click()`);
      check('新建面板弹出来了', await t.waitFor(`!!document.querySelector('#modal .modal-body')`));

      const fixed = await t.eval(`(document.querySelector('#modal .cat-fixed')?.textContent || '').trim()`);
      check('分类栏收成了一行，不再铺整排', await t.eval(`!!document.querySelector('#modal .cat-fixed')`));
      check('自动带上了当前分类', fixed.includes('副业'), fixed || '(空)');
      check('没有铺开整排分类选择',
        await t.eval(`!document.querySelector('#modal .cat-grid')`));
      check('留了「更改」的入口',
        await t.eval(`!!document.querySelector('#modal [data-act="m-cat-open"]')`));
      check('附件投放区也在新建面板里',
        await t.eval(`!!document.querySelector('#modal .att-zone')`));

      // 光「看着像」不算数：真正要保证的是保存时写进去的就是当前分类
      const def = await t.eval(`state.editing && state.editing.categoryId`);
      check('编辑对象里的分类已经是当前分类', def === 2, `categoryId=${def}`);
      check('带的不是「第一个分类」那种兜底值', def !== 1);

      await t.eval(`document.querySelector('#modal [data-act="m-cat-open"]').click()`);
      check('点「更改」后铺开整排分类',
        await t.waitFor(`!!document.querySelector('#modal .cat-grid')`));
      check('铺开后不再显示那行固定的',
        await t.eval(`!document.querySelector('#modal .cat-fixed')`));
      check('铺开时把当前分类标成了选中',
        await t.eval(`!!document.querySelector('#modal .cat-grid .cat-pill.on, #modal .cat-grid .cat-pill.active')`));

      // 存一条进去，看它是不是真落在当前分类下（全程没碰过分类选择）
      const landed = await t.eval(`(async () => {
        document.getElementById('m-title').value = '冒烟测试：分类自动关联';
        document.querySelector('#modal [data-act="m-save"]').click();
        await new Promise(r => setTimeout(r, 400));
        const task = state.tasks.find(x => x.title === '冒烟测试：分类自动关联');
        return task ? task.categoryId : null;
      })()`);
      check('存下来的工作就落在当前分类下', landed === 2, `categoryId=${landed}`);
      check('存完面板关掉了', await t.eval(`!document.querySelector('#modal .modal-body')`));
    },
  },

  {
    key: 'todaynew',
    title: '在「今天」新建：仍让人选分类',
    preview: { view: 'today', mode: 'list' },
    async run(t) {
      await t.eval(`document.getElementById('btn-new').click()`);
      await t.waitFor(`!!document.querySelector('#modal .modal-body')`);
      check('「今天」这种跨分类的视图照旧让人选',
        await t.eval(`!!document.querySelector('#modal .cat-grid')`));
      check('没有硬塞一个分类',
        await t.eval(`!document.querySelector('#modal .cat-fixed')`));
    },
  },

  {
    key: 'empty',
    title: '空数据：全新安装不炸',
    preview: { view: 'all', empty: true },
    async run(t) {
      check('主区画出来了', await t.eval(`!!document.getElementById('view').innerHTML`));
      check('侧栏还在', await t.eval(`!!document.querySelector('#sidebar .sidebar-inner, #sidebar *')`));
      await t.eval(`document.getElementById('btn-new').click()`);
      check('空数据下新建面板也能开', await t.waitFor(`!!document.querySelector('#modal .modal-body')`));
      check('空数据下附件区照样在', await t.eval(`!!document.querySelector('#modal .att-zone')`));
      check('空数据下没有附件卡片', await t.eval(`!document.querySelector('#modal .att-card')`));
    },
  },
];

const wanted = SCENARIOS.filter(s => !only || s.key === only);
if (!wanted.length) {
  console.error(`没有叫 ${only} 的场景。可用的：${SCENARIOS.map(s => s.key).join(' / ')}`);
  process.exit(2);
}

console.log('\n---- 生成静态副本 ----');
fs.rmSync(SMOKE_DIR, { recursive: true, force: true });
for (const s of wanted) {
  const dir = path.join(SMOKE_DIR, s.key);
  build(Object.assign({ out: dir }, s.preview));
  const shown = Object.entries(s.preview).map(([k, v]) => `${k}=${v}`).join(' ');
  console.log(`  ${s.key.padEnd(10)} ${shown}`);
}

/* ---------------- 3. 起一个只监听本机的静态服务 ---------------- */

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.png': 'image/png', '.jpg': 'image/jpeg', '.svg': 'image/svg+xml',
};

function serve(root) {
  const misses = [];
  const srv = http.createServer((req, res) => {
    const rel = decodeURIComponent(req.url.split('?')[0]);
    const file = path.join(root, rel === '/' ? 'index.html' : rel);
    // 只放行 root 底下的文件，别让 ../ 跑到项目外面去
    if (!path.resolve(file).startsWith(path.resolve(root))) { res.writeHead(403).end(); return; }
    fs.readFile(file, (err, buf) => {
      if (err) { misses.push(rel); res.writeHead(404).end(); return; }
      res.writeHead(200, { 'Content-Type': MIME[path.extname(file)] || 'application/octet-stream' });
      res.end(buf);
    });
  });
  return new Promise(r => srv.listen(0, '127.0.0.1', () => {
    srv.misses = misses;
    r(srv);
  }));
}

/* ---------------- 4. 一个够用就行的 CDP 客户端 ---------------- */

function connect(wsUrl) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(wsUrl);
    let seq = 0;
    const pending = new Map();
    const events = [];
    ws.addEventListener('message', ev => {
      const msg = JSON.parse(ev.data);
      if (msg.id && pending.has(msg.id)) {
        const { res, rej } = pending.get(msg.id);
        pending.delete(msg.id);
        msg.error ? rej(new Error(JSON.stringify(msg.error))) : res(msg.result);
      } else if (msg.method) {
        events.push(msg);
      }
    });
    ws.addEventListener('error', () => reject(new Error('连不上调试端口')));
    ws.addEventListener('open', () => resolve({
      events,
      send(method, params) {
        return new Promise((res, rej) => {
          const id = ++seq;
          pending.set(id, { res, rej });
          ws.send(JSON.stringify({ id, method, params: params || {} }));
        });
      },
      close: () => ws.close(),
    }));
  });
}

/** 把控制台里的错误挑出来。冒烟测试最怕的其实不是「某个元素没画出来」，
 *  而是「界面看着正常、底下一直在抛异常」—— 那种只有看控制台才发现。 */
function errorsIn(events) {
  const out = [];
  for (const e of events) {
    const p = e.params || {};
    if (e.method === 'Runtime.exceptionThrown') {
      const d = p.exceptionDetails || {};
      out.push('未捕获异常: ' + ((d.exception && d.exception.description) || d.text || '?'));
    } else if (e.method === 'Runtime.consoleAPICalled' && p.type === 'error') {
      out.push('console.error: ' + (p.args || []).map(a => a.value ?? a.description).join(' '));
    } else if (e.method === 'Log.entryAdded' && p.entry && p.entry.level === 'error') {
      out.push('日志错误: ' + p.entry.text + (p.entry.url ? ` (${p.entry.url})` : ''));
    }
  }
  return out;
}

/* ---------------- 5. 跑起来 ---------------- */

(async () => {
  const srv = await serve(OUT);
  const base = `http://127.0.0.1:${srv.address().port}`;

  const profile = fs.mkdtempSync(path.join(os.tmpdir(), 'wll-smoke-'));
  const args = [
    '--remote-debugging-port=0',
    `--user-data-dir=${profile}`,
    '--no-first-run', '--no-default-browser-check', '--disable-extensions',
    '--disable-background-networking', '--disable-sync',
    '--window-size=1280,900',
  ];
  if (!headed) args.push('--headless=new', '--disable-gpu');
  args.push('about:blank');

  const proc = spawn(browser, args, { stdio: 'ignore', detached: false });

  const portFile = path.join(profile, 'DevToolsActivePort');
  let port = 0;
  for (let i = 0; i < 300 && !port; i++) {
    await sleep(100);
    if (fs.existsSync(portFile)) {
      const line = fs.readFileSync(portFile, 'utf8').split('\n')[0].trim();
      if (line) port = Number(line);
    }
  }
  if (!port) { console.error('浏览器没起来（等不到调试端口）'); process.exit(1); }
  console.log(`调试端口：${port}`);

  let cleaned = false;
  const cleanup = () => {
    if (cleaned) return;
    cleaned = true;
    if (!keep) {
      // 浏览器会派生一堆子进程，光 kill 主进程它们会变成孤儿，得连整棵树一起收
      try { spawnSync('taskkill', ['/PID', String(proc.pid), '/T', '/F'], { stdio: 'ignore' }); } catch {}
    }
    srv.close();
  };
  process.on('exit', cleanup);
  process.on('SIGINT', () => { cleanup(); process.exit(130); });

  for (const s of wanted) {
    console.log(`\n---- ${s.title} ----`);
    const url = `${base}/_smoke/${s.key}/index.html`;

    const res = await fetch(`http://127.0.0.1:${port}/json/new?${encodeURIComponent(url)}`, { method: 'PUT' });
    const target = await res.json();
    const s_ = await connect(target.webSocketDebuggerUrl);
    await s_.send('Runtime.enable');
    await s_.send('Log.enable');
    await s_.send('Page.enable');

    const t = {
      async eval(expr) {
        const r = await s_.send('Runtime.evaluate', {
          expression: expr, returnByValue: true, awaitPromise: true, userGesture: true,
        });
        if (r.exceptionDetails) {
          const d = r.exceptionDetails;
          throw new Error((d.exception && d.exception.description) || d.text || '页面里抛了异常');
        }
        return r.result ? r.result.value : undefined;
      },
      async waitFor(expr, ms = 6000) {
        const t0 = Date.now();
        for (;;) {
          try { if (await t.eval(expr)) return true; } catch {}
          if (Date.now() - t0 > ms) return false;
          await sleep(80);
        }
      },
    };

    const missBefore = srv.misses.length;
    try {
      // 等启动那段 init() 跑完：侧栏画出导航项就说明 loadAll 走完了
      const up = await t.waitFor(`!!document.querySelector('#sidebar .nav-item')`);
      check('界面启动完成（没有卡在加载中）', up);

      await s.run(t);

      // 静态副本里没有 favicon，浏览器会自己来问一次，服务器就回 404。
      // 这是「用 http 托管静态页」的副产物：真实程序里图标走打包资源，不发这个请求。
      // 单独列出来看，但别让它把「控制台干净」这条染红。
      const missed = [...new Set(srv.misses.slice(missBefore))];
      const realMiss = missed.filter(u => !/favicon/.test(u));
      if (realMiss.length) note(`静态服务没找到：${realMiss.join(', ')}`);

      const errs = errorsIn(s_.events).filter(m => !/favicon\.ico/.test(m));
      check('控制台没有报错', errs.length === 0, errs.length ? errs.slice(0, 3).join(' | ') : '');
    } catch (e) {
      check(`${s.key} 场景跑完没中断`, false, e.message);
    } finally {
      try { await fetch(`http://127.0.0.1:${port}/json/close/${target.id}`); } catch {}
      s_.close();
    }
  }

  console.log('\n---- 冒烟结果 ----');
  console.log(`  通过 ${passed} 项，失败 ${failures.length} 项`);
  if (notes.length) for (const n of notes) console.log(`  说明：${n}`);
  if (failures.length) {
    console.log('\n失败清单：');
    for (const f of failures) console.log(`  - ${f}`);
  }
  console.log(failures.length ? '\n有未通过项' : '\n全部通过');
  cleanup();
  process.exit(failures.length ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
