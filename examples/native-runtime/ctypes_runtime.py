"""ctypes host adapter: consumes native JSON; it never executes project JavaScript."""
import argparse
import ctypes
import json
from pathlib import Path


class Runtime:
    def __init__(self, library, project):
        self.lib = ctypes.CDLL(str(Path(library).resolve()))
        self.lib.forma_runtime_create.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.POINTER(ctypes.c_void_p)]
        self.lib.forma_runtime_create.restype = ctypes.c_void_p
        self.lib.forma_runtime_request.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_size_t]
        self.lib.forma_runtime_request.restype = ctypes.c_void_p
        self.lib.forma_runtime_destroy.argtypes = [ctypes.c_void_p]
        self.lib.forma_runtime_destroy.restype = None
        self.lib.forma_runtime_result_free.argtypes = [ctypes.c_void_p]
        self.lib.forma_runtime_result_free.restype = None
        self.handle = ctypes.c_void_p()
        data = json.dumps(project, ensure_ascii=False).encode("utf-8")
        self.initial = self._result(self.lib.forma_runtime_create(data, len(data), ctypes.byref(self.handle)))

    def _result(self, pointer):
        if not pointer:
            raise RuntimeError("native library returned NULL")
        try:
            response = json.loads(ctypes.string_at(pointer).decode("utf-8"))
        finally:
            # Keep the raw pointer, NOT c_char_p's automatically converted Python bytes.
            self.lib.forma_runtime_result_free(pointer)
        if not response["ok"]:
            raise RuntimeError(response["error"])
        return response["result"]

    def request(self, request):
        if not self.handle.value:
            raise RuntimeError("runtime is closed")
        data = json.dumps(request, ensure_ascii=False).encode("utf-8")
        return self._result(self.lib.forma_runtime_request(self.handle, data, len(data)))

    def close(self):
        if self.handle.value:
            self.lib.forma_runtime_destroy(self.handle)
            self.handle = ctypes.c_void_p()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("library", help="path to libforma_runtime.so/.dylib or forma_runtime.dll")
    parser.add_argument("project", nargs="?", default=str(Path(__file__).with_name("demo.forma.json")))
    args = parser.parse_args()
    with Runtime(args.library, json.loads(Path(args.project).read_text(encoding="utf-8"))) as runtime:
        print(json.dumps(runtime.initial))
        print(json.dumps(runtime.request({"type": "tick", "delta": 0.5})))
        # Input targeting is a host responsibility; there is no native hit testing.
        print(json.dumps(runtime.request({"type": "event", "event": {"name": "click", "target": "box"}})))
        print(json.dumps(runtime.request({"type": "tick", "delta": 0.5})))
