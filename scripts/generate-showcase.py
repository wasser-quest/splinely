#!/usr/bin/env python3
"""Generate canonical projects and reviewed, explicitly opted-in SVG showcases via CLI."""
import argparse
import html
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent

TEMPLATE = r"""<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>@@TITLE@@ — Splinely</title>
<style>
:root{color-scheme:dark;font-family:system-ui,sans-serif;background:#081321;color:#edf6ff}
body{max-width:1100px;margin:36px auto;padding:0 22px}header{display:flex;justify-content:space-between;align-items:center;gap:20px}
h1{font-size:clamp(24px,4vw,38px);letter-spacing:-.04em;margin:8px 0}p{color:#a9bed2;line-height:1.6}
.badge{color:#89e1e9;font-size:12px;letter-spacing:.16em}object{display:block;width:100%;aspect-ratio:3/2;border:1px solid #29425c;border-radius:24px;background:#0c172b;box-shadow:0 24px 80px #0005}
.controls{display:flex;align-items:center;gap:12px;flex-wrap:wrap;margin:22px 0}a{font:inherit;color:#e6faff}a:focus-visible{outline:3px solid #7ee7ef;outline-offset:4px}
output{color:#8ce0de;font-size:14px}section{border:1px solid #263b52;border-radius:16px;padding:16px 22px;margin-top:18px}h2{font-size:16px}pre{white-space:pre-wrap;color:#adc5dc;font-size:13px;max-height:170px;overflow:auto}a{color:#8edce8}
</style></head><body>
<header><div><div class="badge">SPLINELY / AUTONOMOUS AUTHORING</div><h1>@@TITLE@@</h1></div><a href="@@STEM@@.splinely.json">Project JSON</a></header>
<p>@@DESCRIPTION@@</p>
<object id="scene" type="image/svg+xml" data="@@STEM@@.svg" aria-label="@@TITLE@@ interactive animation">
<a href="@@STEM@@.svg">Open the interactive SVG directly</a>
</object>
<div class="controls"><output id="status" aria-live="polite">Loading local SVG…</output></div>
<section><h2>Runtime observation</h2><pre id="events">Waiting for instance…</pre></section>
<p>SVG scripts require a script-capable browser context, not an img. Resources are local and relative.
If file-origin restrictions prevent observation, serve this directory over localhost (for example <code>python3 -m http.server</code>).
The state cycle follows animation time.</p>
<script>
'use strict';
const object=document.getElementById('scene'), status=document.getElementById('status'), log=document.getElementById('events');
let splinely=null, data=null, timer=null, off=[], lines=[];
function detach(){clearInterval(timer);for(const unsubscribe of off)unsubscribe();off=[];splinely=null;}
function refresh(){
  if(!splinely || splinely.destroyed)return;
  const states=data.stateMachines.map(m=>m.id+': '+splinely.machine(m.id).state).join(' · ');
  status.textContent=(states || 'Water drop')+' · '+(splinely.timeline.playing?'playing':'paused')+' · '+splinely.timeline.time.toFixed(1)+'s';
  const notes=splinely.diagnostics.map(d=>d.message).filter(m=>m!=='customJs is preserved but not executed; attach trusted handlers explicitly');
  log.textContent=(lines.join('\n') || 'Mounted — the animation starts by itself.')+(notes.length?'\nDiagnostics: '+notes.slice(-4).join(' | '):'');
}
function attach(){
  detach();lines=[];
  try{
    const svg=object.contentDocument.documentElement;
    splinely=svg.splinely;if(!splinely)throw Error('SVG runtime unavailable (scripts blocked?)');
    data=splinely.sourceProject;
    off.push(splinely.on('state.change',e=>{lines.push(splinely.timeline.time.toFixed(1)+'s '+e.machine+': '+e.from+' → '+e.to+' ['+e.event+']');lines=lines.slice(-12);refresh();}));
    off.push(splinely.on('error',refresh));
    timer=setInterval(refresh,200);refresh();
  }catch(error){status.textContent='Observation blocked. Open SVG directly or use localhost.';log.textContent=String(error);}
}
object.addEventListener('load',attach);
window.addEventListener('pageshow',event=>{if(event.persisted)attach();});
object.addEventListener('error',()=>{status.textContent='Could not load local SVG.';});
window.addEventListener('pagehide',detach);
</script></body></html>
"""


def add_accessibility(cli, project, description):
    """Screen-reader data (opt-in): scene description, decorative nodes hidden, every
    figure's phrase announced politely when its state starts. Uses the real `a11y` command."""
    data = json.loads(project.read_text(encoding="utf-8"))
    phrases = {n["id"]: n["kind"]["text"] for n in data["nodes"] if n["kind"].get("type") == "text"}
    command = [str(cli), "a11y", str(project), "--description", description]
    subprocess.run(command, check=True, capture_output=True)
    for decoration in ("background", "ambient", "ambient-inner"):
        subprocess.run([str(cli), "a11y", str(project), "--node", decoration, "--hidden", "true"], check=True, capture_output=True)
    machine = data["stateMachines"][0]["id"]
    announce = []
    for state in data["stateMachines"][0]["states"]:
        phrase = phrases.get("phrase-" + state["id"].rsplit("-", 1)[-1])
        if phrase:
            announce += ["--state-announce", f"{machine}.{state['id']}={phrase}"]
    if announce:
        subprocess.run([str(cli), "a11y", str(project), *announce], check=True, capture_output=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "examples")
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--count", type=int, default=6)
    args = parser.parse_args()
    # Enforce bounds before touching output. CLI independently validates these too.
    if not 0 <= args.seed < 2**64 or not 3 <= args.count <= 12:
        parser.error("seed must be a u64 and count must be 3..12")
    subprocess.run(["cargo", "build", "--quiet", "-p", "splinely-cli"], cwd=ROOT, check=True)
    cli = ROOT / "target/debug/splinely-cli"
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    for stem, title, description in [
        ("water-drop", "A drop in the blue", "A softly lit blue sphere, a luminous drop, and a gentle impact ripple. Squash/stretch is authored with canonical numeric tracks, not unsupported path morphs."),
        ("random-figures", "The shape conversation", "Seeded, warm-colored geometric characters take turns on stage. It starts by itself: the machine visits every figure in order and repeats without random state jumps."),
    ]:
        project = output / (stem + ".splinely.json")
        command = [str(cli), stem, "--output", str(project)]
        if stem == "random-figures":
            command += ["--seed", str(args.seed), "--count", str(args.count)]
        subprocess.run(command, check=True)
        if stem == "random-figures":
            add_accessibility(cli, project, description)
        subprocess.run([str(cli), "validate", str(project)], check=True)
        # Explicitly reviewed controller JS.
        export = [str(cli), "export", str(project), "--output", str(output / (stem + ".svg")), "--trusted-js"]
        subprocess.run(export, check=True)
        wrapper = TEMPLATE.replace("@@STEM@@", stem).replace("@@TITLE@@", html.escape(title)).replace("@@DESCRIPTION@@", html.escape(description))
        (output / (stem + ".html")).write_text(wrapper, encoding="utf-8")


if __name__ == "__main__":
    main()
