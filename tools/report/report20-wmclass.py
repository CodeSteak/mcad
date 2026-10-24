#!/usr/bin/env python3
"""Test report 20: WM_CLASS of the windows of the desktop app under Xvfb (without x11-utils).

Starts `cad` (with UI) in an Xvfb, waits for its window and prints name and WM_CLASS of every
top-level window via libX11 (ctypes). GTK uses the same program name as the Wayland app_id.

    python3 tools/report/report20-wmclass.py target/release/cad > reports/tmp/report20/wmclass.txt
"""
import ctypes
import ctypes.util
import os
import subprocess
import sys
import tempfile
import time

cad = sys.argv[1] if len(sys.argv) > 1 else "target/release/cad"
disp = ":%d" % (90 + os.getpid() % 50)
xvfb = subprocess.Popen(["Xvfb", disp, "-screen", "0", "1440x900x24", "-nolisten", "tcp"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(1.0)
env = dict(os.environ, DISPLAY=disp, CAD_NO_DAEMON="1", CAD_IPC_DIR=tempfile.mkdtemp(prefix="wm"))
model = os.path.join(tempfile.mkdtemp(prefix="wm"), "teil.scad")
with open(model, "w") as f:
    f.write("cube(10);\n")
app = subprocess.Popen([cad, model], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

x = ctypes.cdll.LoadLibrary(ctypes.util.find_library("X11"))


class ClassHint(ctypes.Structure):
    _fields_ = [("res_name", ctypes.c_char_p), ("res_class", ctypes.c_char_p)]


x.XOpenDisplay.restype = ctypes.c_void_p
x.XDefaultRootWindow.restype = ctypes.c_ulong
x.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
x.XQueryTree.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.POINTER(ctypes.c_ulong)), ctypes.POINTER(ctypes.c_uint)]
x.XGetClassHint.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ClassHint)]
x.XFetchName.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_char_p)]

found = []
try:
    d = x.XOpenDisplay(disp.encode())
    root = x.XDefaultRootWindow(d)
    for _ in range(60):
        time.sleep(0.5)
        r, p = ctypes.c_ulong(), ctypes.c_ulong()
        kids = ctypes.POINTER(ctypes.c_ulong)()
        n = ctypes.c_uint()
        x.XQueryTree(d, root, ctypes.byref(r), ctypes.byref(p), ctypes.byref(kids), ctypes.byref(n))
        found = []
        for i in range(n.value):
            h = ClassHint()
            if x.XGetClassHint(d, kids[i], ctypes.byref(h)):
                name = ctypes.c_char_p()
                x.XFetchName(d, kids[i], ctypes.byref(name))
                found.append((hex(kids[i]), (name.value or b"").decode(errors="replace"), h.res_name.decode(), h.res_class.decode()))
        if any(f[1] for f in found):
            break
finally:
    app.terminate()
    try:
        app.wait(5)
    except subprocess.TimeoutExpired:
        app.kill()
    xvfb.terminate()

print("Fenster (xid, Titel, WM_CLASS res_name, res_class) unter Xvfb:")
for f in found:
    print(f"  {f[0]}  {f[1]!r:40}  {f[2]!r}  {f[3]!r}")
ok = any(f[2] == "dev.controller-cad.desktop" for f in found)
desktop = "packaging/linux/dev.controller-cad.desktop.desktop"
print(f"Desktop-Datei {desktop}: {'vorhanden' if os.path.exists(desktop) else 'fehlt'}")
print("WM_CLASS = App-ID = Name der Desktop-Datei:", "ja" if ok else "NEIN")
sys.exit(0 if ok else 1)
