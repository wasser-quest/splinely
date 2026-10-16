"""Stickfight 'BLAZE vs VOLT' – erzeugt die Schritte fuer splinely-cli batch und baut
examples/blaze-vs-volt.sply, examples/blaze-vs-volt.svg und assets/blaze-vs-volt.gif.
Aufruf: python3 scripts/generate-blaze-vs-volt.py (splinely-cli muss im PATH liegen).

Jede Kameraeinstellung (Shot) haengt an einem eigenen Kamera-Bone in der Bildmitte.
Schnitt = Kamera-Bone springt (step) ins Bild bzw. 5000 px daneben; Drehung = schiefe Kamera; Jitter = Wackeln.
"""
import math, os, random, shlex, shutil, subprocess, tempfile

W, H, CX, CY, HID = 960, 540, 480, 270, 5480
T = 7.8
random.seed(7)
S = []
def cmd(*a): S.append(' '.join(shlex.quote(str(x)) for x in a))

def track(target, prop, keys, kind='--value'):
    """keys: [(t, value, easing)] – setzt explizite Keys ohne Auto-Startkey."""
    keys = sorted(keys, key=lambda k: k[0])
    if keys[0][0] > 0: keys.insert(0, (0, keys[0][1], 'step'))
    for t, v, e in keys:
        cmd('keyframe', '--clip', 'fight', '--target', target, '--property', prop, '--time', round(t, 3),
            kind, (round(v, 2) if isinstance(v, float) else v), '--easing', e, '--no-start-key')

# ---------------------------------------------------------------- Kameras / Shots
SHOTS = {}
def shot(name, t0, t1, keys=(), bg='#141826'):
    """keys: [(t, dx, dy, rot, easing)] innerhalb des Fensters."""
    cam = name + '_cam'
    cmd('bone', 'add', '--id', cam, '--x', CX if t0 == 0 else HID, '--y', CY, '--rotation', 0, '--length', 1)
    keys = list(keys) or [(t0, 0, 0, 0, 'linear')]
    if keys[0][0] > t0: keys.insert(0, (t0, keys[0][1], keys[0][2], keys[0][3], 'linear'))
    xs = [] if t0 == 0 else [(0, HID, 'step')]
    inner = [k for k in keys if k[0] < t1 - 1e-6] if t1 < T else keys
    xs += [(t, CX + dx, e) for t, dx, dy, r, e in inner[:-1]] + [(inner[-1][0], CX + inner[-1][1], 'step')]
    if t1 < T: xs.append((t1, HID, 'step'))   # harter Schnitt: letzter Key im Shot haelt (step) bis zum Schnitt
    track(cam, 'x', xs)
    if any(k[2] for k in keys): track(cam, 'y', [(t, CY + dy, e) for t, dx, dy, r, e in keys])
    if any(k[3] for k in keys): track(cam, 'rotation', [(t, float(r), e) for t, dx, dy, r, e in keys])
    SHOTS[name] = (cam, t0, t1)
    node(name + '_bg', cam, 'rect', x=-800, y=-700, width=1600, height=1400, fill=bg)
    return cam

def shake(t0, t1, amp, rot=0.0, base_rot=0.0, step=0.045):
    ks, t = [], t0
    while t < t1:
        ks.append((t, random.uniform(-amp, amp), random.uniform(-amp, amp), base_rot + random.uniform(-rot, rot), 'linear'))
        t += step
    ks.append((t1, 0, 0, base_rot, 'linear'))
    return ks

def node(i, bone, kind, **kw):
    a = ['node', 'add', '--kind', kind, '--id', i]
    for k, v in kw.items(): a += ['--' + k.replace('_', '-'), v]
    cmd(*a)
    cmd('bone', 'bind', '--node', i, '--bone', bone)

def fx(i, prop, keys, e='linear', kind='--value'):
    track(i, prop, [(k[0], k[1], k[2] if len(k) > 2 else e) for k in keys], kind)

# ---------------------------------------------------------------- Strichmaennchen-Rig
SEG = dict(t=50, h=6, ua=28, fa=26, th=34, sh=34)
POSES = {  # absolute Winkel (Grad, y nach unten, 0 = Blickrichtung) fuer Blick nach rechts
    'guard':   dict(t=-80, h=-80, ua1=70, fa1=-50, ua2=110, fa2=-30, th1=65, sh1=100, th2=115, sh2=100),
    'run1':    dict(t=-62, h=-68, ua1=20, fa1=-60, ua2=150, fa2=100, th1=15, sh1=95, th2=140, sh2=175),
    'run2':    dict(t=-62, h=-68, ua1=150, fa1=100, ua2=20, fa2=-60, th1=140, sh1=175, th2=15, sh2=95),
    'punch':   dict(t=-70, h=-75, ua1=-8, fa1=-8, ua2=130, fa2=-80, th1=35, sh1=85, th2=135, sh2=120),
    'hit':     dict(t=-125, h=-150, ua1=-150, fa1=-110, ua2=-165, fa2=-125, th1=50, sh1=80, th2=110, sh2=130),
    'kick':    dict(t=-118, h=-100, ua1=150, fa1=170, ua2=-140, fa2=-115, th1=-12, sh1=-8, th2=95, sh2=92),
    'fly':     dict(t=-160, h=-172, ua1=-60, fa1=-30, ua2=-100, fa2=-70, th1=10, sh1=35, th2=-12, sh2=20),
    'crouch':  dict(t=-55, h=-60, ua1=80, fa1=40, ua2=120, fa2=80, th1=10, sh1=110, th2=155, sh2=100),
    'charge':  dict(t=-90, h=-92, ua1=40, fa1=20, ua2=140, fa2=160, th1=55, sh1=100, th2=125, sh2=80),
    'chargeup':dict(t=-92, h=-110, ua1=-35, fa1=-70, ua2=-145, fa2=-110, th1=55, sh1=100, th2=125, sh2=80),
    'blast':   dict(t=-80, h=-80, ua1=0, fa1=0, ua2=8, fa2=0, th1=40, sh1=95, th2=140, sh2=95),
    'down':    dict(t=180, h=185, ua1=150, fa1=172, ua2=-150, fa2=-170, th1=-4, sh1=6, th2=8, sh2=-2),
    'victory': dict(t=-90, h=-96, ua1=-70, fa1=-95, ua2=110, fa2=60, th1=75, sh1=95, th2=105, sh2=90),
}
def mirror(p): return {k: 180 - v for k, v in p.items()}
def norm(a): return (a + 180) % 360 - 180
def P(pose, facing): return POSES[pose] if facing > 0 else mirror(POSES[pose])

def joints(p, s, px=0.0, py=0.0):
    v = lambda a, L: (L * s * math.cos(math.radians(a)), L * s * math.sin(math.radians(a)))
    add = lambda a, b: (a[0] + b[0], a[1] + b[1])
    hip = (px, py); sh = add(hip, v(p['t'], SEG['t'])); neck = add(sh, v(p['h'], SEG['h']))
    head = add(neck, v(p['h'], 12))
    j = dict(hip=hip, sh=sh, head=head)
    for n in '12':
        el = add(sh, v(p['ua' + n], SEG['ua'])); j['hand' + n] = add(el, v(p['fa' + n], SEG['fa']))
        kn = add(hip, v(p['th' + n], SEG['th'])); j['foot' + n] = add(kn, v(p['sh' + n], SEG['sh']))
    return j

def ground_y(p, s, ground):
    j = joints(p, s)
    low = max(j['foot1'][1] + 5 * s, j['foot2'][1] + 5 * s, j['head'][1] + 12 * s, j['hand1'][1], j['hand2'][1])
    return ground - low

RIGS = {}
def rig(rid, cam, s, color, back, keys, ground, facing=1, glow=None):
    """keys: [(t, pose, x, lift, easing)]; Huefthoehe wird so gesetzt, dass der tiefste Punkt den Boden beruehrt."""
    first = P(keys[0][1], facing)
    b = lambda n: f'{rid}_{n}'
    y0 = ground_y(first, s, ground) - keys[0][3]
    cmd('bone', 'add', '--id', b('pel'), '--parent', cam, '--x', keys[0][2] - 1, '--y', round(y0, 2), '--rotation', 0, '--length', 0.1)
    rel = lambda p: dict(t=p['t'], h=p['h'] - p['t'], ua1=p['ua1'] - p['t'], fa1=p['fa1'] - p['ua1'],
                         ua2=p['ua2'] - p['t'], fa2=p['fa2'] - p['ua2'], th1=p['th1'], sh1=p['sh1'] - p['th1'],
                         th2=p['th2'], sh2=p['sh2'] - p['th2'])
    r0 = {k: norm(v) for k, v in rel(first).items()}
    cmd('bone', 'add', '--id', b('t'), '--parent', b('pel'), '--rotation', r0['t'], '--length', SEG['t'] * s)
    cmd('bone', 'add', '--id', b('h'), '--parent', b('t'), '--rotation', r0['h'], '--length', SEG['h'] * s)
    for n in '12':
        cmd('bone', 'add', '--id', b('ua' + n), '--parent', b('t'), '--rotation', r0['ua' + n], '--length', SEG['ua'] * s)
        cmd('bone', 'add', '--id', b('fa' + n), '--parent', b('ua' + n), '--rotation', r0['fa' + n], '--length', SEG['fa'] * s)
        cmd('bone', 'add', '--id', b('th' + n), '--parent', b('pel'), '--rotation', r0['th' + n], '--length', SEG['th'] * s)
        cmd('bone', 'add', '--id', b('sh' + n), '--parent', b('th' + n), '--rotation', r0['sh' + n], '--length', SEG['sh'] * s)
    w = 9 * s
    def limb(n, L, col, width=w):
        node(b(n) + '_g', b(n), 'rect', x=round(-width / 2, 2), y=round(-width / 2, 2), width=round(L * s + width, 2),
             height=round(width, 2), rx=round(width / 2, 2), fill=col)
    for n in ['ua2', 'fa2', 'th2', 'sh2']: limb(n, SEG[n[:2]], back)
    limb('t', SEG['t'], color, 11 * s)
    for n in ['th1', 'sh1', 'ua1', 'fa1']: limb(n, SEG[n[:2]], color)
    node(b('head'), b('h'), 'circle', x=round(18 * s, 2), y=0, radius=round(12 * s, 2), fill=color)
    if glow:  # leuchtende Augen (erst sichtbar, wenn fx sie einblendet)
        node(b('eye'), b('h'), 'circle', x=round(20 * s, 2), y=round(5 * s * facing, 2), radius=round(3 * s, 2), fill=glow, opacity=0)
    # Tracks
    last = dict(r0)
    tr = {k: [] for k in r0}; px_, py_ = [], []
    for t, pose, x, lift, e in keys:
        p = P(pose, facing)
        for k, v in rel(p).items():
            v = norm(v); v += 360 * round((last[k] - v) / 360); last[k] = v
            tr[k].append((t, float(v), e))
        px_.append((t, float(x - 1), e)); py_.append((t, float(ground_y(p, s, ground) - lift), e))
    for k, ks in tr.items(): track(b('t' if k == 't' else k), 'rotation', ks)
    track(b('pel'), 'x', px_); track(b('pel'), 'y', py_)
    RIGS[rid] = (s, facing, keys, ground)

def where(rid, t, part):
    """Weltposition (im Shot-Rahmen) eines Gelenks zum Key-Zeitpunkt t (fuer Effekt-Platzierung)."""
    s, facing, keys, ground = RIGS[rid]
    k = [k for k in keys if k[0] <= t + 1e-6][-1]
    p = P(k[1], facing)
    return joints(p, s, k[2], ground_y(p, s, ground) - k[3])[part]

A, AB, B, BB = '#ff5a36', '#b8341b', '#3ec8ff', '#1f7fa8'
FIRE1, FIRE2, BOLT = '#ffb627', '#fff2b0', '#d6f6ff'

cmd('new', '--width', W, '--height', H, '--name', 'Stickfight: BLAZE vs VOLT', '--background', '#0b0d14')
cmd('clip', 'add', '--id', 'fight', '--duration', T, '--autoplay', '--loop')

# ================================================================ SHOT 1 – Totale, Sprint + Faust-Clash
c = shot('s1', 0.0, 1.0, [(0, 0, 0, 0, 'linear'), (0.6, 0, 0, 0, 'linear')] + shake(0.62, 0.9, 10), bg='#161a2b')
node('s1_moon', c, 'circle', x=260, y=-160, radius=60, fill='#2a3150')
for i in range(9):
    h = random.randint(60, 190)
    node(f's1_bld{i}', c, 'rect', x=-480 + i * 110, y=150 - h, width=96, height=h, fill='#1d2238')
node('s1_floor', c, 'rect', x=-800, y=150, width=1600, height=400, fill='#0e101a')
node('s1_line', c, 'rect', x=-800, y=148, width=1600, height=4, fill='#39406a')
run = lambda x0, x1, t0, t1: [(t0 + (t1 - t0) * i / 4, 'run1' if i % 2 == 0 else 'run2', x0 + (x1 - x0) * i / 4, 0, 'linear') for i in range(4)]
rig('s1A', c, 1.3, A, AB, run(-420, -120, 0, 0.45) + [(0.6, 'punch', -62, 0, 'ease-out'), (0.78, 'punch', -62, 0, 'ease-in'), (1.0, 'guard', -130, 0, 'linear')], 150)
rig('s1B', c, 1.3, B, BB, run(420, 120, 0, 0.45) + [(0.6, 'punch', 62, 0, 'ease-out'), (0.78, 'punch', 62, 0, 'ease-in'), (1.0, 'guard', 130, 0, 'linear')], 150, facing=-1)
hx, hy = where('s1A', 0.6, 'hand1')
node('s1_clash', c, 'star', x=0, y=round(hy, 1), radius=70, tips=8, inner=0.35, fill=FIRE2, opacity=0, pivot='center')
fx('s1_clash', 'opacity', [(0.58, 0, 'step'), (0.6, 1), (0.8, 0)])
fx('s1_clash', 'scaleX', [(0.6, 0.2, 'ease-out'), (0.8, 1.8)]); fx('s1_clash', 'scaleY', [(0.6, 0.2, 'ease-out'), (0.8, 1.8)])
fx('s1_clash', 'rotation', [(0.6, 0), (0.8, 40)])
node('s1_flash', c, 'rect', x=-800, y=-700, width=1600, height=1400, fill='#ffffff', opacity=0)
fx('s1_flash', 'opacity', [(0.59, 0, 'step'), (0.61, 0.85), (0.72, 0)])
for i, sgn in enumerate([-1, 1]):
    node(f's1_dust{i}', c, 'circle', x=sgn * 260, y=140, radius=22, fill='#3a4160', opacity=0, pivot='center')
    fx(f's1_dust{i}', 'opacity', [(0.1, 0, 'step'), (0.12, 0.7), (0.5, 0)]); fx(f's1_dust{i}', 'scaleX', [(0.12, 0.5), (0.5, 2.2)])

# ================================================================ SHOT 2 – Nah, Untersicht, schiefe Kamera: Treffer ins Gesicht
c = shot('s2', 1.0, 1.55, [(1.0, -30, 0, -14, 'ease-out'), (1.14, 0, 0, -14, 'linear')] + shake(1.16, 1.42, 18, 3, -14) + [(1.55, 10, 0, -10, 'linear')], bg='#2a1220')
for i in range(14):
    y = random.randint(-300, 300); L = random.randint(200, 600)
    node(f's2_speed{i}', c, 'rect', x=random.randint(-700, 200), y=y, width=L, height=random.choice([3, 5, 8]), fill='#ffffff', opacity=0)
    fx(f's2_speed{i}', 'opacity', [(1.0, 0, 'step'), (1.02, 0.35), (1.18, 0.15), (1.5, 0)])
    fx(f's2_speed{i}', 'x', [(1.0, 0), (1.5, -500)])
rig('s2A', c, 3.2, A, AB, [(1.0, 'guard', -420, 0, 'ease-in'), (1.13, 'punch', -260, 0, 'ease-out'), (1.4, 'punch', -240, 0, 'ease-in-out'), (1.55, 'guard', -300, 0, 'linear')], 330)
rig('s2B', c, 3.2, B, BB, [(1.0, 'guard', 28, 0, 'step'), (1.13, 'guard', 28, 0, 'ease-out'), (1.2, 'hit', -40, 0, 'ease-out'), (1.55, 'hit', 420, 60, 'linear')], 405, facing=-1)
fxh, fyh = where('s2A', 1.13, 'hand1')
node('s2_impact', c, 'star', x=round(fxh + 20, 1), y=round(fyh, 1), radius=120, tips=10, inner=0.3, fill=FIRE1, opacity=0, pivot='center')
node('s2_impact2', c, 'star', x=round(fxh + 20, 1), y=round(fyh, 1), radius=70, tips=10, inner=0.4, fill='#ffffff', opacity=0, pivot='center')
for i in ('', '2'):
    fx('s2_impact' + i, 'opacity', [(1.12, 0, 'step'), (1.14, 1), (1.36, 0)])
    fx('s2_impact' + i, 'scaleX', [(1.14, 0.3, 'ease-out'), (1.36, 1.6)]); fx('s2_impact' + i, 'scaleY', [(1.14, 0.3, 'ease-out'), (1.36, 1.6)])
node('s2_txt', c, 'text', x=-90, y=-170, text='WHAM!', font_size=96, fill='#fff2b0', opacity=0, pivot='center')
fx('s2_txt', 'opacity', [(1.13, 0, 'step'), (1.15, 1), (1.45, 1), (1.55, 0)])
fx('s2_txt', 'scaleX', [(1.15, 0.4, 'ease-out'), (1.3, 1.15)]); fx('s2_txt', 'scaleY', [(1.15, 0.4, 'ease-out'), (1.3, 1.15)])

# ================================================================ SHOT 3 – Vogelperspektive: umkreisen + Schlagabtausch
c = shot('s3', 1.55, 2.35, [(1.55, 0, 0, 0, 'linear'), (2.35, 0, 0, 25, 'linear')], bg='#12141f')
node('s3_ring', c, 'circle', x=0, y=0, radius=255, fill='#181c2c', stroke='#39406a', stroke_width=6)
node('s3_ring2', c, 'circle', x=0, y=0, radius=140, fill='none', stroke='#252a42', stroke_width=3)
def topfighter(fid, col, back, phase, punches):
    body = f'{fid}_body'
    cmd('bone', 'add', '--id', body, '--parent', c, '--x', 0, '--y', 0, '--rotation', 0, '--length', 0.1)
    for n, off in (('L', -40), ('R', 40)):
        cmd('bone', 'add', '--id', f'{fid}_arm{n}', '--parent', body, '--x', -0.1, '--y', off, '--rotation', 0, '--length', 66)
    node(f'{fid}_shadow', body, 'circle', x=9, y=9, radius=58, fill='#00000055')
    for n, off in (('L', -26), ('R', 26)):
        node(f'{fid}_a{n}', f'{fid}_arm{n}', 'rect', x=-9, y=-9, width=84, height=18, rx=9, fill=back)
    node(f'{fid}_sh', body, 'rect', x=-15, y=-51, width=30, height=102, rx=15, fill=col)
    node(f'{fid}_hd', body, 'circle', x=6, y=0, radius=26, fill=col)
    node(f'{fid}_nose', body, 'circle', x=26, y=0, radius=6, fill='#ffffff')
    xs, ys, rs, aL, aR = [], [], [], [], []
    n = 16
    for i in range(n + 1):
        t = 1.55 + 0.8 * i / n
        ang = phase + 2 * math.pi * 1.1 * (i / n) ** 0.8
        r = 175 - 65 * math.sin(math.pi * i / n)
        x, y = r * math.cos(ang), r * math.sin(ang)
        face = math.degrees(math.atan2(-y, -x))
        xs.append((t, x - 1, 'linear')); ys.append((t, y, 'linear')); rs.append((t, face, 'linear'))
        hit = i in punches
        aL.append((t, -8.0 if hit and i % 2 == 0 else 35.0, 'linear')); aR.append((t, 8.0 if hit and i % 2 else -35.0, 'linear'))
    # Winkel stetig halten
    for j in range(1, len(rs)):
        v = rs[j][1]; v += 360 * round((rs[j - 1][1] - v) / 360); rs[j] = (rs[j][0], v, 'linear')
    track(body, 'x', xs); track(body, 'y', ys); track(body, 'rotation', rs)
    track(f'{fid}_armL', 'rotation', aL); track(f'{fid}_armR', 'rotation', aR)
topfighter('s3A', A, AB, math.pi, {4, 7, 10, 13})
topfighter('s3B', B, BB, 0.0, {5, 8, 11, 14})
for i, t in enumerate([1.78, 1.93, 2.08, 2.23]):
    node(f's3_spark{i}', c, 'star', x=0, y=0, radius=60, tips=6, inner=0.4, fill=FIRE2 if i % 2 else BOLT, opacity=0, pivot='center')
    fx(f's3_spark{i}', 'opacity', [(t - 0.01, 0, 'step'), (t, 1), (t + 0.1, 0)])
    fx(f's3_spark{i}', 'x', [(t, random.uniform(-40, 40), 'step')]); fx(f's3_spark{i}', 'y', [(t, random.uniform(-40, 40), 'step')])

# ================================================================ SHOT 4 – Totale Gegenschuss: Volt-Kick, Blaze fliegt
c = shot('s4', 2.35, 3.35, [(2.35, 0, 0, 0, 'linear'), (2.6, 0, 0, 0, 'linear')] + shake(2.62, 2.85, 14) + [(3.35, 40, 0, 0, 'linear')], bg='#1b1626')
for i in range(8):
    h = random.randint(80, 210)
    node(f's4_bld{i}', c, 'rect', x=-520 + i * 140, y=150 - h, width=120, height=h, fill='#251d33')
node('s4_floor', c, 'rect', x=-800, y=150, width=1600, height=400, fill='#120f1a')
node('s4_line', c, 'rect', x=-800, y=148, width=1600, height=4, fill='#5a3f6e')
rig('s4B', c, 1.3, B, BB, [(2.35, 'guard', -160, 0, 'ease-in'), (2.5, 'run1', -60, 0, 'ease-out'), (2.6, 'kick', 10, 0, 'ease-out'), (2.85, 'kick', 20, 0, 'ease-in-out'), (3.1, 'guard', -10, 0, 'linear')], 150)
rig('s4A', c, 1.3, A, AB, [(2.35, 'guard', 130, 0, 'step'), (2.6, 'guard', 125, 0, 'ease-out'), (2.64, 'hit', 150, 10, 'ease-out'), (2.85, 'fly', 300, 90, 'ease-in'), (3.05, 'crouch', 400, 0, 'ease-out'), (3.35, 'crouch', 380, 0, 'linear')], 150, facing=-1)
kx, ky = where('s4B', 2.6, 'foot1')
node('s4_impact', c, 'star', x=round(kx + 10, 1), y=round(ky, 1), radius=80, tips=8, inner=0.3, fill=BOLT, opacity=0, pivot='center')
fx('s4_impact', 'opacity', [(2.6, 0, 'step'), (2.62, 1), (2.82, 0)])
fx('s4_impact', 'scaleX', [(2.62, 0.3, 'ease-out'), (2.82, 1.7)]); fx('s4_impact', 'scaleY', [(2.62, 0.3, 'ease-out'), (2.82, 1.7)])
node('s4_slide', c, 'rect', x=300, y=144, width=160, height=8, rx=4, fill='#ff8a5c', opacity=0)
fx('s4_slide', 'opacity', [(3.0, 0, 'step'), (3.05, 0.9), (3.35, 0)])
for i in range(5):
    node(f's4_spark{i}', c, 'circle', x=400, y=146, radius=4, fill=FIRE1, opacity=0)
    fx(f's4_spark{i}', 'opacity', [(3.04, 0, 'step'), (3.06, 1), (3.3, 0)])
    fx(f's4_spark{i}', 'x', [(3.06, 0, 'ease-out'), (3.3, random.uniform(40, 120))]); fx(f's4_spark{i}', 'y', [(3.06, 0, 'ease-out'), (3.3, random.uniform(-80, -20))])

# ================================================================ SHOT 5 – Nah: Blaze laedt Superkraft auf
c = shot('s5', 3.35, 4.45, [(3.35, 0, 0, 4, 'linear')] + shake(3.6, 4.45, 3, 1, 4, 0.05)[:8] + shake(3.98, 4.45, 12, 2.5, 4, 0.04), bg='#1a0d0b')
for i, (r, col, op) in enumerate([(330, '#5c1a0c', 0.9), (250, '#c2410c', 0.75), (170, FIRE1, 0.8), (100, FIRE2, 0.9)]):
    node(f's5_aura{i}', c, 'star', x=0, y=-40, radius=r, tips=14 - i * 2, inner=0.6, fill=col, opacity=0, pivot='center')
    fx(f's5_aura{i}', 'opacity', [(3.45, 0, 'step'), (3.6 + i * 0.08, op), (4.45, op)])
    sc = [(3.45, 0.1, 'ease-out'), (3.75 + i * 0.05, 1.0)]
    for j in range(8):
        sc.append((3.85 + j * 0.075, 1.12 if j % 2 == 0 else 0.92))
    fx(f's5_aura{i}', 'scaleX', sc); fx(f's5_aura{i}', 'scaleY', sc)
    fx(f's5_aura{i}', 'rotation', [(3.45, 0), (4.45, (90 if i % 2 else -90))])
node('s5_floor', c, 'rect', x=-800, y=200, width=1600, height=500, fill='#120806')
node('s5_crack', c, 'path', d='M-20 200 L-60 240 L-40 270 L-110 330 M10 200 L50 250 L30 290 L120 340 M-5 200 L0 260 L-20 330', fill='none', stroke=FIRE1, stroke_width=6, opacity=0)
fx('s5_crack', 'opacity', [(3.9, 0, 'step'), (3.95, 1), (4.45, 1)])
rig('s5A', c, 3.0, A, AB, [(3.35, 'crouch', 0, 0, 'ease-out'), (3.6, 'charge', 0, 0, 'ease-in-out'), (3.95, 'chargeup', 0, 0, 'ease-out'), (4.3, 'chargeup', 0, 12, 'ease-in-out'), (4.45, 'blast', 0, 0, 'linear')], 200, glow='#fff7c2')
fx('s5A_eye', 'opacity', [(3.9, 0, 'step'), (3.95, 1)])
for i in range(12):
    x0 = random.uniform(-260, 260); t0 = 3.5 + random.uniform(0, 0.6)
    node(f's5_ember{i}', c, 'circle', x=round(x0, 1), y=200, radius=random.choice([4, 6, 8]), fill=random.choice([FIRE1, FIRE2, '#ff6a2b']), opacity=0)
    fx(f's5_ember{i}', 'opacity', [(t0 - 0.01, 0, 'step'), (t0, 1), (t0 + 0.4, 0)])
    fx(f's5_ember{i}', 'y', [(t0, 0, 'ease-out'), (t0 + 0.4, -random.uniform(250, 450))])
node('s5_txt', c, 'text', x=-280, y=190, text='BLAZE OVERDRIVE', font_size=64, fill=FIRE2, opacity=0, pivot='center')
fx('s5_txt', 'opacity', [(3.95, 0, 'step'), (4.0, 1)]); fx('s5_txt', 'scaleX', [(4.0, 1.6, 'ease-out'), (4.15, 1)]); fx('s5_txt', 'scaleY', [(4.0, 1.6, 'ease-out'), (4.15, 1)])

# ================================================================ SHOT 6 – Ueber-die-Schulter (POV Volt): Strahl kommt auf die Kamera zu
c = shot('s6', 4.45, 5.05, shake(4.45, 5.05, 6, 1, 0, 0.05), bg='#0f1222')
node('s6_floor', c, 'path', d='M-800 -10 L800 -10 L800 400 L-800 400 Z', fill='#0b0d18')
for i in range(-6, 7):  # Fluchtlinien zum Horizont
    node(f's6_lane{i+6}', c, 'path', d=f'M{150 + i * 12} -10 L{150 + i * 260} 400', fill='none', stroke='#232945', stroke_width=2)
rig('s6A', c, 0.75, A, AB, [(4.45, 'blast', 150, 0, 'linear'), (5.05, 'blast', 150, 0, 'linear')], 40, facing=-1, glow='#fff7c2')
fx('s6A_eye', 'opacity', [(4.45, 1, 'step')])
hx6, hy6 = where('s6A', 4.45, 'hand1')
for i, (col, r1) in enumerate([('#c2410c', 700), (FIRE1, 560), (FIRE2, 420), ('#ffffff', 260)]):
    node(f's6_beam{i}', c, 'circle', x=round(hx6 - 6, 1), y=round(hy6, 1), radius=4, fill=col, opacity=0.95)
    fx(f's6_beam{i}', 'radius', [(4.5 + i * 0.03, 4.0, 'ease-in'), (5.05, float(r1))])
    fx(f's6_beam{i}', 'x', [(4.5, 0, 'ease-in'), (5.05, -150 - (hx6 - 150) * 0.0)])
# Volt im Vordergrund, von hinten (Silhouette)
node('s6_Bsh', c, 'rect', x=-760, y=170, width=560, height=300, rx=120, fill='#0c3a52')
node('s6_Bhd', c, 'circle', x=-470, y=110, radius=120, fill='#0f4a68')
node('s6_Brim', c, 'path', d='M-360 30 Q-340 110 -370 190', fill='none', stroke=B, stroke_width=6)
node('s6_flash', c, 'rect', x=-800, y=-700, width=1600, height=1400, fill='#ffffff', opacity=0)
fx('s6_flash', 'opacity', [(4.9, 0), (5.05, 0.9)])

# ================================================================ SHOT 7 – Totale: Feuerstrahl gegen Blitz
c = shot('s7', 5.05, 5.85, shake(5.05, 5.85, 9, 1.5, 0, 0.04), bg='#101425')
node('s7_floor', c, 'rect', x=-800, y=160, width=1600, height=400, fill='#0b0d18')
node('s7_line', c, 'rect', x=-800, y=158, width=1600, height=4, fill='#39406a')
rig('s7A', c, 1.15, A, AB, [(5.05, 'blast', -340, 0, 'linear'), (5.85, 'blast', -330, 0, 'linear')], 160, glow='#fff7c2')
rig('s7B', c, 1.15, B, BB, [(5.05, 'guard', 340, 0, 'ease-out'), (5.15, 'blast', 340, 0, 'linear'), (5.65, 'blast', 360, 0, 'ease-in'), (5.85, 'hit', 400, 0, 'linear')], 160, facing=-1, glow='#ffffff')
fx('s7A_eye', 'opacity', [(5.05, 1, 'step')]); fx('s7B_eye', 'opacity', [(5.15, 0, 'step'), (5.17, 1)])
ax7, ay7 = where('s7A', 5.05, 'hand1'); bx7, by7 = where('s7B', 5.15, 'hand1')
for i, (hgt, col) in enumerate([(56, '#c2410c'), (36, FIRE1), (14, FIRE2)]):
    node(f's7_beam{i}', c, 'rect', x=round(ax7, 1), y=round(ay7 - hgt / 2, 1), width=1, height=hgt, rx=hgt / 2, fill=col)
    fx(f's7_beam{i}', 'width', [(5.05, 1.0, 'ease-out'), (5.25, float(-ax7 - 10)), (5.55, float(-ax7 + 20)), (5.7, float(-ax7 + 120)), (5.85, float(-ax7 + 300))])
zig = lambda amp, seed: (random.seed(seed), ' '.join(f'L{bx7 + (0 - bx7) * k / 9:.1f} {by7 + (random.uniform(-amp, amp) if 0 < k < 9 else 0):.1f}' for k in range(10)))[1]
z1, z2, z3 = ('M' + zig(a, sd)[1:] for a, sd in [(30, 1), (34, 2), (28, 3)])
random.seed(99)
for i, (col, wdt) in enumerate([(B, 12), (BOLT, 4)]):
    node(f's7_bolt{i}', c, 'path', d=z1, fill='none', stroke=col, stroke_width=wdt, opacity=0)
    fx(f's7_bolt{i}', 'opacity', [(5.14, 0, 'step'), (5.16, 1), (5.65, 1), (5.7, 0)])
    fx(f's7_bolt{i}', 'd', [(5.16 + j * 0.06, [z1, z2, z3][j % 3], 'step') for j in range(9)], kind='--path')
node('s7_orb', c, 'circle', x=-10, y=round((ay7 + by7) / 2, 1), radius=6, fill='#ffffff', stroke=FIRE1, stroke_width=10)
fx('s7_orb', 'radius', [(5.05, 0.0, 'step'), (5.22, 10.0, 'ease-out'), (5.5, 55.0), (5.6, 40), (5.7, 70)])
fx('s7_orb', 'x', [(5.55, 0, 'ease-in'), (5.85, 330)])
fx('s7_orb', 'stroke', [(5.2, B, 'linear'), (5.4, FIRE1), (5.5, B), (5.6, FIRE1)], kind='--color')

# ================================================================ SHOT 8 – Extrem nah: Explosion
c = shot('s8', 5.85, 6.3, shake(5.85, 6.3, 22, 4, 8, 0.035), bg='#2b0f05')
for i, (col, r) in enumerate([('#c2410c', 700), (FIRE1, 520), (FIRE2, 360), ('#ffffff', 220)]):
    node(f's8_boom{i}', c, 'circle', x=0, y=0, radius=10, fill=col)
    fx(f's8_boom{i}', 'radius', [(5.85 + i * 0.03, 10.0, 'ease-out'), (6.2, float(r))])
node('s8_rays', c, 'star', x=0, y=0, radius=650, tips=16, inner=0.15, fill='#ffffff', opacity=0.0, pivot='center')
fx('s8_rays', 'opacity', [(5.9, 0, 'step'), (5.95, 0.6), (6.3, 0.9)]); fx('s8_rays', 'rotation', [(5.9, 0), (6.3, 30)])
node('s8_flash', c, 'rect', x=-800, y=-700, width=1600, height=1400, fill='#ffffff', opacity=0)
fx('s8_flash', 'opacity', [(6.15, 0), (6.3, 1)])

# ================================================================ SHOT 9 – Aufsicht schraeg von oben: K.O.
c = shot('s9', 6.3, T, [(6.3, 0, -20, -6, 'ease-out'), (7.8, 0, 10, -2, 'linear')], bg='#141826')
node('s9_ground', c, 'path', d='M-700 -60 L700 -60 L900 400 L-900 400 Z', fill='#1c2133')
node('s9_crater', c, 'path', d='M60 150 C60 110 360 110 360 150 C360 190 60 190 60 150 Z', fill='#0d0f19', stroke='#3a2a2a', stroke_width=4)
rig('s9B', c, 1.0, B, BB, [(6.3, 'down', 210, 0, 'linear'), (7.8, 'down', 210, 0, 'linear')], 155, facing=-1)
rig('s9A', c, 1.0, A, AB, [(6.3, 'blast', -150, 0, 'ease-out'), (6.7, 'guard', -150, 0, 'ease-out'), (7.0, 'victory', -150, 0, 'linear'), (7.8, 'victory', -150, 0, 'linear')], 130, glow='#fff7c2')
fx('s9A_eye', 'opacity', [(6.3, 1, 'step'), (7.0, 0)])
for i in range(7):
    node(f's9_smoke{i}', c, 'circle', x=round(random.uniform(90, 330), 1), y=round(random.uniform(110, 160), 1), radius=random.randint(26, 46), fill='#4a5068', opacity=0, pivot='center')
    fx(f's9_smoke{i}', 'opacity', [(6.3, 0.7, 'step'), (7.6, 0)])
    fx(f's9_smoke{i}', 'y', [(6.3, 0, 'ease-out'), (7.6, -random.uniform(60, 140))])
    fx(f's9_smoke{i}', 'scaleX', [(6.3, 0.6, 'ease-out'), (7.6, 1.8)]); fx(f's9_smoke{i}', 'scaleY', [(6.3, 0.6, 'ease-out'), (7.6, 1.8)])
node('s9_flash', c, 'rect', x=-800, y=-700, width=1600, height=1400, fill='#ffffff', opacity=1)
fx('s9_flash', 'opacity', [(6.3, 1.0, 'ease-out'), (6.6, 0)])
node('s9_ko', c, 'text', x=-150, y=-120, text='K.O.!', font_size=130, fill=FIRE2, opacity=0, pivot='center')
fx('s9_ko', 'opacity', [(6.75, 0, 'step'), (6.8, 1)]); fx('s9_ko', 'scaleX', [(6.8, 2.5, 'ease-out'), (7.0, 1)]); fx('s9_ko', 'scaleY', [(6.8, 2.5, 'ease-out'), (7.0, 1)])
node('s9_win', c, 'text', x=-120, y=-60, text='BLAZE WINS', font_size=40, fill='#ffffff', opacity=0)
fx('s9_win', 'opacity', [(7.05, 0, 'step'), (7.2, 1)])

cmd('a11y', '--description', 'Schneller Stickfight in neun Kameraeinstellungen: Sprint und Faust-Clash, Nahaufnahme-Treffer, '
    'Vogelperspektive, Gegenschuss-Kick, Blaze laedt Feuerkraft auf, Strahl aus Volts Perspektive, Feuer gegen Blitz, Explosion, K.O.')

# ---------------------------------------------------------------- Prompt-Einblendung (an keinem Bone: steht still ueber allen Shots)
PROMPT = 'kannst du ein sehr schnellen stickfight machen mit mehreren perspektiv wechsel und super power. mit splinecli'
PROMPT_EN = 'can you make a very fast stickfight with multiple perspective changes and super power, with splinecli'
cmd('node', 'add', '--kind', 'rect', '--id', 'prompt_band', '--x', 0, '--y', H - 58, '--width', W, '--height', 58, '--fill', '#05070e', '--opacity', 0.88)
cmd('node', 'add', '--kind', 'text', '--id', 'prompt_label', '--x', 16, '--y', H - 34, '--text', 'PROMPT', '--font-size', 14, '--fill', '#9fb3cc')
cmd('node', 'add', '--kind', 'text', '--id', 'prompt_text', '--x', 86, '--y', H - 34, '--text', f'\u201c{PROMPT}\u201d', '--font-size', 15, '--fill', '#eef4ff')
cmd('node', 'add', '--kind', 'text', '--id', 'prompt_en', '--x', 86, '--y', H - 12, '--text', f'({PROMPT_EN})', '--font-size', 13, '--fill', '#8fa2bd')
cmd('node', 'add', '--kind', 'text', '--id', 'made_with', '--x', W - 196, '--y', 30, '--text', 'made with splinely-cli', '--font-size', 14, '--fill', '#c9d4e6')

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
project, svg, gif = (os.path.join(ROOT, p) for p in ('examples/blaze-vs-volt.sply', 'examples/blaze-vs-volt.svg', 'assets/blaze-vs-volt.gif'))
with tempfile.TemporaryDirectory(prefix='splinely-blaze-') as tmp:
    steps, built = os.path.join(tmp, 'steps.txt'), os.path.join(tmp, 'fight.sply')
    open(steps, 'w').write('\n'.join(S) + '\n')
    r = subprocess.run(['splinely-cli', 'batch', built, '--steps', steps], capture_output=True, text=True)
    if r.returncode: print(r.stderr[-2000:]); raise SystemExit(1)
    shutil.move(built, project)
subprocess.run(['splinely-cli', 'export', project, '--output', svg], check=True, stdout=subprocess.DEVNULL)
subprocess.run(['splinely-cli', 'render', project, '--output', gif, '--width', '640', '--fps', '20'], check=True, stdout=subprocess.DEVNULL)
print(f'ok: {len(S)} Schritte -> {project}, {svg}, {gif}')
