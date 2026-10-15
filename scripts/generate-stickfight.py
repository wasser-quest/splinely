#!/usr/bin/env python3
"""Original Cinder / Flux duel. Rebuild only examples/stickfight* via splinely-cli.

Child bones start at parent tips; leg offsets return to the torso root. All
motion is canonical numeric bone/scene tracks, not custom JavaScript animation.
"""
import argparse
import json
import math
from pathlib import Path
import subprocess
import tempfile
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
DURATION = 8.0
TIMES = [0, .8, 1.25, 1.65, 1.9, 2.3, 3.1, 3.6, 3.85, 4.5, 5.15, 5.5, 5.8, 6.3, 7.2, 8]
# hip x/y, torso world angle, front upper/forearm world angles,
# rear upper/forearm world angles. Coordinates face right; Flux is mirrored.
GUARD = (350, 318, -88, -22, -90, 38, -65)
CINDER = [GUARD, (342,320,-100,15,-115,45,-70),
          (370,316,-78,-18,-32,65,-90), (422,312,-68,-5,-5,70,-90),
          (416,317,-75,5,-45,55,-70), GUARD,
          (354,320,-92,-20,-85,40,-65), (349,320,-105,-60,-110,25,-70),
          (306,323,-117,-80,-135,60,-10), (329,325,-105,-40,-90,40,-65),
          GUARD, (397,314,-73,-8,-12,55,-90),
          (382,318,-87,-35,-90,25,-100), (338,321,-96,-25,-90,40,-65), GUARD, GUARD]
FLUX = [GUARD, GUARD, (340,330,-110,-10,-90,60,-55),
        (323,339,-126,-35,-115,60,-70), (330,330,-111,-20,-90,50,-60), GUARD,
        (337,324,-102,40,-135,55,-65), (470,314,-68,-4,-4,75,-85),
        (400,317,-76,-5,-35,65,-85), GUARD,
        (348,318,-87,-20,-90,40,-65), (367,315,-89,-35,80,15,-100),
        (381,318,-81,-8,-5,55,-85), (340,321,-95,-25,-90,40,-65), GUARD, GUARD]


def track(target, prop, values):
    return {'nodeId': target, 'property': prop, 'keyframes': [
        {'time': t, 'value': {'type': 'number', 'value': v}, 'easing': {'type': 'linear'}}
        for t, v in values]}


def capsule(length, width):
    r = width / 2
    return f'M0 {-r} H{length} A{r} {r} 0 0 1 {length} {r} H0 A{r} {r} 0 0 1 0 {-r} Z'


def leg_angles(hip, foot):
    dx, dy = foot[0]-hip[0], foot[1]-hip[1]
    d = math.hypot(dx, dy)
    assert 0 < d < 124, (hip, foot)
    a = math.degrees(math.atan2(dy, dx))
    bend = math.degrees(math.acos(d / 124))
    return a-bend, 2*bend


def build_scene():
    shapes = []
    binds = {}
    bones = []
    tracks = []

    def path(id, d, color, bone=None):
        shapes.append(f'<path id="{id}" d="{d}" fill="{color}"/>')
        if bone:
            binds[id] = bone

    def circle(id, x, y, r, color, bone=None):
        shapes.append(f'<circle id="{id}" cx="{x}" cy="{y}" r="{r}" fill="{color}"/>')
        if bone:
            binds[id] = bone

    shapes.append('<rect id="stage" width="960" height="540" fill="#0b101d"/>')
    circle('orb-outer', 480, 246, 205, '#111d30')
    circle('orb-inner', 480, 246, 168, '#142239')
    path('horizon', 'M0 432 H960 V540 H0 Z', '#101929')
    path('floor-line', 'M55 431 H905 V433 H55 Z', '#30465d')
    for i in range(7):
        x = 110+i*123
        path(f'floor-grid-{i}', f'M{x} 433 L{x-70} 540 L{x-69} 540 L{x+1} 433 Z', '#1b293c')
    for side, x, color in [('cinder',350,'#ffb15e'), ('flux',610,'#64e7ec')]:
        path(f'{side}-shadow', 'M-85 0 C-85 -12 85 -12 85 0 C85 12 -85 12 -85 0 Z', '#070c16')
        tracks.append(track(f'{side}-shadow','x',zip(TIMES, [p[0] if side=='cinder' else 960-p[0] for p in (CINDER if side=='cinder' else FLUX)])))
        tracks.append(track(f'{side}-shadow','y',[(0,434),(8,434)]))
        path(f'{side}-marker', f'M{x-55} 465 H{x+55} V467 H{x-55} Z', color)

    for side, poses, color, trim in [('cinder', CINDER, '#ffb15e', '#fff0d5'), ('flux', FLUX, '#64e7ec', '#deffff')]:
        mirror = side == 'flux'
        def angle(a):
            return -180-a if mirror else a
        # Mirroring uses continuous angle values (never short-arc interpolation).
        authored = {}
        for t, (x,y,torso,upper,fore,back,bfore) in zip(TIMES, poses):
            x = 960-x if mirror else x
            torso,upper,fore,back,bfore = map(angle, (torso,upper,fore,back,bfore))
            vals = {'torso': (x,y,torso), 'neck': (0,0,0),
                    'arm-front': (0,0,upper-torso), 'fore-front': (0,0,fore-upper),
                    'arm-back': (0,0,back-torso), 'fore-back': (0,0,bfore-back)}
            for name, offset in [('leg-back',-35), ('leg-front',44)]:
                foot = (x+(-offset if mirror else offset), 426)
                a,b = leg_angles((x,y),foot)
                vals[name] = (-88,0,a-torso)
                vals[name.replace('leg','shin')] = (0,0,b)
            for name, value in vals.items():
                authored.setdefault(name, []).append((t,value))
        lengths = {'torso':88, 'neck':18, 'arm-front':57, 'fore-front':57,
                   'arm-back':57, 'fore-back':57, 'leg-front':62, 'shin-front':62,
                   'leg-back':62, 'shin-back':62}
        parents = {'torso':None, 'neck':'torso', 'arm-front':'torso','arm-back':'torso',
                   'fore-front':'arm-front','fore-back':'arm-back', 'leg-front':'torso',
                   'leg-back':'torso', 'shin-front':'leg-front', 'shin-back':'leg-back'}
        for name, length in lengths.items():
            id = f'{side}-{name}'
            x,y,a = authored[name][0][1]
            bones.append({'id':id, 'parentId':f'{side}-{parents[name]}' if parents[name] else None,
                          'x':x, 'y':y, 'rotation':a, 'length':length})
            for index, prop in enumerate(['x','y','rotation']):
                values = [(t,v[index]) for t,v in authored[name]]
                if len(set(v for _,v in values)) > 1:
                    tracks.append(track(id, prop, values))
        # Back limbs first; foreground limbs/head last. Rounded filled paths survive import.
        for name in ['leg-back','shin-back','arm-back','fore-back','torso','leg-front','shin-front','neck','arm-front','fore-front']:
            id = f'{side}-{name}'
            shade = '#b66c3d' if side=='cinder' else '#278eaa'
            path(f'{id}-shape', capsule(lengths[name], 13 if name=='torso' else 9), shade if 'back' in name else color, id)
            circle(f'{id}-joint', 0, 0, 5.5, trim if name=='torso' else color, id)
            if name.startswith('fore'):
                circle(f'{id}-fist', lengths[name], 0, 8, trim, id)
            if name.startswith('shin'):
                path(f'{id}-boot', capsule(17, 10), color, id)
                # local transform moves the foot shape to the shin tip after import
        circle(f'{side}-head', 18, 0, 19, color, f'{side}-neck')
        # face band points into the fight, remaining attached in local neck coordinates
        path(f'{side}-visor', 'M17 3 H23 V17 H17 Z' if not mirror else 'M17 -17 H23 V-3 H17 Z', '#0b101d', f'{side}-neck')
        path(f'{side}-chest-mark', 'M28 -3 H68 V3 H28 Z', trim, f'{side}-torso')

    # Hand-drawn tapered impact rays; keyed opacity/translation creates crisp bursts.
    for event, when, x, y, color in [('dodge',1.65,570,220,'#ffb15e'),('hit',3.6,351,239,'#fff0d5'),('block',5.5,480,223,'#64e7ec')]:
        for i in range(8):
            a = i*math.pi/4
            dx,dy = math.cos(a),math.sin(a)
            px,py = -dy*2,dx*2
            d = f'M{dx*18+px} {dy*18+py} L{dx*48} {dy*48} L{dx*18-px} {dy*18-py} Z'
            id = f'{event}-spark-{i}'
            path(id,d,color)
            tracks.extend([track(id,'opacity',[(0,0),(when-.07,0),(when,1),(when+.22,0),(8,0)]),
                           track(id,'x',[(0,x),(when,x),(when+.22,x+dx*24),(8,x+dx*24)]),
                           track(id,'y',[(0,y),(when,y),(when+.22,y+dy*24),(8,y+dy*24)])])
    svg = '<svg xmlns="http://www.w3.org/2000/svg" width="960" height="540" viewBox="0 0 960 540">\n'+'\n'.join(shapes)+'\n</svg>\n'
    return svg, binds, bones, tracks


HTML = '''<!doctype html>
<html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Cinder / Flux · Splinely stickfight</title>
<style>
*{box-sizing:border-box}body{margin:0;background:#080d17;color:#edf5ff;font:16px system-ui,sans-serif}main{max-width:1120px;margin:6vh auto;padding:24px}header{display:flex;justify-content:space-between;align-items:end;gap:24px}.eyebrow{color:#70dce7;text-transform:uppercase;letter-spacing:.25em;font-size:12px}h1{font-size:clamp(32px,6vw,64px);letter-spacing:-.05em;margin:10px 0}h1 span{color:#ffb15e}p{color:#9aacbf;line-height:1.6}.frame{margin-top:24px;border:1px solid #2a3c54;border-radius:18px;overflow:hidden;box-shadow:0 24px 80px #0008;background:#0b101d}object{width:100%;aspect-ratio:16/9;display:block}.controls{display:flex;flex-wrap:wrap;gap:10px;align-items:center;padding:18px;border-top:1px solid #28374c}#status{margin-left:auto;font:13px ui-monospace,monospace;color:#9bb2c7}.beats{display:grid;grid-template-columns:repeat(4,1fr);gap:12px;margin-top:22px}.beats div{border-top:2px solid #26384b;padding-top:10px;font-size:14px;color:#b2c0d2}.beats b{color:#edf5ff;display:block}footer{margin-top:25px;color:#778ba2;font-size:13px}a{color:#64e7ec}@media(max-width:620px){header{display:block}.beats{grid-template-columns:repeat(2,1fr)}#status{width:100%;margin:0}}
</style>
<main><header><div><div class="eyebrow">Splinely motion study / 008</div><h1><span>Cinder</span> / Flux</h1></div><p>Two temperaments. One dark stage.<br>An original, eight-second kinetic loop that plays by itself.</p></header>
<section class="frame" aria-label="Animated stickfigure duel"><object id="scene" type="image/svg+xml" data="stickfight.svg" aria-label="Amber and cyan fighters exchange punches, dodges and blocks">Open <a href="stickfight.svg">the self-contained SVG</a>.</object>
<div class="controls"><output id="status" aria-live="off">Loading runtime…</output></div></section>
<div class="beats"><div><b>01 / Feint</b>Guard → anticipation</div><div><b>02 / Slip</b>Amber jab → cyan dodge</div><div><b>03 / Counter</b>Cyan retaliation → recoil</div><div><b>04 / Reset</b>Clash → disengage → loop</div></div>
<footer>Original choreography & vector art · Canonical parent-tip FK bones · No gore, external assets or network runtime.<br><a href="stickfight.splinely.json">Editable project</a> · <a href="stickfight.svg">Standalone animated SVG</a></footer></main>
<script>
const obj=document.querySelector('#scene');let f;
function ready(){try{f=obj.contentDocument?.documentElement.splinely}catch(e){return}if(!f)return;
// The 'duel' clip is an autoplay loop in the project data: no button starts it.
function status(){document.querySelector('#status').textContent=(f.timeline.playing?'PLAYING':'PAUSED')+' / '+f.clip('duel').time.toFixed(2)+'s / 8.00s';requestAnimationFrame(status)}status();}
obj.addEventListener('load',ready);ready();
</script></html>
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', type=Path, default=ROOT/'target/debug/splinely-cli')
    parser.add_argument('--output', type=Path, default=ROOT/'examples')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)

    def cli(*words):
        result = subprocess.run([str(args.cli.resolve()), *map(str, words)], check=True, capture_output=True, text=True)
        return json.loads(result.stdout)

    source, binds, bones, tracks = build_scene()
    (args.output/'stickfight-source.svg').write_text(source)
    with tempfile.TemporaryDirectory(prefix='splinely-stickfight-') as tmp:
        tmp = Path(tmp)
        imported = tmp/'imported.json'
        cli('import-svg', args.output/'stickfight-source.svg', '--output', imported)
        nodes = json.loads(imported.read_text())['nodes']
        for node in nodes:
            if node['id'] in binds:
                node['boneId'] = binds[node['id']]
            if node['id'].endswith('-boot'):
                node['transform']['x'] = 62
        patch = {'name':'Cinder / Flux · original stickfight', 'nodes':nodes, 'bones':bones,
                 'clips':[{'id':'duel','name':'Feint / slip / counter / reset','duration':DURATION,
                           'looping':True,'autoplay':True,'tracks':tracks}]}
        patch_file = tmp/'patch.json'
        patch_file.write_text(json.dumps(patch))
        project = args.output/'stickfight.splinely.json'
        cli('apply', imported, '--patch', patch_file, '--output', project)
        cli('validate', project)
        # Evaluate CLI-posed attached nodes and independently check canonical FK.
        samples = []
        for t in [0,1.65,3.6,3.85,5.5,8]:
            evaluated = cli('evaluate', project, '--time', t)
            posed = {n['id']:n for n in evaluated['nodes']}
            samples.append(posed['cinder-arm-front-shape']['transform'])
            for side, poses in [('cinder',CINDER),('flux',FLUX)]:
                pose = poses[TIMES.index(t)]
                x,y,a = pose[:3]
                if side=='flux': x,a = 960-x,-180-a
                shoulder = (x+88*math.cos(math.radians(a)), y+88*math.sin(math.radians(a)))
                actual = posed[f'{side}-arm-front-shape']['transform']
                assert abs(actual['x']-shoulder[0]) < 1e-7 and abs(actual['y']-shoulder[1]) < 1e-7, (t,actual,shoulder)
        assert samples[0] != samples[1] and samples[0] == samples[-1]
        cli('export', project, '--output', args.output/'stickfight.svg')
        ET.parse(args.output/'stickfight.svg')
    (args.output/'stickfight.html').write_text(HTML)
    print('PASS: import-svg → apply → validate → evaluate FK/loop → export; wrote stickfight*')


if __name__ == '__main__':
    main()
