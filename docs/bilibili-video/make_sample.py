# -*- coding: utf-8 -*-
"""工作记录本 B站介绍片 —— 打样版（15 秒，验证动效与节奏）

只做片头 + 一个功能演示，用来看动效观感和字幕节奏对不对。
满意再往下铺全片。
"""
import os
import math
import subprocess
import numpy as np
from PIL import Image, ImageDraw, ImageFont
import imageio_ffmpeg

ROOT = r"F:\工作记录本"
SHOTS = os.path.join(ROOT, "docs", "screenshots")
OUT = os.path.join(ROOT, "docs", "bilibili-video")
os.makedirs(OUT, exist_ok=True)

W, H = 1920, 1080
FPS = 30
FONT_B = r"C:\Windows\Fonts\msyhbd.ttc"
FONT_R = r"C:\Windows\Fonts\msyh.ttc"

# 配色：跟软件界面一套（靛蓝主色 + 浅灰底）
BG      = (244, 246, 250)
INK     = (23, 28, 45)
MUTED   = (108, 118, 140)
BRAND   = (79, 91, 232)
CARD    = (255, 255, 255)

def font(path, size):
    return ImageFont.truetype(path, size)

def ease_out(t):
    """缓出。开场动效都用它，比线性有分量。"""
    return 1 - (1 - t) ** 3

def ease_in_out(t):
    return 3 * t * t - 2 * t * t * t

def rounded(draw, box, radius, fill, outline=None, width=1):
    draw.rounded_rectangle(box, radius=radius, fill=fill, outline=outline, width=width)

def paste_shot(base, img, cx, cy, scale, shadow=True, radius=12):
    """把界面截图缩放后贴到 base 上，(cx, cy) 是截图中心的目标位置。"""
    w = int(img.width * scale)
    h = int(img.height * scale)
    s = img.resize((w, h), Image.LANCZOS)
    x, y = int(cx - w / 2), int(cy - h / 2)

    if shadow:
        sh = Image.new("RGBA", (w + 60, h + 60), (0, 0, 0, 0))
        d = ImageDraw.Draw(sh)
        d.rounded_rectangle((30, 34, 30 + w, 34 + h), radius=radius,
                            fill=(23, 28, 45, 46))
        sh = sh.filter(__import__("PIL.ImageFilter", fromlist=["ImageFilter"])
                       .GaussianBlur(18))
        base.alpha_composite(sh, (x - 30, y - 30))

    mask = Image.new("L", (w, h), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, w, h), radius=radius, fill=255)
    base.paste(s, (x, y), mask)
    return (x, y, w, h)

def new_frame():
    img = Image.new("RGBA", (W, H), BG + (255,))
    return img

def grade(img):
    """统一收尾：加一点顶部渐变，避免全平。"""
    overlay = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    d = ImageDraw.Draw(overlay)
    for i in range(240):
        a = int(10 * (1 - i / 240))
        d.line([(0, i), (W, i)], fill=(79, 91, 232, a))
    img.alpha_composite(overlay)
    return img

FRAMES = []
def hold(img, sec):
    FRAMES.extend([img.convert("RGB")] * int(round(sec * FPS)))

# ────────────────────────────────────────────
# 镜头 1（0.0 - 3.2s）LOGO + 一句话
# ────────────────────────────────────────────
def shot1():
    dur = 3.2
    total = int(dur * FPS)
    for i in range(total):
        t = i / total
        img = new_frame()
        d = ImageDraw.Draw(img)

        # 标题从下方浮上来 + 淡入
        p = ease_out(min(1, t / 0.45))
        dy = int(28 * (1 - p))
        alpha = int(255 * min(1, t / 0.35))

        layer = Image.new("RGBA", (W, H), (0, 0, 0, 0))
        ld = ImageDraw.Draw(layer)
        title = "工作记录本"
        f1 = font(FONT_B, 118)
        tw = ld.textlength(title, font=f1)
        ld.text(((W - tw) / 2, 400 + dy), title, font=f1, fill=INK + (alpha,))

        sub = "一个 4 MB 的单文件，替我记住所有周期性的事"
        f2 = font(FONT_R, 42)
        sw = ld.textlength(sub, font=f2)
        # 副标题往下挪到 592：大标题墨迹到 541 为止，中间要留出一条线的位置，
        # 556 太挤（两行只差 24px，6px 的线一放就贴上去了）
        ld.text(((W - sw) / 2, 592 + dy), sub, font=f2, fill=MUTED + (alpha,))
        img.alpha_composite(layer)

        # 一条靛蓝短线，从中间向两侧展开。
        # 位置靠 getbbox 实测过：大标题锚点 400 时墨迹落在 427~541，
        # 副标题锚点 556 时墨迹落在 565~607，所以线放 548 才夹在两者之间 ——
        # 靠眼睛估会跑到副标题下面去（试过两次）。
        q = ease_out(max(0, min(1, (t - 0.30) / 0.40)))
        half = int(150 * q)
        if half > 0:
            d.rounded_rectangle((W // 2 - half, 548, W // 2 + half, 554),
                                radius=3, fill=BRAND)

        # 整体淡出
        if t > 0.86:
            k = (t - 0.86) / 0.14
            ov = Image.new("RGBA", (W, H), BG + (int(255 * k),))
            img.alpha_composite(ov)
        FRAMES.append(grade(img).convert("RGB"))

# ────────────────────────────────────────────
# 镜头 2（3.2 - 5.4s）痛点字幕卡
# ────────────────────────────────────────────
def shot2():
    dur = 2.2
    total = int(dur * FPS)
    rows = [
        ("周五交周报", "周一要开会", "月度对账"),
        ("这些事总在重复", ),
    ]
    for i in range(total):
        t = i / total
        img = new_frame()
        d = ImageDraw.Draw(img)

        # 三行待办卡片依次滑入
        items = ["周五交周报", "周一要开会", "每月 15 号对账"]
        f = font(FONT_R, 46)
        for k, s in enumerate(items):
            start = 0.05 + k * 0.13
            p = ease_out(max(0, min(1, (t - start) / 0.30)))
            if p <= 0:
                continue
            cw, ch = 520, 92
            cx = W // 2 - cw // 2 + int((1 - p) * 90)
            cy = 300 + k * 116
            a = int(255 * p)
            lay = Image.new("RGBA", (W, H), (0, 0, 0, 0))
            ld = ImageDraw.Draw(lay)
            rounded(ld, (cx, cy, cx + cw, cy + ch), 16, CARD + (a,))
            # 左侧小色条
            ld.rounded_rectangle((cx + 22, cy + ch // 2 - 13, cx + 27, cy + ch // 2 + 13),
                                 radius=3, fill=(203, 213, 225, a))
            ld.text((cx + 52, cy + ch // 2 - 30), s, font=f, fill=INK + (a,))
            img.alpha_composite(lay)

        # 中间一句质问
        q = ease_out(max(0, min(1, (t - 0.58) / 0.32)))
        if q > 0:
            lay = Image.new("RGBA", (W, H), (0, 0, 0, 0))
            ld = ImageDraw.Draw(lay)
            fq = font(FONT_B, 58)
            s = "翻聊天记录，翻到手酸"
            sw = ld.textlength(s, font=fq)
            ld.text(((W - sw) / 2, 692), s, font=fq, fill=BRAND + (int(255 * q),))
            img.alpha_composite(lay)

        if t > 0.88:
            k = (t - 0.88) / 0.12
            img.alpha_composite(Image.new("RGBA", (W, H), BG + (int(255 * k),)))
        FRAMES.append(grade(img).convert("RGB"))

# ────────────────────────────────────────────
# 镜头 3（5.4 - 10.4s）主界面 + 分类自动关联
# ────────────────────────────────────────────
def shot3():
    dur = 5.0
    total = int(dur * FPS)
    main = Image.open(os.path.join(SHOTS, "01-today.png")).convert("RGB")

    for i in range(total):
        t = i / total
        img = new_frame()
        d = ImageDraw.Draw(img)

        # 截图推近：从 0.86 缓推到 1.0，制造"镜头靠近"的感觉。
        # 截图长宽比 1280×820 ≈ 1.561，整屏留出 140px 边距刚好放得下，
        # 之前 cy 给到 560 会让底部的"周期任务"一栏被切掉。
        p = ease_in_out(min(1, t / 0.9))
        scale = 0.86 + 0.14 * p
        # 前 0.15 秒整体淡入
        a = min(1, t / 0.15)
        if a < 1:
            img.putalpha(255)
        cx = W / 2 + (1 - p) * -30
        cy = 592 + (1 - p) * 18
        paste_shot(img, main, cx, cy, scale)

        # 顶部标题
        if t > 0.22:
            q = ease_out(min(1, (t - 0.22) / 0.30))
            lay = Image.new("RGBA", (W, H), (0, 0, 0, 0))
            ld = ImageDraw.Draw(lay)
            f1 = font(FONT_B, 54)
            s = "打开就知道今天该干什么"
            sw = ld.textlength(s, font=f1)
            ld.text(((W - sw) / 2, 78), s, font=f1, fill=INK + (int(255 * q),))
            img.alpha_composite(lay)

        if a < 1:
            img.alpha_composite(Image.new("RGBA", (W, H), BG + (int(255 * (1 - a)),)))
        if t > 0.90:
            k = (t - 0.90) / 0.10
            img.alpha_composite(Image.new("RGBA", (W, H), BG + (int(255 * k),)))
        FRAMES.append(grade(img).convert("RGB"))

# ────────────────────────────────────────────
# 镜头 4（10.4 - 14.2s）收尾卡
# ────────────────────────────────────────────
def shot4():
    dur = 3.8
    total = int(dur * FPS)
    for i in range(total):
        t = i / total
        img = new_frame()
        d = ImageDraw.Draw(img)

        p = ease_out(min(1, t / 0.4))
        lay = Image.new("RGBA", (W, H), (0, 0, 0, 0))
        ld = ImageDraw.Draw(lay)

        f1 = font(FONT_B, 76)
        s1 = "免费 · 开源 · 单文件"
        w1 = ld.textlength(s1, font=f1)
        ld.text(((W - w1) / 2, 414 + int((1 - p) * 24)), s1, font=f1,
                fill=INK + (int(255 * p),))

        f2 = font(FONT_R, 40)
        s2 = "下载解压，双击就能用"
        w2 = ld.textlength(s2, font=f2)
        q = ease_out(max(0, min(1, (t - 0.25) / 0.4)))
        ld.text(((W - w2) / 2, 546), s2, font=f2, fill=MUTED + (int(255 * q),))
        img.alpha_composite(lay)

        if t > 0.85:
            k = (t - 0.85) / 0.15
            img.alpha_composite(Image.new("RGBA", (W, H), BG + (int(255 * k),)))
        FRAMES.append(grade(img).convert("RGB"))

print("渲染帧...")
shot1(); shot2(); shot3(); shot4()
print(f"共 {len(FRAMES)} 帧 = {len(FRAMES)/FPS:.1f} 秒")

# 编码
ff = imageio_ffmpeg.get_ffmpeg_exe()
raw = os.path.join(OUT, "_sample_raw.yuv")
with open(raw, "wb") as f:
    for fr in FRAMES:
        f.write(fr.tobytes())

mp4 = os.path.join(OUT, "打样-15秒.mp4")
# 码率必须给够：B站对低码率稿会二次压制，1080P 建议 ≥6 Mbps。
# 这片的画面是大片纯色，CRF 模式会自动压到 1 Mbps 以下（实测 950 kbps），
# 所以这里不用 CRF，改成锁目标码率 + minrate=maxrate（CBR），
# 保证上传的码率稳定在 8 Mbps 以上。
cmd = [
    ff, "-y", "-f", "rawvideo", "-pix_fmt", "rgb24",
    "-s", f"{W}x{H}", "-r", str(FPS), "-i", raw,
    "-c:v", "libx264", "-preset", "slow",
    "-b:v", "10M", "-minrate", "10M", "-maxrate", "10M", "-bufsize", "20M",
    "-x264-params", "nal-hrd=cbr:force-cfr=1",
    "-pix_fmt", "yuv420p",
    "-movflags", "+faststart", mp4,
]
r = subprocess.run(cmd, capture_output=True, text=True)
os.remove(raw)
if r.returncode != 0:
    print("编码失败：", r.stderr[-1500:])
else:
    print("输出：", mp4, f"{os.path.getsize(mp4)/1024/1024:.1f} MB")
