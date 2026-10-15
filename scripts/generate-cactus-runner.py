#!/usr/bin/env python3
"""Reproduce Dune / Stride via CLI import-svg, apply, validate and trusted export."""
import argparse
import json
import math
import os
from pathlib import Path
import random
import subprocess
import tempfile
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
os.environ.setdefault('TMPDIR', str(ROOT / '.work-tmp'))
SEED, PERIOD, SPEED = 731, 2880, 120


def number(v):
    return {'type': 'number', 'value': v}


def track(id, prop, keys):
    return {'nodeId': id, 'property': prop, 'keyframes': [
        {'time': t, 'value': number(v), 'easing': {'type': 'linear'}} for t, v in keys]}


def blend(id, weight, duration=.24):
    return {'type': 'blendClip', 'clipId': id, 'weight': weight, 'duration': duration}


def transition(start, end, event, actions=None):
    return {'from': start, 'to': end, 'event': {'name': event}, 'actions': actions or []}


CONTROLLER = r"""
// Reviewed trusted code. No geometry animation, timers, globals or network.
// Scene motion is canonical looping tracks; all locomotion mixing is blendClip.
const scene = splinely.machine('scene'), runner = splinely.machine('runner');
const scrollIds = ['scroll', 'terrain', 'distance'];
let heldFade = null;
function jump() {
  if (scene.state === 'running' && runner.state === 'ground') runner.send('JUMP');
}
splinely.on('click', jump);
splinely.on('keydown', detail => {
  if (detail.event.code === 'Space') { detail.event.preventDefault(); jump(); }
});
splinely.on('clip.end', detail => {
  if (detail.clip === 'jump') runner.send('LAND');
});
splinely.on('runner.jump', jump);
splinely.on('runner.pause', () => {
  splinely.timeline.pause();
  const walk = splinely.clip('walk').weight, jump = splinely.clip('jump').weight;
  heldFade = {walk, jump, seconds:.24 * (runner.state === 'jumping' ? walk : jump)};
  // Runtime ramps intentionally tick even on paused clips. Freeze this demo's
  // two ramps explicitly, then resume their remaining duration below.
  splinely.clip('walk').setWeight(walk); splinely.clip('jump').setWeight(jump);
  scene.send('PAUSE');
});
splinely.on('runner.play', () => {
  if (scene.state !== 'paused') return;
  scene.send('RESUME');
  for (const id of [...scrollIds, 'walk']) splinely.clip(id).play();
  if (runner.state === 'jumping') splinely.clip('jump').play();
  if (heldFade) {
    const airborne = runner.state === 'jumping';
    splinely.clip('walk').fadeTo(airborne ? 0 : 1, heldFade.seconds);
    splinely.clip('jump').fadeTo(airborne ? 1 : 0, heldFade.seconds);
    heldFade = null;
  }
  splinely.timeline.play();
});
splinely.on('runner.restart', () => {
  heldFade = null;
  splinely.timeline.pause();
  for (const id of [...scrollIds, 'walk', 'jump']) splinely.clip(id).stop().seek(0);
  splinely.clip('walk').setWeight(1); splinely.clip('jump').setWeight(0);
  runner.send('RESET'); scene.send('RESET');
  splinely.timeline.seek(0);
  for (const id of [...scrollIds, 'walk']) splinely.clip(id).play();
  splinely.timeline.play();
});
"""

HTML = '''<!doctype html>
<html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Dune / Stride — cactus runner</title>
<style>
*{box-sizing:border-box}body{margin:0;background:#13292b;color:#f8eed6;font:16px system-ui}main{max-width:1100px;margin:5vh auto;padding:24px}.eyebrow{font-size:12px;color:#e8b979;letter-spacing:.28em;text-transform:uppercase}h1{font-size:clamp(36px,7vw,74px);letter-spacing:-.06em;margin:12px 0}h1 span{color:#e8b979}p{color:#b7c8bc;line-height:1.6}.frame{border-radius:22px;overflow:hidden;border:1px solid #46605a;box-shadow:0 25px 75px #0004}#scene{display:block;width:100%;height:auto;aspect-ratio:16/9;background:#eedbb4}.controls{display:flex;gap:10px;flex-wrap:wrap;padding:18px;background:#203b3b}button{font:inherit;color:#f8eed6;background:#2d504d;border:1px solid #668279;border-radius:9px;padding:10px 17px;cursor:pointer}button:disabled{opacity:.4}button:focus-visible{outline:3px solid #e8b979}#jump{background:#e8b979;color:#13292b}output{margin-left:auto;font:12px ui-monospace,monospace;align-self:center}footer{font-size:13px;margin-top:22px;color:#b7c8bc}a{color:#e8b979}
</style><main><div class="eyebrow">Splinely / motion field notes 009</div><h1>Dune <span>/ Stride</span></h1>
<p>A copper-scarfed wanderer, a quiet desert, an endless trail.<br>Click the scene or press Space to jump. The world never waits for your feet.</p>
<section class="frame">__EMBEDDED_SVG__
<div class="controls"><button id="play" disabled>Play</button><button id="pause" disabled>Pause</button><button id="restart" disabled>Restart</button><button id="jump" disabled>Jump / Space</button><output id="status">Loading trusted runtime…</output></div></section>
<footer>Weighted bone poses, independent scene layers, seeded cactus loop. No collision penalty: this is a motion study, not a survival game.<br><a href="cactus-runner.splinely.json">Editable project</a> · <a href="cactus-runner.svg">Offline SVG</a> · <a href="../docs/cactus-runner.md">Technical notes</a><br>Reviewed custom JavaScript requires an explicit trusted export. No external assets, libraries, audio or network.</footer></main>
<script>
const scene=document.querySelector('#scene');let f;
function ready(){if(f)return;f=scene?.splinely;if(!f)return;
for(const id of ['play','pause','restart','jump']){const b=document.getElementById(id);b.disabled=false;b.onclick=()=>f.emit('runner.'+id)}
scene.setAttribute('tabindex','0');
document.addEventListener('keydown',e=>{if(e.code==='Space'){e.preventDefault();if(!e.repeat)f.emit('runner.jump')}});
function status(){document.querySelector('#status').textContent=f.machine('scene').state+' / '+f.machine('runner').state+' / walk '+f.clip('walk').weight.toFixed(2)+' + jump '+f.clip('jump').weight.toFixed(2);requestAnimationFrame(status)}status();}
ready();
</script></html>
'''


def build():
    nodes, bones = [], []

    def node(id, kind, color, bone=None, x=0, y=0):
        color = color.lstrip('#')
        nodes.append({'id': id, 'name': id, 'kind': kind, 'boneId': bone,
                      'transform': {'x': x, 'y': y, 'rotation': 0, 'scaleX': 1, 'scaleY': 1},
                      'style': {'fill': dict(zip('rgba', [int(color[i:i+2], 16)/255 for i in (0,2,4)]+[1])),
                                'stroke': None, 'strokeWidth': 1, 'opacity': 1}})

    def path(id, d, color, bone=None, x=0, y=0):
        node(id, {'type': 'path', 'd': d, 'bounds': [-150, -150, 3200, 700]}, color, bone, x, y)

    def circle(id, cx, cy, r, color, bone=None):
        node(id, {'type': 'circle', 'cx': cx, 'cy': cy, 'radius': r}, color, bone)

    node('sky', {'type':'rect','x':0,'y':0,'width':960,'height':540,'rx':0}, '#eedbb4')
    circle('sun-ring', 759, 118, 63, '#e7c797')
    circle('sun', 759, 118, 43, '#f7edce')
    # Three repeated tiles make scroll wrapping visually continuous.
    for i in range(3):
        x = i*960
        path(f'ridge-{i}', 'M0 365 Q170 275 320 334 Q480 390 620 305 Q790 235 960 365 V540 H0 Z', '#c6c7a5', x=x)
        path(f'dune-{i}', 'M0 402 Q230 345 480 402 Q720 459 960 402 V540 H0 Z', '#ddc197', x=x)
    node('sand', {'type':'rect','x':0,'y':426,'width':960,'height':114,'rx':0}, '#d1a577')
    for i in range(3):
        x=i*960
        path(f'trail-{i}', 'M0 426 H960 V429 H0 Z M30 472 H106 V474 H30 Z M242 451 H275 V453 H242 Z M415 503 H529 V505 H415 Z M690 463 H756 V465 H690 Z M865 490 H911 V492 H865 Z', '#b48760', x=x)
    path('shadow', 'M165 430 C165 419 302 419 302 430 C302 441 165 441 165 430 Z', '#ba9069')
    rng = random.Random(SEED)
    cactus_tracks=[]
    for i in range(8):
        x=400+i*320+rng.randrange(-55,56)
        h=rng.randrange(42,82)
        id=f'cactus-{i}'
        # Rounded asymmetric arms, original silhouette; intermittent gaps are seeded.
        path(id, f'M-7 0 V{-h+7} Q-7 {-h-7} 7 {-h+7} V{-h*.48} H19 V{-h*.77} Q19 {-h*.77-10} 29 {-h*.77} V{-h*.4} Q29 {-h*.3} 7 {-h*.3} V0 Z M-7 {-h*.34} H-24 Q-31 {-h*.34} -31 {-h*.47} V{-h*.7} Q-31 {-h*.7-9} -21 {-h*.7} V{-h*.48} H-7 Z', '#466b58' if i%2 else '#385d4f', x=x, y=426)
        wrap=(x+140)/SPEED
        keys=[(0,x),(wrap,-140),(wrap+.001,PERIOD-140-SPEED*.001),(24,x)]
        cactus_track=track(id,'x',keys)
        # Canonical times must be strictly increasing. Step across the 1ms
        # offscreen wrap interval; never interpolate a cactus through the runner.
        cactus_track['keyframes'][1]['easing']={'type':'step'}
        cactus_tracks.append(cactus_track)
    # Local child offsets rotate with the parent; legs return to hip from torso tip.
    rig=[('torso',None,230,310,-87,68),('neck','torso',0,0,-3,12),
         ('arm-back','torso',-7,0,143,42),('fore-back','arm-back',0,0,-28,39),
         ('leg-back','torso',-68,0,173,57),('shin-back','leg-back',0,0,14,57),
         ('leg-front','torso',-68,0,173,57),('shin-front','leg-front',0,0,14,57),
         ('arm-front','torso',-7,0,143,42),('fore-front','arm-front',0,0,-28,39)]
    for id,parent,x,y,a,length in rig:
        bones.append({'id':id,'parentId':parent,'x':x,'y':y,'rotation':a,'length':length})
    for id in ['leg-back','shin-back','arm-back','fore-back','torso','leg-front','shin-front','neck','arm-front','fore-front']:
        length=next(b['length'] for b in bones if b['id']==id)
        r=5 if id=='torso' else 3.8
        path(id+'-ink', f'M0 {-r} H{length} A{r} {r} 0 0 1 {length} {r} H0 A{r} {r} 0 0 1 0 {-r} Z', '#64776a' if 'back' in id else '#233f3e', id)
        circle(id+'-joint',0,0,r+1,'#64776a' if 'back' in id else '#233f3e',id)
        if id.startswith('shin'):
            path(id+'-shoe','M-5 -4 H12 Q18 -4 18 3 H-5 Z','#233f3e',id,x=length)
    circle('head',17,0,17,'#233f3e','neck')
    circle('eye',21,11,2.2,'#f8eed6','neck')
    path('scarf','M-3 -8 H7 V8 H-3 Z M0 4 Q-17 13 -31 7 L-26 17 Q-9 18 5 8 Z','#bc704b','neck')
    # Both adjacent clips contribute to identical angle channels, not parallel-state fakery.
    times=[0,.2,.4,.6,.8]
    walk=[]
    angles={'torso':[-87,-83,-87,-91,-87], 'arm-front':[120,143,166,143,120],
            'fore-front':[-42,-28,-18,-28,-42], 'arm-back':[166,143,120,143,166],
            'fore-back':[-18,-28,-42,-28,-18], 'leg-front':[149,173,197,173,149],
            'shin-front':[20,38,10,8,20], 'leg-back':[197,173,149,173,197],
            'shin-back':[10,8,20,38,10]}
    for id,vals in angles.items(): walk.append(track(id,'rotation',zip(times,vals)))
    jump=[]
    tucked={'torso':[-87,-78,-85,-87], 'arm-front':[120,72,90,120],
            'fore-front':[-42,-65,-50,-42], 'arm-back':[166,195,170,166],
            'fore-back':[-18,-70,-45,-18], 'leg-front':[149,133,170,149],
            'shin-front':[20,85,28,20], 'leg-back':[197,215,185,197],
            'shin-back':[10,70,35,10]}
    for id,vals in tucked.items(): jump.append(track(id,'rotation',zip([0,.3,.7,1.1],vals)))
    jump.append(track('torso','y',[(0,310),(.15,274),(.4,190),(.55,177),(.7,190),(.95,274),(1.1,310)]))
    def clip(id,duration,tracks,loop=True,weight=1,mode='override'):
        return {'id':id,'name':id,'duration':duration,'looping':loop,'autoplay':id!='jump',
                'blendMode':mode,'weight':weight,'tracks':tracks}
    clips=[clip('scroll',24,cactus_tracks),
           clip('terrain',8,[track(f'trail-{i}','x',[(0,i*960),(8,(i-1)*960)]) for i in range(3)]),
           clip('distance',32,[track(f'{prefix}-{i}','x',[(0,i*960),(32,(i-1)*960)]) for prefix in ['ridge','dune'] for i in range(3)]),
           clip('walk',.8,walk,mode='weighted'),clip('jump',1.1,jump,False,0,'weighted')]
    machines=[{'id':'scene','initial':'running','states':[{'id':'running'},{'id':'paused'}],
               'transitions':[transition('running','paused','PAUSE'),transition('paused','running','RESUME')]+
               [transition(s,'running','RESET') for s in ['paused','running']]},
              {'id':'runner','initial':'ground','states':[
                  {'id':'ground','onEnter':[blend('walk',1),blend('jump',0)]},
                  {'id':'jumping','onEnter':[{'type':'playClip','clipId':'jump'},blend('walk',0),blend('jump',1)]}],
               'transitions':[transition('ground','jumping','JUMP'),transition('jumping','ground','LAND')]+
               [transition(s,'ground','RESET') for s in ['ground','jumping']]}]
    return {'schemaVersion':1,'name':'Dune / Stride — original cactus runner','width':960,'height':540,
            'nodes':nodes,'bones':bones,'clips':clips,'stateMachines':machines,'variables':{},
            'audioAssets':[],'audioCues':[],'speechCues':[],'customJs':CONTROLLER,'rawSvg':''}


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--cli',type=Path,default=ROOT/'target/debug/splinely-cli')
    p.add_argument('--output',type=Path,default=ROOT/'examples')
    p.add_argument('--author-only',action='store_true',help='Write draft JSON only, without CLI validation or SVG/HTML export')
    args=p.parse_args()
    args.output.mkdir(parents=True,exist_ok=True)
    project=build()
    filename=args.output/'cactus-runner.splinely.json'
    filename.write_text(json.dumps(project,ensure_ascii=False,indent=2)+'\n')
    if args.author_only:
        print('AUTHORED only: cactus-runner JSON; SVG/HTML export not attempted.')
        return
    def cli(*words):
        result=subprocess.run([str(args.cli.resolve()),*map(str,words)],check=True,capture_output=True,text=True)
        return json.loads(result.stdout)
    # Import every original vector through the real CLI, then attach rig,
    # transforms, animation and machines using its validated merge-patch path.
    with tempfile.TemporaryDirectory(prefix='cactus-author-', dir=os.environ['TMPDIR']) as temp:
        temp=Path(temp)
        source=ET.Element('svg', xmlns='http://www.w3.org/2000/svg', width='960', height='540', viewBox='0 0 960 540')
        for n in project['nodes']:
            kind=n['kind']
            attrs={k:str(v) for k,v in kind.items() if k not in ('type','bounds')}
            if kind['type']=='circle': attrs['r']=attrs.pop('radius')
            attrs.update(id=n['id'], fill='#'+''.join(f"{round(n['style']['fill'][c]*255):02x}" for c in 'rgb'))
            ET.SubElement(source,kind['type'],attrs)
        ET.ElementTree(source).write(temp/'source.svg',encoding='unicode')
        cli('import-svg',temp/'source.svg','--output',temp/'imported.json')
        imported=json.loads((temp/'imported.json').read_text())
        kinds={n['id']:n['kind'] for n in imported['nodes']}
        assert set(kinds)=={n['id'] for n in project['nodes']}, 'Import lost source geometry'
        for n in project['nodes']: n['kind']=kinds[n['id']]
        (temp/'patch.json').write_text(json.dumps(project,ensure_ascii=False))
        cli('apply',temp/'imported.json','--patch',temp/'patch.json','--output',filename)
    cli('validate',filename)
    svg=args.output/'cactus-runner.svg'
    cli('export',filename,'--output',svg,'--trusted-js')
    tree=ET.parse(svg)
    exported=json.loads(tree.find("{http://www.w3.org/2000/svg}metadata[@id='splinely-project']").text)
    assert exported['clips'][3]['blendMode']=='weighted' and exported['clips'][4]['weight']==0, 'Old CLI dropped weighted schema; rebuild before export'
    assert exported['stateMachines'][1]['states'][1]['onEnter'][1]['type']=='blendClip'
    embedded=svg.read_text().replace('<svg ', '<svg id="scene" role="img" aria-label="Walking stickfigure; click to jump" ', 1)
    (args.output/'cactus-runner.html').write_text(HTML.replace('__EMBEDDED_SVG__',embedded))
    # Independent static FK sanity from the compiled core's evaluator.
    evaluated=cli('evaluate',filename,'--time',0)
    posed={n['id']:n for n in evaluated['nodes']}
    torso=posed['torso-ink']['transform']
    arm=posed['arm-front-ink']['transform']
    a=math.radians(torso['rotation'])
    assert abs(arm['x']-(torso['x']+61*math.cos(a)))<1e-7
    assert abs(arm['y']-(torso['y']+61*math.sin(a)))<1e-7
    print('EXPORTED: compiled CLI import/apply/validate/export/evaluate FK confirmed weighted schema. Run scripts/runner-smoke.py to verify Firefox runtime.')


if __name__=='__main__':
    main()
