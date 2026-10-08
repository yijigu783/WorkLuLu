# -*- coding: utf-8 -*-
"""语速—时长曲线探针。

用途：在动手改文案之前，先把「同一句话在不同语速下分别要多少秒」量出来。
不然很容易凭感觉删字，删多了再补回来，来回好几轮。

为什么单独一个脚本而不是塞进 make_voice.py：
  make_voice.py 是「交付用」的（落盘到 voices/，给剪辑当素材），
  这个是「决策用」的（只出数字，删完就扔）。两件事混在一起，
  下次改文案时会分不清哪些 mp3 是当前版本的。

用法：
  python probe_voice.py                 # 对 LINES 里的每句，跑 0/10/15/20%
  python probe_voice.py --rates 0,15    # 只跑两档
"""
import os
import re
import sys
import json
import asyncio
import tempfile
import subprocess

import edge_tts
import imageio_ffmpeg

FF = imageio_ffmpeg.get_ffmpeg_exe()

# 直接引用交付脚本里的台词，保证两边永远一致。
# （复制一份台词过来是隐患：改了 make_voice.py 忘了改这里，
#   探针给出的曲线就对不上实际会合成的内容。）
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from make_voice import LINES, DEFAULT_VOICE


def dur_of(path):
    r = subprocess.run([FF, "-i", path], capture_output=True, text=True)
    m = re.search(r"Duration: (\d+):(\d+):([\d.]+)", r.stderr)
    if not m:
        return None
    return int(m.group(1)) * 3600 + int(m.group(2)) * 60 + float(m.group(3))


async def one(text, voice, rate, tmpdir, idx):
    path = os.path.join(tmpdir, f"p{idx}.mp3")
    await edge_tts.Communicate(text, voice, rate=rate).save(path)
    return dur_of(path)


async def main():
    argv = sys.argv[1:]
    voice = DEFAULT_VOICE
    if "--voice" in argv:
        voice = argv[argv.index("--voice") + 1]
    rates = ["+0%", "+10%", "+15%", "+20%"]
    if "--rates" in argv:
        raw = argv[argv.index("--rates") + 1]
        rates = [r if r.startswith(("+", "-")) else "+" + r for r in raw.split(",")]

    print(f"音色：{voice}\n")
    header = f"{'镜头':<12}" + "".join(f"{r:>9}" for r in rates) + f"{'目标':>8} {'字数':>5}"
    print(header)
    print("-" * len(header.encode("gbk", errors="ignore")))

    out = []
    with tempfile.TemporaryDirectory() as td:
        for i, (name, text, target) in enumerate(LINES):
            ds = []
            for r in rates:
                d = await one(text, voice, r, td, f"{i}_{r}")
                ds.append(d)
            # 中文字数（不含标点）——用来看「每秒几个字」是否稳定
            han = len(re.findall(r"[\u4e00-\u9fff]", text))
            cells = "".join(f"{d:>8.2f}s" for d in ds)
            print(f"{name:<12}{cells}{target:>7.1f}s {han:>5}")
            out.append({"name": name, "target": target, "han": han,
                        "durations": dict(zip(rates, [round(d, 2) for d in ds]))})

    print("\n【每秒字数】看这个数是否稳定，它是估算删字量的依据")
    for row in out:
        han, d0 = row["han"], row["durations"][rates[0]]
        print(f"  {row['name']:<12} {han / d0:>5.2f} 字/秒   "
              f"（{han} 字 / {d0:.2f}s）")

    with open(os.path.join(os.path.dirname(os.path.abspath(__file__)),
                           "_probe.json"), "w", encoding="utf-8") as f:
        json.dump({"voice": voice, "rates": rates, "rows": out},
                  f, ensure_ascii=False, indent=2)
    print("\n→ _probe.json")


if __name__ == "__main__":
    asyncio.run(main())
