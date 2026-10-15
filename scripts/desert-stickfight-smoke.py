#!/usr/bin/env python3
"""Bounded real Firefox offline playback test; never seek/tick to fake motion."""
import base64
import json
import math
import os
from pathlib import Path
import re
import tempfile
import time
import xml.etree.ElementTree as ET
from firefox_harness import Firefox

ROOT=Path(__file__).resolve().parents[1]
TMP=ROOT/'.work-tmp'
TMP.mkdir(exist_ok=True)
os.environ['TMPDIR']=str(TMP)
tempfile.tempdir=str(TMP)
SAMPLES=[]
PHASES=['approach','punches','sweep','impact','accord','finished']


def sample(browser):
    s=browser.js("""
const d=document.querySelector('#scene')||document.documentElement,f=d.splinely;
return {state:f.machine('story').state,playing:f.timeline.playing,clock:f.timeline.time,
 clips:['approach','punches','sweep','impact','accord'].map(id=>({id,time:f.clip(id).time})),
 bones:f.bones(),nodes:['reed-arm-front-ink','clay-arm-front-ink','reed-head','clay-head','cactus-2','dust-7-1','spark-0'].map(id=>({id,transform:d.getElementById(id).getAttribute('transform'),opacity:d.getElementById(id).getAttribute('opacity')})),
 diagnostics:f.diagnostics.map(e=>e.message||String(e))};
""")
    SAMPLES.append({'wallTime':time.monotonic(),**s})
    assert not s['diagnostics'],s['diagnostics']
    for side in ['reed','clay']:
        b={p['id']:p for p in s['bones']}
        root=b[side+'-torso']; arm=b[side+'-arm-front']; fore=b[side+'-fore-front']
        a=math.radians(root['rotation'])
        assert abs(arm['x']-(root['x']+61*math.cos(a)))<1e-6
        assert abs(arm['y']-(root['y']+61*math.sin(a)))<1e-6
        assert abs(fore['x']-arm['tipX'])<1e-6 and abs(fore['y']-arm['tipY'])<1e-6
        node=next(n for n in s['nodes'] if n['id']==side+'-arm-front-ink')
        xy=[float(v) for v in re.match(r'translate\(([^)]+)\)',node['transform'])[1].split()]
        assert abs(xy[0]-arm['x'])<1e-5 and abs(xy[1]-arm['y'])<1e-5
    return s


def click(browser,id):
    el=browser.command('WebDriver:FindElement',{'using':'css selector','value':'#'+id})
    browser.command('WebDriver:ElementClick',{'id':el.get('element-6066-11e4-a52e-4f735466cecf',el.get('ELEMENT'))})


def screenshot(browser,name):
    data=browser.command('WebDriver:TakeScreenshot',{'id':None,'full':True,'scroll':False})
    (TMP/name).write_bytes(base64.b64decode(data))


def changed_both(a,b):
    for side in ['reed','clay']:
        assert [p for p in a['bones'] if p['id'].startswith(side)] != [p for p in b['bones'] if p['id'].startswith(side)],side+' bones did not move'
        assert next(n for n in a['nodes'] if n['id']==side+'-arm-front-ink') != next(n for n in b['nodes'] if n['id']==side+'-arm-front-ink'),side+' DOM did not move'


def main():
    svg=ROOT/'examples/desert-stickfight.svg'; html=ROOT/'examples/desert-stickfight.html'
    tree=ET.parse(svg); p=json.loads(tree.find("{http://www.w3.org/2000/svg}metadata[@id='splinely-project']").text)
    canonical=json.loads((ROOT/'examples/desert-stickfight.sply').read_text())
    assert len(p['bones'])==20 and sum(c['duration'] for c in p['clips'])==20
    assert p['clips']==canonical['clips'] and p['stateMachines']==canonical['stateMachines']
    assert p['customJs']==canonical['customJs'] and not p['audioAssets'] and 'speechCues' not in p
    for e in tree.iter():
        for key,value in e.attrib.items():
            if key.split('}')[-1] in ['href','src']: assert value.startswith(('#','data:'))
            assert not re.search(r'url\(\s*["\']?(?:https?:|//)',value)
    text=html.read_text()
    assert '<svg id="scene"' in text and '<img' not in text and '<object' not in text
    assert not re.search(r'(?:src|href|data)=["\'](?:https?:|//)',text)
    embedded=ET.fromstring(text[text.index('<svg '):text.index('</svg>')+6])
    assert json.loads(embedded.find("{http://www.w3.org/2000/svg}metadata[@id='splinely-project']").text)==p
    checkpoints={}
    with Firefox() as browser:
        browser.command('WebDriver:Navigate',{'url':html.as_uri()})
        deadline=time.monotonic()+15
        while time.monotonic()<deadline:
            if browser.js("return !document.querySelector('#pause').disabled;"): break
            time.sleep(.1)
        assert browser.js("return !document.querySelector('#pause').disabled;"),'Embedded runtime not ready'
        click(browser,'replay'); a=sample(browser); time.sleep(.45); b=sample(browser)
        changed_both(a,b); assert a['nodes'][4]!=b['nodes'][4],'Cactus background static'
        click(browser,'pause'); held=sample(browser); time.sleep(.35)
        assert sample(browser)==held and not held['playing'],'Pause changed bones/scene/clip time'
        screenshot(browser,'desert-stickfight-approach.png')
        click(browser,'play'); time.sleep(.3); resumed=sample(browser); changed_both(held,resumed)
        assert resumed['state']==held['state'] and .2<resumed['clock']-held['clock']<.7,'Resume restarted or jumped'
        # Replay mid-clip, then observe every phase in real wall time, including an impact pause.
        click(browser,'replay'); fresh=sample(browser)
        assert fresh['state']=='approach' and fresh['clips'][0]['time']<.35
        assert all(c['time']==0 for c in fresh['clips'][1:]),fresh
        seen=['approach']; per_phase={}; shot=set(); paused_impact=False
        start_wall=time.monotonic(); hold_seconds=0
        deadline=start_wall+27
        while time.monotonic()<deadline:
            time.sleep(.12); current=sample(browser); state=current['state']
            if state!=seen[-1]: seen.append(state)
            per_phase.setdefault(state,[]).append(current)
            if state in ['punches','sweep','impact','accord']:
                local=next(c['time'] for c in current['clips'] if c['id']==state)
                target={'punches':1.05,'sweep':1.15,'impact':1.5,'accord':2.15}[state]
                if local>=target and state not in shot:
                    screenshot(browser,'desert-stickfight-'+state+'.png'); shot.add(state); checkpoints[state]=current
                if state=='impact' and local>.9 and not paused_impact:
                    click(browser,'pause'); hold_start=time.monotonic(); hold=sample(browser)
                    time.sleep(.3); assert sample(browser)==hold
                    click(browser,'play'); hold_seconds+=time.monotonic()-hold_start
                    after=sample(browser)
                    assert after['state']=='impact' and 0<=after['clock']-hold['clock']<.25
                    paused_impact=True
            if state=='finished': break
        else: raise AssertionError('Film did not finish within bounded wall-time test')
        elapsed=time.monotonic()-start_wall-hold_seconds
        assert abs(elapsed-20)<.75,('Wall-time duration excluding hold',elapsed)
        assert abs(current['clock']-20)<.05,current['clock']
        assert seen==PHASES,seen
        assert not current['playing'] and current['clips'][-1]['time']==4.5,current
        for state in PHASES[:-1]:
            samples=per_phase[state]; changed_both(samples[0],samples[-1])
        assert min(next(b['y'] for b in s['bones'] if b['id']=='clay-torso') for s in per_phase['sweep'])<255,'Vault never left ground'
        for index,label in [(5,'dust'),(6,'spark')]:
            values=[float(s['nodes'][index]['opacity']) for s in per_phase['impact']]
            assert values[0]==0 and max(values)>.5 and values[-1]==0,('Missing transient '+label,values)
        time.sleep(.25); assert sample(browser)==current,'Ending did not hold'
        screenshot(browser,'desert-stickfight-finished.png')
        click(browser,'play'); assert sample(browser)['state']=='finished','Play unexpectedly replayed'
        click(browser,'replay'); replay=sample(browser)
        assert replay['state']=='approach' and replay['clips'][0]['time']<.35 and replay['playing']
        assert all(c['time']==0 for c in replay['clips'][1:]),replay
        agent=browser.js('return navigator.userAgent;')
        browser.command('WebDriver:Navigate',{'url':svg.as_uri()}); time.sleep(.4); direct=sample(browser)
        time.sleep(.4); later=sample(browser); changed_both(direct,later)
        assert later['clips'][0]['time']>direct['clips'][0]['time']+.2,'Standalone SVG did not autoplay'
    (TMP/'desert-stickfight-telemetry.json').write_text(json.dumps({'result':'PASS','browser':agent,'seen':seen,'playbackWallSecondsExcludingHold':elapsed,'checkpoints':checkpoints,'samples':SAMPLES},indent=2)+'\n')
    print('PASS: installed Firefox file://; both rigs/DOM/FK, moving cacti, five sequential phases, airborne vault, dust/sparks, exact pause/resume, mid-film and ending replay, held finish, standalone SVG. Evidence: .work-tmp/desert-stickfight-*')


if __name__=='__main__': main()
