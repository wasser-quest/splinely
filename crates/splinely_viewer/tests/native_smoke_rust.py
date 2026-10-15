"""Opt-in X11 smoke for `--runtime rust`: build first, then python3 this_file [binary].
Needs xdotool, ImageMagick `import`, Pillow and a working DISPLAY. Launches the real binary on
examples/demo.splinely.json, clicks the circle and checks that the Rust runtime moved and
recoloured it in the webview (frame applier + IPC round trip), and that the diagnostic is shown.
"""
import pathlib
import subprocess
import sys
import tempfile
import time
from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parents[3]
BINARY = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "target/debug/splinely-viewer"
PROJECT = ROOT / "examples/demo.splinely.json"


def run(*args):
    return subprocess.check_output([str(a) for a in args], text=True).strip()


def circle_columns(path):
    im = Image.open(path).convert("RGB")
    xs = [x for x in range(0, im.width, 2) for y in range(100, 700, 6)
          if im.getpixel((x, y))[0] > 200 and im.getpixel((x, y))[2] < 120]
    return (min(xs), max(xs)) if xs else None


with tempfile.TemporaryDirectory(prefix="splinely-rust-smoke-") as tmp:
    log = open(pathlib.Path(tmp) / "viewer.log", "w+")
    process = subprocess.Popen([str(BINARY), str(PROJECT), "--runtime", "rust"], stdout=log, stderr=log)
    try:
        window = None
        for _ in range(100):
            assert process.poll() is None, "viewer exited early"
            found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(process.pid), "--name", "Splinely Viewer"], capture_output=True, text=True)
            if found.returncode == 0:
                window = found.stdout.splitlines()[-1]
                break
            time.sleep(0.1)
        assert window, "native window did not appear"
        run("xdotool", "windowsize", "--sync", window, 1000, 720)
        time.sleep(2)
        run("xdotool", "mousemove", "--window", window, 125, 360, "click", 1)
        time.sleep(1.2)  # the clip moves the circle to the right and recolours it (orange)
        after = pathlib.Path(tmp) / "after.png"
        run("import", "-window", window, after)
        columns = circle_columns(after)
        assert columns and columns[0] > 500, f"circle did not move after the click: {columns}"
        log.seek(0)
        text = log.read()
        assert "customJs" in text and "NOT executed" in text, text
        print("rust-mode smoke ok", columns)
    finally:
        process.terminate()
        process.wait(timeout=10)
