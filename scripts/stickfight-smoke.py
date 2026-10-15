#!/usr/bin/env python3
"""Real Firefox, file:// only: FK/scene wall-time motion and transport controls.

Uses the repository's standard-library Marionette harness; no server,
Selenium, remote assets, or manual animation ticks. Rebuild first with
python3 scripts/generate-stickfight.py.
"""
import base64
import json
from pathlib import Path
import re
import time
import xml.etree.ElementTree as ET
from firefox_harness import Firefox
from sply_py import load_project_file, project_from_metadata

ROOT = Path(__file__).resolve().parents[1]
SVG = ROOT/'examples/stickfight.svg'


def sample(browser):
    return browser.js("""
const d=document.querySelector('#scene').contentDocument,f=d.documentElement.splinely;
return {time:f.clip('duel').time,playing:f.timeline.playing,bones:f.bones(),
 nodes:['cinder-arm-front-shape','flux-arm-front-shape','cinder-head','flux-head','cinder-shadow'].map(id=>d.getElementById(id).getAttribute('transform')),
 diagnostics:f.diagnostics};
""")


def transport(browser, action):
    # Scripting API of the runtime; the host page offers no buttons.
    browser.js("""const f=document.querySelector('#scene').contentDocument.documentElement.splinely;
if(arguments[0]==='play')f.clip('duel').play();f.timeline[arguments[0]]();""", [action])


def main():
    tree = ET.parse(SVG)
    ns = '{http://www.w3.org/2000/svg}'
    metadata = tree.find(f"{ns}metadata[@id='splinely-project']")
    project = project_from_metadata(metadata)
    assert len(project['bones']) == 20 and project['clips'][0]['duration'] == 8
    assert not project['customJs'] and not project['audioAssets']
    for element in tree.iter():
        for key, value in element.attrib.items():
            if key.split('}')[-1] in ('href','src'):
                assert value.startswith(('#','data:')), (key,value)
            assert not re.search(r'url\(\s*["\']?(?:https?:|//)',value), value
    html = (ROOT/'examples/stickfight.html').read_text()
    assert not re.search(r'(?:src|href|data)=["\'](?:https?:|//)',html)
    with Firefox() as browser:
        hold = lambda action: transport(browser, action)
        browser.command('WebDriver:Navigate', {'url':(ROOT/'examples/stickfight.html').as_uri()})
        deadline = time.monotonic()+15
        while time.monotonic() < deadline:
            if browser.js("try{return !!document.querySelector('#scene').contentDocument.documentElement.splinely;}catch(e){return false;}"):
                break
            time.sleep(.1)
        assert browser.js("return !!document.querySelector('#scene').contentDocument.documentElement.splinely;"), 'Local-file runtime not ready'
        # Nothing is pressed: the page has no buttons and the autoplay loop already runs.
        assert not browser.js("return document.querySelectorAll('button').length;"), 'Host page still has buttons'
        first = sample(browser)
        assert first['playing'], 'Duel did not start by itself'
        time.sleep(.85)
        second = sample(browser)
        assert second['time'] > first['time']+.5 and second['bones'] != first['bones'], (first,second)
        assert second['nodes'][:4] != first['nodes'][:4], 'Attached limbs/head did not move'
        assert second['nodes'][4] != first['nodes'][4], 'Independent scene shadow track did not move'
        # Check true parent-tip FK, not just unrelated SVG movement.
        poses = {b['id']:b for b in second['bones']}
        assert abs(poses['cinder-arm-front']['x']-poses['cinder-torso']['tipX']) < 1e-7
        assert abs(poses['cinder-arm-front']['y']-poses['cinder-torso']['tipY']) < 1e-7
        hold('pause')
        held = sample(browser)
        time.sleep(.6)
        assert sample(browser) == held and not held['playing'], 'Pause failed to hold exact bone/DOM sample'
        hold('play')
        time.sleep(.4)
        resumed = sample(browser)
        assert resumed['time'] > held['time']+.2 and resumed['nodes'] != held['nodes'], 'Resume failed'
        browser.js("const f=document.querySelector('#scene').contentDocument.documentElement.splinely;f.timeline.pause();f.timeline.seek(0);f.clip('duel').play();f.timeline.play();")
        restarted = sample(browser)
        assert restarted['time'] < .35 and restarted['playing'], restarted
        time.sleep(3.8)
        recoil = sample(browser)
        assert recoil['time'] > 3.5 and recoil['bones'] != restarted['bones'] and not recoil['diagnostics'], recoil
        screenshot = browser.command('WebDriver:TakeScreenshot', {'id':None,'full':True,'scroll':False})
        Path('/tmp/splinely-stickfight.png').write_bytes(base64.b64decode(screenshot))
        # A real full wall-clock loop: no seek/tick standing in for playback.
        time.sleep(4.5)
        wrapped = sample(browser)
        assert wrapped['time'] < 1.5 and wrapped['playing'] and not wrapped['diagnostics'], wrapped
        # The SVG itself is standalone: no HTML host is needed for motion.
        browser.command('WebDriver:Navigate', {'url':SVG.as_uri()})
        time.sleep(.5)
        direct = browser.js('return {time:document.documentElement.splinely.clip("duel").time,bones:document.documentElement.splinely.bones()};')
        time.sleep(.5)
        later = browser.js('return {time:document.documentElement.splinely.clip("duel").time,bones:document.documentElement.splinely.bones()};')
        assert later['time'] > direct['time']+.2 and later['bones'] != direct['bones']
    print('PASS: XML/local resources; Firefox file:// starts on load without any button, bone FK + scene wall-time movement, scripted pause/resume/restart, 8s wrap, standalone SVG. Screenshot: /tmp/splinely-stickfight.png')


if __name__ == '__main__':
    main()
