# -*- coding: utf-8 -*-
"""把微信赞赏码海报裁成「只有二维码本体」的方形图，供界面弹窗展示。

为什么要裁：微信导出的是整张海报（底部有黄色色块 + 「xxx的赞赏码」署名）。
海报版式不适合直接放进软件界面 —— 既占地方，署名又和软件署名对不上。
软件里只展示码本身，引导语由页面自己渲染。

为什么要手工量边界而不是等比切：二维码外围是放射状装饰线，长短不一，
按整图比例切容易切掉最外侧的短线、或者把下方那行「感谢您的支持！」带进来。
所以先扫描深色像素求出包围盒，再向外留一点安全边距。

用法：
    <venv>/Scripts/python.exe prep_reward_qr.py <原始海报.png> <输出.png>

依赖 Pillow（放在隔离 venv 里，不进项目）。
"""
import os
import sys

from PIL import Image

DARK_THRESHOLD = 128   # 低于此灰度算「二维码像素」
SCAN_TOP = 120         # 扫描上界：躲开顶部装饰线
SCAN_BOTTOM = 806      # 扫描下界：躲开下方「感谢您的支持！」那行字
MARGIN_RATIO = 0.08    # 安全边距，按包围盒边长取比例
OUT_SIZE = 512         # 输出边长。界面展示约 260px，2x 屏下 512 足够清晰
PALETTE_COLORS = 32    # 量化到 32 色调色板：512 的彩色 PNG 要 127KB，量化后约 28KB


def qr_bbox(gray):
    """扫描出二维码深色像素的包围盒，返回 (left, top, right, bottom)（右下开区间）。"""
    w, h = gray.size
    px = gray.load()
    minx, miny, maxx, maxy = w, h, -1, -1
    for y in range(SCAN_TOP, min(SCAN_BOTTOM, h)):
        xs = [x for x in range(w) if px[x, y] < DARK_THRESHOLD]
        if not xs:
            continue
        minx = min(minx, xs[0])
        maxx = max(maxx, xs[-1])
        miny = min(miny, y)
        maxy = max(maxy, y)
    if maxx < 0:
        raise SystemExit('没扫到任何深色像素，图片可能不是赞赏码海报')
    return minx, miny, maxx + 1, maxy + 1


def main():
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    src, dst = sys.argv[1], sys.argv[2]

    im = Image.open(src).convert('RGB')
    w, h = im.size
    left, top, right, bottom = qr_bbox(im.convert('L'))

    # 撑成正方形（取长边），否则缩放会把圆的压扁
    side = max(right - left, bottom - top)
    cx, cy = (left + right) / 2, (top + bottom) / 2
    side += side * MARGIN_RATIO
    half = side / 2

    box = (round(cx - half), round(cy - half), round(cx + half), round(cy + half))
    if box[0] < 0 or box[1] < 0 or box[2] > w or box[3] > h:
        raise SystemExit('安全边距超出原图范围，请调小 MARGIN_RATIO：%r' % (box,))

    out = im.crop(box).resize((OUT_SIZE, OUT_SIZE), Image.LANCZOS)
    # 必须 PNG 无损 —— 有损压缩会在二维码边缘产生伪影，可能扫不出来。
    # 但 512 的彩色 PNG 有 127KB，对一张只在弹窗里看一眼的图太浪费；
    # 量化到调色板后约 28KB，放射线与三个定位点仍然清晰可辨。
    # dither 用 NONE：抖动会给黑白边缘撒上噪点，既增大体积也不利于扫码。
    out = out.quantize(colors=PALETTE_COLORS, method=Image.MEDIANCUT, dither=Image.NONE)
    out.save(dst, 'PNG', optimize=True)

    print('原图      %d x %d' % (w, h))
    print('二维码盒  x %d..%d  y %d..%d  (%d x %d)'
          % (left, right, top, bottom, right - left, bottom - top))
    print('裁剪框    %r  (%d x %d)' % (box, box[2] - box[0], box[3] - box[1]))
    print('已写出    %s  %d x %d  %.1f KB'
          % (dst, OUT_SIZE, OUT_SIZE, os.path.getsize(dst) / 1024))


if __name__ == '__main__':
    main()
