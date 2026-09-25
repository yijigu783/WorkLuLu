# 工作记录本

> 单文件、绿色免安装的 Windows 工作记录与周期排期工具。一个 exe，双击即用，数据全在本机。

<p>
  <img alt="platform" src="https://img.shields.io/badge/platform-Windows%2010%2F11%20x64-4F5BE8">
  <img alt="size" src="https://img.shields.io/badge/size-4.04%20MB-3B82F6">
  <img alt="license" src="https://img.shields.io/badge/license-MIT-10B981">
  <img alt="stack" src="https://img.shields.io/badge/Tauri%202-Rust%20%2B%20SQLite-8B5CF6">
</p>

![今天](docs/screenshots/01-today.png)

---

## 这是什么

把日常要做的事分成三类，各自用合适的时间口径来管：

| 类型 | 时间怎么定 | 典型场景 |
| --- | --- | --- |
| **一次性** | 一个截止时间，到点提醒一次 | 临时任务、交差办事 |
| **周期性** | 设一条规则，之后自动排下一次 | 周报月报、例会、对账 |
| **阶段性** | 开始日 + 结束日，中间可拆子任务 | 项目、专项、学习计划 |

## 为什么要再造一个

市面上的待办工具不少，但这个工具在两个地方做了不同的取舍：

1. **周期性工作不需要"做完一次再手动建下一条"。** 排期引擎按规则直接算出下一次时间 —— 每周多选星期、每月指定日号（含月末语义）、每季度锚点月。做完一次，下次时间自动前移，历史留痕。
2. **阶段性工作看的是"结束日期"，不是"开始日期"。** 一个刚启动、还有两个月的项目，不该从第一天起就挂满红字。看板上按交期分列，阶段性任务只有真的过了结束日才进"已逾期"。

另外，所有数据都在本机 SQLite 里，程序不联网、不上传、不采集。

## 功能

### 周期规则

| 频率 | 说明 |
| --- | --- |
| 每天 | 到点提醒一次 |
| 每周 | 周一到周日任选，可多选（比如每周一、三、五） |
| 每月 | 自己挑 1~31 号。**选 31 号表示"月末"**，遇到 2 月这类小月会自动落到当月最后一天，不会整月跳过 |
| 每季度 | 先选从哪个月起算，再选几号。锚点 3 月 → 3/6/9/12 月；锚点 1 月 → 1/4/7/10 月 |

每种频率都可以单独设提醒时间。

### 提醒

- 到点弹 Windows 系统通知
- **点通知直接跳回那一条工作**，不是只把窗口叫起来
- 托盘右键可「暂停到期提醒 / 恢复到期提醒」，开会或休假时不用去改任务本身

### 托盘常驻

- 右键菜单：`打开主窗口 / 今天待办（N）/ 新建工作 / 暂停·恢复到期提醒 / 退出`
- 菜单项和悬停提示都直接显示今天有几件待办
- 左键点图标 = 打开窗口并切到「今天」
- 关窗口默认收进托盘继续后台跑，提醒不会因为关窗口而失效

### 三种看的方式

三种视图共用同一套筛选条件，顶部搜索框在哪种视图下都生效。

**看板** —— 按「什么时候要」分列：已逾期 / 今天 / 接下来 7 天 / 更晚 / 没排期 / 已完成。卡片上直接带备注和子任务进度条。

![看板](docs/screenshots/02-board.png)

**日历** —— 月历网格，任务按截止日落格。每格最多显示 3 条，多出来的显示「还有 N 项」，可翻月、可回今天。

![日历](docs/screenshots/03-calendar.png)

**统计看板** —— 完成趋势（近 30 天，区分周期性与一次性/阶段性）、活跃热力图（近半年），以及本周完成 / 本月完成 / 活跃天数 / 按时完成率四个指标。

![统计](docs/screenshots/04-stats.png)

### 其他

- **自定义分类**：新建、改名、改色、拖拽排序、删除
- **备注**：列表里直接显示（最多两行），点开可编辑
- **子任务**：阶段性工作可拆成步骤逐条打勾，进度自动算进卡片
- **搜索**：顶部输入即过滤，标题和备注一起搜
- **快捷键**：`Ctrl+N` 新建工作，`Esc` 关闭当前面板
- **开机自启**：可选，登录后在托盘静默启动

### 备份、恢复与导出

设置页里有两个按钮管数据：

- **备份数据** —— 导出成一个独立的 `.db` 文件，拷到 U 盘或网盘都行。用的是 SQLite 的 `VACUUM INTO` 而不是复制文件：库跑在 WAL 模式下，直接复制可能漏掉还留在 `-wal` 里没并回主文件的改动，而且这种缺失是**静默**的（文件能打开、表也在，等真要用的时候才发现少了最近几天）。
- **从备份恢复** —— 选中备份文件后完全替换当前数据。这件事不可逆，所以恢复前会**自动把现在的数据另存一份**到 `backups` 文件夹，选错了还能切回来；恢复过程整体放在一个事务里，要么全成要么原样不动。备份文件会先校验（四张表在不在、列数对不对得上），不是本程序导出的、或者版本对不上的，直接拒绝，不会恢复出一半。

另外可以 **导出为 CSV**，Excel / WPS 直接打开（带 UTF-8 BOM，中文不乱码）。

<p>
  <img src="docs/screenshots/07-settings.png" width="80%" alt="设置页：备份、恢复与导出">
</p>

这些都走系统原生文件对话框，用的是 Tauri 的 dialog 插件；本项目的 `withGlobalTauri` 开着，所以插件自带的 JS 包装直接就能用，不需要 npm 依赖。

<p>
  <img src="docs/screenshots/05-detail.png" width="49%" alt="工作详情">
  <img src="docs/screenshots/06-new-quarterly.png" width="49%" alt="新建每季度规则">
</p>

## 下载

到 [Releases](../../releases) 页下载 `工作记录本.exe`，双击即可运行，不需要安装。

## 数据在哪

程序会在本机写这两处，都可以随时手动删除：

| 位置 | 内容 | 能否删 |
| --- | --- | --- |
| `%APPDATA%\工作记录本\` | 数据库 `worklog.db`，你的全部记录 | 删掉即清空数据 |
| `%APPDATA%\工作记录本\backups\` | 恢复数据前自动留下的快照 | 可以随时删 |
| `HKCU\Software\Classes\AppUserModelId\com.local.worklog` | 一个通知标识，作用是让系统通知的标题显示「工作记录本」而不是 exe 文件名 | 可删，只是通知标题会变回文件名 |

**数据不在 exe 所在目录**，所以 exe 可以随便挪位置、放 U 盘，数据不受影响。

> 顺带一提：这个 AUMID 是用 Win32 注册表 API 写的，不是调 `reg.exe` —— 调外部进程既慢，又容易被安全软件记一笔。

## 自行构建

### 前置

- Rust **1.77+**
- MSVC 生成工具（VS 2022 Build Tools，勾选「使用 C++ 的桌面开发」+ Windows SDK）

**不需要 Node.js、不需要 npm**。前端是原生的 HTML/CSS/JS，没有构建步骤，Tauri 直接把 `app/ui` 当静态资源打包。

### 构建

```bash
git clone https://github.com/yijigu783/WorkLuLu.git
cd WorkLuLu/app/src-tauri
cargo build --release
```

产物在 `app/src-tauri/target/release/worklog.exe`，拷出来重命名成 `工作记录本.exe` 就能用了。

### 开发时看界面

改 UI 不用每次重新编译 Rust。`app/tools/make_preview.js` 会生成一份直接用 mock 数据渲染的静态副本，浏览器打开即所见：

```bash
cd app
node tools/make_preview.js today   out/         # 停在「今天」视图
node tools/make_preview.js today   out/ --mode=board
node tools/make_preview.js stats   out/ --drawer=2
node tools/make_preview.js today   out/ --modal=recurring.quarterly
```

## 项目结构

```
app/
├── ui/                     前端（无框架、无打包器）
│   ├── index.html
│   └── assets/{app.js, style.css}
├── src-tauri/              Rust 后端
│   ├── src/
│   │   ├── main.rs         托盘、窗口、30 秒巡检线程
│   │   ├── commands.rs     所有前端命令
│   │   ├── db.rs           schema 与数据目录
│   │   ├── schedule.rs     排期引擎（纯函数，含单测）
│   │   └── notify.rs       Windows 通知与 AUMID 注册
│   └── tauri.conf.json
└── tools/                  自检与预览脚本（Node）
docs/                       截图与发布说明
```

## 技术栈

- **Tauri 2** + **Rust**
- **rusqlite**（bundled SQLite，WAL 模式）
- **原生 HTML/CSS/JS**，无框架、无打包器
- 发布配置开了 `opt-level = "z"` + `lto` + `strip`，所以单文件只有 4 MB

## 自检

项目里带了一套防回归脚本，改完代码跑一遍：

```bash
cd app/src-tauri && cargo test      # 48 个单测，排期引擎 + 备份恢复
cd app && node tools/check_ui.js    # 62 项渲染自检
cd app && node tools/check_stats.js # 8 项统计口径自检
```

## 已知限制

这些都是有意留着的，不是没做完：

- **没有标签功能**，目前只有分类 + 搜索
- **只有浅色主题**，没做深色模式
- **只支持 64 位 Windows 10 (1809+) / Windows 11**，不支持 Win7 —— 底层框架已停止支持 Win7，不是偷懒省事
- **依赖系统的 WebView2 运行时**。Win11 和打过较新补丁的 Win10 都自带；如果双击后没反应，装一次微软官方的 [WebView2 Evergreen Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) 即可
- 如果通知不弹，先检查「设置 → 系统 → 通知」里是否放行了本程序，以及「专注助手（勿扰）」是否开着
- 备份要手动点，**没有自动定时备份**
- 恢复是整体替换，**不能合并两份数据**

## 关于名字

项目的软件名叫 **工作记录本**，仓库名用了 **WorkLuLu**。

## License

[MIT](LICENSE)
