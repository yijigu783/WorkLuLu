# -*- coding: utf-8 -*-
"""工作记录本 B站介绍片 —— 全片渲染（89.3 秒）。

设计要点（这些决定了这片子做得对不对）：

1. **时长以配音为准，不写死。**
   每镜的时长从 voices/_durations.json 读 —— 也就是实测音频长度。
   画面去凑声音，不是声音去凑画面。这样剪辑阶段一滴水都不用对。
   （打样版是写死时长的，铺全片时必须改掉，否则改一句台词就全乱。）

2. **逐帧渲染 + 全片存内存。**
   89.3s × 30fps = 2679 帧 × 1920×1080×3B ≈ 16.6 GB。
   存不下 —— 所以每镜渲完立刻写进裸流文件，编码时再流式读。
   （打样只有 426 帧才敢全存内存。）

3. **每镜首尾各留 0.35s 淡入淡出。**
   镜头之间不做硬切 —— 字幕片硬切会显得抖。淡入淡出还有个好处：
   即使某镜配音比画面早一点晚一点，接缝处也看不出来。

4. **不叠加背景音乐。**
   没找到可商用又符合片子的曲子，宁可不加。要加的话建议
   用 B站音频库里的免版权曲，音量压到 -22dB 左右，别盖过人声。
"""
import os
import re
import json
import math
import subprocess

import numpy as np
from PIL import Image, ImageDraw, ImageFont, ImageFilter
import imageio_ffmpeg

ROOT = r"F:\工作记录本"
SHOTS = os.path.join(ROOT, "docs", "screenshots")
HERE = os.path.join(ROOT, "docs", "bilibili-video")
OUT = HERE

W, H = 1920, 1080
FPS = 30
FONT_B = r"C:\Windows\Fonts\msyhbd.ttc"
FONT_R = r"C:\Windows\Fonts\msyh.ttc"

# 配色跟软件界面一套（靛蓝主色 + 浅灰底）
BG    = (244, 246, 250)
INK   = (23, 28, 45)
MUTED = (108, 118, 140)
BRAND = (79, 91, 232)
CARD  = (255, 255, 255)
LINE  = (203, 213, 225)
GREEN = (16, 152, 106)   # 完成/正向
RED   = (216, 74, 74)    # 逾期/警示

XIN = 0.35               # 每镜首尾淡入淡出时长（秒）


# ── 基础工具 ────────────────────────────────────────────────

def font(bold, size):
    return ImageFont.truetype(FONT_B if bold else FONT_R, size)


def ease_out(t):
    return 1 - (1 - t) ** 3


def ease_in_out(t):
    return 3 * t * t - 2 * t * t * t


def clamp(v, lo=0.0, hi=1.0):
    return max(lo, min(hi, v))


def track(t, start, dur):
    """在 t 时刻、从 start 开始、历时 dur 的进度（0~1，缓出）。"""
    return ease_out(clamp((t - start) / dur))


def new_frame():
    return Image.new("RGBA", (W, H), BG + (255,))


def grade(img):
    """统一收尾：顶部加一层极淡的靛蓝渐变，避免大片纯色显得空。"""
    ov = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    d = ImageDraw.Draw(ov)
    for i in range(260):
        d.line([(0, i), (W, i)], fill=BRAND + (int(9 * (1 - i / 260)),))
    img.alpha_composite(ov)
    return img


def fade_ends(img, t, dur):
    """每镜首尾淡入淡出。t/dur 为这一镜内的相对进度。"""
    a = clamp(t / XIN) * clamp((dur - t) / XIN)
    if a >= 1:
        return img
    ov = Image.new("RGBA", (W, H), BG + (int(255 * (1 - a)),))
    img.alpha_composite(ov)
    return img


def text_at(layer, s, f, y, fill, alpha=255, cx=None):
    """在 layer 上居中画一行字，返回墨迹宽度。"""
    d = ImageDraw.Draw(layer)
    w = d.textlength(s, font=f)
    x = (W - w) / 2 if cx is None else cx - w / 2
    d.text((x, y), s, font=f, fill=fill + (int(alpha),))
    return w


def ink_top(path, size, s):
    """用 getbbox 量一段文字的墨迹上边界（相对锚点）。
    画多行时靠它算行距 —— 靠字号乘系数估会偏。"""
    f = font(True, size)
    d = ImageDraw.Draw(Image.new("RGBA", (10, 10)))
    b = d.textbbox((0, 0), s, font=f)
    return b


def paste_shot(base, img, cx, cy, scale, radius=12, shadow=True):
    """把界面截图缩放贴到 base，(cx,cy) 是中心。带阴影和圆角。"""
    w, h = int(img.width * scale), int(img.height * scale)
    s = img.resize((w, h), Image.LANCZOS)
    x, y = int(cx - w / 2), int(cy - h / 2)
    if shadow:
        sh = Image.new("RGBA", (w + 80, h + 80), (0, 0, 0, 0))
        ImageDraw.Draw(sh).rounded_rectangle(
            (40, 46, 40 + w, 46 + h), radius=radius, fill=(23, 28, 45, 44))
        base.alpha_composite(sh.filter(ImageFilter.GaussianBlur(22)), (x - 40, y - 40))
    mask = Image.new("L", (w, h), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, w, h), radius=radius, fill=255)
    base.paste(s, (x, y), mask)
    return x, y, w, h


def chip(layer, x, y, s, f, fg=BRAND, bg=None, pad=(22, 12), r=14, alpha=255):
    """画一个圆角小标签（角标），返回它的宽高。"""
    if bg is None:
        bg = (255, 255, 255)
    d = ImageDraw.Draw(layer)
    w = d.textlength(s, font=f)
    b = d.textbbox((0, 0), s, font=f)
    tw, th = w, b[3] - b[1]
    cw, ch = int(tw + pad[0] * 2), int(th + pad[1] * 2)
    d.rounded_rectangle((x, y, x + cw, y + ch), radius=r, fill=bg + (alpha,))
    d.text((x + pad[0], y + pad[1] - b[1]), s, font=f, fill=fg + (alpha,))
    return cw, ch


def shot(name):
    return Image.open(os.path.join(SHOTS, name)).convert("RGB")


def load_durations():
    """读实测配音时长 —— 画面时长以它为准。"""
    p = os.path.join(HERE, "voices", "_durations.json")
    if not os.path.exists(p):
        raise SystemExit("没找到 voices/_durations.json，先跑 python make_voice.py")
    data = json.load(open(p, encoding="utf-8"))
    if data.get("failures"):
        raise SystemExit(f"配音有失败项：{data['failures']}，先重跑 make_voice.py")
    d = {}
    for row in data["lines"]:
        d[row["name"][:2]] = row["seconds"]
    return d


DUR = load_durations()
TAIL = 3.0     # 片尾纯画面


# ── 各镜画面 ────────────────────────────────────────────────
# 每镜是 generator，yield 一帧（RGB）并自己算这一帧属于第几秒。
# 用生成器而不是"先算总帧数再循环"，是因为这样能边渲边写，
# 不用把 16 GB 帧全堆在内存里等编码。

def shot01(dur):
    """片头：标题浮起 + 靛蓝短线展开。"""
    total = int(dur * FPS)
    for i in range(total):
        t = i / FPS
        img = new_frame()
        lay = Image.new("RGBA", (W, H), (0, 0, 0, 0))
        p = ease_out(clamp(t / 0.5))
        dy = int(30 * (1 - p))
        a = int(255 * clamp(t / 0.4))

        text_at(lay, "工作记录本", font(True, 118), 396 + dy, INK, a)
        text_at(lay, "一个 4 MB 的单文件，替我记住所有周期性的事",
                font(False, 42), 592 + dy, MUTED, a)

        q = ease_out(clamp((t - 0.35) / 0.45))
        half = int(150 * q)
        if half > 0:
            ImageDraw.Draw(lay).rounded_rectangle(
                (W // 2 - half, 548, W // 2 + half, 554), radius=3,
                fill=BRAND + (int(255 * clamp((t - 0.35) / 0.3)),))
        img.alpha_composite(lay)
        yield fade_ends(grade(img), t, dur).convert("RGB")


def shot02(dur):
    """痛点：三张待办卡滑入 + 钩子句。"""
    total = int(dur * FPS)
    items = ["周五交周报", "周一要开会", "每月 15 号对账"]
    f = font(False, 46)
    for i in range(total):
        t = i / FPS
        img = new_frame()
        lay = Image.new("RGBA", (W, H), (0, 0, 0, 0))
        ld = ImageDraw.Draw(lay)

        for k, s in enumerate(items):
            p = ease_out(clamp((t - 0.10 - k * 0.16) / 0.34))
            if p <= 0:
                continue
            cw, ch = 540, 92
            cx = W // 2 - cw // 2 + int((1 - p) * 100)
            cy = 268 + k * 118
            a = int(255 * p)
            ld.rounded_rectangle((cx, cy, cx + cw, cy + ch), radius=16,
                                 fill=CARD + (a,))
            ld.rounded_rectangle((cx + 24, cy + ch // 2 - 13, cx + 29, cy + ch // 2 + 13),
                                 radius=3, fill=LINE + (a,))
            ld.text((cx + 54, cy + ch // 2 - 30), s, font=f, fill=INK + (a,))

        # 钩子句：画面做，不占口播
        q = ease_out(clamp((t - 0.72) / 0.36))
        if q > 0:
            text_at(lay, "翻聊天记录，翻到手酸", font(True, 60), 692, BRAND,
                    int(255 * q))
        img.alpha_composite(lay)
        yield fade_ends(grade(img), t, dur).convert("RGB")


def shot03(dur):
    """主界面：截图缓推 + 顶部标题。"""
    total = int(dur * FPS)
    main = shot("01-today.png")
    for i in range(total):
        t = i / FPS
        img = new_frame()
        p = ease_in_out(clamp(t / 0.9))
        scale = 0.86 + 0.14 * p
        cx = W / 2 + (1 - p) * -30
        cy = 592 + (1 - p) * 18
        paste_shot(img, main, cx, cy, scale)

        q = track(t, 0.30, 0.34)
        if q > 0:
            lay = Image.new("RGBA", (W, H), (0, 0, 0, 0))
            text_at(lay, "打开就知道今天该干什么", font(True, 54), 74, INK,
                    int(255 * q))
            img.alpha_composite(lay)
        yield fade_ends(grade(img), t, dur).convert("RGB")


def shot04(dur):
    """周期任务：日历截图 + 高亮重复条目 + 角标。"""
    total = int(dur * FPS)
    cal = shot("03-calendar.png")
    for i in range(total):
        t = i / FPS
        img = new_frame()
        p = ease_in_out(clamp(t / 0.8))
        paste_shot(img, cal, W / 2 + (1 - p) * -26, 600 + (1 - p) * 16,
                   0.88 + 0.12 * p)

        # 标题
        q = track(t, 0.25, 0.32)
        if q > 0:
            lay = Image.new("RGBA", (W, H), (0, 0, 0, 0))
            text_at(lay, "做完就划掉？这个是给做不完的事准备的", font(True, 52),
                    64, INK, int(255 * q))
            img.alpha_composite(lay)

        # 角标：重复规则。0.6s 后从右侧滑进来
        r = track(t, 0.95, 0.36)
        if r > 0:
            lay = Image.new("RGBA", (W, H), (0, 0, 0, 0))
            f = font(True, 34)
            cw, ch = chip(lay, 0, 0, "每周五 17:00 交周报", f, BRAND,
                          pad=(26, 14))
            x = int(W - cw - 110 + (1 - r) * 90)
            y = 190
            lay2 = Image.new("RGBA", (W, H), (0, 0, 0, 0))
            ld2 = ImageDraw.Draw(lay2)
            ld2.rounded_rectangle((x, y, x + cw, y + ch), radius=14,
                                  fill=CARD + (int(255 * r),))
            b = ld2.textbbox((0, 0), "每周五 17:00 交周报", font=f)
            ld2.text((x + 26, y + 14 - b[1]), "每周五 17:00 交周报", font=f,
                     fill=BRAND + (int(255 * r),))
            ld2.rounded_rectangle((x - 6, y + 6, x, y + ch - 6), radius=2,
                                  fill=BRAND + (int(255 * r),))
            img.alpha_composite(lay2)
        yield fade_ends(grade(img), t, dur).convert("RGB")


def shot05(dur):
    """附件留痕：详情抽屉 + 一张"刚粘进来"的聊天截图缩略图。"""
    total = int(dur * FPS)
    det = shot("05-detail.png")
    for i in range(total):
        t = i / FPS
        img = new_frame()

        p = ease_in_out(clamp(t / 0.8))
        # 详情截图往左让出位置，给右侧的"粘贴示意"留空间
        paste_shot(img, det, W / 2 - 210 + (1 - p) * -40, 606 + (1 - p) * 16,
                   0.84 + 0.10 * p)

        q = track(t, 0.22, 0.32)
        if q > 0:
            lay = Image.new("RGBA", (W, H), (0, 0, 0, 0))
            text_at(lay, "凭证直接粘进任务里，备份还是一个文件", font(True, 52),
                    62, INK, int(255 * q))
            img.alpha_composite(lay)

        # 模拟"刚 Ctrl+V 粘进来"的一张聊天截图卡片
        r = track(t, 1.05, 0.40)
        if r > 0:
            lay = Image.new("RGBA", (W, H), (0, 0, 0, 0))
            ld = ImageDraw.Draw(lay)
            cw, ch = 360, 400
            x = int(1400 - cw / 2 + (1 - r) * 120)
            y = 250
            a = int(255 * r)
            # 卡片
            ld.rounded_rectangle((x, y, x + cw, y + ch), radius=16,
                                 fill=CARD + (a,))
            # 模拟聊天泡泡
            ld.rounded_rectangle((x + 26, y + 30, x + cw - 92, y + 86),
                                 radius=14, fill=(233, 238, 248, a))
            ld.rounded_rectangle((x + 92, y + 100, x + cw - 26, y + 156),
                                 radius=14, fill=(224, 231, 255, a))
            ld.rounded_rectangle((x + 26, y + 170, x + cw - 58, y + 226),
                                 radius=14, fill=(233, 238, 248, a))
            # 中间压一张"图片"占位（灰底 + 山形，示意截图内容）
            ix0, iy0, ix1, iy1 = x + 26, y + 246, x + cw - 26, y + 356
            ld.rounded_rectangle((ix0, iy0, ix1, iy1), radius=10,
                                 fill=(241, 244, 250, a))
            ld.polygon([(ix0 + 32, iy1 - 22), (ix0 + 114, iy0 + 40),
                        (ix0 + 180, iy1 - 22)], fill=(199, 210, 232, a))
            ld.ellipse((ix1 - 90, iy0 + 28, ix1 - 54, iy0 + 64),
                       fill=(199, 210, 232, a))
            img.alpha_composite(lay)

            # 「Ctrl+V 直接粘」角标 —— 放在卡片**正下方**居中。
            # 原来贴在卡片左下，会压到抽屉底部的"删除/保存"按钮上，
            # 抽帧才发现。改成跟着卡片垂直居中、水平铺开。
            lay2 = Image.new("RGBA", (W, H), (0, 0, 0, 0))
            f = font(True, 30)
            b = ImageDraw.Draw(lay2).textbbox((0, 0), "Ctrl+V 直接粘", font=f)
            tw = b[2] - b[0]
            bx = x + (cw - (tw + 44)) // 2
            by = y + ch + 22
            ImageDraw.Draw(lay2).rounded_rectangle(
                (bx, by, bx + tw + 44, by + 58), radius=14, fill=BRAND + (a,))
            ImageDraw.Draw(lay2).text((bx + 22, by + 12 - b[1]), "Ctrl+V 直接粘",
                                      font=f, fill=(255, 255, 255, a))
            img.alpha_composite(lay2)
        yield fade_ends(grade(img), t, dur).convert("RGB")


def shot06(dur):
    """模板与子任务：模板截图 + 进度条特写。"""
    total = int(dur * FPS)
    tpl = shot("08-from-template.png")
    # 截图缩到 0.78，底部才不会撞上进度条。
    # 原来给 0.92 时截图底边到 900px，进度条画在 958 —— 抽帧一看糊在一起了。
    for i in range(total):
        t = i / FPS
        img = new_frame()
        p = ease_in_out(clamp(t / 0.8))
        paste_shot(img, tpl, W / 2, 470 + (1 - p) * 16, 0.66 + 0.09 * p)

        q = track(t, 0.22, 0.32)
        if q > 0:
            lay = Image.new("RGBA", (W, H), (0, 0, 0, 0))
            text_at(lay, "整套流程一键铺开，进度自己算", font(True, 52), 62, INK,
                    int(255 * q))
            img.alpha_composite(lay)

        # 进度条动效：0 → 72%（演示"打完勾进度条自己动"）
        r = track(t, 0.95, 1.6)
        if r > 0:
            lay = Image.new("RGBA", (W, H), (0, 0, 0, 0))
            ld = ImageDraw.Draw(lay)
            bx, by, bw, bh = W // 2 - 300, 846, 600, 16
            ld.rounded_rectangle((bx, by, bx + bw, by + bh), radius=8,
                                 fill=(226, 232, 242, 255))
            ld.rounded_rectangle((bx, by, bx + int(bw * 0.72 * r), by + bh),
                                 radius=8, fill=BRAND + (255,))
            f = font(True, 32)
            s = f"已完成 {int(72 * r)}%"
            b = ld.textbbox((0, 0), s, font=f)
            ld.text((W // 2 - (b[2] - b[0]) / 2, by + 34), s, font=f, fill=MUTED)
            img.alpha_composite(lay)
        yield fade_ends(grade(img), t, dur).convert("RGB")


def shot07(dur):
    """绿色与便携：设置页特写 + 三枚要点。
    要点放在截图**下方**的独立区域，别压在截图上 —— 压上去会挡住设置项，
    而且三枚胶囊挤在一起会看起来像没对齐。"""
    total = int(dur * FPS)
    st = shot("07-settings.png")
    pts = ["不联网", "不上传", "整个文件夹拷进 U 盘就走"]
    for i in range(total):
        t = i / FPS
        img = new_frame()
        p = ease_in_out(clamp(t / 0.8))
        paste_shot(img, st, W / 2, 430 + (1 - p) * 16, 0.62 + 0.08 * p)

        q = track(t, 0.22, 0.32)
        if q > 0:
            lay = Image.new("RGBA", (W, H), (0, 0, 0, 0))
            text_at(lay, "数据在本地，不联网、不上传", font(True, 52), 62, INK,
                    int(255 * q))
            img.alpha_composite(lay)

        # 三枚要点等间距排一行，整体居中
        f = font(True, 34)
        widths = []
        probe = ImageDraw.Draw(Image.new("RGBA", (10, 10)))
        for s in pts:
            b = probe.textbbox((0, 0), s, font=f)
            widths.append(b[2] - b[0] + 56)
        gap = 34
        total_w = sum(widths) + gap * (len(pts) - 1)
        x = (W - total_w) // 2
        for k, s in enumerate(pts):
            r = track(t, 0.9 + k * 0.28, 0.34)
            if r <= 0:
                x += widths[k] + gap
                continue
            lay = Image.new("RGBA", (W, H), (0, 0, 0, 0))
            ld = ImageDraw.Draw(lay)
            b = ld.textbbox((0, 0), s, font=f)
            cw = widths[k]
            y = 900 + int((1 - r) * 22)
            ld.rounded_rectangle((x, y, x + cw, y + 62), radius=16,
                                 fill=CARD + (int(255 * r),))
            ld.text((x + 28, y + 14 - b[1]), s, font=f,
                    fill=GREEN + (int(255 * r),))
            img.alpha_composite(lay)
            x += cw + gap
        yield fade_ends(grade(img), t, dur).convert("RGB")


def shot08(dur):
    """三个数字：4 MB / 0 广告 / 1 个文件。"""
    total = int(dur * FPS)
    nums = [("4 MB", "整个程序就这么大", BRAND),
            ("0 广告", "没捆绑、不收费", GREEN),
            ("1 个文件", "双击就跑", INK)]
    for i in range(total):
        t = i / FPS
        img = new_frame()
        lay = Image.new("RGBA", (W, H), (0, 0, 0, 0))
        ld = ImageDraw.Draw(lay)

        q0 = track(t, 0.05, 0.3)
        if q0 > 0:
            text_at(lay, "最后几个数字", font(True, 46), 148, MUTED, int(255 * q0))

        for k, (big, sub, col) in enumerate(nums):
            r = ease_out(clamp((t - 0.42 - k * 0.42) / 0.4))
            if r <= 0:
                continue
            cx = 380 + k * 580
            cy = 520
            a = int(255 * r)
            # 数字缩放入场
            sc = 0.86 + 0.14 * r
            fb = font(True, int(112 * sc))
            b = ld.textbbox((0, 0), big, font=fb)
            ld.text((cx - (b[2] - b[0]) / 2, cy - (b[3] - b[1]) / 2 - b[1]),
                    big, font=fb, fill=col + (a,))
            fs = font(False, 36)
            b2 = ld.textbbox((0, 0), sub, font=fs)
            ld.text((cx - (b2[2] - b2[0]) / 2, cy + 118), sub, font=fs,
                    fill=MUTED + (a,))
            # 每个数字下面一条短线
            ld.rounded_rectangle((cx - 46, cy + 92, cx + 46, cy + 97),
                                 radius=3, fill=col + (int(a * 0.55),))
        img.alpha_composite(lay)

        # 开源声明（合规要求，必须出现在画面里）
        q2 = track(t, 2.0, 0.4)
        if q2 > 0:
            lay2 = Image.new("RGBA", (W, H), (0, 0, 0, 0))
            text_at(lay2, "免费 · 开源 · GitHub 上就能下", font(True, 40), 880,
                    INK, int(255 * q2))
            img.alpha_composite(lay2)
        yield fade_ends(grade(img), t, dur).convert("RGB")


def shot09(dur):
    """收尾卡。"""
    total = int(dur * FPS)
    for i in range(total):
        t = i / FPS
        img = new_frame()
        lay = Image.new("RGBA", (W, H), (0, 0, 0, 0))
        p = ease_out(clamp(t / 0.5))
        dy = int(26 * (1 - p))
        text_at(lay, "免费 · 开源 · 单文件", font(True, 78), 408 + dy, INK,
                int(255 * p))
        q = track(t, 0.35, 0.4)
        if q > 0:
            text_at(lay, "试试看你自己的周期任务", font(False, 40), 548, MUTED,
                    int(255 * q))
        img.alpha_composite(lay)
        yield fade_ends(grade(img), t, dur).convert("RGB")


def tail(dur):
    """片尾：logo + 仓库地址，声音停了别立刻黑屏。"""
    total = int(dur * FPS)
    for i in range(total):
        t = i / FPS
        img = new_frame()
        lay = Image.new("RGBA", (W, H), (0, 0, 0, 0))
        p = ease_out(clamp(t / 0.6))
        a = int(255 * p)
        text_at(lay, "工作记录本 WorkLuLu", font(True, 64), 428, INK, a)
        q = track(t, 0.4, 0.5)
        if q > 0:
            text_at(lay, "github.com/yijigu783/WorkLuLu", font(False, 38), 540,
                    BRAND, int(255 * q))
        r = track(t, 0.9, 0.5)
        if r > 0:
            text_at(lay, "Windows 10 1809+ / Windows 11 · 64 位", font(False, 30),
                    640, MUTED, int(255 * r))
        img.alpha_composite(lay)
        # 片尾只淡入不淡出（最后一帧停住，避免黑屏结尾）
        img2 = grade(img)
        if t < XIN:
            img2.alpha_composite(Image.new("RGBA", (W, H),
                                           BG + (int(255 * (1 - t / XIN)),)))
        yield img2.convert("RGB")


SHOTS_FN = [
    ("01", shot01), ("02", shot02), ("03", shot03), ("04", shot04),
    ("05", shot05), ("06", shot06), ("07", shot07), ("08", shot08),
    ("09", shot09),
]


def main():
    import sys
    argv = sys.argv[1:]
    # --only=05,06,07 只渲这几镜（其余镜从已有的裸流里跳过）。
    # 改一个镜头的版式时不用重渲全片 —— 全片要 8 分钟，
    # 但改版式往往要来回试好几遍，这个开关省的是来回的时间。
    only = None
    if "--only" in argv:
        only = set(argv[argv.index("--only") + 1].split(","))
    # --preview 只出 PNG 抽帧，不编码视频，用来快速看版式对不对
    preview = "--preview" in argv

    ff = imageio_ffmpeg.get_ffmpeg_exe()
    raw = os.path.join(OUT, "_film_raw.yuv")
    total_s = sum(DUR[k] for k, _ in SHOTS_FN) + TAIL
    print(f"目标时长 {total_s:.1f}s = {int(total_s*FPS)} 帧"
          + (f"   只渲 {sorted(only)}" if only else ""))

    if preview:
        os.makedirs(os.path.join(OUT, "_check"), exist_ok=True)
        for key, fn in SHOTS_FN:
            if only and key not in only:
                continue
            d = DUR[key]
            frames = list(fn(d))
            mid = frames[len(frames) // 2]
            mid.save(os.path.join(OUT, "_check", f"{key}.png"))
            print(f"{key}  中点帧已出（{d:.2f}s）")
        return

    print(f"{'镜头':<6}{'时长':>8}{'累计':>9}")
    acc = 0.0
    with open(raw, "wb") as f:
        for key, fn in SHOTS_FN:
            d = DUR[key]
            n = 0
            for fr in fn(d):
                f.write(fr.tobytes())
                n += 1
            acc += d
            print(f"{key:<6}{d:>7.2f}s{acc:>8.2f}s   {n} 帧")
        n = 0
        for fr in tail(TAIL):
            f.write(fr.tobytes())
            n += 1
        acc += TAIL
        print(f"{'尾':<6}{TAIL:>7.2f}s{acc:>8.2f}s   {n} 帧")
    print(f"\n裸流 {os.path.getsize(raw)/1024/1024/1024:.2f} GB，开始编码...")

    # 视频（无声）
    vpath = os.path.join(OUT, "_film_v.mp4")
    cmd = [
        ff, "-y", "-f", "rawvideo", "-pix_fmt", "rgb24",
        "-s", f"{W}x{H}", "-r", str(FPS), "-i", raw,
        "-c:v", "libx264", "-preset", "slow",
        "-b:v", "10M", "-minrate", "10M", "-maxrate", "10M", "-bufsize", "20M",
        "-x264-params", "nal-hrd=cbr:force-cfr=1",
        "-pix_fmt", "yuv420p", "-movflags", "+faststart", vpath,
    ]
    r = subprocess.run(cmd, capture_output=True, text=True)
    os.remove(raw)
    if r.returncode != 0:
        raise SystemExit("视频编码失败：" + r.stderr[-1500:])
    print(f"无声视频 {os.path.getsize(vpath)/1024/1024:.1f} MB")

    # 音频：把 9 段配音按顺序拼起来，再尾部补 TAIL 秒静音。
    # 每段配音的时长 == 该镜画面的时长（画面就是按配音时长渲的），
    # 所以顺序 concat 完毕，音视频时间轴天然严格对齐 ——
    # 这里千万不要再手工填静音去"对表"，那是在制造漂移。
    apath = os.path.join(OUT, "_film_a.m4a")
    rows = json.load(open(os.path.join(HERE, "voices", "_durations.json"),
                          encoding="utf-8"))["lines"]
    by_key = {row["name"][:2]: row["file"] for row in rows}
    parts = []
    for key, _ in SHOTS_FN:
        parts += ["-i", os.path.join(HERE, "voices", by_key[key])]
    fc = "".join(f"[{i}:a]" for i in range(len(SHOTS_FN)))
    fc += f"concat=n={len(SHOTS_FN)}:v=0:a=1[voice];"
    fc += f"[voice]apad=pad_dur={TAIL}[out]"
    cmd = [ff, "-y"] + parts + [
        "-filter_complex", fc, "-map", "[out]",
        "-c:a", "aac", "-b:a", "192k", "-ar", "44100", apath]
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        raise SystemExit("音频合成失败：" + r.stderr[-1500:])
    print(f"音轨 {os.path.getsize(apath)/1024/1024:.1f} MB")

    # 合流
    final = os.path.join(OUT, "工作记录本-介绍片-1080p.mp4")
    cmd = [ff, "-y", "-i", vpath, "-i", apath,
           "-c:v", "copy", "-c:a", "copy", "-shortest",
           "-movflags", "+faststart", final]
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        raise SystemExit("合流失败：" + r.stderr[-1500:])
    os.remove(vpath)
    os.remove(apath)

    size = os.path.getsize(final)
    print(f"\n成片：{final}")
    print(f"  {size/1024/1024:.1f} MB · {acc:.1f}s")


if __name__ == "__main__":
    main()
