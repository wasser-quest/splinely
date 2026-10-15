#!/usr/bin/env python3
"""Real Firefox file:// wall-time checks of compiled cactus-runner export."""
import base64
import json
import math
import os
from pathlib import Path
import re
import time
import xml.etree.ElementTree as ET
from firefox_harness import Firefox

ROOT=Path(__file__).resolve().parents[1]
os.environ.setdefault('TMPDIR',str(ROOT/'.work-tmp'))
SVG=ROOT/'examples/cactus-runner.svg'
EVIDENCE=ROOT/'.work-tmp/cactus-runner-telemetry.json'
SAMPLES=[]


def sample(browser):
    result=browser.js("""
const d=document.querySelector('#scene') || document.documentElement,f=d.splinely;
const walk=f.clip('walk').weight,jump=f.clip('jump').weight;
return {walk,jump,scene:f.machine('scene').state,state:f.machine('runner').state,
 scroll:f.clip('scroll').time,terrain:f.clip('terrain').time,walkTime:f.clip('walk').time,jumpTime:f.clip('jump').time,
 bones:f.bones(),cacti:Array.from({length:8},(_,i)=>d.getElementById('cactus-'+i).getAttribute('transform')),
 limb:d.getElementById('arm-front-ink').getAttribute('transform'),diagnostics:f.diagnostics.map(e=>e.message || String(e))};
""")
    SAMPLES.append({'wallTime':time.monotonic(),**result})
    return result


def clean_diagnostics(s):
    # Runtime construction emits this advisory before the explicitly trusted
    # exporter bootstrap runs. Keep it in telemetry; reject any other message.
    assert not s['diagnostics'],s['diagnostics']


def wait_for(browser,predicate,seconds=3):
    deadline=time.monotonic()+seconds
    last=None
    while time.monotonic()<deadline:
        last=sample(browser)
        if predicate(last): return last
        time.sleep(.015)
    raise AssertionError(('Expected wall-time state not observed',last))


def click(browser,id):
    element=browser.command('WebDriver:FindElement',{'using':'css selector','value':'#'+id})
    browser.command('WebDriver:ElementClick',{'id':element.get('element-6066-11e4-a52e-4f735466cecf',element.get('ELEMENT'))})


def screenshot(browser,name):
    image=browser.command('WebDriver:TakeScreenshot',{'id':None,'full':True,'scroll':False})
    path=ROOT/'.work-tmp'/name
    path.write_bytes(base64.b64decode(image))
    return path


def fk(s):
    poses={b['id']:b for b in s['bones']}
    root=poses['torso']
    assert abs(root['x']-230)<1e-7, ('Horizontal jump teleport',root)
    a=math.radians(root['rotation'])
    arm=poses['arm-front']
    assert abs(arm['x']-(root['x']+61*math.cos(a)))<1e-7
    assert abs(arm['y']-(root['y']+61*math.sin(a)))<1e-7
    fore=poses['fore-front']
    assert abs(fore['x']-arm['tipX'])<1e-7 and abs(fore['y']-arm['tipY'])<1e-7
    actual=[float(v) for v in re.match(r'translate\(([^)]+)\)',s['limb'])[1].replace(',',' ').split()]
    assert abs(actual[0]-arm['x'])<1e-5 and abs(actual[1]-arm['y'])<1e-5


def sampled(track,t):
    keys=track['keyframes']
    if t<=keys[0]['time']: return keys[0]['value']['value']
    for a,b in zip(keys,keys[1:]):
        if t<b['time']:
            if a['easing']['type']=='step': return a['value']['value']
            f=(t-a['time'])/(b['time']-a['time'])
            return a['value']['value']*(1-f)+b['value']['value']*f
    return keys[-1]['value']['value']


def weighted_pose(s,project):
    poses={b['id']:b for b in s['bones']}
    torso=next(b for b in project['bones'] if b['id']=='torso')
    angles=[]
    for id,time in [('walk',s['walkTime']),('jump',s['jumpTime'])]:
        clip=next(c for c in project['clips'] if c['id']==id)
        tr=next(t for t in clip['tracks'] if t['nodeId']=='torso' and t['property']=='rotation')
        angles.append(sampled(tr,time)*s[id])
    expected=torso['rotation']*max(0,1-s['walk']-s['jump'])+sum(angles)/max(1,s['walk']+s['jump'])
    assert abs(poses['torso']['rotation']-expected)<.3,('Weighted local rotation not applied',s,expected)
    tr=next(t for t in project['clips'][4]['tracks'] if t['property']=='y')
    expected=310*(1-s['jump'])+sampled(tr,s['jumpTime'])*s['jump']
    assert abs(poses['torso']['y']-expected)<.5,('Sparse channel residual baseline missing',s,expected)


def cactus_xs(s):
    return [float(re.match(r'translate\(([-.\d]+)',v)[1]) for v in s['cacti']]


def cactus_pose(s,project):
    for actual,tr in zip(cactus_xs(s),project['clips'][0]['tracks']):
        assert abs(actual-sampled(tr,s['scroll']))<2,(s['scroll'],actual,tr)


def main():
    tree=ET.parse(SVG)
    project=json.loads(tree.find("{http://www.w3.org/2000/svg}metadata[@id='splinely-project']").text)
    canonical=json.loads((ROOT/'examples/cactus-runner.splinely.json').read_text())
    assert len(project['bones'])==10 and not project['audioAssets']
    for index in [3,4]:
        assert project['clips'][index]['blendMode']=='weighted'
        assert project['clips'][index]['weight']==canonical['clips'][index]['weight']
    assert project['customJs']==canonical['customJs']
    assert project['stateMachines']==canonical['stateMachines'], 'Compiled exporter dropped actions'
    for element in tree.iter():
        for key,value in element.attrib.items():
            if key.split('}')[-1] in ('href','src'): assert value.startswith(('#','data:')),(key,value)
            assert not re.search(r'url\(\s*["\']?(?:https?:|//)',value)
    html=(ROOT/'examples/cactus-runner.html').read_text()
    assert '<svg id="scene"' in html and not re.search(r'(?:src|href|data)=["\'](?:https?:|//)',html)
    with Firefox() as browser:
        browser.command('WebDriver:Navigate',{'url':(ROOT/'examples/cactus-runner.html').as_uri()})
        deadline=time.monotonic()+15
        while time.monotonic()<deadline:
            if browser.js("return !document.querySelector('#pause').disabled;"): break
            time.sleep(.1)
        assert browser.js("return !document.querySelector('#pause').disabled;"),'Offline runtime not ready'
        click(browser,'restart')
        initial=sample(browser)
        assert initial['walk']==1 and initial['jump']==0 and initial['state']=='ground'
        time.sleep(.35)
        walking=sample(browser)
        assert walking['scroll']>initial['scroll']+.2 and walking['cacti']!=initial['cacti']
        assert walking['bones']!=initial['bones']
        fk(walking)
        # Real scene click, not a hand-driven tick or setWeight masquerading as a jump.
        click(browser,'sky')
        overlap=wait_for(browser,lambda s:s['state']=='jumping' and .05<s['walk']<.95 and .05<s['jump']<.95)
        assert abs(overlap['walk']+overlap['jump']-1)<.03,overlap
        fk(overlap)
        weighted_pose(overlap,project)
        # Pause in the actual crossfade, not only after the ramps have finished.
        click(browser,'pause')
        paused_blend=sample(browser)
        time.sleep(.3)
        assert sample(browser)==paused_blend,'Paused blend continued changing'
        click(browser,'play')
        apex=wait_for(browser,lambda s:.40<s['jumpTime']<.65)
        fk(apex)
        weighted_pose(apex,project)
        assert apex['walk']==0 and apex['jump']==1
        assert next(b['y'] for b in apex['bones'] if b['id']=='torso')<205
        assert apex['scroll']>walking['scroll']+.3 and apex['cacti']!=overlap['cacti'],'Scroll stopped during jump'
        screenshot(browser,'cactus-runner-jump.png')
        back=wait_for(browser,lambda s:s['state']=='ground' and .02<s['walk']<.98 and .02<s['jump']<.98)
        fk(back)
        weighted_pose(back,project)
        settled=wait_for(browser,lambda s:s['state']=='ground' and s['walk']==1 and s['jump']==0)
        assert abs(next(b['y'] for b in settled['bones'] if b['id']=='torso')-310)<1e-7
        assert settled['scroll']>apex['scroll']+.4
        # All seeded cacti translate at one fixed speed; their gap pattern is preserved.
        delta=settled['scroll']-walking['scroll']
        for before,after in zip(cactus_xs(walking),cactus_xs(settled)):
            assert abs((after-before)+120*delta)<3, (before,after,delta)
        click(browser,'pause')
        held=sample(browser)
        time.sleep(.3)
        assert sample(browser)==held and held['scene']=='paused','Pause did not hold scene/rig'
        click(browser,'play')
        time.sleep(.2)
        assert sample(browser)['scroll']>held['scroll']+.1
        # Host keyboard path uses the same trusted event/state/blend controller.
        browser.js("document.dispatchEvent(new KeyboardEvent('keydown',{code:'Space',bubbles:true}));")
        wait_for(browser,lambda s:s['state']=='jumping')
        click(browser,'restart')
        restarted=sample(browser)
        assert restarted['scroll']<.35 and restarted['state']=='ground' and restarted['walk']==1 and restarted['jump']==0,restarted
        fk(restarted)
        clean_diagnostics(restarted)
        # Actual 24s loop and all eight offscreen respawns; no seek/tick shortcut.
        previous=restarted
        respawned=set()
        deadline=time.monotonic()+29
        while time.monotonic()<deadline:
            time.sleep(.12)
            wrapped=sample(browser)
            cactus_pose(wrapped,project)
            for i,(before,after) in enumerate(zip(cactus_xs(previous),cactus_xs(wrapped))):
                if after-before>2000:
                    assert before<-120 and after>2700,(i,before,after)
                    respawned.add(i)
            if wrapped['scroll']<previous['scroll']:
                break
            previous=wrapped
        else:
            raise AssertionError('24s wall-time clip did not wrap')
        assert wrapped['scene']=='running' and len(respawned)==8,(wrapped,respawned)
        clean_diagnostics(wrapped)
        # A full deterministic restart recreates positions at the same sample time.
        click(browser,'pause')
        click(browser,'restart')
        click(browser,'pause')
        fresh=sample(browser)
        for expected,actual in zip(cactus_xs(initial),cactus_xs(fresh)):
            assert abs((actual-expected)+120*(fresh['scroll']-initial['scroll']))<3
        shot=screenshot(browser,'cactus-runner.png')
        agent=browser.js('return navigator.userAgent;')
        browser.command('WebDriver:Navigate',{'url':SVG.as_uri()})
        time.sleep(.4)
        before=browser.js("return document.documentElement.splinely.clip('scroll').time;")
        time.sleep(.3)
        after=browser.js("return document.documentElement.splinely.clip('scroll').time;")
        assert after>before+.15,'Standalone SVG did not autoplay'
        click(browser,'sky')
        standalone_overlap=wait_for(browser,lambda s:s['state']=='jumping' and .05<s['walk']<.95 and .05<s['jump']<.95)
        weighted_pose(standalone_overlap,project)
        standalone_settled=wait_for(browser,lambda s:s['state']=='ground' and s['walk']==1 and s['jump']==0)
        assert standalone_settled['scroll']>standalone_overlap['scroll']+.8
        clean_diagnostics(standalone_settled)
    EVIDENCE.write_text(json.dumps({'browser':agent,'result':'PASS','respawned':sorted(respawned),
        'standaloneScroll':[before,after],
        'checkpoints':dict(initial=initial,walking=walking,overlap=overlap,apex=apex,
            returnBlend=back,settled=settled,loop=wrapped,standaloneOverlap=standalone_overlap,
            standaloneSettled=standalone_settled),'samples':SAMPLES},indent=2)+'\n')
    print('Telemetry: '+str(EVIDENCE))
    print('PASS: compiled SVG schema/actions; offline Firefox wall-time walk/jump fade overlap + return, scrolling/cacti, FK, pause/resume/restart, 24s seeded loop and standalone. Screenshot: '+str(shot))


if __name__=='__main__':
    main()
