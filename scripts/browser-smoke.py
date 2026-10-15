#!/usr/bin/env python3
"""Real Firefox/Marionette smoke for an offline CLI export, without third-party packages.
Requires Firefox. Validates script initialization, Web Audio decode/unlock, animation and state.
Does not prove audible device output or latency. Use: python3 scripts/browser-smoke.py [SVG] [corpus manifest]
Optional manifest compares original/imported SVG pixels in the same real Firefox instance.
"""
import json
import os
import re
import base64
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time

svg = Path(sys.argv[1] if len(sys.argv) > 1 else "examples/demo.svg").resolve()
assert svg.is_file(), svg
with tempfile.TemporaryDirectory(prefix="splinely-firefox-") as profile:
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    Path(profile, "user.js").write_text(
        f'user_pref("marionette.port", {port});\n'
        'user_pref("browser.shell.checkDefaultBrowser", false);\n'
        'user_pref("media.autoplay.default", 1);\n'
    )
    with tempfile.TemporaryFile() as log:
        process = subprocess.Popen(
            ["firefox", "--headless", "--no-remote", "--marionette", "--profile", profile],
            env={**os.environ, "MOZ_HEADLESS": "1", "MOZ_DISABLE_GPU_ACCEL": "1"},
            stdout=log, stderr=log,
        )
        connection = None
        try:
            deadline = time.monotonic() + 25
            while time.monotonic() < deadline:
                try:
                    connection = socket.create_connection(("127.0.0.1", port), timeout=1)
                    break
                except OSError:
                    if process.poll() is not None:
                        raise RuntimeError("Firefox exited before Marionette startup")
                    time.sleep(0.1)
            if connection is None:
                raise RuntimeError("Firefox Marionette startup timed out")
            connection.settimeout(20)
            def receive():
                size = b""
                while not size.endswith(b":"):
                    part = connection.recv(1)
                    if not part:
                        raise RuntimeError("Marionette disconnected")
                    size += part
                remaining = int(size[:-1])
                payload = b""
                while remaining:
                    part = connection.recv(remaining)
                    if not part:
                        raise RuntimeError("Marionette disconnected")
                    payload += part
                    remaining -= len(part)
                return json.loads(payload)
            receive()  # Protocol handshake.
            def command(name, parameters=None):
                # Sequence is kept on the function to avoid module-scope nonlocal.
                command.sequence += 1
                payload = json.dumps([0, command.sequence, name, parameters or {}]).encode()
                connection.sendall(str(len(payload)).encode() + b":" + payload)
                reply = receive()
                assert reply[1] == command.sequence, reply
                if reply[2] is not None:
                    raise RuntimeError(str(reply[2]))
                return reply[3]
            command.sequence = 0
            def js(source):
                result = command("WebDriver:ExecuteScript", {"script": source, "args": [], "newSandbox": False})
                return result.get("value", result) if isinstance(result, dict) else result
            command("WebDriver:NewSession", {"capabilities": {"alwaysMatch": {"browserName": "firefox"}}})
            command("WebDriver:Navigate", {"url": svg.as_uri()})
            initial = js("const f=document.documentElement.splinely;return {mounted:!!f,playing:f.timeline.playing,controls:document.querySelectorAll('g[role=button]').length};")
            assert initial == {"mounted": True, "playing": False, "controls": 1}, initial
            element = command("WebDriver:FindElement", {"using": "css selector", "value": "g[role=button] rect"})
            element = element.get("value", element)
            identifier = element.get("element-6066-11e4-a52e-4f735466cecf", element.get("ELEMENT"))
            command("WebDriver:ElementClick", {"id": identifier})
            time.sleep(0.5)
            after = js("const f=document.documentElement.splinely;return {playing:f.timeline.playing,ready:f.audio.ready,state:f.audio.context.state,time:f.timeline.time,buffers:f.audio.buffers.size,controls:document.querySelectorAll('g[role=button]').length};")
            assert after["playing"] and after["ready"] and after["state"] == "running", after
            assert after["buffers"] >= 1 and after["time"] > 0.1 and after["controls"] == 0, after
            circle = command("WebDriver:FindElement", {"using": "css selector", "value": "#ball"})
            circle = circle.get("value", circle)
            command("WebDriver:ElementClick", {"id": circle.get("element-6066-11e4-a52e-4f735466cecf", circle.get("ELEMENT"))})
            time.sleep(0.15)
            result = js("const f=document.documentElement.splinely;return {fill:document.getElementById('ball').getAttribute('fill'),machines:f.sourceProject.stateMachines.map(m=>f.machine(m.id).state)};")
            assert "clicked" in result["machines"] and result["fill"] == "rgb(255,102,51)", result
            security = js("""
const root=document.documentElement, f=root.splinely, ns='http://www.w3.org/2000/svg';
f.set('ball',{fill:'u'+String.fromCharCode(92)+'72l(http://127.0.0.1:1/paint.svg)',srcset:'http://127.0.0.1:1/image 1x',constructor:1});
const nested=document.createElementNS(ns,'svg'), fo=document.createElementNS(ns,'foreignObject');
const div=document.createElementNS('http://www.w3.org/1999/xhtml','div');div.id='ball';div.setAttribute('data-splinely-project','');div.textContent='invalid JSON';
fo.append(div);nested.append(fo);root.append(nested);f.set('ball',{opacity:.75});
const isolated=div.getAttribute('opacity')===null, safe=!root.querySelector('#ball').getAttribute('fill').includes('127.0.0.1')&&!root.querySelector('#ball').hasAttribute('srcset');
f.destroy();const controls=root.querySelectorAll('[data-splinely-host-control]').length;
const retry=window.SplinelyRuntime.createRuntime(root,f.sourceProject,{autoTick:false});
let drag=false;retry.on('dragstart',()=>drag=true);retry.on('pointerdown',()=>retry.destroy());
root.querySelector('#ball').dispatchEvent(new PointerEvent('pointerdown',{bubbles:true,pointerId:7}));
nested.remove();return {isolated,safe,controls,destroyed:!root.splinely,drag};
""")
            assert security == {"isolated": True, "safe": True, "controls": 0, "destroyed": True, "drag": False}, security
            print("PASS: real DOM property filters, nested foreignObject isolation, destroy/remount and pointer lifecycle")
            print("PASS: real offline Firefox export, trusted gesture, audio unlock/decode, timeline and click state:", json.dumps(after))
            if len(sys.argv) > 2:
                command('WebDriver:Navigate', {'url': 'about:blank'})
                command('WebDriver:SetTimeouts', {'script': 15000})
                manifest = json.loads(Path(sys.argv[2]).read_text())
                comparisons = []
                for fixture in (r for r in manifest if r['imported']):
                    reply = command('WebDriver:ExecuteAsyncScript', {'script': """
const done=arguments[arguments.length-1], sources=[arguments[0],arguments[1]];
Promise.all(sources.map(source=>new Promise((resolve,reject)=>{
 const image=new Image();const timeout=setTimeout(()=>reject(Error('SVG decode timed out')),8000);image.onload=()=>{clearTimeout(timeout);const canvas=document.createElement('canvas');canvas.width=128;canvas.height=128;const context=canvas.getContext('2d');context.drawImage(image,0,0,128,128);resolve(context.getImageData(0,0,128,128).data);};image.onerror=()=>reject(Error('SVG image decode failed'));image.src='data:image/svg+xml;base64,'+btoa(unescape(encodeURIComponent(source)));
}))).then(([a,b])=>{let total=0,changed=0,max=0;for(let i=0;i<a.length;i++){const d=Math.abs(a[i]-b[i]);total+=d;max=Math.max(max,d);if(d>4)changed++;}done({mean:total/a.length,changed:changed/a.length,max});}).catch(e=>done({error:String(e)}));
""", 'args': [Path(fixture['source']).read_text(), Path(fixture['export']).read_text()], 'newSandbox': False})
                    comparison = reply.get('value', reply)
                    assert 'error' not in comparison and comparison['mean'] < 1 and comparison['changed'] < .05, (fixture['source'], comparison)
                    comparisons.append({'source': fixture['source'], **comparison})
                Path('/tmp/splinely-svg-browser-comparisons.json').write_text(json.dumps(comparisons, indent=2))
                print('PASS: Firefox pixel comparisons of original vs imported/exported real SVGs:', json.dumps(comparisons))
            if len(sys.argv) > 3:
                rig = Path(sys.argv[3]).resolve()
                command('WebDriver:Navigate', {'url': rig.with_name(rig.name.removesuffix('.splinely.json').removesuffix('.json') + '.svg').as_uri()})
                for time_value in [0., .5, 1.5, 2.5]:
                    expected = json.loads(subprocess.check_output(['target/debug/splinely-cli', 'evaluate', str(rig), '--time', str(time_value)], text=True))['nodes']
                    actual = js(f"const f=document.documentElement.splinely;f.timeline.pause();f.timeline.seek({time_value});return f.sourceProject.nodes.map(n=>({{id:n.id,transform:document.getElementById(n.id).getAttribute('transform')}}));")
                    by_id = {n['id']: n['transform'] for n in actual}
                    for node in expected:
                        t = node['transform']
                        values = [float(n) for n in re.findall(r'[-+]?(?:\d*\.\d+|\d+\.?\d*)(?:[eE][-+]?\d+)?', by_id[node['id']])]
                        want = [t['x'], t['y'], t['rotation'], t['scaleX'], t['scaleY']]
                        assert len(values) == 5 and max(abs(a-b) for a,b in zip(values,want)) < 1e-7, (time_value,node['id'],values,want)
                screenshot = command('WebDriver:TakeScreenshot', {'full': False})
                Path('/tmp/splinely-rig-browser.png').write_bytes(base64.b64decode(screenshot.get('value', screenshot)))
                print('PASS: actual rig export matches Rust FK/animation for every node at 0, .5, 1.5, 2.5 seconds')
            command("Marionette:Quit", {"flags": ["eForceQuit"]})
        finally:
            if connection:
                connection.close()
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
