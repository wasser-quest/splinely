"""Opt-in X11 native UI smoke: build first, then python3 this_file [binary].
Requires xdotool, ImageMagick import, Pillow, and a working DISPLAY/audio backend.
This launches the actual binary and checks rendered colors after real pointer input.
"""
import argparse
import base64
import ctypes
import os
import pathlib
import signal
import subprocess
import sys
import tempfile
import time
from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parents[3]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", nargs="?", default=str(ROOT / "target/debug/splinely-viewer"))
parser.add_argument("--svg", action="store_true", help="launch the exported SVG rather than the project")
parser.add_argument("--crash", action="store_true", help="verify Linux renderer termination reporting")
args = parser.parse_args()
BINARY = pathlib.Path(args.binary)
FIXTURE = pathlib.Path(__file__).parent / "fixtures/native-smoke.splinely.json"


def command(*args):
    return subprocess.check_output([str(x) for x in args], text=True).strip()


with tempfile.TemporaryDirectory(prefix="splinely-native-smoke-") as tmp:
    log = open(pathlib.Path(tmp) / "native.log", "w+")
    source = FIXTURE
    if args.svg:
        wrapper = pathlib.Path(tmp) / "export.html"
        command(BINARY, FIXTURE, "--trusted-js", "--check", "--output", wrapper)
        encoded = wrapper.read_text().split('src="data:image/svg+xml;base64,')[1].split('"')[0]
        source = pathlib.Path(tmp) / "export.svg"
        source.write_bytes(base64.b64decode(encoded))
    process = subprocess.Popen([str(BINARY), str(source), "--trusted-js"], stdout=log, stderr=log)
    try:
        window = None
        for _ in range(100):
            if process.poll() is not None:
                log.seek(0)
                raise RuntimeError(log.read())
            found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(process.pid), "--name", "Splinely Viewer"], capture_output=True, text=True)
            if found.returncode == 0:
                window = found.stdout.splitlines()[-1]
                break
            time.sleep(0.1)
        assert window, "native window did not appear"
        command("xdotool", "windowraise", window)
        command("xdotool", "windowfocus", "--sync", window)
        command("xdotool", "windowsize", "--sync", window, 1000, 720)
        command("xdotool", "mousemove", "--window", window, 950, 680)
        time.sleep(3)

        def pixel(x, y):
            time.sleep(0.5)
            screenshot = pathlib.Path(tmp) / "frame.png"
            command("import", "-window", window, screenshot)
            return Image.open(screenshot).convert("RGB").getpixel((x, y))

        def expect(x, y, rgb, stage):
            actual = pixel(x, y)
            assert all(abs(a-b) < 20 for a, b in zip(actual, rgb)), (stage, actual, rgb)
            print("PASS:", stage)

        expect(500, 360, (255, 0, 0), "initial runtime state")
        expect(500, 612, (255, 0, 255), "trusted customJS executed")
        command("xdotool", "mousemove", "--window", window, 500, 360)
        expect(500, 360, (0, 255, 0), "hover state")
        command("xdotool", "click", 1)
        expect(500, 360, (0, 0, 255), "click state")
        command("xdotool", "mousemove", "--window", window, 120, 50)
        command("xdotool", "click", 1)
        time.sleep(2)
        command("xdotool", "mousemove", "--window", window, 500, 360)
        command("xdotool", "click", 1)
        expect(500, 612, (0, 255, 255), "Web Audio unlocked and embedded WAV scheduled")
        command("xdotool", "windowsize", "--sync", window, 800, 600)
        expect(400, 300, (0, 0, 255), "native resize retains interactive SVG")
        if args.crash:
            rows = [line.split(None, 2) for line in command("ps", "-eo", "pid,ppid,comm").splitlines()[1:]]
            descendants = {process.pid}
            for _ in range(20):
                descendants.update(int(pid) for pid, parent, name in rows if int(parent) in descendants)
            renderer = next(int(pid) for pid, parent, name in rows if int(pid) in descendants and "WebKitWeb" in name)
            os.kill(renderer, signal.SIGKILL)
            assert process.wait(timeout=15) == 1
            log.seek(0)
            assert "renderer process terminated" in log.read()
            print("PASS: renderer crash reported with failing exit status")
            sys.exit(0)
        # Send WM_DELETE_WINDOW, not xdotool windowclose (which destroys the
        # X window and can cause a GTK BadWindow error without a window manager).
        class MessageData(ctypes.Union):
            _fields_ = [("l", ctypes.c_long * 5)]
        class ClientMessage(ctypes.Structure):
            _fields_ = [("type", ctypes.c_int), ("serial", ctypes.c_ulong),
                        ("send_event", ctypes.c_int), ("display", ctypes.c_void_p),
                        ("window", ctypes.c_ulong), ("message_type", ctypes.c_ulong),
                        ("format", ctypes.c_int), ("data", MessageData)]
        class XEvent(ctypes.Union):
            _fields_ = [("message", ClientMessage), ("pad", ctypes.c_long * 24)]
        xlib = ctypes.CDLL("libX11.so.6")
        xlib.XOpenDisplay.restype = ctypes.c_void_p
        xlib.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
        xlib.XInternAtom.restype = ctypes.c_ulong
        xlib.XSendEvent.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_long, ctypes.POINTER(XEvent)]
        xlib.XFlush.argtypes = [ctypes.c_void_p]
        xlib.XCloseDisplay.argtypes = [ctypes.c_void_p]
        display = xlib.XOpenDisplay(None)
        event = XEvent()
        event.message.type = 33
        event.message.display = display
        event.message.window = int(window)
        event.message.message_type = xlib.XInternAtom(display, b"WM_PROTOCOLS", 0)
        event.message.format = 32
        event.message.data.l[0] = xlib.XInternAtom(display, b"WM_DELETE_WINDOW", 0)
        xlib.XSendEvent(display, int(window), 0, 0, ctypes.byref(event))
        xlib.XFlush(display)
        xlib.XCloseDisplay(display)
        assert process.wait(timeout=10) == 0
        print("PASS: native close")
    finally:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=10)
        log.close()
