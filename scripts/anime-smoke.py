#!/usr/bin/env python3
"""Real Firefox smoke: embedded offline WAV decode/start and animated greeting."""
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import base64
from pathlib import Path
import threading
import time
from firefox_harness import Firefox

root = Path(__file__).resolve().parents[1]
server = ThreadingHTTPServer(('127.0.0.1', 0), partial(SimpleHTTPRequestHandler, directory=str(root)))
thread = threading.Thread(target=server.serve_forever, daemon=True)
thread.start()
try:
    with Firefox() as browser:
        browser.command('WebDriver:Navigate', {'url': f'http://127.0.0.1:{server.server_port}/examples/anime-hello.html'})
        for _ in range(100):
            if browser.js("try{return !!document.querySelector('object').contentDocument.documentElement.splinely;}catch(e){return false;}"):
                break
            time.sleep(.1)
        assert browser.js("return !!document.querySelector('object').contentDocument.documentElement.splinely;"), 'Runtime not ready'
        # No host buttons: the only gesture is the SVG's own audio gate that browser autoplay policy requires.
        assert browser.js("return document.querySelectorAll('button').length;") == 0, 'Host page still has buttons'
        browser.js("""
const doc=document.querySelector('object').contentDocument,w=doc.defaultView;
window.trusted=[];window.audioStarts=0;
const start=w.AudioBufferSourceNode.prototype.start;
w.AudioBufferSourceNode.prototype.start=function(...args){window.audioStarts++;return start.apply(this,args);};
doc.addEventListener('click',e=>window.trusted.push(e.isTrusted),true);
""")
        initial = browser.js("return document.querySelector('object').contentDocument.querySelector('#wave-hand').getAttribute('transform');")

        def pointer_click(selector):
            point = browser.js("""
const object=document.querySelector('object'),doc=object.contentDocument,el=doc.querySelector(arguments[0]);
object.scrollIntoView({block:'center'});
const b=el.getBoundingClientRect(),o=object.getBoundingClientRect();
return {x:Math.round(o.left+object.clientLeft+(b.left+b.width/2)*object.clientWidth/doc.defaultView.innerWidth),
 y:Math.round(o.top+object.clientTop+(b.top+b.height/2)*object.clientHeight/doc.defaultView.innerHeight)};
""", [selector])
            try:
                browser.command('WebDriver:PerformActions', {'actions': [{'type': 'pointer', 'id': 'mouse', 'parameters': {'pointerType': 'mouse'}, 'actions': [
                    {'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', **point},
                    {'type': 'pointerDown', 'button': 0}, {'type': 'pointerUp', 'button': 0}]}]})
            finally:
                browser.command('WebDriver:ReleaseActions')

        # Before any click the browser keeps audio locked, so the gate holds the greeting at t=0.
        assert browser.js("return document.querySelector('object').contentDocument.querySelectorAll('[data-splinely-host-control]').length;") == 1
        pointer_click('[data-splinely-host-control] rect')
        time.sleep(.7)
        result = browser.js("""
const d=document.querySelector('object').contentDocument,f=d.documentElement.splinely;
return {trusted:window.trusted,time:f.timeline.time,starts:window.audioStarts,ready:f.audio.ready,context:f.audio.context?.state,buffers:f.audio.buffers.size,
 gate:d.querySelectorAll('[data-splinely-host-control]').length,
 transform:d.querySelector('#wave-hand').getAttribute('transform'),status:document.querySelector('#status').textContent};
""")
        assert result['trusted'] == [True] and result['gate'] == 0, result
        assert result['starts'] >= 1 and result['ready'] and result['context'] == 'running' and result['buffers'] == 1, result
        assert result['time'] > .3 and result['transform'] != initial, result
        # Once the 3.2 s greeting has ended a click on the character itself replays it (no button).
        deadline = time.monotonic() + 6
        while time.monotonic() < deadline and browser.js("return document.querySelector('object').contentDocument.documentElement.splinely.clip('greeting').playing;"):
            time.sleep(.1)
        assert not browser.js("return document.querySelector('object').contentDocument.documentElement.splinely.clip('greeting').playing;"), 'Greeting never ended'
        pointer_click('#wave-hand')
        time.sleep(.5)
        assert browser.js('return window.audioStarts;') > result['starts'], 'Replay missing'
        screenshot = browser.command('WebDriver:TakeScreenshot', {'id': None, 'full': True, 'scroll': False})
        Path('/tmp/splinely-anime-hello.png').write_bytes(base64.b64decode(screenshot))
        print('PASS: CLI anime SVG, no host button, audio gate click, trusted offline WAV decode/start, click-to-replay and actual bone movement:', result)
finally:
    server.shutdown()
    server.server_close()
