#!/usr/bin/env python3
"""Generate an interactive brown owl via CLI import/apply/validate/trusted export."""
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
CLI = ROOT / 'target/debug/splinely-cli'
OUT = ROOT / 'examples'
STEM = 'brown-owl'


# A plain brown owl on a branch: ear tufts, facial disc, amber eyes,
# a speckled belly and talons. Lids and wing poses are animated by states.
def chevrons():
    out = []
    rows=[(372,[272,306,340]),(404,[256,289,322,355]),(436,[262,296,330,364]),(468,[278,312,346])]
    for y,xs in rows:
        for x in xs:
            out.append(f"M{x} {y} Q{x+11} {y+9} {x+22} {y}")
    return ' '.join(out)
SVG=f'''<svg xmlns="http://www.w3.org/2000/svg" width="640" height="640" viewBox="0 0 640 640">
<title>Brown owl</title>
<defs>
<radialGradient id="glow" cx="0.5" cy="0.45" r="0.5"><stop offset="0" stop-color="#f8f1e4"/><stop offset="1" stop-color="#eadcc4"/></radialGradient>
<linearGradient id="bodyShade" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#9a6638"/><stop offset="1" stop-color="#6f4524"/></linearGradient>
<linearGradient id="bellyShade" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#e3c69c"/><stop offset="1" stop-color="#c9a173"/></linearGradient>
<linearGradient id="wingShade" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#7a4d29"/><stop offset="1" stop-color="#55341b"/></linearGradient>
<radialGradient id="iris" cx="0.5" cy="0.5" r="0.5"><stop offset="0" stop-color="#ffc24a"/><stop offset="1" stop-color="#e2861d"/></radialGradient>
<linearGradient id="branchShade" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#7b5535"/><stop offset="1" stop-color="#553820"/></linearGradient>
</defs>
<rect id="background" width="640" height="640" fill="#f3ebdd"/>
<circle id="halo" cx="320" cy="300" r="250" fill="url(#glow)"/>
<path id="branch" d="M48 514 C180 494 430 500 596 478 L600 506 C440 532 190 530 52 544 Z" fill="url(#branchShade)"/>
<path id="branch-twig" d="M468 498 Q512 470 540 432 L552 440 Q528 480 488 508 Z" fill="#6a4529"/>
<path id="owl-tuft-left" d="M236 214 Q206 168 212 112 Q254 138 282 188 Z" fill="#5c391d"/>
<path id="owl-tuft-right" d="M404 214 Q434 168 428 112 Q386 138 358 188 Z" fill="#5c391d"/>
<path id="owl-body" d="M320 160 C236 156 200 214 206 288 C186 356 172 410 208 470 C248 534 392 534 432 470 C468 410 454 356 434 288 C440 214 404 156 320 160 Z" fill="url(#bodyShade)"/>
<path id="owl-wing-left" d="M214 296 Q158 346 176 438 Q190 478 226 446 L248 352 Z" fill="url(#wingShade)"/>
<path id="owl-wing-right" d="M426 296 Q482 346 464 438 Q450 478 414 446 L392 352 Z" fill="url(#wingShade)"/>
<path id="owl-wing-marks" d="M188 374 Q204 384 224 380 M184 404 Q202 414 222 408 M188 434 Q204 442 220 436 M452 374 Q436 384 416 380 M456 404 Q438 414 418 408 M452 434 Q436 442 420 436" fill="none" stroke="#3f2614" stroke-width="5"/>
<path id="owl-belly" d="M320 330 C262 322 232 366 234 424 C232 484 276 506 320 508 C372 508 408 482 406 424 C408 368 376 324 320 330 Z" fill="url(#bellyShade)"/>
<path id="owl-belly-marks" d="{chevrons()}" fill="none" stroke="#8a5a32" stroke-width="5"/>
<path id="owl-talons" d="M262 498 L256 522 L266 522 L270 500 Z M282 500 L280 526 L290 526 L292 500 Z M302 498 L306 522 L316 520 L310 496 Z M330 496 L324 520 L334 522 L340 498 Z M348 500 L350 526 L360 526 L358 500 Z M370 500 L374 522 L384 522 L378 498 Z" fill="#3e3128"/>
<path id="owl-disc" d="M320 224 C302 188 238 186 220 230 C202 274 228 316 270 320 C294 322 310 310 320 298 C330 310 346 322 370 320 C412 316 438 274 420 230 C402 186 338 188 320 224 Z" fill="#ead6b3" stroke="#a9794b" stroke-width="5"/>
<path id="owl-brows" d="M232 214 Q276 196 316 240 M408 214 Q364 196 324 240" fill="none" stroke="#6b4426" stroke-width="8"/>
<circle id="owl-eye-ring-left" cx="272" cy="258" r="42" fill="#3a2718"/>
<circle id="owl-eye-ring-right" cx="370" cy="258" r="42" fill="#3a2718"/>
<circle id="owl-eye-left" cx="272" cy="258" r="37" fill="url(#iris)"/>
<circle id="owl-eye-right" cx="370" cy="258" r="37" fill="url(#iris)"/>
<circle id="owl-pupil-left" cx="272" cy="260" r="17" fill="#160f0a"/>
<circle id="owl-pupil-right" cx="370" cy="260" r="17" fill="#160f0a"/>
<circle id="owl-glint-left" cx="266" cy="253" r="5" fill="#ffffff"/>
<circle id="owl-glint-right" cx="364" cy="253" r="5" fill="#ffffff"/>
<circle id="owl-lid-left" cx="272" cy="258" r="43" fill="#a97a4c" opacity="0"/>
<circle id="owl-lid-right" cx="370" cy="258" r="43" fill="#a97a4c" opacity="0"/>
<path id="owl-lid-lines" d="M238 262 Q272 286 306 262 L306 268 Q272 292 238 268 Z M336 262 Q370 286 404 262 L404 268 Q370 292 336 268 Z" fill="#3a2718" opacity="0"/>
<path id="owl-beak" d="M304 282 Q320 272 336 282 L325 312 Q320 320 315 312 Z" fill="#4b3b2e"/>
<path id="owl-beak-light" d="M309 283 Q320 277 331 283 L322 290 Z" fill="#7a6656"/>
</svg>'''

CUSTOM_JS = r"""
// Reviewed, instance-local controller. All state changes go through the machine.
const machine = splinely.machine('owl-mood');
const pupils = ['owl-pupil-left', 'owl-pupil-right', 'owl-glint-left', 'owl-glint-right'];
let settle = null;
function isOwl(element) {
  return !!(element && element.id && element.id.startsWith('owl-') && element.localName !== 'svg');
}
function resetEyes() { for (const id of pupils) splinely.set(id, null); }
function track(detail) {
  // Runtime detail.x/y are already viewBox coordinates. The radial limit keeps
  // pupil and glint inside each iris, even at the figure's edges.
  for (const [side, cx] of [['left', 272], ['right', 370]]) {
    const dx = detail.x - cx, dy = detail.y - 260;
    const length = Math.hypot(dx, dy), factor = length ? Math.min(12, length / 7) / length : 0;
    for (const part of ['pupil', 'glint']) splinely.set('owl-' + part + '-' + side, {x: dx * factor, y: dy * factor});
  }
}
splinely.on('pointermove', detail => {
  if (isOwl(detail.event.target)) {
    if (machine.state === 'idle') machine.send('owl.hover');
    if (machine.state === 'attentive') track(detail);
  } else if (machine.state !== 'idle') machine.send('owl.leave');
});
splinely.on('pointerenter', detail => {
  if (isOwl(detail.event.target) && machine.state === 'idle') machine.send('owl.hover');
});
splinely.on('pointerleave', detail => {
  // Ignore moving from one owl piece to another.
  if (!isOwl(detail.event.relatedTarget)) machine.send('owl.leave');
});
splinely.on('click', detail => {
  if (isOwl(detail.event.target)) machine.send('owl.click');
});
splinely.on('state.change', detail => {
  if (detail.machine !== 'owl-mood') return;
  clearTimeout(settle);
  if (detail.to !== 'attentive') resetEyes();
  if (detail.to === 'content') settle = setTimeout(() => machine.send('owl.settled'), 1400);
});
splinely.onDestroy(() => clearTimeout(settle));
"""

HTML = '''<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Brown owl</title>
<style>body{font-family:system-ui,sans-serif;background:#f3ebdd;color:#4a2f1a;max-width:680px;margin:24px auto;padding:0 18px}h1{margin-bottom:8px}p{line-height:1.5}object{display:block;width:100%;aspect-ratio:1;border-radius:28px}a{color:#8a5a32}</style>
</head><body><h1>Brown owl</h1>
<p>A brown owl on a branch. Move the pointer over it and it perks up its ear tufts while its eyes follow you; click it and it closes its eyes contentedly and lifts its wings. No button or audio needed.</p>
<object id="scene" type="image/svg+xml" data="brown-owl.svg" aria-label="Interactive brown owl"><a href="brown-owl.svg">Open the interactive SVG</a></object>
<p><a href="brown-owl-source.svg">Source SVG</a> · <a href="brown-owl.splinely.json">Project JSON</a> · <a href="../scripts/generate-owl-demo.py">Generator</a></p>
<p>Hover follows a mouse/pointer; touch can click but has no persistent hover. The SVG starts by itself in a script-capable context (object or direct SVG), not an img. Restrictive CSP may block the trusted custom JavaScript. No external assets or network are required.</p>
</body></html>
'''


def cli(*args):
    subprocess.run([str(CLI), *map(str, args)], cwd=ROOT, check=True)


def number(value):
    return {'type': 'number', 'value': value}


def override(node_id, prop, value):
    return {'nodeId': node_id, 'property': prop, 'value': number(value)}


def main():
    OUT.mkdir(exist_ok=True)
    source = OUT / (STEM + '-source.svg')
    source.write_text(SVG, encoding='utf-8')
    lids = ['owl-lid-left', 'owl-lid-right', 'owl-lid-lines']
    states = []
    for state in ['idle', 'attentive', 'content']:
        content, attentive = state == 'content', state == 'attentive'
        states.append({
            'id': state,
            'onEnter': [{'type': 'setVariable', 'name': 'mood', 'value': state}],
            'overrides': [override(id, 'opacity', int(content)) for id in lids]
            + [override('owl-brows', 'y', -6 if attentive else 0),
               override('owl-tuft-left', 'y', -7 if attentive else 0),
               override('owl-tuft-right', 'y', -7 if attentive else 0),
               override('owl-wing-left', 'rotation', 8 if content else 0),
               override('owl-wing-right', 'rotation', -8 if content else 0)],
        })
    transitions = []
    for start, end, event in [
        ('idle', 'attentive', 'owl.hover'),
        ('idle', 'content', 'owl.click'),
        ('attentive', 'content', 'owl.click'),
        ('content', 'idle', 'owl.click'),
        ('content', 'idle', 'owl.settled'),
        ('attentive', 'idle', 'owl.leave'),
        ('content', 'idle', 'owl.leave'),
    ]:
        transitions.append({'from': start, 'to': end, 'event': {'name': event}, 'actions': []})
    with tempfile.TemporaryDirectory(prefix='splinely-owl-') as directory:
        directory = Path(directory)
        imported = directory / 'imported.json'
        patch = directory / 'patch.json'
        cli('import-svg', source, '--output', imported)
        # Wings turn around their own centre, not the canvas origin.
        for wing in ['owl-wing-left', 'owl-wing-right']:
            cli('node', 'update', imported, wing, '--pivot', 'center')
        patch.write_text(json.dumps({
            'name': 'Brown owl', 'variables': {'mood': 'idle'},
            'stateMachines': [{'id': 'owl-mood', 'initial': 'idle', 'states': states, 'transitions': transitions}],
            'customJs': CUSTOM_JS,
        }, ensure_ascii=False), encoding='utf-8')
        project_path = OUT / (STEM + '.splinely.json')
        cli('apply', imported, '--patch', patch, '--output', project_path)
        # Opt-in screen-reader data: describe the owl, hide purely decorative backdrop nodes.
        cli('a11y', project_path, '--description', 'A brown owl on a branch reacts to the pointer: its mood changes between idle, attentive and content.')
        for decoration in ['background', 'halo', 'branch', 'branch-twig']:
            cli('a11y', project_path, '--node', decoration, '--hidden', 'true')
        cli('validate', project_path)
        cli('convert', project_path, '--output', OUT / (STEM + '.sply'))
        cli('export', project_path, '--output', OUT / (STEM + '.svg'), '--trusted-js')
    (OUT / (STEM + '.html')).write_text(HTML, encoding='utf-8')
    print('Created examples/brown-owl{,-source}.svg, .splinely.json, .sply and .html')


if __name__ == '__main__':
    main()
