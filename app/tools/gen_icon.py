"""生成应用图标：圆角方块 + 中央方框轮廓，与界面 brand-mark 一致。"""
import zlib, struct, os

PRIMARY = (79, 91, 232)
WHITE = (255, 255, 255)


def inside_rr(px, py, x0, y0, x1, y1, r):
    if px < x0 or py < y0 or px > x1 or py > y1:
        return False
    cx = min(max(px, x0 + r), x1 - r)
    cy = min(max(py, y0 + r), y1 - r)
    dx, dy = px - cx, py - cy
    return dx * dx + dy * dy <= r * r


def cov(x, y, x0, y0, x1, y1, r, n=4):
    hit = 0
    for i in range(n):
        for j in range(n):
            if inside_rr(x + (i + .5) / n, y + (j + .5) / n, x0, y0, x1, y1, r):
                hit += 1
    return hit / (n * n)


def render(size):
    s = float(size)
    bg_r = s * 0.22
    # 中央方框：外框与内框
    fo0, fo1 = s * 0.30, s * 0.70
    fi0, fi1 = s * 0.385, s * 0.615
    f_r = s * 0.055

    rows = []
    for y in range(size):
        row = []
        for x in range(size):
            a_bg = cov(x, y, 0, 0, s, s, bg_r)
            if a_bg <= 0:
                row.append((0, 0, 0, 0))
                continue
            a_ring = cov(x, y, fo0, fo0, fo1, fo1, f_r) - cov(x, y, fi0, fi0, fi1, fi1, f_r * 0.7)
            a_ring = max(0.0, min(1.0, a_ring))
            r = PRIMARY[0] * (1 - a_ring) + WHITE[0] * a_ring
            g = PRIMARY[1] * (1 - a_ring) + WHITE[1] * a_ring
            b = PRIMARY[2] * (1 - a_ring) + WHITE[2] * a_ring
            row.append((int(r), int(g), int(b), int(a_bg * 255)))
        rows.append(row)
    return rows


def encode_png(size, rows):
    raw = bytearray()
    for row in rows:
        raw.append(0)
        for px in row:
            raw += bytes(px)

    def chunk(tag, data):
        return (struct.pack('>I', len(data)) + tag + data +
                struct.pack('>I', zlib.crc32(tag + data) & 0xffffffff))

    return (b'\x89PNG\r\n\x1a\n'
            + chunk(b'IHDR', struct.pack('>IIBBBBB', size, size, 8, 6, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(bytes(raw), 9))
            + chunk(b'IEND', b''))


def write_ico(path, entries):
    n = len(entries)
    header = struct.pack('<HHH', 0, 1, n)
    offset = 6 + 16 * n
    dirs, data = b'', b''
    for size, png in entries:
        d = 0 if size >= 256 else size
        dirs += struct.pack('<BBBBHHII', d, d, 0, 0, 1, 32, len(png), offset)
        offset += len(png)
        data += png
    with open(path, 'wb') as f:
        f.write(header + dirs + data)


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    out = os.path.join(here, '..', 'src-tauri', 'icons')
    out = os.path.normpath(out)
    os.makedirs(out, exist_ok=True)

    cache = {}
    for s in (16, 32, 48, 64, 128, 256, 512):
        cache[s] = render(s)
        print(f'rendered {s}x{s}')

    def save(name, s):
        with open(os.path.join(out, name), 'wb') as f:
            f.write(encode_png(s, cache[s]))

    save('32x32.png', 32)
    save('128x128.png', 128)
    save('128x128@2x.png', 256)
    save('icon.png', 512)

    write_ico(os.path.join(out, 'icon.ico'),
              [(s, encode_png(s, cache[s])) for s in (16, 32, 48, 64, 128, 256)])

    print('icons written to', out)


if __name__ == '__main__':
    main()
