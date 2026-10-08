# -*- coding: utf-8 -*-
"""B站封面图 —— 1146 × 717（16:10，B站标准尺寸）。

封面是唯一能自己控制的"点击率变量"，所以这张图承担三件事，
缺一件这张封面就白做了：

  1. **一眼认出是什么**：大标题「工作记录本」必须够大 ——
     手机上刷到视频时，封面会被缩到约 320px 宽，
     标题字号小于 60px 就糊成一团认不出。
  2. **给一个点进来的理由**：副标题写"周期性的事，替你记住"，
     说人话、说场景，不写"高效管理"这种空词。
  3. **放一个反直觉数字**：「4 MB」放在角标。
     同类软件动辄几百 MB，这个数字是这张封面最锋利的地方。

配色沿用软件界面（靛蓝 + 浅灰），保持片子整体一致。
右下角压一张真实界面截图 —— 实拍感比纯色块可信，也避免"空洞大字图"。
"""
import os
import subprocess
from PIL import Image, ImageDraw, ImageFont, ImageFilter

ROOT = r"F:\工作记录本"
SHOTS = os.path.join(ROOT, "docs", "screenshots")
OUT = os.path.join(ROOT, "docs", "bilibili-video")

W, H = 1146, 717
FONT_B = r"C:\Windows\Fonts\msyhbd.ttc"
FONT_R = r"C:\Windows\Fonts\msyh.ttc"

BG    = (244, 246, 250)
INK   = (23, 28, 45)
MUTED = (108, 118, 140)
BRAND = (79, 91, 232)
CARD  = (255, 255, 255)


def font(bold, size):
    return ImageFont.truetype(FONT_B if bold else FONT_R, size)


img = Image.new("RGBA", (W, H), BG + (255,))
d = ImageDraw.Draw(img)

# 左上角靛蓝色块：把视觉重心拉到标题这边，也避免整张太平
d.rectangle((0, 0, 520, H), fill=(238, 241, 252, 255))
# 一条斜向色带增加动感（封面静态，靠几何形制造"在设计"的感觉）
d.polygon([(520, 0), (640, 0), (520, 250)], fill=(79, 91, 232, 26))

# ── 主标题 ─────────────────────────────────────────────
f_title = font(True, 92)
title = "工作记录本"
d.text((62, 118), title, font=f_title, fill=INK)
# 标题下的靛蓝短线
d.rounded_rectangle((66, 238, 236, 246), radius=4, fill=BRAND)

# ── 副标题 ─────────────────────────────────────────────
f_sub = font(True, 44)
d.text((64, 288), "周期性的事", font=f_sub, fill=BRAND)
d.text((64, 348), "替你记住", font=f_sub, fill=INK)

# ── 角标：4 MB ─────────────────────────────────────────
f_big = font(True, 74)
b = d.textbbox((0, 0), "4 MB", font=f_big)
bw = b[2] - b[0]
d.rounded_rectangle((62, 452, 62 + bw + 56, 452 + 106), radius=20,
                    fill=BRAND)
d.text((62 + 28, 452 + 12 - b[1]), "4 MB", font=f_big, fill=(255, 255, 255))

f_sm = font(False, 30)
d.text((62, 578), "单文件 · 免安装 · 免费开源", font=f_sm, fill=MUTED)

# ── 右下角：真实界面截图 ───────────────────────────────
shot = Image.open(os.path.join(SHOTS, "01-today.png")).convert("RGB")
# 裁成右侧竖长比例。**从左边裁**，不要从右边 ——
# 界面右侧是空白边缘，裁右边会得到一张"半截卡片"，
# 左侧才是"今天 / 2 / 5 / 1"这些一眼能看懂的内容。
sw, sh = shot.size
target_ratio = 560 / 380
crop_w = int(sh * target_ratio)
if crop_w > sw:
    crop_w = sw
shot = shot.crop((0, 0, crop_w, sh)).resize((560, 380), Image.LANCZOS)

rx, ry = W - 560 - 56, H - 380 - 64
# 阴影
sh_layer = Image.new("RGBA", (560 + 80, 380 + 80), (0, 0, 0, 0))
ImageDraw.Draw(sh_layer).rounded_rectangle(
    (40, 44, 40 + 560, 44 + 380), radius=14, fill=(23, 28, 45, 52))
img.alpha_composite(sh_layer.filter(ImageFilter.GaussianBlur(24)),
                    (rx - 40, ry - 40))
# 圆角裁切
mask = Image.new("L", (560, 380), 0)
ImageDraw.Draw(mask).rounded_rectangle((0, 0, 560, 380), radius=14, fill=255)
img.paste(shot, (rx, ry), mask)
# 截图描边，浅底上不描边会糊在一起
ImageDraw.Draw(img).rounded_rectangle((rx, ry, rx + 560, ry + 380),
                                      radius=14, outline=(222, 228, 240, 255),
                                      width=2)

# ── 完成后转 JPG ───────────────────────────────────────
out_png = os.path.join(OUT, "封面-1146x717.png")
out_jpg = os.path.join(OUT, "封面-1146x717.jpg")
img.convert("RGB").save(out_png)
img.convert("RGB").save(out_jpg, quality=94, subsampling=0)

for p in (out_png, out_jpg):
    print(f"{os.path.basename(p)}  {os.path.getsize(p)/1024:.0f} KB")
print(f"尺寸 {W}x{H}（16:10）")
