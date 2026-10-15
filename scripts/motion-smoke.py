#!/usr/bin/env python3
"""Compare real Firefox export matrices to Rust CLI for animated motion paths.
Requires cargo build -p splinely-cli; no third-party Python dependencies.
"""
import json
import math
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import subprocess
import tempfile
import threading
import time

from firefox_harness import Firefox

ROOT = Path(__file__).resolve().parents[1]
CLI = ROOT / 'target/debug/splinely-cli'


def cli(*args):
    result = subprocess.run([str(CLI), *map(str, args)], capture_output=True, text=True, check=True)
    return json.loads(result.stdout)


def key(t, value):
    return {'time': t, 'value': {'type': 'number', 'value': value}}


def main():
    with tempfile.TemporaryDirectory(prefix='splinely-motion-') as directory:
        root = Path(directory)
        shapes = ['M0 0 L100 0 L100 300', 'm0 0 q50 100 100 0',
                  'M0 0 C0 100 100 100 100 0', 'M0 0 L0 0 L10 0 M100 0 L130 0',
                  'M0 0 L100 0 L100 100 Z']
        nodes, tracks = [], []
        for i, d in enumerate(shapes):
            nodes.extend([
                {'id': f'route{i}', 'visible': False,
                 'kind': {'type': 'path', 'd': d, 'bounds': [[0, 0, 100, 300], [0, 0, 100, 50], [0, 0, 100, 75], [0, 0, 130, 0], [0, 0, 100, 100]][i]},
                 'transform': {'x': 10, 'y': 20, 'rotation': 37, 'scaleX': 2, 'scaleY': .75}},
                {'id': f'ball{i}', 'kind': {'type': 'circle', 'cx': 0, 'cy': 0, 'radius': 3},
                 'transform': {'x': 4, 'rotation': 13},
                 'motionPath': {'path': f'route{i}', 'orient': True}}
            ])
            tracks.append({'nodeId': f'ball{i}', 'property': 'motionProgress', 'keyframes': [key(0, 0), key(2, 1)]})
            tracks.append({'nodeId': f'route{i}', 'property': 'x', 'keyframes': [key(0, 10), key(2, 70)]})
        # Source path morphs too; motion must see the evaluated shape.
        tracks.append({'nodeId': 'route1', 'property': 'd', 'keyframes': [
            {'time': 0, 'value': {'type': 'path', 'value': shapes[1]}},
            {'time': 2, 'value': {'type': 'path', 'value': 'm0 0 q100 200 200 0'}}]})
        project = root / 'motion.sply'
        project.write_text(json.dumps({'nodes': nodes, 'clips': [{'id': 'travel', 'duration': 2,
            'autoplay': True, 'tracks': tracks}]}))
        cli('export', project, '--output', root / 'motion.svg')
        server = ThreadingHTTPServer(('127.0.0.1', 0), partial(SimpleHTTPRequestHandler, directory=directory))
        threading.Thread(target=server.serve_forever, daemon=True).start()
        samples = 0
        try:
            with Firefox() as browser:
                browser.command('WebDriver:Navigate', {'url': f'http://127.0.0.1:{server.server_port}/motion.svg'})
                deadline = time.monotonic() + 8
                while not browser.js('return !!document.documentElement.splinely;'):
                    assert time.monotonic() < deadline, 'runtime did not start'
                    time.sleep(.04)
                browser.js('document.documentElement.splinely.timeline.pause();')
                for t in [0, .1, .5, 1, 1.5, 1.9, 2]:
                    expected = cli('evaluate', project, '--time', t)['nodes']
                    actual = browser.js('''
const f=document.documentElement.splinely; f.clip('travel').seek(arguments[0]);
return {matrices:Array.from({length:5},(_,i)=>{
 const m=document.getElementById('ball'+i).transform.baseVal.consolidate().matrix;
 return [m.a,m.b,m.c,m.d,m.e,m.f];}),diagnostics:f.diagnostics.map(e=>e.message)};
''', [t])
                    assert not actual['diagnostics'], actual
                    for i, matrix in enumerate(actual['matrices']):
                        tr = next(n for n in expected if n['id'] == f'ball{i}')['transform']
                        angle = math.radians(tr['rotation'])
                        want = [math.cos(angle) * tr['scaleX'], math.sin(angle) * tr['scaleX'],
                                -math.sin(angle) * tr['scaleY'], math.cos(angle) * tr['scaleY'], tr['x'], tr['y']]
                        assert all(abs(a-b) < 1e-3 for a, b in zip(matrix, want)), (i, t, matrix, want)
                        samples += 1
                # Explicit progress overrides are immediate, without changing the binding.
                browser.js("document.documentElement.splinely.set('ball0',{motionProgress:0});")
                x = browser.js("return document.getElementById('ball0').transform.baseVal.consolidate().matrix.e;")
                assert abs(x - 74) < 1e-3, x
            print(f'PASS: Firefox motion paths: {samples} matrices match Rust; lines, Q/C curves, relative commands, closure, gaps, hidden sources, nonuniform scale/rotation, animated source transform and d, explicit progress; no diagnostics')
        finally:
            server.shutdown()
            server.server_close()


if __name__ == '__main__':
    main()
