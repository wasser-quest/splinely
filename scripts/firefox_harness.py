"""Standard-library Firefox Marionette harness (no geckodriver required)."""
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time


class Firefox:
    def __enter__(self):
        self.profile = tempfile.TemporaryDirectory(prefix="forma-showcase-firefox-")
        self.log = tempfile.TemporaryFile()
        self.connection = None
        self.sequence = 0
        with socket.socket() as reservation:
            reservation.bind(("127.0.0.1", 0))
            port = reservation.getsockname()[1]
        Path(self.profile.name, "user.js").write_text(
            f'user_pref("marionette.port", {port});\n'
            'user_pref("browser.shell.checkDefaultBrowser", false);\n'
        )
        self.process = subprocess.Popen(
            [os.environ.get("FIREFOX", "firefox"), "--headless", "--no-remote",
             "--marionette", "--profile", self.profile.name],
            env={**os.environ, "MOZ_HEADLESS": "1", "MOZ_DISABLE_GPU_ACCEL": "1"},
            stdout=self.log, stderr=self.log,
        )
        try:
            deadline = time.monotonic() + 25
            while time.monotonic() < deadline:
                try:
                    self.connection = socket.create_connection(("127.0.0.1", port), timeout=1)
                    break
                except OSError:
                    if self.process.poll() is not None:
                        raise RuntimeError("Firefox exited before Marionette startup")
                    time.sleep(.1)
            if self.connection is None:
                raise RuntimeError("Firefox Marionette startup timed out")
            self.connection.settimeout(20)
            self.receive()
            self.command("WebDriver:NewSession", {"capabilities": {"alwaysMatch": {"browserName": "firefox"}}})
            return self
        except BaseException as error:
            diagnostics = self.diagnostics()
            self.close()
            raise RuntimeError(f"{error}\nFirefox startup diagnostics:\n{diagnostics}") from error

    def receive(self):
        size = b""
        while not size.endswith(b":"):
            part = self.connection.recv(1)
            if not part:
                raise RuntimeError("Marionette disconnected")
            size += part
        remaining = int(size[:-1])
        payload = b""
        while remaining:
            part = self.connection.recv(remaining)
            if not part:
                raise RuntimeError("Marionette disconnected")
            payload += part
            remaining -= len(part)
        return json.loads(payload)

    def command(self, name, parameters=None):
        self.sequence += 1
        payload = json.dumps([0, self.sequence, name, parameters or {}]).encode()
        self.connection.sendall(str(len(payload)).encode() + b":" + payload)
        reply = self.receive()
        if reply[1] != self.sequence or reply[2] is not None:
            raise RuntimeError(f"{name}: {reply}")
        result = reply[3]
        return result.get("value", result) if isinstance(result, dict) else result

    def js(self, script, args=None):
        return self.command("WebDriver:ExecuteScript", {
            "script": script, "args": args or [], "newSandbox": False,
        })

    def diagnostics(self):
        self.log.flush()
        self.log.seek(0)
        return self.log.read().decode(errors="replace")

    def close(self):
        if self.connection:
            self.connection.close()
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
        self.log.close()
        self.profile.cleanup()

    def __exit__(self, *exc):
        self.close()
