"""Real shared-library + subprocess smoke; requires artifacts built by cargo build."""
import argparse
import json
from pathlib import Path
import subprocess
from ctypes_runtime import Runtime


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--library", required=True)
    parser.add_argument("--binary", required=True)
    args = parser.parse_args()
    project = json.loads(Path(__file__).with_name("demo.splinely.json").read_text(encoding="utf-8"))
    requests = [
        {"type": "tick", "delta": 0.5},
        {"type": "event", "event": {"name": "click", "target": "box"}},
        {"type": "schedule", "id": "hostCustom", "time": 0.75, "data": {"message": "你好"}},
        {"type": "tick", "delta": 0.5},
        {"type": "evaluate"},
    ]
    with Runtime(args.library, project) as runtime:
        results = [runtime.initial] + [runtime.request(r) for r in requests]
        assert results[1]["frame"]["bones"][0]["x"] == 5
        assert results[2]["frame"]["states"]["main"] == "talking"
        assert results[2]["hostActions"][1]["type"] == "speak"
        assert results[4]["frame"]["clips"][0]["weight"] == 0.5
        assert results[4]["frame"]["nodes"][0]["transform"]["x"] == 5
        assert results[4]["hostActions"] == [{"type": "callback", "id": "hostCustom", "data": {"message": "你好"}}]
        assert results[5]["hostActions"] == []
        try:
            runtime.request({"type": "tick", "delta": -1})
            raise AssertionError("invalid delta accepted")
        except RuntimeError:
            pass
        assert runtime.request({"type": "evaluate"}) == results[5]
        # Exercise invalid UTF-8 on the actual C ABI, not a mocked adapter.
        try:
            runtime._result(runtime.lib.splinely_runtime_request(runtime.handle, b"\xff", 1))
            raise AssertionError("invalid UTF-8 accepted")
        except RuntimeError as exc:
            assert "UTF-8" in str(exc)
        with Runtime(args.library, project) as independent:
            assert independent.initial["frame"]["time"] == 0
    runtime.close()  # Python-side close is idempotent; native destroy is exactly once.
    lines = [{"type": "load", "project": project}] + requests
    run = subprocess.run([args.binary], input="\n".join(json.dumps(r, ensure_ascii=False) for r in lines) + "\n", text=True, encoding="utf-8", capture_output=True, check=True)
    responses = [json.loads(line) for line in run.stdout.splitlines()]
    assert all(r["ok"] for r in responses), responses
    assert [r["result"] for r in responses] == results
    assert run.stderr == "", run.stderr
    print("PASS: real ctypes library, ordered state/speech output, fades/bones, callback UTF-8, rollback, independent handles, JSON-lines parity")


if __name__ == "__main__":
    main()
