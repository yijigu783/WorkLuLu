/* 生成一份「打开就停在指定视图」的界面副本，用于截图校对。
   用法：node tools/make_preview.js <视图> [输出目录] [--empty] [--drawer=<id>] [--mode=<list|board|calendar>]
                                          [--modal=recurring.quarterly | once | stage | cat | reward]
   --empty        检查空数据下的表现（全新安装、还没录过任何工作）
   --drawer=<id>  打开时顺便展开某条工作的详情抽屉
   --mode=xxx     排布方式；日历默认停在 2026-09，方便和 mock 数据对上
   --modal=x.y    打开新建面板并预置内容；x 是节奏类型，y 是重复频率（仅 recurring 用） */
const fs = require('fs');
const path = require('path');

const view = process.argv[2] || 'stats';
const out = process.argv[3] || path.join(__dirname, '_preview');
const empty = process.argv.includes('--empty');
const drawerArg = process.argv.find(a => a.startsWith('--drawer='));
const drawer = drawerArg ? Number(drawerArg.split('=')[1]) : null;
const modeArg = process.argv.find(a => a.startsWith('--mode='));
const mode = modeArg ? modeArg.split('=')[1] : null;
const modalArg = process.argv.find(a => a.startsWith('--modal='));
const modal = modalArg ? modalArg.split('=')[1] : null;
const app = path.join(__dirname, '..');

fs.mkdirSync(path.join(out, 'assets'), { recursive: true });

let js = fs.readFileSync(path.join(app, 'ui', 'assets', 'app.js'), 'utf8');
const anchor = `(async function init() {\n  await loadAll();`;
if (!js.includes(anchor)) { console.error('找不到启动段落，app.js 结构变了'); process.exit(1); }

const injected = [`  state.view = ${JSON.stringify(view)};`];
if (mode) injected.push(`  state.mode = ${JSON.stringify(mode)};`);
// 日历默认跟今天走；mock 数据是按「今天」相对生成的，直接停在当前月即可
if (drawer) injected.push(`  state.drawerId = ${drawer};`);
// 面板要等数据加载完、界面画好之后再弹，所以挂在定时器里
if (modal) injected.push(`  setTimeout(() => __previewModal(${JSON.stringify(modal)}), 60);`);
js = js.replace(anchor, `(async function init() {\n${injected.join('\n')}\n  await loadAll();`);

if (empty) {
  const m = `function mockData() {\n  const now = new Date();`;
  if (!js.includes(m)) { console.error('找不到 mockData 开头'); process.exit(1); }
  js = js.replace(m, `function mockData() {\n  return { categories: [{id:1,name:'本职工作',color:'#4F5BE8',sort:0}], tasks: [], subtasks: [], completions: [] };\n  const now = new Date();`);
}

// 截图专用：直接摆出面板的某个状态。只在本脚本注入，不进 app.js
js += `
function __previewModal(spec) {
  const [pat, freq] = spec.split('.');
  if (pat === 'cat') { openCatEditor(null); return; }
  if (pat === 'reward') { state.reward = true; renderModal(); return; }
  openEditor(null);
  const e = state.editing;
  e.pattern = pat || 'once';
  e.title = '整理季度复盘材料';
  e.note = '先对齐三个部门的数字';
  if (pat === 'recurring') {
    const f = freq || 'weekly';
    e.rule = Object.assign({ freq: f, time: '17:00' },
      f === 'daily'   ? {} :
      f === 'weekly'  ? { byDay: [1, 3, 5] } :
      f === 'monthly' ? { byDay: [15] } :
                        { byDay: [31], byMonth: [3] });
  }
  renderModal();
}
`;

fs.writeFileSync(path.join(out, 'assets', 'app.js'), js);
// 除 app.js（上面那份是注入过的）以外的资源整份带过去。
// 漏了图片的话预览里二维码是裂的，截图就校对不出真实效果。
for (const f of fs.readdirSync(path.join(app, 'ui', 'assets'))) {
  if (f !== 'app.js') fs.copyFileSync(path.join(app, 'ui', 'assets', f), path.join(out, 'assets', f));
}
// 截图要的是「动画已经跑完」的样子：.modal 带 180ms 的 pop 动画（opacity 0 → 1），
// 无头浏览器常常在动画中途就截了图，弹窗看着半透明、背后内容透过来。
// 只对这份静态副本生效，真实程序里的动画照常。
let html = fs.readFileSync(path.join(app, 'ui', 'index.html'), 'utf8');
if (modal) {
  html = html.replace('</head>',
    '<style>*, *::before, *::after { animation: none !important; transition: none !important; }</style>\n</head>');
}
fs.writeFileSync(path.join(out, 'index.html'), html);

console.log(path.join(out, 'index.html'));
