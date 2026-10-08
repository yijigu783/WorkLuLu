"""1.5.0 实测：单实例 + 贴边自动隐藏。

沙箱里唯一可行的办法：PowerShell 的 Add-Type 被安全策略拦（要编译 .NET），
所以直接 ctypes 调 user32，用 SendInput / mouse_event 做**真实拖动**。

为什么不自己 MoveWindow 摆位置：那是另一条代码路径。上一版就是这么「验证通过」的
——外部瞬移时窗口左边缘正好落在 0，恰好满足当时那个「边缘正好贴住 ±8px」的判据；
而用户真实拖动时手抓在标题栏中间，松手瞬间窗口早就滑出屏幕一百多像素。
这里必须走真实输入。
"""
import ctypes
import ctypes.wintypes as wt
import json
import os
import subprocess
import sys
import time

user32 = ctypes.WinDLL("user32", use_last_error=True)

user32.FindWindowW.argtypes = [ctypes.c_wchar_p, ctypes.c_wchar_p]
user32.FindWindowW.restype = wt.HWND
user32.GetWindowRect.argtypes = [wt.HWND, ctypes.POINTER(wt.RECT)]
user32.SetCursorPos.argtypes = [ctypes.c_int, ctypes.c_int]
user32.GetCursorPos.argtypes = [ctypes.POINTER(wt.POINT)]
user32.SetForegroundWindow.argtypes = [wt.HWND]
user32.mouse_event.argtypes = [wt.DWORD, wt.DWORD, wt.DWORD, wt.DWORD, ctypes.c_void_p]

TITLE = "工作记录本 WorkLuLu"
LEFTDOWN, LEFTUP = 0x0002, 0x0004
EXE_DIR = os.path.dirname(os.path.abspath(__file__))
EXE = os.path.join(EXE_DIR, "WorkLuLu.exe")


def pause(sec=0.4):
    time.sleep(sec)


def rect_of(hwnd):
    r = wt.RECT()
    user32.GetWindowRect(hwnd, ctypes.byref(r))
    return {"l": r.left, "t": r.top, "w": r.right - r.left, "h": r.bottom - r.top}


def hwnd_of():
    return user32.FindWindowW(None, TITLE)


def cursor():
    p = wt.POINT()
    user32.GetCursorPos(ctypes.byref(p))
    return (p.x, p.y)


def move_cursor(x, y, steps=25):
    """分步移动。一步到位会让拖动逻辑来不及处理中间位置。"""
    x0, y0 = cursor()
    for i in range(1, steps + 1):
        user32.SetCursorPos(int(x0 + (x - x0) * i / steps), int(y0 + (y - y0) * i / steps))
        time.sleep(0.012)


def drag(grab, target):
    """在 `grab`（标题栏）按住左键，拖到 `target` 再松开 —— 模拟用户抓标题栏拖窗口。"""
    move_cursor(*grab)
    pause(0.25)
    user32.mouse_event(LEFTDOWN, 0, 0, 0, None)
    pause(0.2)
    move_cursor(*target)
    pause(0.3)
    user32.mouse_event(LEFTUP, 0, 0, 0, None)
    pause(0.3)


def running_count():
    out = subprocess.run(
        ["tasklist", "/FI", "IMAGENAME eq WorkLuLu.exe", "/FO", "CSV", "/NH"],
        capture_output=True, text=True, errors="replace",
    ).stdout
    return sum(1 for line in out.splitlines() if line.strip().startswith('"'))


def launch():
    return subprocess.Popen([EXE], cwd=EXE_DIR)


def main():
    global _DRAG_FROM
    report = {}

    # ---------- 启动 ----------
    if not hwnd_of():
        report["pid"] = launch().pid
    for _ in range(40):
        h = hwnd_of()
        if h:
            break
        time.sleep(0.5)
    hwnd = hwnd_of()
    if not hwnd:
        print(json.dumps({"fatal": "找不到窗口，程序没起来"}, ensure_ascii=False))
        return
    user32.SetForegroundWindow(hwnd)
    pause(0.5)
    report["启动后"] = rect_of(hwnd)
    report["进程数"] = running_count()

    # ---------- 单实例：再点一次 exe ----------
    second = launch()
    try:
        code = second.wait(timeout=8)
    except subprocess.TimeoutExpired:
        code = "还活着（超时）"
    pause(1.0)
    report["第二次点击的退出码"] = code
    report["第二次之后进程数"] = running_count()

    # ---------- 贴边：真实拖动到屏幕左边 ----------
    r = report["启动后"]
    grab = (r["l"] + 200, r["t"] + 18)          # 标题栏（不靠边的位置，抓得稳）
    report["抓取点"] = grab
    report["屏幕宽"] = user32.GetSystemMetrics(0)
    drag(grab, (0, r["t"] + 18))                # 鼠标一路顶到屏幕左缘
    pause(0.4)
    report["拖到左缘后光标"] = cursor()
    report["拖到左缘后(松手瞬间)"] = rect_of(hwnd)
    pause(1.5)
    report["停稳 1.5 秒后"] = rect_of(hwnd)

    # ---------- 鼠标挪开（布防），再碰回左缘 → 应该滑出来 ----------
    move_cursor(800, 500)
    pause(0.6)
    report["鼠标挪到中间后"] = rect_of(hwnd)
    move_cursor(2, 500)
    pause(0.8)
    report["碰回左缘后"] = rect_of(hwnd)
    report["碰回左缘后光标"] = cursor()

    # ---------- 鼠标离开窗口 → 应该自己收回去 ----------
    move_cursor(1400, 500)
    pause(1.5)
    report["鼠标走开 1.5 秒后"] = rect_of(hwnd)
    report["进程数(末)"] = running_count()

    print(json.dumps(report, ensure_ascii=False, indent=2))
    try:
        second.kill()
    except Exception:
        pass


_DRAG_FROM = (0, 0)
if __name__ == "__main__":
    sys.exit(main())
