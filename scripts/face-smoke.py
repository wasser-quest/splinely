#!/usr/bin/env python3
"""Real Firefox check of examples/curious-face.svg: two state machines crossfade
while pupils follow the real pointer and clicks toggle the mood. No custom JS,
no synthetic events: only WebDriver pointer actions."""
import base64
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import threading
import time

from firefox_harness import Firefox

ROOT = Path(__file__).resolve().parents[1]


def wait_for(browser, script, description, timeout=8):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if browser.js(script):
            return
        time.sleep(.04)
    raise AssertionError(description)


def main():
    server = ThreadingHTTPServer(('127.0.0.1', 0), partial(SimpleHTTPRequestHandler, directory=str(ROOT)))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        with Firefox() as browser:
            browser.command('WebDriver:Navigate', {'url': f'http://127.0.0.1:{server.server_port}/examples/curious-face.svg'})
            wait_for(browser, 'return !!document.documentElement.splinely;', 'Runtime did not self-start')
            browser.js("window.trusted=[];document.documentElement.splinely.on('click',d=>{if(d.event)trusted.push(d.event.isTrusted);});")

            def actions(items):
                browser.command('WebDriver:PerformActions', {'actions': [{
                    'type': 'pointer', 'id': 'mouse', 'parameters': {'pointerType': 'mouse'}, 'actions': items}]})

            def move(x, y, duration=60):
                point = browser.js("""
const svg=document.documentElement, p=svg.createSVGPoint();p.x=arguments[0];p.y=arguments[1];
const q=p.matrixTransform(svg.getScreenCTM());return {x:Math.round(q.x),y:Math.round(q.y)};""", [x, y])
                actions([{'type': 'pointerMove', 'duration': duration, 'origin': 'viewport', **point}])

            def state():
                return browser.js("""
const d=document, f=d.documentElement.splinely;
const m=id=>d.getElementById(id).transform.baseVal.consolidate().matrix;
return {states:f.snapshot().states, pupil:{x:m('pupil-left').e,y:m('pupil-left').f},
 scale:m('head').a, fill:d.getElementById('head').getAttribute('fill'),
 cheek:Number(d.getElementById('cheek-left').getAttribute('opacity')),
 mouth:d.getElementById('mouth').getAttribute('d'),
 diagnostics:f.diagnostics.map(e=>e.message)};""")

            base = state()['pupil']['x']  # before any pointer: the rest position
            move(5, 5)
            time.sleep(.4)
            rest = state()
            assert rest['states']['attention'] == 'idle' and rest['states']['mood'] == 'idle', rest
            # Hover: the head leans in softly while the pupils look right.
            move(290, 150)
            time.sleep(.6)
            right = state()
            assert right['states']['attention'] == 'near', right
            assert abs(right['scale'] - 1.06) < 1e-3, right
            assert right['pupil']['x'] - base > 3, right
            move(110, 150)
            time.sleep(.6)
            left = state()
            assert left['pupil']['x'] - base < -3, left
            # Click: crossfade to happy while hover stays active and pupils keep following.
            actions([{'type': 'pointerDown', 'button': 0}, {'type': 'pointerUp', 'button': 0}])
            time.sleep(.12)
            mid = state()
            assert mid['states']['attention'] == 'near' and mid['states']['mood'] == 'happy', mid
            assert 0 < mid['cheek'] < .85, ('expected a crossfade in progress', mid)
            time.sleep(.6)
            happy = state()
            assert abs(happy['cheek'] - .85) < 1e-6 and happy['fill'] == 'rgb(255,179,71)', happy
            assert happy['mouth'] != rest['mouth'], happy
            move(250, 120)
            time.sleep(.5)
            assert state()['pupil']['x'] > left['pupil']['x'], 'pupils keep following during the mood'
            # Second click fades back; leaving the face returns hover and pupils to rest.
            actions([{'type': 'pointerDown', 'button': 0}, {'type': 'pointerUp', 'button': 0}])
            move(5, 5)
            time.sleep(1)
            back = state()
            assert back['states']['attention'] == 'idle' and back['states']['mood'] == 'idle', back
            assert back['cheek'] == 0 and abs(back['scale'] - 1) < 1e-6, back
            assert back['mouth'] == rest['mouth'], back
            assert back['diagnostics'] == [], back
            assert browser.js('return trusted;') == [True, True]
            # The blink timer runs alongside everything else.
            wait_for(browser, "return document.documentElement.splinely.snapshot().states.blink==='shut';", 'No blink within 4 s', 4.5)
            wait_for(browser, "return document.documentElement.splinely.snapshot().states.blink==='open';", 'Eyes did not reopen', 1)
            browser.command('WebDriver:ReleaseActions')
            shot = browser.command('WebDriver:TakeScreenshot', {'id': None, 'full': True, 'scroll': False})
            Path('/tmp/splinely-curious-face.png').write_bytes(base64.b64decode(shot))
            print('PASS: Firefox curious face: three machines (hover, click, blink timer) crossfade, path morph, pupils follow the real pointer, trusted clicks, no diagnostics')
    finally:
        server.shutdown()
        server.server_close()


if __name__ == '__main__':
    main()
