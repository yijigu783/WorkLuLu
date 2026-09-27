/* 契约差集检查：前后端之间「说好的名字」有没有对不上。
   这几处都只靠字符串约定连着，编译器和 linter 都管不到，改漏一处只有运行时才炸：

     1. 前端 inv('x')               ↔ 后端 generate_handler![commands::x]
     2. 前端 data-act="x"           ↔ 点击处理器里的 a === 'x' 分支
     3. 后端 emit("x")              ↔ 前端 listen('x')
     4. 托盘菜单 MenuItem::with_id("x") ↔ on_menu_event 里的 "x" => 分支

   写成脚本而不是每次手敲：这几组差集上一轮就是这么查的，但没留下工具，
   下次还得重新拼一遍 grep。用法：node tools/check_contract.js */
const fs = require('fs');
const path = require('path');

const root = path.join(__dirname, '..');
const read = p => fs.readFileSync(path.join(root, p), 'utf8');

const appJs = read(path.join('ui', 'assets', 'app.js'));
const indexHtml = read(path.join('ui', 'index.html'));
const mainRs = read(path.join('src-tauri', 'src', 'main.rs'));
const srcDir = path.join(root, 'src-tauri', 'src');
const allRs = fs.readdirSync(srcDir)
  .filter(f => f.endsWith('.rs'))
  .map(f => fs.readFileSync(path.join(srcDir, f), 'utf8'))
  .join('\n');

const uniq = arr => [...new Set(arr)];
const pick = (text, re) => uniq([...text.matchAll(re)].map(m => m[1]));
const missing = (from, have) => from.filter(x => !have.includes(x));

let bad = 0;
const report = (title, a, labelA, b, labelB) => {
  const onlyA = missing(a, b);
  const onlyB = missing(b, a);
  const line = `  ${title}  ${a.length} ↔ ${b.length}`;
  if (!onlyA.length && !onlyB.length) {
    console.log(`${line}   ok`);
    return;
  }
  bad++;
  console.log(`${line}   不一致`);
  if (onlyA.length) console.log(`    ${labelA} 有、${labelB} 没有：${onlyA.join(', ')}`);
  if (onlyB.length) console.log(`    ${labelB} 有、${labelA} 没有：${onlyB.join(', ')}`);
};

console.log('---- 契约差集 ----');

/* ---- 1. 前端调用的命令 ↔ 后端注册的命令 ----
   命令名一律小写下划线，所以 `commands::[a-z_]+` 刚好把 commands::Rule
   这类类型名挡在外面（Rule 是大写开头）。 */
report('调用命令',
  pick(appJs, /\binv\('([a-z_][a-z0-9_]*)'/g), '前端',
  pick(mainRs, /commands::([a-z_][a-z0-9_]*)/g), '后端');

/* ---- 2. 界面上的 data-act ↔ 处理器分支 ---- */
report('data-act',
  [...pick(appJs, /data-act="([a-z0-9-]+)"/g), ...pick(indexHtml, /data-act="([a-z0-9-]+)"/g)]
    .filter((v, i, arr) => arr.indexOf(v) === i),
  '界面',
  pick(appJs, /a === '([a-z0-9-]+)'/g), '处理器');

/* ---- 3. 后端发的事件 ↔ 前端监听的 ---- */
report('事件名',
  pick(allRs, /emit\("([^"]+)"/g), '后端 emit',
  pick(appJs, /\blisten\('([^']+)'/g), '前端 listen');

/* ---- 4. 托盘菜单项 ↔ 菜单事件分支 ---- */
const trayBlock = (mainRs.split('.on_menu_event')[1] || '').split('.on_tray_icon_event')[0];
report('托盘菜单',
  pick(mainRs, /MenuItem::with_id\(\s*app,\s*"([a-z-]+)"/g), '菜单项',
  pick(trayBlock, /"([a-z-]+)"\s*=>/g), '事件分支');

console.log(bad ? '\n有契约对不上，见上' : '\n四类差集全空');
process.exit(bad ? 1 : 0);
