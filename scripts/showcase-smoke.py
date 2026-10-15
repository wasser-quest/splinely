#!/usr/bin/env python3
"""Real Firefox integration test for the two offline showcase HTML wrappers."""
import argparse
import base64
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import threading
import time
import traceback
from urllib.parse import quote

from firefox_harness import Firefox

# Observe the actual exported instance; never click, drive its clock, emit
# transitions or call Start from JS: the showcases must start by themselves.
DOCUMENT = """
const object=document.querySelector(arguments[0]);
if(!object || !object.contentDocument) return null;
const doc=object.contentDocument, root=doc.documentElement, f=root.splinely;
if(!f) return null;
"""
INSTALL = DOCUMENT + """
const w=doc.defaultView, p=f.sourceProject;
if(!w.__showcaseSmoke) {
 const t=w.__showcaseSmoke={states:[],errors:[],clicks:[],installedAt:performance.now()};
 w.addEventListener('error',e=>t.errors.push({type:'error',message:e.message,source:e.filename,line:e.lineno}));
 w.addEventListener('unhandledrejection',e=>t.errors.push({type:'unhandledrejection',message:String(e.reason)}));
 w.addEventListener('click',e=>t.clicks.push({trusted:e.isTrusted,target:e.target.id,host:!!e.target.closest('[data-splinely-host-control]')}),true);
 f.on('click',e=>{if(e.event)t.clicks.push({trusted:e.event.isTrusted,target:e.target,host:false});});
 f.on('state.change',e=>t.states.push({at:f.timeline.time,...e}));
}
return {mounted:true,
 project:p,control:!!doc.querySelector('[data-splinely-host-control]')};
"""
SNAPSHOT = DOCUMENT + """
const p=f.sourceProject, t=doc.defaultView.__showcaseSmoke;
return {time:f.timeline.time,playing:f.timeline.playing,
 machines:(p.stateMachines||p.machines||[]).map(m=>({id:m.id,state:f.machine(m.id).state})),
 attributes:p.nodes.map(n=>{const el=doc.getElementById(n.id);return {id:n.id,attrs:el?Object.fromEntries([...el.attributes].map(a=>[a.name,a.value])):null};}),
 telemetry:t,diagnostics:f.diagnostics.map(e=>({message:e.message||String(e),stack:e.stack||null})),control:!!doc.querySelector('[data-splinely-host-control]')};
"""


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


def wait_for(browser, script, selector, timeout):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = browser.js(script, [selector])
        if result:
            return result
        time.sleep(.1)
    raise RuntimeError("HTML object SVG mount timed out")


def run_case(browser, url, kind, args, result):
    browser.command("WebDriver:Navigate", {"url": url})
    result["initial"] = wait_for(browser, INSTALL, args.object_selector, args.mount_timeout)
    initial = result["initial"]
    project = initial["project"]
    expected = {m["id"]: {s["id"] for s in m["states"]} for m in project.get("stateMachines", project.get("machines", []))}
    seen = {m["id"]: set() for m in project.get("stateMachines", project.get("machines", []))}
    result["expectedStates"] = {key: sorted(value) for key, value in expected.items()}
    result["before"] = browser.js(SNAPSHOT, [args.object_selector])
    for machine in result["before"]["machines"]:
        seen[machine["id"]].add(machine["state"])
    # No Play button exists and nothing is pressed: only wall-clock observation follows.
    result["hostButtons"] = browser.js("return document.querySelectorAll('button,[role=button]').length;")
    samples = result["samples"] = []
    deadline = time.monotonic() + args.duration
    changed = False
    while time.monotonic() < deadline:
        time.sleep(args.interval)
        snapshot = browser.js(SNAPSHOT, [args.object_selector])
        if not snapshot:
            raise AssertionError("SVG runtime disappeared")
        samples.append(snapshot)
        before_nodes = {n["id"]: n["attrs"] for n in result["before"]["attributes"]}
        animated = args.water_attribute or ["transform", "x", "y", "cx", "cy", "r", "d", "opacity"]
        changes = [n["id"] for n in snapshot["attributes"] if n["attrs"] is not None
                   and before_nodes.get(n["id"]) is not None
                   and (kind != "water" or not args.water_node or n["id"] in args.water_node)
                   and any(n["attrs"].get(key) != before_nodes[n["id"]].get(key) for key in animated)]
        result["changedNodes"] = sorted(set(result.get("changedNodes", [])) | set(changes))
        changed |= bool(changes)
        for event in snapshot["telemetry"]["states"]:
            seen.setdefault(event["machine"], set()).update([event["from"], event["to"]])
        for machine in snapshot["machines"]:
            seen.setdefault(machine["id"], set()).add(machine["state"])
        # At least three wall-clock observations, no seeking or synthetic machine input.
        complete = kind == "water" or (bool(expected) and all(states <= seen[key] for key, states in expected.items()))
        if len(samples) >= 3 and changed and complete:
            break
    last = samples[-1]
    result["observedStates"] = {key: sorted(value) for key, value in seen.items()}
    result["attributeChanges"] = changed
    checks = result["checks"] = {
        "mountedInHtmlObject": initial["mounted"],
        "noHostButtons": result["hostButtons"] == 0 and not any(
            n["id"].endswith("-control") for n in project["nodes"]),
        "noInputWasSent": not last["telemetry"]["clicks"],
        "startedWithoutInput": last["playing"] and last["time"] > result["before"]["time"] and not last["control"],
        "domAttributesChanged": changed,
        # Runtime emits this advisory before the reviewed export host runs customJs.
        # Retain it in the JSON; fail every other diagnostic, including host errors.
        "noUnexpectedRuntimeDiagnostics": not any(d["message"] !=
            "customJs is preserved but not executed; attach trusted handlers explicitly"
            for d in last["diagnostics"]),
        "noObservedWindowErrors": not last["telemetry"]["errors"],
    }
    if kind == "random":
        checks["allStatesAutomaticallyVisited"] = bool(expected) and all(states <= seen[key] for key, states in expected.items())
        checks["stateChangeEventsRecorded"] = bool(last["telemetry"]["states"])
    if not all(checks.values()):
        raise AssertionError("Failed checks: " + ", ".join(key for key, value in checks.items() if not value))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("water", nargs="?", default="examples/water-drop.html")
    parser.add_argument("random", nargs="?", default="examples/random-figures.html")
    parser.add_argument("--root", default=".", help="HTTP document root containing both wrappers and their assets")
    parser.add_argument("--object-selector", default="object[type='image/svg+xml']")
    parser.add_argument("--water-node", action="append", help="water animated SVG node ID (repeatable)")
    parser.add_argument("--water-attribute", action="append", help="animated water attribute to compare (repeatable)")
    parser.add_argument("--duration", type=float, default=30, help="maximum seconds of automatic playback per showcase")
    parser.add_argument("--interval", type=float, default=.25)
    parser.add_argument("--mount-timeout", type=float, default=10)
    args = parser.parse_args()
    if args.duration <= 0 or args.interval <= 0 or args.mount_timeout <= 0:
        parser.error("timeouts and interval must be positive")
    report = {"passed": False, "cases": {}, "errors": [], "unproven": [
        "Offline file:// mounting and non-Firefox browser compatibility",
        "Errors before telemetry attachment, unless retained by runtime diagnostics"],
        "screenshot": "/tmp/splinely-showcase.png", "screenshotCaptured": False}
    server = None
    browser = None
    try:
        root = Path(args.root).resolve()
        paths = [Path(p).resolve() for p in (args.water, args.random)]
        for path in paths:
            if not path.is_file():
                raise FileNotFoundError(path)
            path.relative_to(root)
        server = ThreadingHTTPServer(("127.0.0.1", 0), partial(QuietHandler, directory=str(root)))
        server.daemon_threads = True
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        report["origin"] = f"http://127.0.0.1:{server.server_port}"
        with Firefox() as browser:
            for kind, path in zip(("water", "random"), paths):
                result = report["cases"][kind] = {"path": str(path), "passed": False}
                try:
                    run_case(browser, report["origin"] + "/" + quote(path.relative_to(root).as_posix()), kind, args, result)
                    result["passed"] = True
                except Exception:
                    result["error"] = traceback.format_exc()
                    try:
                        result["failureSnapshot"] = browser.js(SNAPSHOT, [args.object_selector])
                    except Exception as error:
                        result["snapshotError"] = str(error)
                finally:
                    try:
                        # Preserve a failing case over a subsequent passing case.
                        if not result["passed"] or not any(not c["passed"] for c in report["cases"].values()):
                            screenshot = browser.command("WebDriver:TakeScreenshot", {"full": False})
                            Path(report["screenshot"]).write_bytes(base64.b64decode(screenshot))
                            report["screenshotCase"] = kind
                            report["screenshotCaptured"] = True
                    except Exception as error:
                        report["errors"].append("Screenshot: " + str(error))
            report["firefoxLog"] = browser.diagnostics()
            report["passed"] = all(case["passed"] for case in report["cases"].values()) and not report["errors"]
    except Exception:
        report["errors"].append(traceback.format_exc())
    finally:
        if server:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)
        Path("/tmp/splinely-showcase-results.json").write_text(json.dumps(report, indent=2) + "\n")
    print(("PASS" if report["passed"] else "FAIL") + ": /tmp/splinely-showcase-results.json; /tmp/splinely-showcase.png")
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
