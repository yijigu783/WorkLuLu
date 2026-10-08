#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""把 gen/android 里三处「本机网络环境导致必须改」的 Gradle 配置修好。

跑完 `cargo tauri android init` 之后**必须**跑一遍 —— init 会重新生成这些文件，
把三处都改回去。脚本是幂等的，重复跑不会出问题。

    python android/tools/fix_gradle.py            # 改
    python android/tools/fix_gradle.py --dry      # 只看要改什么

# 修什么

## 1. 仓库地址：`google()` 换成官方内容源

`maven.google.com` 在开发机上**连不通**（DNS 能解析，TCP 连不上，走代理也一样），
而它就是 Gradle 里 `google()` 的地址 —— Android Gradle Plugin 的唯一来源。
不换掉，构建会在依赖解析阶段直接失败。

`maven.google.com` 只是 `https://dl.google.com/dl/android/maven2` 的别名，
后者直连可用（实测真实构件返回 206）。

## 1b. 光换地址还不够 —— 官方源能通，但**慢到没法用**

换完地址后构建能跑起来，却卡在依赖下载上：Gradle 的临时下载文件
（`~/.gradle/.tmp/gradle_download*bin`）**只有 3~7 KB/s**，一个 15 MB 的 jar
要下几十分钟。同一个文件用 curl 实测：

| 源 | 实测速度 |
|---|---|
| `dl.google.com`（直连） | **6,407 KB/s** |
| `dl.google.com`（走代理） | **8,322 KB/s** |
| `repo1.maven.org`（直连） | 256 KB/s |
| 阿里云 `repository/google` | **4,037 KB/s** |
| 阿里云 `repository/central` | **7,269 KB/s** |
| **Gradle 自己下的** | **3~7 KB/s** |

也就是说**不是网络慢，是 Gradle 那一路慢**（原因未查明；`netstat` 显示它直连
`dl.google.com` / `repo1.maven.org` 的 IP，全是 `CLOSE_WAIT`，疑似连接被反复掐断重连）。
既然 curl 走镜像能到 4~7 MB/s，最省事的解法就是**让 Gradle 也走镜像**：
在每个仓库块最前面插入三个阿里云端点，官方源留在后面当兜底。

三个端点各有分工，缺一不可：

| 端点 | 顶替 | 实测 |
|---|---|---|
| `repository/google` | `google()` / `dl.google.com` | AGP 9.3.1 / androidx / material 全 200 |
| `repository/central` | `mavenCentral()` | junit 200 |
| `repository/gradle-plugin` | `plugins.gradle.org` | kotlin plugin marker 200 |

> 别用 `repository/public` 顶 google 的东西 —— 实测 `com.google.android.material:material`
> 在它上面 **404**，它只聚合 central + jcenter。

影响文件：`build.gradle.kts`（2 处）、`buildSrc/build.gradle.kts`（1 处）。

## 2. 网络超时：Gradle wrapper 默认只等 10 秒

`gradle-wrapper.properties` 里**没有** `networkTimeout` 时，wrapper 用的是
10 秒硬超时。本机到 `services.gradle.org` 首次建连经常超过 10 秒，于是：

    Downloading https://services.gradle.org/distributions/gradle-9.6.1-bin.zip
    ...... Attempt 1/1 failed. Reason: ... failed: timeout (10000ms)
    java.net.SocketTimeoutException: Read timed out

第一次就撞上了。而且 wrapper **不重试**（Attempt 1/1），下载了 6.8 MB 的
`.part` 就整个失败。

## ⚠️ 但真正的问题不是超时，是官方源根本下不动

把超时放宽到 120 秒之后**依然下不动** —— 实测官方源 12 秒**一个字节都拿不到**：

| 源 | 实测速度 |
|---|---|
| `services.gradle.org`（官方，HEAD 返回 307 跳到 CDN） | **0 字节 / 12 秒** |
| `mirrors.cloud.tencent.com/gradle/` | **9,233 KB/s**（12 秒下了 113 MB） |
| `mirrors.huaweicloud.com/gradle/` | 3,324 KB/s |
| `mirrors.aliyun.com/gradle/` | 404，没有这个镜像 |

**所以正确做法是从腾讯云镜像手动下到 wrapper 期望的路径**：

    # 目录名是 distributionUrl 的哈希，从已有目录照抄即可
    D=~/.gradle/wrapper/dists/gradle-9.6.1-bin/<哈希>
    rm -f "$D"/*.part "$D"/*.lck          # 清掉失败留下的半成品，否则 wrapper 会接着它续
    curl -L -o "$D/gradle-9.6.1-bin.zip" \
      https://mirrors.cloud.tencent.com/gradle/gradle-9.6.1-bin.zip

zip 放对位置后 wrapper 直接解压，不再联网。**这台机器上已经放好了**，
所以 `networkTimeout` 那行其实只在「再换一台机器重装」时才起作用 ——
留着是给那时候的保险，顺手也让重跑 init 之后不至于又要重来一遍。

> 注意 `curl -sI https://services.gradle.org/distributions/` 返回 200 是**假象**：
> 那只是目录列表页能通，真正的 zip 会 307 跳到 downloads.gradle.org，那台是慢/不通的。
> 判断可用性必须**实际下一段**，不能只看 HEAD 状态码。
"""

import io
import os
import re
import sys

GOOGLE_MIRROR = "https://dl.google.com/dl/android/maven2"
REPLACEMENT = 'maven("%s")' % GOOGLE_MIRROR
# `fix_repos` 写进去的那行注释。镜像要插在它**前面**，这样注释各归其位：
# 镜像注释跟着镜像行，原注释继续跟着 dl.google.com 那行。
REPO_COMMENT = "// google() 在本机连不通，换成等价内容源（见本脚本顶部说明）"

# 阿里云镜像，插在仓库块最前面。顺序有意义：Gradle 按声明顺序试源，
# 慢的官方源留在后面只有当兜底的机会。
ALIYUN_MIRRORS = [
    "https://maven.aliyun.com/repository/google",
    "https://maven.aliyun.com/repository/central",
    "https://maven.aliyun.com/repository/gradle-plugin",
]
# 用第一个当「已经插过了」的探针。
MIRROR_PROBE = ALIYUN_MIRRORS[0]
MIRROR_COMMENT = "// 官方源实测只有几 KB/s，国内镜像优先（见本脚本顶部说明）"

# 仓库声明所在的两个文件
KTS_TARGETS = [
    "android/src-tauri/gen/android/build.gradle.kts",
    "android/src-tauri/gen/android/buildSrc/build.gradle.kts",
]

WRAPPER_PROPS = "android/src-tauri/gen/android/gradle/wrapper/gradle-wrapper.properties"
# 2 分钟。Gradle 官方文档里给慢网络的建议值，比默认的 10 秒宽裕得多。
NETWORK_TIMEOUT_MS = 120000


def repo_root():
    """从脚本位置往上找到仓库根（认 Cargo.toml + core/ 同时存在的那层）。"""
    here = os.path.dirname(os.path.abspath(__file__))
    cur = here
    for _ in range(6):
        if os.path.exists(os.path.join(cur, "Cargo.toml")) and os.path.isdir(
            os.path.join(cur, "core")
        ):
            return cur
        cur = os.path.dirname(cur)
    raise SystemExit("找不到仓库根（要从 android/tools/ 往上找到含 Cargo.toml + core/ 的目录）")


def fix_repos(root, dry):
    """把 google() 换成能连通的镜像。返回改了几处。"""
    changed = 0
    for rel in KTS_TARGETS:
        path = os.path.join(root, rel.replace("/", os.sep))
        if not os.path.exists(path):
            print("跳过（不存在）  %s" % rel)
            continue

        src = io.open(path, encoding="utf-8", newline="").read()
        # ⚠️ 只能数「整行就是 google()」的行。第一版写成了 src.count("google()")，
        # 结果把下面自己写进去的那行注释（「// google() 在本机连不通…」）也数上了，
        # 于是复跑时明明没改任何东西却报「已改 2 处」—— 幂等性检查因此形同虚设。
        n = len([l for l in src.split("\n") if l.strip() == "google()"])

        if n == 0:
            if GOOGLE_MIRROR in src:
                print("已就绪        %s" % rel)
            else:
                print("⚠️  没有 google() 也没有镜像 —— %s 的结构可能变了，请人工看一下" % rel)
            continue

        out = []
        for line in src.split("\n"):
            if line.strip() == "google()":
                indent = line[: len(line) - len(line.lstrip())]
                out.append("%s// google() 在本机连不通，换成等价内容源（见本脚本顶部说明）" % indent)
                out.append("%s%s" % (indent, REPLACEMENT))
            else:
                out.append(line)

        print("%-6s %s  （%d 处 google()）" % ("预览:" if dry else "已改:", rel, n))
        if not dry:
            io.open(path, "w", encoding="utf-8", newline="").write("\n".join(out))
        changed += n
    return changed


def fix_mirrors(root, dry):
    """在每个仓库块最前面插入阿里云镜像。返回插了几个块。

    Gradle 按声明顺序试源，所以必须插在**前面** —— 插后面等于没插。
    锚点优先用 `REPO_COMMENT`（它紧挨在 dl.google.com 那行上面），
    没有时才退而用 `REPLACEMENT` 那行，免得把注释和它说明的那行拆开。
    """
    changed = 0
    for rel in KTS_TARGETS:
        path = os.path.join(root, rel.replace("/", os.sep))
        if not os.path.exists(path):
            print("跳过（不存在）  %s" % rel)
            continue

        src = io.open(path, encoding="utf-8", newline="").read()
        if MIRROR_PROBE in src:
            print("已就绪        %s" % rel)
            continue

        # 整个文件只选一个锚点：两个锚点都用会把镜像插两遍。
        has_comment = any(l.strip() == REPO_COMMENT for l in src.split("\n"))
        anchor = REPO_COMMENT if has_comment else REPLACEMENT

        out = []
        blocks = 0
        for line in src.split("\n"):
            if line.strip() == anchor:
                indent = line[: len(line) - len(line.lstrip())]
                out.append("%s%s" % (indent, MIRROR_COMMENT))
                for m in ALIYUN_MIRRORS:
                    out.append('%smaven("%s")' % (indent, m))
                blocks += 1
            out.append(line)

        if blocks == 0:
            print("⚠️  找不到仓库锚点，跳过 —— %s 请人工看一下" % rel)
            continue

        print("%-6s %s  （%d 个仓库块）" % ("预览:" if dry else "已改:", rel, blocks))
        if not dry:
            io.open(path, "w", encoding="utf-8", newline="").write("\n".join(out))
        changed += blocks
    return changed


def fix_network_timeout(root, dry):
    """给 wrapper 补上 networkTimeout。返回改了几行。"""
    path = os.path.join(root, WRAPPER_PROPS.replace("/", os.sep))
    if not os.path.exists(path):
        print("跳过（不存在）  %s" % WRAPPER_PROPS)
        return 0

    src = io.open(path, encoding="utf-8", newline="").read()

    if "networkTimeout" in src:
        cur = re.search(r"networkTimeout=(\d+)", src)
        print("已就绪        %s  (networkTimeout=%s)" % (WRAPPER_PROPS, cur.group(1) if cur else "?"))
        return 0

    # 追加在 distributionUrl 后面，紧跟它更符合「读配置时一眼看到」的习惯。
    # 文件是 Java Properties 格式，末尾要有换行。
    if not src.endswith("\n"):
        src += "\n"
    src += (
        "# 默认 10 秒对本机太短：到 services.gradle.org 首次建连经常超时，\n"
        "# 而 wrapper 不重试（Attempt 1/1），会下到一半就整个失败。见本脚本顶部说明。\n"
        "networkTimeout=%d\n" % NETWORK_TIMEOUT_MS
    )

    print("%-6s %s  (补上 networkTimeout=%d)" % ("预览:" if dry else "已改:", WRAPPER_PROPS, NETWORK_TIMEOUT_MS))
    if not dry:
        io.open(path, "w", encoding="utf-8", newline="").write(src)
    return 1


def main():
    dry = "--dry" in sys.argv
    root = repo_root()

    print("=== 1. Gradle 仓库地址 ===")
    n1 = fix_repos(root, dry)
    print()
    print("=== 2. 国内镜像优先 ===")
    n2 = fix_mirrors(root, dry)
    print()
    print("=== 3. Gradle wrapper 网络超时 ===")
    n3 = fix_network_timeout(root, dry)

    print()
    total = n1 + n2 + n3
    if total == 0:
        print("无需改动，三处都已就绪。")
    elif dry:
        print("以上是预览，去掉 --dry 才会真正写入。")
    else:
        print("共改动 %d 处。接下来可以跑：" % total)
        print("  python android/tools/with_android_env.py cargo tauri android build --apk --target aarch64")


if __name__ == "__main__":
    main()
