#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""带着安卓构建所需的环境变量执行任意命令。

# 为什么要这个包装

安卓构建需要五组环境变量，其中 `CARGO_TARGET_DIR` **只对安卓构建成立**：

  · 仓库根是中文（`F:\\工作记录本`），NDK 的 linker 包装脚本会把非 ASCII 路径
    按系统 ANSI 码页转码（实测「工作记录本」变成 `\\xb9\\xa4\\xd7\\xf7\\xbc\\xc7\\xc2\\xbc\\xb1\\xbe`），
    于是找不到目标文件。把 cargo 的 target 指到纯英文路径就绕开了。
  · 但**绝不能**把它写进用户级环境变量：那样桌面版的产物也会跑到
    `C:\\Android\\wl-target\\release\\worklog.exe`，而打包流程要的是
    `target/release/worklog.exe`（仓库根下）。桌面版用 MSVC 链接器，
    本来就不怕中文路径。

所以这个变量只在跑本脚本时存在，退出即失效。

# 用法

    python android/tools/with_android_env.py cargo tauri android build --apk --target aarch64
    python android/tools/with_android_env.py cargo tauri android build --debug
    python android/tools/with_android_env.py cargo tauri --version
    python android/tools/with_android_env.py --print        # 只看环境，不执行

注意：`cargo tauri android ...` 要在 `android/` 目录下跑（那里才有 `src-tauri/`）。
本脚本会把工作目录切到仓库根的 `android/`，除非你显式传了 `--cwd`。
"""

import os
import subprocess
import sys

JDK = r"C:\Android\jdk\jdk-17.0.20+8"
SDK = r"C:\Android\Sdk"
NDK = r"C:\Android\Sdk\ndk\27.3.13750724"
# 见上面说明：只有安卓构建才把 target 挪到英文路径
TARGET_DIR = r"C:\Android\wl-target"


def repo_root():
    here = os.path.dirname(os.path.abspath(__file__))
    cur = here
    for _ in range(6):
        if os.path.exists(os.path.join(cur, "Cargo.toml")) and os.path.isdir(
            os.path.join(cur, "core")
        ):
            return cur
        cur = os.path.dirname(cur)
    raise SystemExit("找不到仓库根")


def build_env():
    env = dict(os.environ)
    env["JAVA_HOME"] = JDK
    env["ANDROID_HOME"] = SDK
    env["ANDROID_SDK_ROOT"] = SDK
    env["NDK_HOME"] = NDK
    env["ANDROID_NDK_HOME"] = NDK
    env["CARGO_TARGET_DIR"] = TARGET_DIR
    # 把工具链的 bin 前置，免得依赖调用方 shell 里已经 export 过
    env["PATH"] = os.pathsep.join([
        os.path.join(SDK, "platform-tools"),
        os.path.join(SDK, "cmdline-tools", "latest", "bin"),
        os.path.join(JDK, "bin"),
        env.get("PATH", ""),
    ])
    return env


def main():
    argv = sys.argv[1:]
    if not argv:
        print(__doc__)
        raise SystemExit(2)

    if "--print" in argv:
        for k, v in build_env().items():
            if k in ("JAVA_HOME", "ANDROID_HOME", "ANDROID_SDK_ROOT", "NDK_HOME",
                     "ANDROID_NDK_HOME", "CARGO_TARGET_DIR"):
                print("%-18s = %s" % (k, v))
        return

    cwd = None
    if "--cwd" in argv:
        i = argv.index("--cwd")
        cwd = argv[i + 1]
        del argv[i:i + 2]
    else:
        cwd = os.path.join(repo_root(), "android")

    print("工作目录: %s" % cwd)
    print("执行: %s" % " ".join(argv))
    print("-" * 60)
    sys.stdout.flush()

    p = subprocess.run(argv, cwd=cwd, env=build_env())
    print("-" * 60)
    print("退出码: %d" % p.returncode)
    raise SystemExit(p.returncode)


if __name__ == "__main__":
    main()
