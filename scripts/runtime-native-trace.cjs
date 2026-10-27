#!/usr/bin/env node
'use strict';
// Trace comparison for the native (Web Animations) path of the browser runtime.
//   node scripts/runtime-native-trace.cjs --old path/to/previous/splinely.js [--extra file.svg=file.sply ...] [--json]
// Each scene runs twice in a fake browser (runtime/tests/fake-waapi.cjs) on the same simulated clock and the
// same scripted input (pointer moves, every state-machine event, every clip played, timeline pause/seek/play,
// figure hidden and shown again): once with the previous runtime (pure JavaScript, no element.animate) and
// once with this runtime with native animations on. At every 60 Hz frame it compares what each element shows
// (transform attribute combined with CSS translate/rotate/scale and running animations, and opacity), the
// emitted events and the clip times reported by the API.
const fs = require('node:fs');
const path = require('node:path');
const { createEnv } = require('../runtime/tests/fake-waapi.cjs');
const ROOT = path.join(__dirname, '..');
const args = process.argv.slice(2);
const opt = (name, def) => { const i = args.indexOf(name); return i < 0 ? def : args[i + 1]; };
const oldPath = path.resolve(opt('--old', path.join(ROOT, 'runtime/src/splinely.js')));
const newPath = path.join(ROOT, 'runtime/src/splinely.js');
const OLD = require(oldPath);
delete require.cache[require.resolve(newPath)];
const NEW = oldPath === newPath ? OLD : require(newPath);

const readSply = file => { const b = fs.readFileSync(file); return b[0] === 0x7b ? JSON.parse(b.toString('utf8')) : b; };
const scenes = [];
for (const name of ['curious-face', 'stickfight', 'water-drop', 'cactus-runner', 'blaze-vs-volt', 'brown-owl', 'demo', 'anime-hello', 'desert-stickfight', 'random-figures', 'rig']) {
  const json = path.join(ROOT, 'examples', name + '.splinely.json'), sply = path.join(ROOT, 'examples', name + '.sply');
  const svg = path.join(ROOT, 'examples', name + '.svg');
  if (!fs.existsSync(svg)) continue;
  scenes.push({ name, svg, project: () => (fs.existsSync(json) ? JSON.parse(fs.readFileSync(json, 'utf8')) : readSply(sply)) });
}
for (let i = 0; i < args.length; i++) if (args[i] === '--extra') {
  const [svg, sply] = args[++i].split('=');
  scenes.push({ name: path.basename(sply), svg, project: () => readSply(sply) });
}

// Frame times displaced by up to ±3 ms (--jitter N), as on real displays.
const JITTER = Number(opt('--jitter', 3));
function script(api) {
  const project = api.sourceProject || {};
  const machines = project.stateMachines || project.machines || [];
  const events = [];
  for (const m of machines) for (const t of m.transitions || []) if (t.event && t.event.name && t.event.name !== 'after') events.push([m.id, t.event.name]);
  const clips = api.snapshot().clips.map(c => c.id);
  const steps = [];
  let t = 500;
  steps.push([t, 'pointer', [120, 80]]); t += 400; steps.push([t, 'pointer', [300, 260]]); t += 300; steps.push([t, 'leave']);
  for (const e of events.slice(0, 24)) { t += 900; steps.push([t, 'send', e]); }
  for (const id of clips.slice(0, 16)) { t += 700; steps.push([t, 'play', id]); }
  t += 650; steps.push([t, 'pause']);
  t += 400; steps.push([t, 'seek', 0.75]);
  t += 300; steps.push([t, 'playAll']);
  t += 900; steps.push([t, 'hide']);
  t += 2300; steps.push([t, 'show']);
  t += 1200; steps.push([t, 'stopFirst']);
  t += 1500;
  return { steps, end: t };
}
function act(api, env, svg, kind, arg, clips) {
  switch (kind) {
    case 'pointer': svg.fire('pointermove', { clientX: arg[0], clientY: arg[1] }); break;
    case 'leave': svg.fire('pointerleave', { clientX: 0, clientY: 0 }); break;
    case 'send': api.machine(arg[0]).send(arg[1]); break;
    case 'play': api.clip(arg).play(); break;
    case 'pause': api.timeline.pause(); break;
    case 'seek': for (const c of api.snapshot().clips) if (c.active) api.clip(c.id).seek(Math.min(arg, c.duration)); break;
    case 'playAll': for (const id of clips) if (api.snapshot().clips.find(c => c.id === id).active) api.clip(id).play(); break;
    case 'hide': env.setVisible(false); break;
    case 'show': env.setVisible(true); break;
    case 'stopFirst': { const c = api.snapshot().clips.find(c => c.playing); if (c) api.clip(c.id).stop(); break; }
  }
}
function runScene(scene) {
  if (process.env.TRACE_DEBUG) console.error('scene', scene.name);
  const text = fs.readFileSync(scene.svg, 'utf8');
  const view = { hidden: false };
  const sides = [[OLD, false], [NEW, true]].map(([rt, waapi]) => {
    const { env, svg } = createEnv(text, { waapi, io: true, jitter: JITTER });
    const log = [];
    const api = rt.createRuntime(svg, scene.project(), { reducedMotion: false });
    for (const name of ['clip.end', 'state.change', 'error']) api.on(name, d => log.push({ t: env.time, hidden: view.hidden, text: name + ' ' + (d && (d.clip || d.to || d.state || d.message || '')) }));
    return { rt, env, svg, api, log, hidden: false };
  });
  const plan = script(sides[0].api), clips = sides[0].api.snapshot().clips.map(c => c.id);
  let worstO = '', loopEdge = 0, maxM = 0, maxO = 0, maxT = 0, worst = '', frames = 0, compared = 0, hidden = false;
  const start = sides[0].env.time;
  let step = 0;
  for (;;) {
    const to = sides[0].env.nextFrame;
    if (to > start + plan.end) break;
    for (const s of sides) s.env.advanceTo(to);
    frames++;
    if (process.env.TRACE_WATCH && process.env.TRACE_DEBUG === scene.name) {
      const [id, clip] = process.env.TRACE_WATCH.split(':'), el = sides[1].svg.all.find(e => e.id === id), an = el.animations[0];
      if (clip === 'shown') { const e0 = sides[0].svg.all.find(e => e.id === id); console.error(Math.round(to - start), plan.steps[step - 1] && plan.steps[step - 1][1], e0.shown().m.slice(4).map(v => v.toFixed(3)).join(','), el.shown().m.slice(4).map(v => v.toFixed(3)).join(','), e0.getAttribute('opacity'), el.shown().opacity.toFixed(3)); } else
      console.error(Math.round(to - start), plan.steps[step - 1] && plan.steps[step - 1][1], clip, an && (an.currentTime / 1000).toFixed(4), an && an.playState);
    }
    if (!hidden) compare(to);
    while (step < plan.steps.length && start + plan.steps[step][0] <= to) {
      const [, kind, arg] = plan.steps[step++];
      if (kind === 'hide') view.hidden = hidden = true;
      if (kind === 'show') view.hidden = hidden = false;
      for (const s of sides) act(s.api, s.env, s.svg, kind, arg, clips);
    }
  }
  function compare(to) {
    const [a, b] = sides;
    // A looping clip whose time lands exactly on its duration shows its last keyframe for that one frame in
    // JavaScript (it wraps on the next tick); a native animation already shows the first. Real frame times
    // practically never land exactly there; such frames are counted, not compared.
    if (a.api.snapshot().clips.some(c => c.playing && a.api.clip(c.id).loop && (Math.abs(c.time - c.duration) < 1e-6 || c.time < 1e-6))) { loopEdge++; return; }
    compared++;
    for (let i = 0; i < a.svg.all.length; i++) {
      const ea = a.svg.all[i], eb = b.svg.all[i];
      if (!ea.id) continue;
      const x = ea.shown(), y = eb.shown();
      for (let j = 0; j < 6; j++) { const d = Math.abs(x.m[j] - y.m[j]); if (d > maxM) { maxM = d; worst = ea.id + '@' + Math.round(to - start); } }
      if (process.env.TRACE_DEBUG === scene.name && !a.dbg && (Math.abs(x.opacity - y.opacity) > 1e-6 || x.m.some((v, j) => Math.abs(v - y.m[j]) > 1e-6))) {
        a.dbg = true;
        console.error('first diff', ea.id, 'at', Math.round(to - start), 'step', step, plan.steps[step - 1]);
        console.error(' old', x, ea.getAttribute('transform'), ea.getAttribute('opacity'));
        console.error(' new', y, eb.getAttribute('transform'), eb.getAttribute('opacity'), eb.style, eb.animations.map(n => [n.prop, n.opts, n.currentTime, n.value(), n.frames.map(f => f[n.prop] + '@' + f.offset)]));
        console.error(' clips old', a.api.snapshot().clips.map(c => c.id + ':' + c.time.toFixed(4) + ':' + c.playing));
        console.error(' clips new', b.api.snapshot().clips.map(c => c.id + ':' + c.time.toFixed(4) + ':' + c.playing));
      }
      const o = Math.abs(x.opacity - y.opacity); if (o > maxO) { maxO = o; worstO = ea.id + '@' + Math.round(to - start); }
    }
    const ca = a.api.snapshot().clips, cb = b.api.snapshot().clips;
    for (let i = 0; i < ca.length; i++) { const d = Math.abs(ca[i].time - cb[i].time); if (d > maxT) maxT = d; }
  }
  const [a, b] = sides;
  // Events must match in order and payload; at the same time while visible. While hidden (no frames) the
  // runtime fires them from timers at their due time instead of at the next frame: the shift is reported.
  let eventsEqual = a.log.length === b.log.length, hiddenShift = 0;
  if (eventsEqual) for (let i = 0; i < a.log.length; i++) {
    const x = a.log[i], y = b.log[i];
    if (x.text !== y.text || x.hidden !== y.hidden) { eventsEqual = false; break; }
    if (x.hidden) hiddenShift = Math.max(hiddenShift, Math.abs(x.t - y.t));
    else if (Math.abs(x.t - y.t) > 1e-6) { eventsEqual = false; break; }
  }
  const fmt = l => l.map(e => Math.round(e.t) + (e.hidden ? 'h ' : ' ') + e.text);
  const anims = b.svg.all.reduce((n, e) => n + e.animations.length, 0);
  const result = { scene: scene.name, frames, compared, loopEdge, maxMatrixDiff: +maxM.toExponential(2), worst: maxM > 1e-6 ? worst : '', maxOpacityDiff: +maxO.toExponential(2), worstOpacity: maxO > 1e-6 ? worstO : '', maxClipTimeDiff: +maxT.toExponential(2), events: a.log.length, eventsEqual, hiddenEventShiftMs: Math.round(hiddenShift), nativeAnims: anims, animateCalls: b.env.animateCalls, rafOld: a.env.rafCalls, rafNew: b.env.rafCalls, writesOld: a.env.writes, writesNew: b.env.writes };
  if (!eventsEqual) { result.eventsOld = fmt(a.log); result.eventsNew = fmt(b.log); }
  for (const s of sides) s.api.destroy();
  return result;
}
const only = opt('--only', null);
const results = scenes.filter(s => !only || s.name.startsWith(only)).map(runScene);
if (args.includes('--json')) console.log(JSON.stringify(results, null, 1));
else {
  console.table(results.map(r => { const { eventsOld, eventsNew, ...rest } = r; return rest; }));
  for (const r of results) if (!r.eventsEqual) { console.log(r.scene, 'events differ'); console.log(' old', r.eventsOld.join(' | ')); console.log(' new', r.eventsNew.join(' | ')); }
}
