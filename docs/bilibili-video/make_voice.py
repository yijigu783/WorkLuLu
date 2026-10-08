# -*- coding: utf-8 -*-
"""B站介绍片配音生成 —— 把 lines.py 里的 9 段台词合成语音。

为什么走 edge-tts：
  本机虽然装了中文语音引擎（Huihui/Kangkang/Yaoyao），但沙箱禁止 COM 实例化，
  SAPI / pyttsx3 / System.Speech 三条路全堵死。
  edge-tts 不碰本地语音引擎，走网络调微软的在线合成服务，
  绕开了权限限制，而且音质比 SAPI 那批老引擎好得多。

台词在 lines.py。改文案改那个文件，别改这里 —— 这里只负责合成和量时长。
（改动前记得先跑 lines.py 体检字数，见该文件顶部说明。）

用法：
  python make_voice.py              # 全部合成到 voices/
  python make_voice.py --dry        # 只量时长、不落盘，用来对表
  python make_voice.py --voice zh-CN-YunyangNeural
  python make_voice.py --rate +10%
"""
import os
import re
import sys
import json
import asyncio
import subprocess

import edge_tts
import imageio_ffmpeg

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "voices")

DEFAULT_VOICE = "zh-CN-YunxiNeural"   # 云希，Lively/Sunshine，工具类视频最贴的调性
RATE = "+0%"                          # 语速：文案已按 3.85 字/秒 配平，正常速率就够

sys.path.insert(0, HERE)
from lines import LINES


def dur_of(path):
    """用 ffprobe（imageio-ffmpeg 自带的那个）量时长。"""
    ff = imageio_ffmpeg.get_ffmpeg_exe()
    r = subprocess.run([ff, "-i", path], capture_output=True, text=True)
    m = re.search(r"Duration: (\d+):(\d+):([\d.]+)", r.stderr)
    if not m:
        return None
    h, mi, s = int(m.group(1)), int(m.group(2)), float(m.group(3))
    return h * 3600 + mi * 60 + s


async def synth(text, path, voice, rate, tries=4):
    """合成一段，带重试。

    为什么要重试：微软那个在线合成服务会时不时返回 NoAudioReceived
    （不是参数错，是它对某些长句/短句偶发不给音频）。第一次遇到时
    它会先建好空文件再抛错 —— 也就是说失败之后磁盘上留着一个 0 字节的 mp3，
    后面的 dur_of() 量不出时长，看起来像"未合成"，很容易让人以为是台词问题。
    所以：失败就删掉半成品，退避重试；重试完还不行才往上抛。
    """
    last = None
    for i in range(tries):
        if i:
            await asyncio.sleep(1.5 * i)      # 退避，别连打
        try:
            await edge_tts.Communicate(text, voice, rate=rate).save(path)
            if os.path.getsize(path) > 0:     # 空文件当失败处理
                return
            last = RuntimeError("合成结果是空文件")
        except Exception as e:                # noqa: BLE001 —— 网络类异常五花八门
            last = e
        if os.path.exists(path):
            os.remove(path)
    raise RuntimeError(f"合成失败（试了 {tries} 次）：{last}")


async def main():
    argv = sys.argv[1:]
    dry = "--dry" in argv
    voice = DEFAULT_VOICE
    if "--voice" in argv:
        voice = argv[argv.index("--voice") + 1]
    rate = RATE
    if "--rate" in argv:
        rate = argv[argv.index("--rate") + 1]
    # --only=06 只重做某一镜。改一句台词时不用把 9 段全重跑一遍
    # （重跑会覆盖已听好的那几段，而且没必要再等 30 秒）
    only = None
    if "--only" in argv:
        only = argv[argv.index("--only") + 1]

    os.makedirs(OUT, exist_ok=True)
    print(f"音色：{voice}   语速：{rate}" + (f"   只做：{only}" if only else ""))
    print(f"{'镜头':<14}{'实长':>8}{'目标':>8}{'差':>8}   状态")
    print("-" * 56)

    total_real = 0.0
    total_target = 0.0
    report = []
    failures = []

    for name, text, target in LINES:
        path = os.path.join(OUT, f"{name}.mp3")
        if dry:
            d = None
        elif only and not name.startswith(only):
            d = dur_of(path) if os.path.exists(path) else None
        else:
            try:
                await synth(text, path, voice, rate)
            except Exception as e:            # noqa: BLE001
                failures.append(name)
                print(f"{name:<14}{'!!':>8}{target:>7.1f}s{'':>8}   {e}")
                continue
            d = dur_of(path)
        if d is None:
            print(f"{name:<14}{'—':>8}{target:>7.1f}s{'':>8}   （未合成）")
            continue

        total_real += d
        total_target += target

        diff = d - target
        # ±25% 以内算合格：合成音的自然停顿跟人念不完全一样，
        # 差一点点靠 ffmpeg 拉伸就能圆回来，差太多就得改文案
        if abs(diff) <= target * 0.25:
            mark = "OK"
        elif diff > 0:
            mark = "偏长，建议删字或加速"
        else:
            mark = "偏短，可以补一句或放慢"
        print(f"{name:<14}{d:>7.2f}s{target:>7.1f}s{diff:>+7.2f}s   {mark}")
        report.append({"name": name, "file": os.path.basename(path),
                       "seconds": round(d, 2), "target": target})
    print("-" * 56)
    print(f"{'合计':<14}{total_real:>7.2f}s{total_target:>7.1f}s{total_real-total_target:>+7.2f}s")

    # 成片长度 = 各镜配音时长之和 + 片尾 3s（logo / 仓库地址）。
    # 用实测的 total_real，不用 lines.py 的估算 —— 这里已经在拿真音频说话了，
    # 没理由再退回去用模型。lines.py 那套估算是"写稿时"用的尺子，不是"验收时"的。
    TAIL = 3.0
    budget = total_real + TAIL
    verdict = "OK" if 60 <= budget <= 90 else "超区间，要调"
    print(f"\n成片时长预估 {budget:.1f}s（配音 {total_real:.2f}s 实测 + 片尾 {TAIL:.0f}s）"
          f"   目标 60-90s   {verdict}")
    if not dry and not (60 <= budget <= 90):
        # 逐条列出哪几镜超了模型预估，方便定位是哪句话写长了
        print("   各镜与模型预估的差：")
        for row in report:
            if abs(row["seconds"] - row["target"]) > 1.0:
                print(f"     {row['name']}  实测 {row['seconds']:.2f}s "
                      f"vs 预估 {row['target']:.1f}s  "
                      f"差 {row['seconds'] - row['target']:+.2f}s")

    if not dry:
        meta = os.path.join(OUT, "_durations.json")
        with open(meta, "w", encoding="utf-8") as f:
            json.dump({"voice": voice, "rate": rate, "lines": report,
                       "total_seconds": round(total_real, 2),
                       "runtime_estimate": round(budget, 1),
                       "failures": failures},
                      f, ensure_ascii=False, indent=2)
        print(f"\n配音在 {OUT}\n时长表 _durations.json（下一步拿它对画面）")

    if failures:
        # 用非零退出码收尾，这样在脚本链里能被发现，不会静默出一个残缺的片子
        print(f"\n!! 有 {len(failures)} 镜没合成成功：{', '.join(failures)}")
        print(f"   重跑：python make_voice.py --only={failures[0][:2]}")
        sys.exit(1)



if __name__ == "__main__":
    asyncio.run(main())
