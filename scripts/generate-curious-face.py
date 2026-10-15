#!/usr/bin/env python3
"""Build examples/curious-face.{sply,svg} through the real CLI.

Shows several things working at once without any custom JavaScript:
three state machines that crossfade (hover, mood and a blink timer), a click
toggle and pupils that follow the pointer.
"""
import json
import subprocess
from pathlib import Path

from sply_py import load_project_file  # .sply is binary by default

ROOT = Path(__file__).resolve().parents[1]
CLI = ROOT / 'target/debug/splinely-cli'
OUT = ROOT / 'examples'
PROJECT = OUT / 'curious-face.sply'


def cli(*args):
    result = subprocess.run([str(CLI), *map(str, args)], check=True, capture_output=True, text=True)
    return json.loads(result.stdout)


def main():
    subprocess.run(['cargo', 'build', '--quiet', '-p', 'splinely-cli'], cwd=ROOT, check=True)
    f = PROJECT
    cli('new', '--output', f, '--width', 400, '--height', 300, '--background', '#1b2240')
    cli('node', 'add', f, '--kind', 'circle', '--id', 'head', '--x', 200, '--y', 150, '--radius', 90,
        '--fill', '#f2c14e', '--pivot', 'center')
    for side, x in (('left', 165), ('right', 235)):
        cli('node', 'add', f, '--kind', 'circle', '--id', f'cheek-{side}', '--x', x - 15, '--y', 182,
            '--radius', 13, '--fill', '#ff7b7b', '--opacity', 0)
    for side, x in (('left', 170), ('right', 230)):
        cli('node', 'add', f, '--kind', 'circle', '--id', f'eye-{side}', '--x', x, '--y', 130,
            '--radius', 18, '--fill', 'white', '--pivot', 'center')
        cli('node', 'add', f, '--kind', 'circle', '--id', f'pupil-{side}', '--x', x, '--y', 130,
            '--radius', 8, '--fill', '#1f2333', '--pivot', 'center')
    cli('node', 'add', f, '--kind', 'path', '--id', 'mouth', '--d', 'M 165 188 Q 200 196 235 188',
        '--fill', 'none', '--stroke', '#1f2333', '--stroke-width', 6)
    # An invisible hit area on top keeps hover stable over eyes and mouth.
    cli('node', 'add', f, '--kind', 'circle', '--id', 'face-area', '--x', 200, '--y', 150,
        '--radius', 92, '--fill', 'white', '--opacity', 0)

    # Machine 1: hover makes the head lean in, softly.
    cli('on', f, '--event', 'pointerenter', '--target', 'face-area', '--machine', 'attention',
        '--to', 'near', '--state', 'head.scaleX=1.06', '--state', 'head.scaleY=1.06', '--blend', 0.25)
    cli('on', f, '--event', 'pointerleave', '--target', 'face-area', '--machine', 'attention',
        '--from', 'near', '--to', 'idle', '--blend', 0.4)
    # Machine 2: a click toggles a happy face that fades in and out.
    happy = ['--state', 'cheek-left.opacity=0.85', '--state', 'cheek-right.opacity=0.85',
             '--state', 'head.fill=#ffb347']
    cli('on', f, '--event', 'click', '--target', 'face-area', '--machine', 'mood',
        '--to', 'happy', '--blend', 0.35, *happy)
    cli('on', f, '--event', 'click', '--target', 'face-area', '--machine', 'mood',
        '--from', 'happy', '--to', 'idle', '--blend', 0.6)
    # The smile is a path morph: same commands, different numbers, so it blends too.
    project = load_project_file(PROJECT)
    mood = next(m for m in project['stateMachines'] if m['id'] == 'mood')
    smile = {'nodeId': 'mouth', 'property': 'd', 'value': {'type': 'path', 'value': 'M 165 182 Q 200 220 235 182'}}
    next(s for s in mood['states'] if s['id'] == 'happy')['overrides'].append(smile)
    project['name'] = 'Curious face'
    patch = OUT.parent / '.work-tmp' / 'curious-face-patch.json'
    patch.parent.mkdir(exist_ok=True)
    patch.write_text(json.dumps({'name': project['name'], 'stateMachines': project['stateMachines']}))
    cli('apply', f, '--patch', patch, '--output', f)
    # Machine 3: a timer blinks every few seconds, independent of everything else.
    shut = [arg for part in ('eye-left', 'eye-right', 'pupil-left', 'pupil-right')
            for arg in ('--state', f'{part}.scaleY=0.1')]
    cli('on', f, '--machine', 'blink', '--from', 'open', '--to', 'shut', '--after', 3.2, '--blend', 0.06, *shut)
    cli('on', f, '--machine', 'blink', '--from', 'shut', '--to', 'open', '--after', 0.12, '--blend', 0.06)
    # Pupils follow the pointer, always.
    for side in ('left', 'right'):
        cli('follow', f, '--target', f'pupil-{side}', '--max', 8, '--strength', 0.12, '--smoothing', 0.12)
    cli('check', f)
    cli('export', f, '--output', OUT / 'curious-face.svg')
    print('Created examples/curious-face.sply and examples/curious-face.svg')


if __name__ == '__main__':
    main()
