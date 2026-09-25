/* 统计看板自检：脱离界面把 renderStats 跑一遍，验证生成的 SVG 结构正确、数值合理。
   用法：node tools/check_stats.js */
const fs = require('fs');
const path = require('path');
const vm = require('vm');

const src = fs.readFileSync(path.join(__dirname, '..', 'ui', 'assets', 'app.js'), 'utf8');
// 去掉末尾的启动 IIFE（它会去碰 DOM），只留纯逻辑
const body = src.split('/* ---------------- 启动 ---------------- */')[0];

const ctx = {
  console,
  window: {},                     // 无 __TAURI__ → 走预览数据
  document: { addEventListener() {}, getElementById: () => null, querySelectorAll: () => [], body: { addEventListener() {} } },
  requestAnimationFrame: fn => fn(),
  setTimeout, clearTimeout,
};
vm.createContext(ctx);
vm.runInContext(body + '\n;globalThis.__api = { state, loadAll, renderStats, completionEvents, trendChart, heatmap };', ctx);

(async () => {
  const { state, loadAll, renderStats, completionEvents } = ctx.__api;
  await loadAll();

  const events = completionEvents();
  console.log('分类       :', state.categories.length);
  console.log('工作条数   :', state.tasks.length);
  console.log('完成记录   :', state.completions.length);
  console.log('完成事件   :', events.length, '（合并周期记录与打勾任务后）');

  const html = renderStats();

  // ---- 结构检查 ----
  const checks = [];
  const ok = (name, cond, extra = '') => checks.push([cond ? 'PASS' : 'FAIL', name, extra]);

  const svgs = html.match(/<svg[\s\S]*?<\/svg>/g) || [];
  ok('生成两个 SVG（趋势图 + 热力图）', svgs.length === 2, `实际 ${svgs.length}`);

  const bars = (svgs[0] || '').match(/<path /g) || [];
  // 21×21 的才是热力格子；右下角的图例色块要排除掉
  const heat = (svgs[1] || '').match(/<rect[^>]*width="21" height="21"/g) || [];
  const tips = (svgs[1] || '').match(/<title>/g) || [];
  ok('趋势图有柱子', bars.length > 5, `${bars.length} 段`);
  ok('热力图格子数合理（26 周内不超过 182 天）', heat.length > 100 && heat.length <= 182, `${heat.length} 格`);
  ok('每个热力格子带 tooltip', tips.length === heat.length, `${tips.length}/${heat.length}`);

  // 没有 NaN / undefined 漏进 SVG
  const bad = (html.match(/NaN|undefined|nullpx/g) || []).length;
  ok('输出中没有 NaN/undefined', bad === 0, bad ? `${bad} 处` : '');

  // ---- 数值检查 ----
  const now = new Date();
  const sow = new Date(now); sow.setHours(0, 0, 0, 0); sow.setDate(sow.getDate() - ((sow.getDay() + 6) % 7));
  const weekN = events.filter(e => new Date(e.at) >= sow).length;
  const withDue = events.filter(e => e.dueAt);
  const onTime = withDue.filter(e => {
    const a = new Date(e.at), b = new Date(e.dueAt);
    a.setHours(0, 0, 0, 0); b.setHours(0, 0, 0, 0);
    return a <= b;
  }).length;
  console.log('\n本周完成   :', weekN);
  console.log('按时完成   :', `${onTime}/${withDue.length} = ${Math.round(onTime / withDue.length * 100)}%`);

  // 卡片的数字应与独立算出来的一致
  const mWeek = html.match(/本周完成<\/div>\s*<div class="stat-value">(\d+)/);
  ok('本周完成卡片与独立计算一致', mWeek && Number(mWeek[1]) === weekN, `卡片 ${mWeek && mWeek[1]} / 计算 ${weekN}`);
  ok('按时率不是 0 也不是 100（示例数据应有迟到）', onTime > 0 && onTime < withDue.length);

  // 热力图不能出现未来格子
  const future = (svgs[1] || '').includes(`${now.getFullYear() + 1} 年`);
  ok('热力图不含未来日期', !future);

  console.log('\n---- 自检结果 ----');
  checks.forEach(([s, n, e]) => console.log(`  ${s}  ${n}${e ? '  → ' + e : ''}`));
  const failed = checks.filter(c => c[0] === 'FAIL').length;
  console.log(failed ? `\n${failed} 项未通过` : '\n全部通过');

  process.exit(failed ? 1 : 0);
})();
