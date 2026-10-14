#!/usr/bin/env python3
"""Author Sand / Accord: original 20s desert sparring film; trusted export only."""
import argparse
import json
import math
import os
from pathlib import Path
import subprocess
import tempfile
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
os.environ['TMPDIR'] = str(ROOT / '.work-tmp')
PHASES = [('approach',0,3), ('punches',3,7), ('sweep',7,11), ('impact',11,15.5), ('accord',15.5,20)]
# time, hip x/y, torso world angle, front upper/fore, rear upper/fore,
# front/rear foot x offsets, foot height from ground (vault only).
POSES = [
(0,210,316,-88,25,-75,155,95,30,-32,0),
(.7,253,311,-84,145,100,20,-65,-30,31,0),
(1.4,295,316,-89,20,-70,145,95,31,-30,0),
(2.1,337,311,-83,140,95,20,-65,-30,30,0),
(3,385,319,-87,-25,-85,35,-75,32,-32,0),
(3.6,378,326,-102,20,-120,45,-80,30,-35,0),
(4.05,428,316,-71,-5,-8,60,-85,32,-40,0),
(4.35,425,320,-80,-15,-55,35,-80,32,-35,0),
(4.85,382,330,-115,-35,-100,45,-70,40,-30,0),
(5.4,392,321,-87,-20,-80,30,-65,32,-32,0),
(5.85,429,316,-70,-4,-8,50,-75,32,-38,0),
(6.3,405,325,-95,-30,-85,35,-65,35,-30,0),
(7,388,324,-93,-25,-80,35,-75,35,-35,0),
(7.65,410,357,-140,20,-70,80,-30,68,-28,0),
(8.15,446,365,-148,25,-20,90,-15,100,-25,0),
(8.65,420,352,-125,-20,-70,65,-35,60,-30,0),
(9.2,385,329,-96,-25,-80,35,-75,35,-35,0),
(10,393,322,-87,-20,-85,30,-70,34,-34,0),
(11,400,324,-93,-20,-85,30,-75,34,-34,0),
(11.45,391,332,-108,15,-115,45,-75,32,-35,0),
(11.85,433,315,-72,-4,-5,60,-85,34,-40,0),
(12.25,440,314,-69,-3,-4,65,-90,34,-40,0),
(13.25,442,315,-70,-3,-6,65,-88,35,-40,0),
(13.65,410,323,-96,-30,-90,35,-75,35,-32,0),
(14.5,386,328,-101,-25,-80,40,-70,35,-32,0),
(15.5,380,320,-88,-20,-85,35,-70,32,-32,0),
(16.3,376,322,-91,45,65,110,90,30,-30,0),
(17.2,400,317,-84,-10,-15,100,85,30,-32,0),
(17.65,423,319,-82,-8,-8,105,90,30,-32,0),
(18.2,400,322,-91,55,75,110,95,32,-30,0),
(19,365,318,-90,95,95,90,90,32,-32,0),
(19.5,365,314,-86,100,100,85,85,32,-32,0),
(20,365,318,-90,95,95,90,90,32,-32,0)]
# Partner answers, rather than duplicating the same choreography.
ANSWERS = {3.6:(388,320,-88,-20,-80),4.05:(373,342,-132,-45,-115),4.35:(378,334,-113,-25,-90),
4.85:(430,316,-72,-4,-5),5.85:(374,340,-126,-40,-110),
7.65:(390,292,-78,-65,-115),8.15:(408,240,-65,-70,-140),8.65:(423,252,-78,-45,-120),
9.2:(405,328,-105,-20,-85),11.85:(422,316,-82,-25,5),12.25:(410,324,-96,-35,15),
13.25:(387,337,-115,-50,40),13.65:(376,343,-123,-55,45),14.5:(380,327,-101,-25,-85)}


def track(id, prop, keys):
    return {'nodeId':id,'property':prop,'keyframes':[{'time':t,'value':{'type':'number','value':v},'easing':{'type':'linear'}} for t,v in keys]}


def capsule(length, r):
    return f'M0 {-r} H{length} A{r} {r} 0 0 1 {length} {r} H0 A{r} {r} 0 0 1 0 {-r} Z'


CONTROLLER = """
// Trusted transport only. Geometry and phase progression are canonical clips/actions.
const ids=['approach','punches','sweep','impact','accord'], story=forma.machine('story');
forma.on('film.pause',()=>forma.timeline.pause());
forma.on('film.play',()=>{if(story.state==='finished')return;forma.clip(story.state).play();forma.timeline.play();});
forma.on('film.replay',()=>{forma.timeline.pause();for(const id of ids)forma.clip(id).stop();forma.timeline.seek(0);story.send('RESET');forma.timeline.play();});
forma.on('state.change',d=>{if(story.state==='finished')forma.timeline.pause();});
"""


def build():
    shapes, binds, bones, tracks = [], {}, [], []
    def path(id,d,color,bone=None):
        shapes.append(f'<path id="{id}" d="{d}" fill="{color}"/>')
        if bone: binds[id]=bone
    def circle(id,x,y,r,color,bone=None):
        shapes.append(f'<circle id="{id}" cx="{x}" cy="{y}" r="{r}" fill="{color}"/>')
        if bone: binds[id]=bone
    shapes.append('<rect id="sky" width="960" height="540" fill="#eedbb4"/>')
    circle('sun-ring',759,118,63,'#e7c797'); circle('sun',759,118,43,'#f7edce')
    path('ridge','M-200 365 Q170 275 320 334 Q480 390 620 305 Q790 235 1160 365 V540 H-200 Z','#c6c7a5')
    path('dune','M-200 402 Q230 345 480 402 Q720 459 1160 402 V540 H-200 Z','#ddc197')
    path('sand','M0 426 H960 V540 H0 Z','#d1a577')
    path('trail','M0 426 H960 V429 H0 Z M30 472 H106 V474 H30 Z M242 451 H275 V453 H242 Z M415 503 H529 V505 H415 Z M690 463 H756 V465 H690 Z M865 490 H911 V492 H865 Z','#b48760')
    for id,speed in [('ridge',-1.5),('dune',-3)]: tracks.append(track(id,'x',[(0,0),(20,20*speed)]))
    for i,x in enumerate([100,255,745,925,1110]):
        h=[72,45,63,83,52][i]
        id=f'cactus-{i}'
        path(id,f'M-7 0 V{-h+7} Q-7 {-h-7} 7 {-h+7} V{-h*.48} H19 V{-h*.77} Q19 {-h*.77-10} 29 {-h*.77} V{-h*.4} Q29 {-h*.3} 7 {-h*.3} V0 Z M-7 {-h*.34} H-24 Q-31 {-h*.34} -31 {-h*.47} V{-h*.7} Q-31 {-h*.7-9} -21 {-h*.7} V{-h*.48} H-7 Z','#466b58')
        tracks.extend([track(id,'x',[(0,x),(20,x-90)]),track(id,'y',[(0,426),(20,426)])])
    lengths={'torso':68,'neck':12,'arm-back':42,'fore-back':39,'leg-back':57,'shin-back':57,'leg-front':57,'shin-front':57,'arm-front':42,'fore-front':39}
    parents={'torso':None,'neck':'torso','arm-back':'torso','fore-back':'arm-back','leg-back':'torso','shin-back':'leg-back','leg-front':'torso','shin-front':'leg-front','arm-front':'torso','fore-front':'arm-front'}
    for side in ['reed','clay']:
        mirror=side=='clay'; authored={}
        for p in POSES:
            t,x,y,a,u,f,b,bf,ff,rf,lift=p
            if mirror and t in ANSWERS:
                x,y,a,u,f=ANSWERS[t]
                if t in [7.65,8.15,8.65]: lift=426-y-75; ff,rf=38,-38
            wx=960-x if mirror else x
            angle=lambda v: -180-v if mirror else v
            vals={'torso':(wx,y,angle(a)), 'neck':(0,0,angle(-90)-angle(a)),
                  'arm-front':(-7,0,angle(u)-angle(a)), 'fore-front':(0,0,angle(f)-angle(u)),
                  'arm-back':(-7,0,angle(b)-angle(a)), 'fore-back':(0,0,angle(bf)-angle(b))}
            for name,offset in [('leg-front',ff),('leg-back',rf)]:
                dx,dy=offset,426-lift-y
                # Reachable stylized IK; the extended sweep intentionally lifts its heel.
                d=min(113.5,math.hypot(dx,dy)); bend=math.degrees(math.acos(d/114))
                upper=math.degrees(math.atan2(dy,dx))-bend
                vals[name]=(-68,0,angle(upper)-angle(a))
                vals[name.replace('leg','shin')]=(0,0,angle(upper+2*bend)-angle(upper))
            for name,v in vals.items(): authored.setdefault(name,[]).append((t,v))
        for name,length in lengths.items():
            id=f'{side}-{name}'; x,y,a=authored[name][0][1]
            bones.append({'id':id,'parentId':f'{side}-{parents[name]}' if parents[name] else None,'x':x,'y':y,'rotation':a,'length':length})
            for j,prop in enumerate(['x','y','rotation']): tracks.append(track(id,prop,[(t,v[j]) for t,v in authored[name]]))
        # Both have the runner's dark teal ink, distinguished by scarf and cuff accents.
        ink='#233f3e' if not mirror else '#2d5050'; accent='#bc704b' if not mirror else '#64a99d'
        for name,length in lengths.items():
            id=f'{side}-{name}'; color='#64776a' if 'back' in name else ink
            path(id+'-ink',capsule(length,5 if name=='torso' else 3.8),color,id)
            circle(id+'-joint',0,0,4.5,color,id)
            if name.startswith('fore'): circle(id+'-cuff',length,0,5.5,accent,id)
        circle(side+'-head',17,0,17,ink,side+'-neck')
        circle(side+'-eye',21,-11 if mirror else 11,2.2,'#f8eed6',side+'-neck')
        path(side+'-scarf','M-3 -8 H7 V8 H-3 Z M0 4 Q-17 13 -31 7 L-26 17 Q-9 18 5 8 Z',accent,side+'-neck')
        path(side+'-shadow','M-65 0 C-65 -8 65 -8 65 0 C65 8 -65 8 -65 0 Z','#ba9069')
        tracks.extend([track(side+'-shadow','x',[(t,v[0]) for t,v in authored['torso']]),track(side+'-shadow','y',[(0,431),(20,431)])])
    # Dust puffs grow/squash and drift away from grounded steps; rays never depict injury.
    for i,when in enumerate([.7,1.4,2.1,4.05,4.85,8.15,9.2,11.85,13.65,17.65]):
        x=300 if when<3 else 480
        for j in range(3):
            id=f'dust-{i}-{j}'; circle(id,0,0,9+j*3,'#e8c598')
            tracks.extend([track(id,'x',[(0,x),(when,x),(when+.65,x+(j-1)*65),(20,x+(j-1)*65)]),track(id,'y',[(0,424),(when,424),(when+.65,399-j*6),(20,399-j*6)]),track(id,'opacity',[(0,0),(when-.03,0),(when,.65),(when+.65,0),(20,0)]),track(id,'scaleX',[(0,.2),(when,.6),(when+.65,1.8),(20,1.8)]),track(id,'scaleY',[(0,.2),(when,.3),(when+.65,1),(20,1)])])
    for i in range(9):
        a=i*math.tau/9; dx,dy=math.cos(a),math.sin(a); id=f'spark-{i}'
        path(id,f'M{dx*16-dy*2} {dy*16+dx*2} L{dx*43} {dy*43} L{dx*16+dy*2} {dy*16-dx*2} Z','#f8eed6')
        tracks.extend([track(id,'x',[(0,505),(12.25,505),(13.4,505+dx*28),(20,505+dx*28)]),track(id,'y',[(0,267),(12.25,267),(13.4,267+dy*28),(20,267+dy*28)]),track(id,'opacity',[(0,0),(12.24,0),(12.25,1),(12.8,.8),(13.4,0),(20,0)])])
    # Cinematic letterbox; scene remains original vector geometry.
    path('letterbox','M0 0 H960 V28 H0 Z M0 512 H960 V540 H0 Z','#233f3e')
    source='<svg xmlns="http://www.w3.org/2000/svg" width="960" height="540" viewBox="0 0 960 540">'+''.join(shapes)+'</svg>'
    def value(tr,t):
        keys=tr['keyframes']
        for a,b in zip(keys,keys[1:]):
            if a['time']<=t<=b['time']:
                f=(t-a['time'])/(b['time']-a['time']); return a['value']['value']*(1-f)+b['value']['value']*f
        return keys[-1]['value']['value']
    clips=[]
    for id,start,end in PHASES:
        local=[]
        for tr in tracks:
            times=[start]+[k['time'] for k in tr['keyframes'] if start<k['time']<end]+[end]
            local.append(track(tr['nodeId'],tr['property'],[(t-start,value(tr,t)) for t in times]))
        clips.append({'id':id,'name':id,'duration':end-start,'looping':False,'autoplay':False,'tracks':local})
    states=[{'id':id,'onEnter':[{'type':'playClip','clipId':id}]} for id,_,_ in PHASES]+[{'id':'finished'}]
    transitions=[{'from':id,'to':PHASES[i+1][0] if i<4 else 'finished','event':{'name':'clip.end','target':id},'actions':[{'type':'stopClip','clipId':id}] if i<4 else []} for i,(id,_,_) in enumerate(PHASES)]
    transitions += [{'from':s['id'],'to':'approach','event':{'name':'RESET'},'actions':[]} for s in states]
    return source,binds,bones,clips,[{'id':'story','initial':'approach','states':states,'transitions':transitions}]


HTML = '''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Sand / Accord</title>
<style>*{box-sizing:border-box}body{margin:0;background:#13292b;color:#f8eed6;font:16px system-ui}main{max-width:1100px;margin:4vh auto;padding:24px}.eyebrow{color:#e8b979;letter-spacing:.25em;font-size:12px}h1{font-size:clamp(36px,7vw,74px);letter-spacing:-.06em;margin:12px 0}h1 span{color:#e8b979}p,footer{color:#b7c8bc;line-height:1.6}.frame{border:1px solid #46605a;border-radius:22px;overflow:hidden;box-shadow:0 25px 75px #0004}#scene{display:block;width:100%;height:auto;aspect-ratio:16/9}.controls{display:flex;gap:10px;flex-wrap:wrap;padding:18px;background:#203b3b}button{font:inherit;color:#f8eed6;background:#2d504d;border:1px solid #668279;border-radius:9px;padding:10px 17px;cursor:pointer}button:disabled{opacity:.4}button:focus-visible{outline:3px solid #e8b979}output{margin-left:auto;align-self:center;font:12px monospace}progress{width:100%;accent-color:#e8b979}footer{font-size:13px}a{color:#e8b979}</style>
<main><div class="eyebrow">FORMA / ORIGINAL DESERT SHORT</div><h1>Sand <span>/ Accord</span></h1><p>Reed & Clay: a playful disagreement on the trail.<br>Approach → jab & slip → sweep & vault → stretched-time impact → a friendly fist bump.</p><section class="frame">__SVG__<div class="controls"><button id="play" disabled>Play</button><button id="pause" disabled>Pause</button><button id="replay" disabled>Replay</button><output id="status">Loading…</output><progress id="progress" max="20" value="0" aria-label="Film progress"></progress></div></section><footer>20 seconds · Original vector silhouettes and choreography · No gore, audio, external assets or network.<br><a href="desert-stickfight.frm">Editable project</a> · <a href="desert-stickfight.svg">Standalone SVG</a> · <a href="../docs/desert-stickfight.md">Notes</a></footer></main>
<script>const scene=document.querySelector('#scene'), f=scene.forma; const starts={approach:0,punches:3,sweep:7,impact:11,accord:15.5};
if(f){for(const id of ['play','pause','replay']){const b=document.getElementById(id);b.disabled=false;b.onclick=()=>f.emit('film.'+id)}
function status(){const state=f.machine('story').state,t=state==='finished'?20:starts[state]+f.clip(state).time;document.querySelector('#status').textContent=state+' / '+t.toFixed(1)+' / 20s'+(f.timeline.playing?'':' / held');document.querySelector('#progress').value=t;requestAnimationFrame(status)}status()}
</script></html>'''


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli',type=Path,default=ROOT/'target/debug/forma-cli')
    parser.add_argument('--output',type=Path,default=ROOT/'examples')
    args=parser.parse_args(); args.output.mkdir(parents=True,exist_ok=True); Path(os.environ['TMPDIR']).mkdir(exist_ok=True)
    def cli(*words):
        r=subprocess.run([str(args.cli.resolve()),*map(str,words)],check=True,capture_output=True,text=True)
        return json.loads(r.stdout)
    source,binds,bones,clips,machines=build()
    (args.output/'desert-stickfight-source.svg').write_text(source)
    with tempfile.TemporaryDirectory(prefix='desert-author-',dir=os.environ['TMPDIR']) as tmp:
        tmp=Path(tmp); imported=tmp/'imported.json'
        cli('import-svg',args.output/'desert-stickfight-source.svg','--output',imported)
        nodes=json.loads(imported.read_text())['nodes']
        for n in nodes:
            if n['id'] in binds: n['boneId']=binds[n['id']]
        patch={'name':'Sand / Accord — original desert sparring film','nodes':nodes,'bones':bones,'clips':clips,'stateMachines':machines,'customJs':CONTROLLER}
        (tmp/'patch.json').write_text(json.dumps(patch))
        project=args.output/'desert-stickfight.frm'
        cli('apply',imported,'--patch',tmp/'patch.json','--output',project); cli('validate',project)
        svg=args.output/'desert-stickfight.svg'
        cli('export',project,'--output',svg,'--trusted-js')
        ET.parse(svg)
        embedded=svg.read_text().replace('<svg ','<svg id="scene" role="img" aria-label="Two teal stickfigures spar in a warm cactus desert" ',1)
        (args.output/'desert-stickfight.html').write_text(HTML.replace('__SVG__',embedded))
    print('EXPORTED: import/apply/validate/trusted export; run scripts/desert-stickfight-smoke.py for real Firefox evidence.')


if __name__=='__main__': main()
