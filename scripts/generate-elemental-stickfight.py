#!/usr/bin/env python3
"""Tide / Volt / Ember: a fast six-second three-way stickfight (water vs thunder vs fire).

Built only through splinely-cli: import-svg -> apply (bones + one looping clip) ->
validate -> evaluate (FK check) -> export SVG -> render GIF. The GIF shows the
prompt that produced it. Rebuild: python3 scripts/generate-elemental-stickfight.py
"""
import argparse
import json
import math
from pathlib import Path
import subprocess
import tempfile
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
W, H, GROUND = 960, 540, 426
DURATION = 6.0
PROMPT = 'create a cool stickfight gif that is fast paced and with water vs thunder vs fire 3 sticks fight'
TIMES = [0, .35, .6, .85, 1.1, 1.4, 1.7, 2.0, 2.3, 2.6, 2.9, 3.2, 3.5, 3.8, 4.1, 4.4, 4.8, 5.3, 6.0]

# Pose: hip x/y, torso world angle, front upper/forearm, rear upper/forearm world angles,
# front/rear foot x offsets from the hip, foot lift above the ground. Every fighter is authored
# facing right in its own frame; mirrored fighters flip x around the stage centre.
def guard(x, y=318):
    return (x, y, -88, -22, -90, 38, -65, 44, -35, 0)

WINDUP = lambda x, y=328: (x, y, -104, 150, 115, 140, 105, 40, -45, 0)
BLAST = lambda x: (x, 316, -68, -4, -4, -2, -2, 70, -60, 0)
KNOCKED = lambda x: (x, 334, -122, -70, -140, 35, 15, 30, -40, 0)

TIDE = [  # water, left, faces right
    guard(210), guard(214, 324), (200, 326, -102, 150, 120, 45, -80, 40, -40, 0),
    (300, 316, -70, -5, -6, -10, -8, 70, -55, 0), (305, 320, -78, -10, -20, -15, -25, 66, -55, 0),
    (262, 320, -90, -22, -90, 38, -65, 44, -35, 0), guard(240),
    (232, 322, -95, -62, -112, -50, -118, 44, -38, 0), (214, 328, -104, -64, -118, -48, -122, 40, -45, 0),
    (196, 322, -92, -40, -100, 30, -70, 44, -38, 0), guard(200), WINDUP(196), BLAST(296), BLAST(300),
    KNOCKED(200), (172, 336, -112, -40, -110, 40, -40, 38, -42, 0), (196, 324, -94, -30, -95, 38, -65, 44, -36, 0),
    guard(208), guard(210)]
VOLT = [  # thunder, centre, faces right (towards Ember)
    guard(470), guard(472, 326), (470, 348, -98, -40, -100, 50, -50, 46, -38, 0),
    (478, 246, -80, -120, -100, 150, 120, 30, -30, 98), (490, 220, -78, -110, -95, 160, 130, 26, -34, 124),
    (500, 214, -76, 18, 24, 165, 140, 24, -36, 130),
    (512, 262, -84, -20, -60, 60, -30, 36, -36, 76), (512, 362, -122, -40, -100, 60, -40, 52, -48, 0),
    (508, 360, -120, -40, -100, 60, -40, 52, -48, 0), (500, 322, -90, -22, -90, 38, -65, 44, -36, 0),
    (560, 316, -72, -5, -8, 60, -85, 64, -50, 0), (480, 346, -100, -60, -110, 40, -60, 46, -40, 0),
    (480, 252, -86, -100, -95, -80, -85, 28, -28, 92), (480, 240, -86, -96, -92, -84, -88, 28, -28, 104),
    (470, 214, -55, -150, -170, 20, 60, 10, -50, 120), (470, 300, -95, -60, -120, 50, 0, 40, -40, 26),
    (470, 344, -102, -30, -95, 40, -55, 48, -40, 0), guard(470), guard(470)]
EMBER = [  # fire, right, own frame faces right; mirrored around the stage centre
    guard(210), guard(212, 324), guard(210), guard(210), guard(212, 322),
    (200, 326, -112, -60, -130, 60, -20, 36, -42, 0), (176, 332, -116, -50, -120, 50, -30, 34, -44, 0),
    (240, 316, -70, -5, -6, 60, -85, 66, -52, 0), (246, 318, -74, -8, -15, 55, -85, 64, -52, 0),
    guard(222), (206, 322, -96, -60, -115, -40, -110, 44, -38, 0), WINDUP(198), BLAST(296), BLAST(300),
    KNOCKED(200), (172, 336, -112, -40, -110, 40, -40, 38, -42, 0), (196, 324, -94, -30, -95, 38, -65, 44, -36, 0),
    guard(208), guard(210)]
FIGHTERS = [
    # id, poses, mirrored, main colour, rear-limb shade, trim
    ('tide', TIDE, False, '#3ec7ff', '#1f6fa8', '#d9f6ff'),
    ('volt', VOLT, False, '#ffe04a', '#a07a12', '#fff8cf'),
    ('ember', EMBER, True, '#ff6a2b', '#a8341a', '#ffe0c2'),
]
LENGTHS = {'torso': 88, 'neck': 18, 'arm-front': 57, 'fore-front': 57, 'arm-back': 57, 'fore-back': 57,
           'leg-front': 62, 'shin-front': 62, 'leg-back': 62, 'shin-back': 62}
PARENTS = {'torso': None, 'neck': 'torso', 'arm-front': 'torso', 'arm-back': 'torso', 'fore-front': 'arm-front',
           'fore-back': 'arm-back', 'leg-front': 'torso', 'leg-back': 'torso', 'shin-front': 'leg-front',
           'shin-back': 'leg-back'}


EASINGS = {'linear': {'type': 'linear'},
           'ease-in-out': {'type': 'cubicBezier', 'x1': .42, 'y1': 0, 'x2': .58, 'y2': 1},
           'ease-in': {'type': 'cubicBezier', 'x1': .42, 'y1': 0, 'x2': 1, 'y2': 1},
           'ease-out': {'type': 'cubicBezier', 'x1': 0, 'y1': 0, 'x2': .58, 'y2': 1}}


def track(target, prop, values, easing='linear'):
    values = list(values)
    assert all(a[0] < b[0] for a, b in zip(values, values[1:])) and values[-1][0] <= DURATION, (target, prop, values)
    return {'nodeId': target, 'property': prop, 'keyframes': [
        {'time': t, 'value': {'type': 'number', 'value': v}, 'easing': EASINGS[easing]} for t, v in values]}


def capsule(length, width):
    r = width / 2
    return f'M0 {-r} H{length} A{r} {r} 0 0 1 {length} {r} H0 A{r} {r} 0 0 1 0 {-r} Z'


def leg_angles(hip, foot):
    dx, dy = foot[0] - hip[0], foot[1] - hip[1]
    d = math.hypot(dx, dy)
    assert 0 < d < 124, (hip, foot)
    return math.degrees(math.atan2(dy, dx)) - math.degrees(math.acos(d / 124)), 2 * math.degrees(math.acos(d / 124))


def world(pose, mirrored):
    """Pose in stage coordinates: (x, y, torso, upper, fore, back, bfore, front foot x, rear foot x, lift)."""
    x, y, torso, upper, fore, back, bfore, ff, bf, lift = pose
    if not mirrored:
        return x, y, torso, upper, fore, back, bfore, x + ff, x + bf, lift
    flip = lambda a: -180 - a  # continuous mirrored angles, never short-arc interpolation
    return W - x, y, flip(torso), flip(upper), flip(fore), flip(back), flip(bfore), W - x - ff, W - x - bf, lift


def hand(pose, mirrored):
    x, y, torso, upper, fore = world(pose, mirrored)[:5]
    pt = lambda a, l: (l * math.cos(math.radians(a)), l * math.sin(math.radians(a)))
    s, u, f = pt(torso, 88), pt(upper, 57), pt(fore, 57)
    return x + s[0] + u[0] + f[0], y + s[1] + u[1] + f[1]


def pose_at(poses, t):
    return poses[TIMES.index(t)]


def pulse(id, when, rise=.04, hold=0, fall=.22, peak=1):
    keys = [(0, 0), (max(0, when - rise), 0), (when, peak)] + ([(when + hold, peak)] if hold else [])
    return track(id, 'opacity', keys + [(min(DURATION, when + hold + fall), 0), (DURATION, 0)])


def build_scene():
    shapes, binds, bones, tracks = [], {}, [], []
    defs = []

    def path(id, d, fill, bone=None, extra=''):
        shapes.append(f'<path id="{id}" d="{d}" fill="{fill}"{extra}/>')
        if bone:
            binds[id] = bone

    def circle(id, x, y, r, fill, bone=None):
        shapes.append(f'<circle id="{id}" cx="{x}" cy="{y}" r="{r}" fill="{fill}"/>')
        if bone:
            binds[id] = bone

    def text(id, x, y, size, fill, body):
        shapes.append(f'<text id="{id}" x="{x}" y="{y}" font-size="{size}" fill="{fill}">{body}</text>')

    def gradient(id, stops):
        defs.append(f'<radialGradient id="{id}">' + ''.join(
            f'<stop offset="{o}" stop-color="{c}"/>' for o, c in stops) + '</radialGradient>')
        return f'url(#{id})'

    # Stage: night arena split into three elemental glows.
    shapes.append(f'<rect id="stage" width="{W}" height="{H}" fill="#070a14"/>')
    for id, x, c in [('glow-tide', 190, '#0d2a44'), ('glow-volt', 480, '#2a2610'), ('glow-ember', 770, '#3a1a10')]:
        circle(id, x, 250, 190, gradient(f'{id}-g', [(0, c), (1, '#070a14')]))
    path('floor', f'M0 {GROUND + 4} H{W} V{H} H0 Z', '#0c1220')
    path('floor-line', f'M40 {GROUND + 4} H{W - 40} V{GROUND + 6} H40 Z', '#2b3b55')
    for i in range(9):
        x = 60 + i * 105
        path(f'floor-grid-{i}', f'M{x} {GROUND + 6} L{x - 80} {H - 52} L{x - 79} {H - 52} L{x + 1} {GROUND + 6} Z', '#151f33')

    # Global flashes (lightning and the final clash) sit behind the fighters so silhouettes pop.
    path('flash-volt', f'M0 0 H{W} V{H} H0 Z', '#fff6b0')
    tracks.append(track('flash-volt', 'opacity', [(0, 0), (1.38, 0), (1.42, .55), (1.48, .1), (1.53, .4), (1.7, 0), (DURATION, 0)]))
    path('flash-clash', f'M0 0 H{W} V{H} H0 Z', '#ffffff')
    tracks.append(track('flash-clash', 'opacity', [(0, 0), (3.78, 0), (3.82, .7), (4.2, 0), (DURATION, 0)]))

    for side, poses, mirrored, color, shade, trim in FIGHTERS:
        path(f'{side}-shadow', 'M-70 0 C-70 -10 70 -10 70 0 C70 10 -70 10 -70 0 Z', shade, extra=' opacity="0.55"')
        sx = [(t, world(p, mirrored)[0]) for t, p in zip(TIMES, poses)]
        tracks += [track(f'{side}-shadow', 'x', sx), track(f'{side}-shadow', 'y', [(0, GROUND + 6), (DURATION, GROUND + 6)]),
                   track(f'{side}-shadow', 'scaleX', [(t, 1 - min(p[9], 140) / 220) for t, p in zip(TIMES, poses)])]

    for side, poses, mirrored, color, shade, trim in FIGHTERS:
        authored = {}
        for t, pose in zip(TIMES, poses):
            x, y, torso, upper, fore, back, bfore, ffx, bfx, lift = world(pose, mirrored)
            vals = {'torso': (x, y, torso), 'neck': (0, 0, 0), 'arm-front': (0, 0, upper - torso),
                    'fore-front': (0, 0, fore - upper), 'arm-back': (0, 0, back - torso), 'fore-back': (0, 0, bfore - back)}
            for name, fx in [('leg-back', bfx), ('leg-front', ffx)]:
                reach = math.sqrt(max(0, 120 ** 2 - (GROUND - lift - y) ** 2))  # keep feet inside leg reach
                fx = x + max(-reach, min(reach, fx - x))
                a, b = leg_angles((x, y), (fx, GROUND - lift))
                if mirrored:  # mirrored knees bend the other way
                    a, b = leg_angles((W - x, y), (W - fx, GROUND - lift))
                    a, b = -180 - a, -b
                vals[name] = (-88, 0, a - torso)
                vals[name.replace('leg', 'shin')] = (0, 0, b)
            for name, value in vals.items():
                authored.setdefault(name, []).append((t, value))
        for name, length in LENGTHS.items():
            id = f'{side}-{name}'
            x, y, a = authored[name][0][1]
            bones.append({'id': id, 'parentId': f'{side}-{PARENTS[name]}' if PARENTS[name] else None,
                          'x': x, 'y': y, 'rotation': a, 'length': length})
            for index, prop in enumerate(['x', 'y', 'rotation']):
                values = [(t, v[index]) for t, v in authored[name]]
                if len({round(v, 9) for _, v in values}) > 1:
                    tracks.append(track(id, prop, values, 'ease-in-out'))
        for name in ['leg-back', 'shin-back', 'arm-back', 'fore-back', 'torso', 'leg-front', 'shin-front', 'neck', 'arm-front', 'fore-front']:
            id = f'{side}-{name}'
            path(f'{id}-shape', capsule(LENGTHS[name], 14 if name == 'torso' else 10), shade if 'back' in name else color, id)
            circle(f'{id}-joint', 0, 0, 6, trim if name == 'torso' else color, id)
            if name.startswith('fore'):
                circle(f'{id}-fist', LENGTHS[name], 0, 8.5, trim, id)
            if name.startswith('shin'):
                path(f'{id}-boot', capsule(18, 11), color, id)
        circle(f'{side}-head', 19, 0, 20, color, f'{side}-neck')
        visor = 'M17 -17 H24 V-3 H17 Z' if mirrored else 'M17 3 H24 V17 H17 Z'
        path(f'{side}-visor', visor, '#070a14', f'{side}-neck')
        path(f'{side}-chest-mark', 'M28 -3 H70 V3 H28 Z', trim, f'{side}-torso')

    # 1) Tide's water wave: launched at 0.85 s from the hands, Volt jumps it, it splashes on the floor.
    hx, hy = hand(pose_at(TIDE, .85), False)
    wave_d = 'M-34 0 C-30 -26 -4 -34 18 -22 C34 -12 38 6 26 16 C10 28 -22 24 -34 0 Z'
    path('wave', wave_d, gradient('wave-g', [(0, '#e8fbff'), (.55, '#3ec7ff'), (1, '#0f5f9c')]))
    path('wave-trail', 'M-90 -8 C-60 -16 -40 -12 -30 -4 C-40 4 -60 8 -90 4 Z', '#3ec7ff', extra=' opacity="0.6"')
    for id in ['wave', 'wave-trail']:
        tracks += [track(id, 'x', [(0, hx), (.84, hx), (1.25, 640), (DURATION, 640)]),
                   track(id, 'y', [(0, hy), (.84, hy), (1.25, GROUND - 8), (DURATION, GROUND - 8)], 'ease-out'),
                   track(id, 'opacity', [(0, 0), (.82, 0), (.86, 1), (1.22, 1), (1.3, 0), (DURATION, 0)])]
    for i in range(7):
        a = math.radians(-160 + i * 23)
        id = f'splash-{i}'
        circle(id, 0, 0, 6 - abs(i - 3), '#9fe6ff')
        tracks += [pulse(id, 1.27, .02, .05, .3),
                   track(id, 'x', [(0, 640), (1.25, 640), (1.6, 640 + math.cos(a) * 70), (DURATION, 640 + math.cos(a) * 70)], 'ease-out'),
                   track(id, 'y', [(0, GROUND), (1.25, GROUND), (1.6, GROUND + math.sin(a) * 60), (DURATION, GROUND + math.sin(a) * 60)], 'ease-out')]

    # 2) Volt's lightning: from the raised fist at the apex down into Ember.
    bx, by = hand(pose_at(VOLT, 1.4), False)
    ex, ey = W - 200, 245
    pts, n = [], 7
    for i in range(n + 1):
        f = i / n
        jitter = 0 if i in (0, n) else (18 if i % 2 else -18)
        pts.append((bx + (ex - bx) * f + jitter * .4, by + (ey - by) * f + jitter))
    left = [(x - 5, y) for x, y in pts]
    right = [(x + 5, y + 3) for x, y in reversed(pts)]
    bolt = 'M' + ' L'.join(f'{x:.1f} {y:.1f}' for x, y in left + right) + ' Z'
    path('bolt-glow', bolt, '#fff6b0', extra=' opacity="0.5"')
    path('bolt', bolt, '#ffe04a')
    for id in ['bolt', 'bolt-glow']:
        tracks.append(track(id, 'opacity', [(0, 0), (1.37, 0), (1.4, 1), (1.47, .2), (1.52, 1), (1.62, .3), (1.7, 0), (DURATION, 0)]))
    tracks.append(track('bolt-glow', 'scaleY', [(0, 1), (DURATION, 1)]))

    # 3) Ember's fireball: thrown at 2.0 s over the ducking Volt into Tide's block -> steam burst.
    fx, fy = hand(pose_at(EMBER, 2.0), True)
    tx, ty = hand(pose_at(TIDE, 2.3), False)
    path('fireball-trail', 'M0 -16 C40 -20 80 -6 110 0 C80 6 40 20 0 16 Z', '#ff9a3c', extra=' opacity="0.7"')
    circle('fireball', 0, 0, 22, gradient('fire-g', [(0, '#fff6c0'), (.45, '#ffb02e'), (1, '#e0301e')]))
    for id in ['fireball', 'fireball-trail']:
        tracks += [track(id, 'x', [(0, fx), (1.99, fx), (2.3, tx + 18), (DURATION, tx + 18)]),
                   track(id, 'y', [(0, fy), (1.99, fy), (2.15, 205), (2.3, ty), (DURATION, ty)]),
                   track(id, 'opacity', [(0, 0), (1.97, 0), (2.0, 1), (2.29, 1), (2.33, 0), (DURATION, 0)])]
    tracks.append(track('fireball', 'rotation', [(0, 0), (2.0, 0), (2.3, 540), (DURATION, 540)]))

    def burst(name, when, x, y, colors, count=10, inner=16, outer=50, travel=30, dur=.24):
        for i in range(count):
            a = i * 2 * math.pi / count
            dx, dy = math.cos(a), math.sin(a)
            px, py = -dy * 2.5, dx * 2.5
            id = f'{name}-ray-{i}'
            path(id, f'M{dx * inner + px:.2f} {dy * inner + py:.2f} L{dx * outer:.2f} {dy * outer:.2f} L{dx * inner - px:.2f} {dy * inner - py:.2f} Z',
                 colors[i % len(colors)])
            tracks.extend([pulse(id, when, .03, 0, dur),
                           track(id, 'x', [(0, x), (when, x), (when + dur, x + dx * travel), (DURATION, x + dx * travel)], 'ease-out'),
                           track(id, 'y', [(0, y), (when, y), (when + dur, y + dy * travel), (DURATION, y + dy * travel)], 'ease-out')])

    burst('zap', 1.42, ex, ey, ['#ffe04a', '#fff6b0'], 8)
    burst('steam', 2.31, tx + 14, ty, ['#e8fbff', '#ffb02e', '#9fe6ff'], 12, 18, 58, 40, .35)
    burst('clash', 2.9, *hand(pose_at(VOLT, 2.9), False), ['#ffe04a', '#ff6a2b'], 8, 14, 44, 26, .2)

    # 4) Finale: water and fire beams meet Volt's lightning above the stage, then everything blows apart.
    cx, cy = 480, 150
    for id, src, mirrored, color, glow in [('beam-tide', TIDE, False, '#3ec7ff', '#d9f6ff'), ('beam-ember', EMBER, True, '#ff6a2b', '#ffe0c2')]:
        sx, sy = hand(pose_at(src, 3.5), mirrored)
        ang, length = math.atan2(cy - sy, cx - sx), math.hypot(cx - sx, cy - sy)
        nx, ny = -math.sin(ang), math.cos(ang)
        for suffix, half, fill in [('', 13, color), ('-core', 5, glow)]:
            d = (f'M{sx + nx * half * .5:.1f} {sy + ny * half * .5:.1f} L{cx + nx * half:.1f} {cy + ny * half:.1f} '
                 f'L{cx - nx * half:.1f} {cy - ny * half:.1f} L{sx - nx * half * .5:.1f} {sy - ny * half * .5:.1f} Z')
            path(id + suffix, d, fill)
            tracks.append(track(id + suffix, 'opacity', [(0, 0), (3.42, 0), (3.5, 1), (3.62, .75), (3.7, 1), (3.82, 1), (3.9, 0), (DURATION, 0)]))
    vx, vy = hand(pose_at(VOLT, 3.5), False)
    up = [(vx, vy), (vx + 12, vy - 25), (vx - 10, vy - 48), (cx + 6, cy + 14)]
    zig = 'M' + ' L'.join(f'{x - 4:.1f} {y:.1f}' for x, y in up) + ' L' + ' L'.join(f'{x + 4:.1f} {y:.1f}' for x, y in reversed(up)) + ' Z'
    path('beam-volt', zig, '#ffe04a')
    tracks.append(track('beam-volt', 'opacity', [(0, 0), (3.44, 0), (3.5, 1), (3.58, .3), (3.66, 1), (3.82, 1), (3.9, 0), (DURATION, 0)]))
    circle('core', 0, 0, 30, gradient('core-g', [(0, '#ffffff'), (.4, '#fff6b0'), (1, '#ff6a2b')]))
    circle('ring', 0, 0, 40, gradient('ring-g', [(0, '#070a1400'), (.7, '#070a1400'), (.85, '#9fe6ff'), (1, '#070a1400')]))
    for id in ['core', 'ring']:
        tracks += [track(id, 'x', [(0, cx), (DURATION, cx)]), track(id, 'y', [(0, cy), (DURATION, cy)])]
    tracks += [track('core', 'opacity', [(0, 0), (3.5, 0), (3.55, .8), (3.8, 1), (4.1, 0), (DURATION, 0)]),
               track('core', 'scaleX', [(0, .3), (3.5, .3), (3.8, 1.3), (4.1, 2.2), (DURATION, 2.2)], 'ease-out'),
               track('core', 'scaleY', [(0, .3), (3.5, .3), (3.8, 1.3), (4.1, 2.2), (DURATION, 2.2)], 'ease-out'),
               track('ring', 'opacity', [(0, 0), (3.79, 0), (3.82, 1), (4.35, 0), (DURATION, 0)]),
               track('ring', 'scaleX', [(0, .5), (3.8, .5), (4.35, 6), (DURATION, 6)], 'ease-out'),
               track('ring', 'scaleY', [(0, .5), (3.8, .5), (4.35, 6), (DURATION, 6)], 'ease-out')]
    burst('boom', 3.82, cx, cy, ['#3ec7ff', '#ffe04a', '#ff6a2b'], 15, 30, 90, 120, .45)

    # Titles and the prompt that produced this film.
    path('caption-band', f'M0 {H - 46} H{W} V{H} H0 Z', '#05070e', extra=' opacity="0.92"')
    text('title-tide', 28, 46, 30, '#3ec7ff', 'WATER')
    text('title-vs1', 140, 46, 18, '#6f7f99', 'vs')
    text('title-volt', 174, 46, 30, '#ffe04a', 'THUNDER')
    text('title-vs2', 334, 46, 18, '#6f7f99', 'vs')
    text('title-ember', 368, 46, 30, '#ff6a2b', 'FIRE')
    text('made-with', W - 214, 44, 15, '#6f7f99', 'made with splinely-cli')
    text('prompt-label', 18, H - 17, 15, '#9fb3cc', 'PROMPT')
    text('prompt', 88, H - 17, 15, '#eef4ff', f'“{PROMPT}”')
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">\n<defs>'
           + ''.join(defs) + '</defs>\n' + '\n'.join(shapes) + '\n</svg>\n')
    return svg, binds, bones, tracks


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', type=Path, default=ROOT / 'target/debug/splinely-cli')
    parser.add_argument('--output', type=Path, default=ROOT / 'examples')
    parser.add_argument('--gif', type=Path, default=ROOT / 'assets/elemental-stickfight.gif')
    parser.add_argument('--gif-width', type=int, default=720)
    parser.add_argument('--fps', type=int, default=24)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)

    def cli(*words):
        result = subprocess.run([str(args.cli.resolve()), *map(str, words)], capture_output=True, text=True)
        if result.returncode:
            raise SystemExit(f'splinely-cli {words[0]} failed: {result.stderr.strip()}')
        return json.loads(result.stdout)

    source, binds, bones, tracks = build_scene()
    (args.output / 'elemental-stickfight-source.svg').write_text(source)
    with tempfile.TemporaryDirectory(prefix='splinely-elemental-') as tmp:
        tmp = Path(tmp)
        imported = tmp / 'imported.json'
        cli('import-svg', args.output / 'elemental-stickfight-source.svg', '--output', imported)
        nodes = json.loads(imported.read_text())['nodes']
        for node in nodes:
            if node['id'] in binds:
                node['boneId'] = binds[node['id']]
            if node['id'].endswith('-boot'):
                node['transform']['x'] = 62
        patch = {'name': 'Tide / Volt / Ember · water vs thunder vs fire', 'nodes': nodes, 'bones': bones,
                 'description': 'Three stick fighters - water, thunder and fire - trade a wave, a lightning strike and a '
                                'fireball, then collide in one elemental blast. Six-second loop.',
                 'clips': [{'id': 'brawl', 'name': 'Wave / bolt / fireball / clash', 'duration': DURATION,
                            'looping': True, 'autoplay': True, 'tracks': tracks}]}
        patch_file = tmp / 'patch.json'
        patch_file.write_text(json.dumps(patch))
        project = args.output / 'elemental-stickfight.sply'
        cli('apply', imported, '--patch', patch_file, '--output', project)
        cli('validate', project)
        # Independent FK check: the CLI-evaluated front shoulder matches the authored pose for every fighter.
        for t in [0, 1.4, 2.0, 3.5, 4.1]:
            posed = {n['id']: n for n in cli('evaluate', project, '--time', t)['nodes']}
            for side, poses, mirrored, *_ in FIGHTERS:
                x, y, a = world(pose_at(poses, t), mirrored)[:3]
                shoulder = (x + 88 * math.cos(math.radians(a)), y + 88 * math.sin(math.radians(a)))
                actual = posed[f'{side}-arm-front-shape']['transform']
                assert abs(actual['x'] - shoulder[0]) < 1e-6 and abs(actual['y'] - shoulder[1]) < 1e-6, (side, t, actual, shoulder)
        cli('export', project, '--output', args.output / 'elemental-stickfight.svg')
        ET.parse(args.output / 'elemental-stickfight.svg')
        args.gif.parent.mkdir(parents=True, exist_ok=True)
        cli('render', project, '--output', args.gif, '--width', args.gif_width, '--fps', args.fps)
    print(f'PASS: import-svg -> apply -> validate -> evaluate FK -> export -> render; wrote elemental-stickfight* and {args.gif}')


if __name__ == '__main__':
    main()
