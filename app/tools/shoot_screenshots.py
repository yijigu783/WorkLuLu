# -*- coding: utf-8 -*-
"""重新生成 docs/screenshots/ 里的界面截图，对齐当前版本（1.6.0）。

为什么要重截：原来的 8 张里有 5 张是 9 月 25 日截的，那时还没有附件功能、
也没有分类自动关联。B站视频和 README 都要用这些图，画面跟用户实际下载到的
对不上会显得很敷衍。

做法沿用之前的思路：make_preview.js 生成静态副本 → Edge/Chrome 无头截图。
不编译整个 Tauri 程序，改一行前端就能重截。
"""
import os
import json
import subprocess
import shutil
import tempfile
import urllib.request

ROOT = r"F:\工作记录本"
APP = os.path.join(ROOT, "app")
NODE = r"C:\Users\Administrator\.workbuddy-ai\binaries\node\versions\22.22.2-6\node.exe"
SHOTS = os.path.join(ROOT, "docs", "screenshots")
OUT = os.path.join(APP, "out")

BROWSERS = [
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
]

# (输出文件名, make_preview 参数, 说明)
# --drawer=N 会展开第 N 条工作的详情抽屉，附件区就在抽屉里
PLAN = [
    ("01-today.png",        {"view": "today", "mode": "list"},  "今天"),
    ("02-board.png",        {"view": "all",  "mode": "board"},  "看板"),
    ("03-calendar.png",     {"view": "all",  "mode": "calendar"}, "日历"),
    ("04-stats.png",        {"view": "stats"},                  "统计看板"),
    ("05-detail.png",       {"view": "all", "drawer": 1},       "详情抽屉（含附件区）"),
    ("06-new-quarterly.png",{"view": "today", "modal": "recurring.quarterly"}, "新建面板"),
    ("07-settings.png",     {"view": "settings"},               "设置"),
    ("08-from-template.png",{"view": "tpl"},                    "模板"),
    # 新增：分类自动关联是 1.6.0 的另一处改动。
    # 用 --modal=once 在「本职工作」分类页里开新建面板，
    # 就能看到分类已经替用户带好、只剩一行「更改」入口的那个样子
    ("09-cat-new.png",      {"view": "cat:1", "modal": "once"}, "在分类里新建"),
]


def build_preview(params, outdir):
    """调 make_preview.js 的模块接口生成静态副本。"""
    js = (
        "const {build}=require(process.argv[1]);"
        "build(JSON.parse(process.argv[2]));"
    )
    script = os.path.join(APP, "tools", "make_preview.js")
    args = json.dumps(dict(params, out=outdir))
    r = subprocess.run([NODE, "-e", js, script, args],
                       capture_output=True, text=True, cwd=APP)
    if r.returncode != 0:
        raise RuntimeError(f"make_preview 失败：{r.stderr[-400:]}")


def shoot(html_path, png_path, browser):
    """无头截图。--virtual-time-budget 让动画有时间跑完，
    否则会截到还在淡入的半透明画面。"""
    url = "file:///" + html_path.replace("\\", "/")
    tmp = tempfile.mkdtemp(prefix="wll-shot-")
    cmd = [
        browser,
        "--headless=new", "--disable-gpu", "--hide-scrollbars",
        "--no-first-run", "--no-default-browser-check",
        f"--user-data-dir={tmp}",
        "--window-size=1280,820",
        "--virtual-time-budget=4000",
        f"--screenshot={png_path}",
        url,
    ]
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=90)
    shutil.rmtree(tmp, ignore_errors=True)
    if not os.path.exists(png_path):
        raise RuntimeError(f"截图失败：{r.stderr[-300:]}")


browser = next((b for b in BROWSERS if os.path.exists(b)), None)
if not browser:
    raise SystemExit("没找到 Edge 或 Chrome")
print("浏览器：", browser)

os.makedirs(SHOTS, exist_ok=True)
for name, params, desc in PLAN:
    pid = name.split("-")[0]
    outdir = os.path.join(OUT, f"_shot_{pid}")
    build_preview(params, outdir)
    html = os.path.join(outdir, "index.html")
    png = os.path.join(SHOTS, name)
    shoot(html, png, browser)
    size = os.path.getsize(png)
    print(f"  {name:24s} {desc:20s} {size/1024:.0f} KB")
    shutil.rmtree(outdir, ignore_errors=True)

print("\n完成，共", len(PLAN), "张")
