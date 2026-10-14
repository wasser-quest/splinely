#!/usr/bin/env python3
"""Real Firefox pointer smoke: standalone SVG and object wrapper, no dispatchEvent."""
import base64
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import threading
import time

from firefox_harness import Firefox

ROOT = Path(__file__).resolve().parents[1]


def wait_for(browser, script, description):
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        if browser.js(script):
            return
        time.sleep(.04)
    raise AssertionError(description)


def run_scene(browser, url, wrapper):
    browser.command('WebDriver:Navigate', {'url': url})
    document = "document.querySelector('object').contentDocument" if wrapper else 'document'
    wait_for(browser, f"const d={document}; return !!(d && d.documentElement.forma);", 'Runtime did not self-start')
    browser.js(f"""
window.owlDocument = {document};
window.owlApi = owlDocument.documentElement.forma;
window.owlTrusted = []; window.owlChanges = [];
for (const event of ['click', 'pointermove']) owlApi.on(event, detail => {{
  if (detail.event) owlTrusted.push({{type:event, trusted:detail.event.isTrusted, target:detail.target}});
}});
owlApi.on('state.change', detail => owlChanges.push({{from:detail.from, to:detail.to}}));
""")
    if wrapper:
        assert browser.js("return !!document.querySelector('a[href=\"brown-owl-source.svg\"]');"), 'Source link missing'

    def snapshot():
        return browser.js("""
const d=owlDocument, f=owlApi;
function visible(id) {
 const el=d.getElementById(id), style=d.defaultView.getComputedStyle(el);
 return style.display!=='none' && style.visibility!=='hidden' && Number(style.opacity)>0;
}
function pupil(id) {
 const m=d.getElementById(id).transform.baseVal.consolidate().matrix;
 return {x:m.e,y:m.f};
}
return {state:f.machine('owl-mood').state, mood:f.variable('mood').get(),
 overlays:['owl-lid-left','owl-lid-right','owl-lid-lines'].map(visible),
 left:pupil('owl-pupil-left'),right:pupil('owl-pupil-right'),
 diagnostics:f.diagnostics.map(item=>item.message)};
""")

    def expect(state, overlay):
        result = snapshot()
        assert result['state'] == state and result['mood'] == state, result
        assert result['overlays'] == [overlay] * 3, result
        # Canonical circle geometry has its center in cx/cy, not transform x/y.
        for side in ['left', 'right']:
            offset = result[side]
            assert offset['x'] ** 2 + offset['y'] ** 2 <= 12 ** 2 + .001, result
        return result

    def actions(items):
        browser.command('WebDriver:PerformActions', {'actions': [{
            'type': 'pointer', 'id': 'owl-mouse', 'parameters': {'pointerType': 'mouse'}, 'actions': items,
        }]})

    def move(x, y):
        point = browser.js("""
const svg=owlDocument.documentElement, p=svg.createSVGPoint();
p.x=arguments[0];p.y=arguments[1];
const q=p.matrixTransform(svg.getScreenCTM());
const object=document.querySelector('object'), box=object && object.getBoundingClientRect();
return {x:Math.round(q.x+(box?box.left:0)), y:Math.round(q.y+(box?box.top:0))};
""", [x, y])
        actions([{'type': 'pointerMove', 'duration': 80, 'origin': 'viewport', **point}])

    def click():
        actions([{'type': 'pointerDown', 'button': 0}, {'type': 'pointerUp', 'button': 0}])

    expect('idle', False)
    move(20, 20)
    move(240, 245)
    left = expect('attentive', False)
    move(395, 245)
    right = expect('attentive', False)
    assert right['left']['x'] > left['left']['x'] + 5, (left, right)
    assert right['right']['x'] > left['right']['x'] + 5, (left, right)
    move(430, 370)
    expect('attentive', False)  # Near wing edge: still bounded.
    move(20, 20)
    idle = expect('idle', False)
    assert idle['left'] == idle['right'] == {'x': 0, 'y': 0}, idle

    move(320, 290)
    expect('attentive', False)
    click()
    expect('content', True)
    wait_for(browser, "return owlApi.machine('owl-mood').state==='idle';", 'Content did not settle to idle')
    expect('idle', False)
    # Without another move, a direct figure click exercises idle -> happy.
    click()
    expect('content', True)
    click()
    expect('idle', False)
    move(321, 291)
    expect('attentive', False)
    click()
    expect('content', True)
    move(20, 20)
    expect('idle', False)  # Leave also cancels the content pose.
    time.sleep(1.5)
    final = expect('idle', False)
    events = browser.js('return {trusted:owlTrusted,changes:owlChanges};')
    assert events['trusted'] and all(e['trusted'] for e in events['trusted']), events
    assert sum(e['type'] == 'click' and e['target'].startswith('owl-') for e in events['trusted']) == 4, events
    assert {'from': 'idle', 'to': 'content'} in events['changes'], events
    assert {'from': 'content', 'to': 'idle'} in events['changes'], events
    # Runtime keeps this informational warning even when trusted export executes it.
    assert all(message == 'customJs is preserved but not executed; attach trusted handlers explicitly'
               for message in final['diagnostics']), final
    browser.command('WebDriver:ReleaseActions')
    print('PASS:', 'HTML object' if wrapper else 'standalone SVG', 'trusted figure clicks, hover tracking, bounded pupils, state actions, closing eyes and idle restoration')


def main():
    server = ThreadingHTTPServer(('127.0.0.1', 0), partial(SimpleHTTPRequestHandler, directory=str(ROOT)))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        with Firefox() as browser:
            base = f'http://127.0.0.1:{server.server_port}/examples/brown-owl'
            run_scene(browser, base + '.svg', False)
            run_scene(browser, base + '.html', True)
            screenshot = browser.command('WebDriver:TakeScreenshot', {'id': None, 'full': True, 'scroll': False})
            Path('/tmp/forma-brown-owl.png').write_bytes(base64.b64decode(screenshot))
    finally:
        server.shutdown()
        server.server_close()


if __name__ == '__main__':
    main()
