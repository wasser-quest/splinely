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
            if browser.js("return !document.querySelector('#hello').disabled;"):
                break
            time.sleep(.1)
        assert browser.js("return !document.querySelector('#hello').disabled;"), 'Runtime not ready'
        browser.js("""
const doc=document.querySelector('object').contentDocument,w=doc.defaultView;
window.spoken=[];window.trusted=[];window.audioStarts=0;
w.speechSynthesis.speak=u=>window.spoken.push({text:u.text,lang:u.lang});
const start=w.AudioBufferSourceNode.prototype.start;
w.AudioBufferSourceNode.prototype.start=function(...args){window.audioStarts++;return start.apply(this,args);};
document.querySelector('#hello').addEventListener('click',e=>window.trusted.push(e.isTrusted));
""")
        initial = browser.js("return document.querySelector('object').contentDocument.querySelector('#wave-hand').getAttribute('transform');")
        element = browser.command('WebDriver:FindElement', {'using': 'css selector', 'value': '#hello'})
        identifier = element.get('element-6066-11e4-a52e-4f735466cecf', element.get('ELEMENT'))
        browser.command('WebDriver:ElementClick', {'id': identifier})
        time.sleep(.7)
        result = browser.js("""
const d=document.querySelector('object').contentDocument,f=d.documentElement.splinely;
return {spoken:window.spoken,trusted:window.trusted,time:f.timeline.time,starts:window.audioStarts,ready:f.audio.ready,context:f.audio.context?.state,buffers:f.audio.buffers.size,
 transform:d.querySelector('#wave-hand').getAttribute('transform'),status:document.querySelector('#status').textContent};
""")
        assert result['trusted'] == [True], result
        assert result['spoken'] == [], result
        assert result['starts'] >= 1 and result['ready'] and result['context'] == 'running' and result['buffers'] == 1, result
        assert result['time'] > .3 and result['transform'] != initial, result
        browser.command('WebDriver:ElementClick', {'id': identifier})
        time.sleep(.5)
        assert browser.js('return window.audioStarts;') > result['starts'], 'Replay missing'
        screenshot = browser.command('WebDriver:TakeScreenshot', {'id': None, 'full': True, 'scroll': False})
        Path('/tmp/splinely-anime-hello.png').write_bytes(base64.b64decode(screenshot))
        print('PASS: CLI anime SVG, trusted offline WAV decode/start/replay (no browser speech) and actual bone movement:', result)
finally:
    server.shutdown()
    server.server_close()
