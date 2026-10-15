#!/usr/bin/env node
'use strict';
// Splinely browser matrix: the real-browser smoke scenarios in Chromium, Firefox and WebKit via Playwright,
// numeric cross-browser comparison and the audible-output/latency leg. Writes one machine-readable JSON.
//
//   node scripts/browser-matrix.cjs [--browsers chromium,firefox,webkit] [--scenarios a,b] [--out-dir DIR]
//        [--audio chromium[,firefox]|none] [--audio-nonblocking] [--allow-skip] [--tolerance 1e-6]
//        [--showcase-duration 35] [--scenario-timeout 120]
// Environment: PLAYWRIGHT_MODULE (path of the playwright package), PLAYWRIGHT_BROWSERS_PATH, SPLINELY_CLI.
//
// Status vocabulary (never mixed): pass | fail | skip.  A skip means the thing did NOT run (browser missing or
// not launchable, no audio server, ...) and is printed loudly; any skip makes the overall result "incomplete"
// (exit 3) unless --allow-skip is given.  A timeout is a fail.  Exit: 0 pass, 1 fail, 3 incomplete.
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const {spawn, execFileSync} = require('node:child_process');
const lib = require('./browser-matrix-lib.cjs');
const {ROOT, sleep} = lib;

function parseArgs(argv) {
  const a = {browsers: lib.ENGINES, scenarios: null, outDir: null, audio: ['chromium'], audioNonblocking: false, allowSkip: false,
    tolerance: 1e-6, showcaseDuration: 35, scenarioTimeout: 120};
  for (let i = 0; i < argv.length; i++) {
    const k = argv[i], v = () => argv[++i];
    if (k === '--browsers') a.browsers = v().split(',').filter(Boolean);
    else if (k === '--scenarios') a.scenarios = v().split(',').filter(Boolean);
    else if (k === '--out-dir') a.outDir = v();
    else if (k === '--audio') { const x = v(); a.audio = x === 'none' ? [] : x.split(',').filter(Boolean); }
    else if (k === '--audio-nonblocking') a.audioNonblocking = true;
    else if (k === '--allow-skip') a.allowSkip = true;
    else if (k === '--tolerance') a.tolerance = Number(v());
    else if (k === '--showcase-duration') a.showcaseDuration = Number(v());
    else if (k === '--scenario-timeout') a.scenarioTimeout = Number(v());
    else throw new Error('unknown argument ' + k);
  }
  for (const b of a.browsers) if (!lib.ENGINES.includes(b)) throw new Error('unknown browser ' + b);
  return a;
}

class Skip extends Error {}

// ------------------------------------------------------------------------------------------------------------
// Scenario context
// ------------------------------------------------------------------------------------------------------------
function makeContext(env, engine, page) {
  const ctx = {
    engine, page, origin: env.server.origin, gen: env.gen, args: env.args, prepared: env.prepared,
    checks: [], metrics: {}, notes: [], pageErrors: [], consoleErrors: [],
    check(name, ok, detail) { ctx.checks.push({name, pass: !!ok, ...(ok ? {} : {detail: detail === undefined ? undefined : String(typeof detail === 'string' ? detail : JSON.stringify(detail)).slice(0, 600)})}); return !!ok; },
    metric(name, value) { ctx.metrics[name] = value; },
    note(text) { ctx.notes.push(text); },
    skip(reason) { throw new Skip(reason); },
    async waitMounted(timeout = 15000, selector = null) {
      await page.waitForFunction(sel => { const root = sel ? document.querySelector(sel) : document.documentElement; return !!(root && root.splinely); }, selector, {timeout});
    },
  };
  page.on('pageerror', e => ctx.pageErrors.push(String(e.message || e).slice(0, 300)));
  page.on('console', m => { if (m.type() === 'error' && !/favicon|Failed to load resource/.test(m.text())) ctx.consoleErrors.push(m.text().slice(0, 300)); });
  return ctx;
}

// Real-pointer click at the centre of an element (works inside same-origin <object> documents too).
async function clickElement(page, selector, objectSelector = null) {
  const point = await page.evaluate(([sel, objSel]) => {
    if (!objSel) {
      const b = document.querySelector(sel).getBoundingClientRect();
      return {x: b.left + b.width / 2, y: b.top + b.height / 2};
    }
    const object = document.querySelector(objSel), doc = object.contentDocument, el = doc.querySelector(sel);
    object.scrollIntoView({block: 'center'});
    const b = el.getBoundingClientRect(), o = object.getBoundingClientRect();
    return {x: o.left + object.clientLeft + (b.left + b.width / 2) * object.clientWidth / doc.defaultView.innerWidth,
      y: o.top + object.clientTop + (b.top + b.height / 2) * object.clientHeight / doc.defaultView.innerHeight};
  }, [selector, objectSelector]);
  await page.mouse.click(point.x, point.y);
}

// ------------------------------------------------------------------------------------------------------------
// Generated inputs (browser independent, built once with the CLI)
// ------------------------------------------------------------------------------------------------------------
const MOTION_TIMES = [0, 0.1, 0.5, 1, 1.5, 1.9, 2];
function prepareMotion(gen) {
  const key = (t, value) => ({time: t, value: {type: 'number', value}});
  const shapes = ['M0 0 L100 0 L100 300', 'm0 0 q50 100 100 0', 'M0 0 C0 100 100 100 100 0', 'M0 0 L0 0 L10 0 M100 0 L130 0', 'M0 0 L100 0 L100 100 Z'];
  const bounds = [[0, 0, 100, 300], [0, 0, 100, 50], [0, 0, 100, 75], [0, 0, 130, 0], [0, 0, 100, 100]];
  const nodes = [], tracks = [];
  shapes.forEach((d, i) => {
    nodes.push({id: `route${i}`, visible: false, kind: {type: 'path', d, bounds: bounds[i]}, transform: {x: 10, y: 20, rotation: 37, scaleX: 2, scaleY: 0.75}},
      {id: `ball${i}`, kind: {type: 'circle', cx: 0, cy: 0, radius: 3}, transform: {x: 4, rotation: 13}, motionPath: {path: `route${i}`, orient: true}});
    tracks.push({nodeId: `ball${i}`, property: 'motionProgress', keyframes: [key(0, 0), key(2, 1)]},
      {nodeId: `route${i}`, property: 'x', keyframes: [key(0, 10), key(2, 70)]});
  });
  tracks.push({nodeId: 'route1', property: 'd', keyframes: [{time: 0, value: {type: 'path', value: shapes[1]}}, {time: 2, value: {type: 'path', value: 'm0 0 q100 200 200 0'}}]});
  const project = path.join(gen, 'motion.sply');
  fs.writeFileSync(project, JSON.stringify({nodes, clips: [{id: 'travel', duration: 2, autoplay: true, tracks}]}));
  lib.cli(['export', project, '--output', path.join(gen, 'motion.svg')]);
  const expected = MOTION_TIMES.map(t => {
    const evaluated = JSON.parse(lib.cli(['evaluate', project, '--time', t])).nodes;
    return Array.from({length: 5}, (_, i) => {
      const tr = evaluated.find(n => n.id === `ball${i}`).transform, angle = tr.rotation * Math.PI / 180;
      return [Math.cos(angle) * tr.scaleX, Math.sin(angle) * tr.scaleX, -Math.sin(angle) * tr.scaleY, Math.cos(angle) * tr.scaleY, tr.x, tr.y];
    });
  });
  return {expected};
}

// Accessible lamp: two keyboard-reachable nodes (one labelled, one falling back to its name), a hidden decoration,
// announcements on the lamp states and a looping autoplay clip (reduced-motion check). Built only with the CLI.
function prepareA11y(gen) {
  const file = path.join(gen, 'a11y.splinely.json');
  const run = (...args) => lib.cli(args);
  run('new', '--output', file, '--width', '240', '--height', '120', '--name', 'Lamp scene');
  run('node', 'add', file, '--kind', 'rect', '--id', 'switch', '--x', '10', '--y', '10', '--width', '60', '--height', '30', '--fill', '#ffffff');
  run('node', 'add', file, '--kind', 'rect', '--id', 'dimmer', '--x', '90', '--y', '10', '--width', '60', '--height', '30', '--fill', '#cccccc');
  run('node', 'add', file, '--kind', 'circle', '--id', 'deco', '--x', '200', '--y', '60', '--radius', '8');
  run('on', file, '--event', 'click', '--target', 'switch', '--machine', 'lamp', '--from', 'off', '--to', 'on');
  run('on', file, '--event', 'click', '--target', 'switch', '--machine', 'lamp', '--from', 'on', '--to', 'off');
  run('on', file, '--event', 'click', '--target', 'dimmer', '--machine', 'lamp', '--from', 'on', '--to', 'dim');
  run('on', file, '--event', 'click', '--target', 'dimmer', '--machine', 'lamp', '--from', 'dim', '--to', 'on');
  const project = JSON.parse(fs.readFileSync(file, 'utf8'));
  const key = (time, value) => ({time, value: {type: 'number', value}});
  project.clips = [{id: 'pulse', duration: 1, looping: true, autoplay: true, tracks: [{nodeId: 'deco', property: 'x', keyframes: [key(0, 0), key(1, 5)]}]}];
  fs.writeFileSync(file, JSON.stringify(project));
  run('a11y', file, '--description', 'A lamp with a switch and a dimmer.');
  run('a11y', file, '--node', 'switch', '--label', 'Lamp switch', '--description', 'Turns the lamp on or off');
  run('a11y', file, '--node', 'deco', '--hidden', 'true');
  run('a11y', file, '--state-announce', 'lamp.on=Lamp is on', '--state-announce', 'lamp.off=Lamp is off', '--state-announce', 'lamp.dim=Lamp is dimmed');
  run('export', file, '--output', path.join(gen, 'a11y.svg'));
}


// Plugin demo: one rect + both hover-wiggle tools applied with `plugin apply`, exported with and without the plugin.
function preparePlugin(gen) {
  const file = path.join(gen, 'plugin-demo.splinely.json');
  const plugin = path.join(ROOT, 'examples/plugins/hover-wiggle');
  const run = (...args) => lib.cli(args);
  run('new', '--output', file, '--width', '240', '--height', '120', '--name', 'Plugin demo');
  run('node', 'add', file, '--kind', 'rect', '--id', 'btn', '--x', '60', '--y', '30', '--width', '100', '--height', '50', '--fill', '#ff8800');
  const applied = [run('plugin', 'apply', plugin, 'wiggle', '--project', file, '--node', 'btn'), run('plugin', 'apply', plugin, 'bounce', '--project', file, '--node', 'btn')].map(JSON.parse);
  run('export', file, '--output', path.join(gen, 'plugin-trusted.svg'), '--plugin', plugin, '--trusted-js');
  run('export', file, '--output', path.join(gen, 'plugin-none.svg'));
  let refusal = null;
  try { run('export', file, '--output', path.join(gen, 'plugin-untrusted.svg'), '--plugin', plugin); } catch (e) { refusal = String(e.stderr || e.stdout || e.message || e); }
  return {applied, refusal, untrustedWritten: fs.existsSync(path.join(gen, 'plugin-untrusted.svg'))};
}

function prepare(gen) {
  const prepared = {};
  prepareA11y(gen);
  prepared.plugin = preparePlugin(gen);
  prepared.motion = prepareMotion(gen);
  lib.cli(['export', path.join(ROOT, 'runtime/tests/fixtures/blend-spaces.json'), '--output', path.join(gen, 'blending.svg')]);
  lib.cli(['export', path.join(ROOT, 'examples/desert-stickfight.sply'), '--output', path.join(gen, 'desert.svg'), '--trusted-js']);
  prepared.canonicalStick = JSON.parse(lib.cli(['cat', path.join(ROOT, 'examples/desert-stickfight.sply')]));
  return prepared;
}

// ------------------------------------------------------------------------------------------------------------
// Scenarios. Each: async (ctx) => void; use ctx.check / ctx.metric.  Metrics under "compare" are cross-browser compared.
// ------------------------------------------------------------------------------------------------------------
const scenarios = [];
const scenario = (name, description, run) => scenarios.push({name, description, run});

scenario('motion-smoke', 'motion paths: 35 SVG matrices vs Rust evaluate (scripts/motion-smoke.py)', async ctx => {
  const {page} = ctx;
  await page.goto(`${ctx.origin}/gen/motion.svg`);
  await ctx.waitMounted(8000);
  await page.evaluate(() => document.documentElement.splinely.timeline.pause());
  let worst = 0, matrices = [], diagnostics = [];
  for (let k = 0; k < MOTION_TIMES.length; k++) {
    const actual = await page.evaluate(t => {
      const f = document.documentElement.splinely; f.clip('travel').seek(t);
      return {matrices: Array.from({length: 5}, (_, i) => { const m = document.getElementById('ball' + i).transform.baseVal.consolidate().matrix; return [m.a, m.b, m.c, m.d, m.e, m.f]; }),
        diagnostics: f.diagnostics.map(e => e.message)};
    }, MOTION_TIMES[k]);
    diagnostics.push(...actual.diagnostics);
    actual.matrices.forEach((m, i) => { for (let j = 0; j < 6; j++) worst = Math.max(worst, Math.abs(m[j] - ctx.prepared.motion.expected[k][i][j])); });
    matrices.push(actual.matrices);
  }
  ctx.check('35 matrices match Rust evaluate within 1e-3', worst < 1e-3, `max |diff| ${worst}`);
  ctx.check('no runtime diagnostics', diagnostics.length === 0, diagnostics);
  const e = await page.evaluate(() => { document.documentElement.splinely.set('ball0', {motionProgress: 0}); return document.getElementById('ball0').transform.baseVal.consolidate().matrix.e; });
  ctx.check('explicit motionProgress override is immediate (e=74)', Math.abs(e - 74) < 1e-3, e);
  ctx.metric('worstDiffVsRust', worst);
  ctx.metric('compare', {matrices, overrideE: e});
});

scenario('blend-spaces-smoke', 'additive blend spaces + parameters on the exported fixture (scripts/blend-spaces-smoke.py)', async ctx => {
  const {page} = ctx;
  await page.goto(`${ctx.origin}/gen/blending.svg`);
  await ctx.waitMounted(8000);
  const r = await page.evaluate(() => {
    const f = document.documentElement.splinely, n = document.getElementById('n'), out = {steps: {}};
    f.timeline.pause();
    const pose = () => n.getAttribute('transform').match(/[-+]?\d*\.?\d+(?:e[-+]?\d+)?/gi).map(Number);
    const near = (name, a, b) => { out.steps[name] = Math.abs(Number(a) - b) <= 1e-9; };
    const p0 = pose(); near('x', p0[0], 42); near('y', p0[1], 32); near('rot', p0[2], 15); near('scale', p0[3], 1.25);
    near('fillOpacity', n.getAttribute('fill-opacity'), 0.75); near('boneX', f.bones()[0].x, 5);
    out.initial = {pose: p0, fillOpacity: Number(n.getAttribute('fill-opacity')), boneX: f.bones()[0].x};
    f.variable('speed').set(0); near('speed0', pose()[0], 32);
    f.variable('speed').set(2); near('speed2', pose()[0], 52);
    f.variable('speed').set(1); f.clip('offset').fadeTo(0, 1); f.tick(0.5); near('fade.5', pose()[0], 41); near('boneFade.5', f.bones()[0].x, 4);
    f.tick(0.5); near('fade1', pose()[0], 40);
    let rejected = false; try { f.variable('speed').set(null); } catch (e) { rejected = true; }
    out.steps.invalidParameterRejected = rejected;
    f.set('n', {x: 99}); near('override', pose()[0], 99); f.set('n', null); near('overrideCleared', pose()[0], 40);
    out.diagnostics = f.diagnostics.length; out.final = pose();
    return out;
  });
  for (const [name, ok] of Object.entries(r.steps)) ctx.check(`blend step ${name}`, ok);
  ctx.check('no runtime diagnostics', r.diagnostics === 0, r.diagnostics);
  ctx.metric('compare', {initial: r.initial, final: r.final});
});

scenario('demo-gate-and-state', 'demo.svg: audio gate click, WebAudio decode, timeline, click state, property filters, direct file://', async ctx => {
  const {page} = ctx;
  await page.goto(`${ctx.origin}/examples/demo.svg`);
  await ctx.waitMounted();
  const initial = await page.evaluate(() => { const f = document.documentElement.splinely; return {mounted: !!f, playing: f.timeline.playing, controls: document.querySelectorAll('g[role=button]').length}; });
  ctx.check('mounted, paused, one start control', initial.mounted && !initial.playing && initial.controls === 1, initial);
  await page.click('g[role=button] rect');
  await sleep(500);
  const after = await page.evaluate(() => { const f = document.documentElement.splinely; return {playing: f.timeline.playing, ready: f.audio.ready, state: f.audio.context && f.audio.context.state, time: f.timeline.time, buffers: f.audio.buffers.size, controls: document.querySelectorAll('g[role=button]').length}; });
  ctx.check('real click starts timeline', after.playing && after.time > 0.1 && after.controls === 0, after);
  ctx.check('Web Audio unlocked + WAV decoded (context running)', after.ready && after.state === 'running' && after.buffers >= 1, after);
  await page.click('#ball');
  await sleep(150);
  const state = await page.evaluate(() => { const f = document.documentElement.splinely; return {fill: document.getElementById('ball').getAttribute('fill'), machines: Object.values(f.snapshot().states)}; });
  ctx.check('click on #ball -> clicked state, fill rgb(255,102,51)', state.machines.includes('clicked') && state.fill === 'rgb(255,102,51)', state);
  const security = await page.evaluate(() => {
    const root = document.documentElement, f = root.splinely, ns = 'http://www.w3.org/2000/svg';
    f.set('ball', {fill: 'u' + String.fromCharCode(92) + '72l(http://127.0.0.1:1/paint.svg)', srcset: 'http://127.0.0.1:1/image 1x', constructor: 1});
    const nested = document.createElementNS(ns, 'svg'), fo = document.createElementNS(ns, 'foreignObject');
    const div = document.createElementNS('http://www.w3.org/1999/xhtml', 'div'); div.id = 'ball'; div.setAttribute('data-splinely-project', ''); div.textContent = 'invalid JSON';
    fo.append(div); nested.append(fo); root.append(nested); f.set('ball', {opacity: 0.75});
    const isolated = div.getAttribute('opacity') === null, safe = !root.querySelector('#ball').getAttribute('fill').includes('127.0.0.1') && !root.querySelector('#ball').hasAttribute('srcset');
    f.destroy(); const controls = root.querySelectorAll('[data-splinely-host-control]').length;
    const retry = window.SplinelyRuntime.createRuntime(root, JSON.parse(root.querySelector('metadata#splinely-project').textContent), {autoTick: false});
    let drag = false; retry.on('dragstart', () => drag = true); retry.on('pointerdown', () => retry.destroy());
    root.querySelector('#ball').dispatchEvent(new PointerEvent('pointerdown', {bubbles: true, pointerId: 7}));
    nested.remove(); return {isolated, safe, controls, destroyed: !root.splinely, drag};
  });
  ctx.check('property filters, nested foreignObject isolation, destroy/remount, pointer lifecycle', JSON.stringify(security) === JSON.stringify({isolated: true, safe: true, controls: 0, destroyed: true, drag: false}), security);
  // Direct offline file:// load (what the docs call "offline export").
  const file = await ctx.page.context().newPage();
  const fileErrors = []; file.on('pageerror', e => fileErrors.push(String(e.message)));
  await file.goto('file://' + path.join(ROOT, 'examples/demo.svg'));
  await file.waitForFunction(() => !!document.documentElement.splinely, null, {timeout: 15000});
  ctx.check('file:// direct load auto-mounts the runtime', true);
  ctx.check('file:// load without page errors', fileErrors.length === 0, fileErrors);
  await file.close();
});

scenario('anime-hello-gate', 'anime-hello.html: gate click, trusted WAV decode/start, replay click, bone movement (scripts/anime-smoke.py)', async ctx => {
  const {page} = ctx;
  await page.goto(`${ctx.origin}/examples/anime-hello.html`);
  const sel = "object[type='image/svg+xml'], object";
  await page.waitForFunction(() => { try { return !!document.querySelector('object').contentDocument.documentElement.splinely; } catch (e) { return false; } }, null, {timeout: 20000});
  ctx.check('no host buttons', await page.evaluate(() => document.querySelectorAll('button').length) === 0);
  await page.evaluate(() => {
    const doc = document.querySelector('object').contentDocument, w = doc.defaultView;
    window.trusted = []; window.audioStarts = 0;
    const start = w.AudioBufferSourceNode.prototype.start;
    w.AudioBufferSourceNode.prototype.start = function (...a) { window.audioStarts++; return start.apply(this, a); };
    doc.addEventListener('click', e => window.trusted.push(e.isTrusted), true);
  });
  const initial = await page.evaluate(() => document.querySelector('object').contentDocument.querySelector('#wave-hand').getAttribute('transform'));
  ctx.check('audio gate present before click', await page.evaluate(() => document.querySelector('object').contentDocument.querySelectorAll('[data-splinely-host-control]').length) === 1);
  await clickElement(page, '[data-splinely-host-control] rect', sel);
  await sleep(900);
  const result = await page.evaluate(() => {
    const d = document.querySelector('object').contentDocument, f = d.documentElement.splinely;
    return {trusted: window.trusted, time: f.timeline.time, starts: window.audioStarts, ready: f.audio.ready, context: f.audio.context && f.audio.context.state, buffers: f.audio.buffers.size,
      gate: d.querySelectorAll('[data-splinely-host-control]').length, transform: d.querySelector('#wave-hand').getAttribute('transform')};
  });
  ctx.check('trusted click removed the gate', JSON.stringify(result.trusted) === '[true]' && result.gate === 0, result);
  ctx.check('WAV decoded and started via Web Audio', result.starts >= 1 && result.ready && result.context === 'running' && result.buffers === 1, result);
  ctx.check('greeting runs and the hand bone moved', result.time > 0.3 && result.transform !== initial, result);
  const greeting = () => page.evaluate(() => document.querySelector('object').contentDocument.documentElement.splinely.clip('greeting').playing);
  const deadline = Date.now() + 7000;
  while (Date.now() < deadline && await greeting()) await sleep(100);
  ctx.check('greeting ends', !await greeting());
  await clickElement(page, '#wave-hand', sel);
  await sleep(600);
  ctx.check('click on the character replays the greeting + audio', await page.evaluate(() => window.audioStarts) > result.starts);
});

scenario('curious-face', 'real pointer: hover/click state machines, crossfade, pupils follow, blink timer (scripts/face-smoke.py)', async ctx => {
  const {page} = ctx;
  await page.goto(`${ctx.origin}/examples/curious-face.svg`);
  await ctx.waitMounted(8000);
  await page.evaluate(() => { window.trusted = []; document.documentElement.splinely.on('click', d => { if (d.event) trusted.push(d.event.isTrusted); }); });
  const move = async (x, y) => {
    const p = await page.evaluate(([x, y]) => { const svg = document.documentElement, pt = svg.createSVGPoint(); pt.x = x; pt.y = y; const q = pt.matrixTransform(svg.getScreenCTM()); return {x: q.x, y: q.y}; }, [x, y]);
    await page.mouse.move(p.x, p.y, {steps: 4});
  };
  const state = () => page.evaluate(() => {
    const d = document, f = d.documentElement.splinely, m = id => d.getElementById(id).transform.baseVal.consolidate().matrix;
    return {states: f.snapshot().states, pupil: {x: m('pupil-left').e, y: m('pupil-left').f}, scale: m('head').a, fill: d.getElementById('head').getAttribute('fill'),
      cheek: Number(d.getElementById('cheek-left').getAttribute('opacity')), mouth: d.getElementById('mouth').getAttribute('d'), diagnostics: f.diagnostics.map(e => e.message)};
  });
  const base = (await state()).pupil.x;
  await move(5, 5); await sleep(400);
  const rest = await state();
  ctx.check('rest: attention+mood idle', rest.states.attention === 'idle' && rest.states.mood === 'idle', rest.states);
  await move(290, 150); await sleep(600);
  const right = await state();
  ctx.check('hover: near state, head leans (1.06), pupils look right', right.states.attention === 'near' && Math.abs(right.scale - 1.06) < 1e-3 && right.pupil.x - base > 3, right);
  await move(110, 150); await sleep(600);
  const left = await state();
  ctx.check('pupils follow the pointer to the left', left.pupil.x - base < -3, left.pupil);
  await page.mouse.down(); await page.mouse.up(); await sleep(120);
  const mid = await state();
  ctx.check('click: crossfade in progress (mood happy, 0 < cheek < .85)', mid.states.attention === 'near' && mid.states.mood === 'happy' && mid.cheek > 0 && mid.cheek < 0.85, mid);
  await sleep(600);
  const happy = await state();
  ctx.check('happy end state (cheek .85, orange head, morphed mouth)', Math.abs(happy.cheek - 0.85) < 1e-6 && happy.fill === 'rgb(255,179,71)' && happy.mouth !== rest.mouth, happy);
  await move(250, 120); await sleep(500);
  ctx.check('pupils keep following in happy mood', (await state()).pupil.x > left.pupil.x);
  await page.mouse.down(); await page.mouse.up();
  await move(5, 5); await sleep(1000);
  const back = await state();
  ctx.check('second click + leaving returns everything to rest', back.states.attention === 'idle' && back.states.mood === 'idle' && back.cheek === 0 && Math.abs(back.scale - 1) < 1e-6 && back.mouth === rest.mouth, back);
  ctx.check('no diagnostics, both clicks trusted', back.diagnostics.length === 0 && JSON.stringify(await page.evaluate(() => trusted)) === '[true,true]', back.diagnostics);
  await page.waitForFunction(() => document.documentElement.splinely.snapshot().states.blink === 'shut', null, {timeout: 5000});
  await page.waitForFunction(() => document.documentElement.splinely.snapshot().states.blink === 'open', null, {timeout: 2000});
  ctx.check('blink timer machine runs alongside', true);
  ctx.metric('compare', {restMouth: rest.mouth, happyCheek: happy.cheek, happyFill: happy.fill, hoverScale: right.scale});
});

for (const [name, file, kind] of [['showcase-water-drop', 'examples/water-drop.html', 'water'], ['showcase-random-figures', 'examples/random-figures.html', 'random']]) {
  scenario(name, `${file}: autoplay without input, DOM animation${kind === 'random' ? ', every state of every machine visited' : ''} (scripts/showcase-smoke.py)`, async ctx => {
    const {page} = ctx, sel = "object[type='image/svg+xml']";
    await page.goto(`${ctx.origin}/${file}`);
    await page.waitForFunction(() => { try { return !!document.querySelector("object[type='image/svg+xml']").contentDocument.documentElement.splinely; } catch (e) { return false; } }, null, {timeout: 15000});
    const initial = await page.evaluate(() => {
      const doc = document.querySelector("object[type='image/svg+xml']").contentDocument, f = doc.documentElement.splinely, w = doc.defaultView, p = f.sourceProject;
      const t = w.__showcase = {states: [], errors: [], clicks: []};
      w.addEventListener('error', e => t.errors.push(e.message)); w.addEventListener('unhandledrejection', e => t.errors.push(String(e.reason)));
      w.addEventListener('click', e => t.clicks.push(e.isTrusted), true);
      f.on('state.change', e => t.states.push(e));
      return {machines: (p.stateMachines || []).map(m => ({id: m.id, states: m.states.map(s => s.id)})), control: !!doc.querySelector('[data-splinely-host-control]')};
    });
    const snapshot = () => page.evaluate(() => {
      const doc = document.querySelector("object[type='image/svg+xml']").contentDocument, f = doc.documentElement.splinely, p = f.sourceProject, t = doc.defaultView.__showcase;
      return {time: f.timeline.time, playing: f.timeline.playing, machines: (p.stateMachines || []).map(m => ({id: m.id, state: f.machine(m.id).state})),
        attrs: p.nodes.map(n => { const el = doc.getElementById(n.id); return [n.id, el ? ['transform', 'x', 'y', 'cx', 'cy', 'r', 'd', 'opacity'].map(k => el.getAttribute(k)).join('|') : null]; }),
        errors: t.errors, clicks: t.clicks, states: t.states.length, diagnostics: f.diagnostics.map(d => d.message || String(d)), control: !!doc.querySelector('[data-splinely-host-control]')};
    });
    const before = await snapshot();
    ctx.check('no host buttons, no start control', await page.evaluate(() => document.querySelectorAll('button,[role=button]').length) === 0 && !initial.control);
    const seen = Object.fromEntries(initial.machines.map(m => [m.id, new Set()]));
    for (const m of before.machines) seen[m.id].add(m.state);
    const beforeAttrs = Object.fromEntries(before.attrs);
    let changed = false, last = before;
    const t0 = Date.now(), limit = kind === 'water' ? Math.min(ctx.args.showcaseDuration, 10) : ctx.args.showcaseDuration;
    let samples = 0;
    while ((Date.now() - t0) / 1000 < limit) {
      await sleep(250);
      last = await snapshot(); samples++;
      changed = changed || last.attrs.some(([id, v]) => v !== null && beforeAttrs[id] != null && v !== beforeAttrs[id]);
      for (const m of last.machines) seen[m.id].add(m.state);
      const complete = kind === 'water' || initial.machines.every(m => m.states.every(s => seen[m.id].has(s)));
      if (samples >= 3 && changed && complete) break;
    }
    ctx.check('mounted without any input, timeline playing', last.playing && last.time > before.time, {before: before.time, after: last.time});
    ctx.check('no input was sent (zero clicks)', last.clicks.length === 0, last.clicks);
    ctx.check('rendered DOM attributes changed over wall-clock time', changed);
    if (kind === 'random') {
      const missing = initial.machines.flatMap(m => m.states.filter(s => !seen[m.id].has(s)).map(s => `${m.id}:${s}`));
      ctx.check('every state of every machine visited automatically', missing.length === 0 && initial.machines.length > 0, {missing, seconds: (Date.now() - t0) / 1000});
      ctx.check('state.change events recorded', last.states > 0);
    }
    ctx.check('no unexpected runtime diagnostics', last.diagnostics.every(d => d.startsWith('customJs is preserved but not executed')), last.diagnostics);
    ctx.check('no window errors in the SVG document', last.errors.length === 0, last.errors);
    ctx.metric('observedSeconds', (Date.now() - t0) / 1000);
    ctx.metric('compare', {machineStates: Object.fromEntries(Object.entries(seen).map(([k, v]) => [k, [...v].sort().join(',')])), declared: initial.machines.map(m => m.states.length)});
  });
}

scenario('idle-timeline-interactions', 'runtime/tests/idle-timeline-interactions.test.cjs on a real DOM: interactions/machines/timers never need the transport', async ctx => {
  const {page} = ctx;
  await page.goto(`${ctx.origin}/__blank`);
  await page.waitForFunction(() => !!window.SplinelyRuntime, null, {timeout: 8000});
  const r = await page.evaluate(() => {
    const out = {};
    const frame = (time, value) => ({time, value, easing: {type: 'linear'}}), number = value => ({type: 'number', value});
    const clip = () => ({id: 'a', duration: 2, tracks: [{target: 'n', property: 'cx', keyframes: [frame(0, 0), frame(2, 20)]}]});
    const node = {id: 'n', kind: {type: 'circle', cx: 5, cy: 5, radius: 2}, transform: {x: 0, y: 0, rotation: 0, scaleX: 1, scaleY: 1}};
    const svg = document.getElementById('s'), n = document.getElementById('n');
    const mount = project => window.SplinelyRuntime.createRuntime(svg, project, {autoTick: false});
    let f = mount({schemaVersion: 1, nodes: [node], clips: [clip()],
      interactions: [{target: 'n', event: 'click', actions: [{type: 'playClip', clipId: 'a'}]}],
      stateMachines: [{id: 'm', initial: 'off', states: [{id: 'off'}, {id: 'on', overrides: [{nodeId: 'n', property: 'opacity', value: number(0.5)}]}], transitions: [{from: 'off', to: 'on', event: {name: 'click', target: 'n'}}]}]});
    out.idleBefore = !f.timeline.playing && !f.clip('a').playing;
    const seen = []; f.on('n', 'click', () => seen.push('click'));
    n.dispatchEvent(new MouseEvent('click', {bubbles: true}));
    out.clickSeen = JSON.stringify(seen) === '["click"]'; out.machineOn = f.machine('m').state === 'on'; out.clipStartedByInteraction = f.clip('a').playing === true;
    f.tick(1); out.clipTime = f.clip('a').time; out.diag1 = f.diagnostics.length; f.destroy();
    f = mount({schemaVersion: 1, nodes: [node], clips: [clip()], stateMachines: [{id: 'm', initial: 'idle', states: [{id: 'idle'}, {id: 'busy'}], transitions: [{from: 'idle', to: 'busy', after: 2}]},
      {id: 'show', initial: 'first', states: [{id: 'first', onEnter: [{type: 'playClip', clipId: 'a'}]}, {id: 'done'}], transitions: [{from: 'first', to: 'done', event: {name: 'clip.end'}}]}]});
    out.onEnterStartsTransport = f.timeline.playing === true && f.machine('show').state === 'first';
    f.tick(1.9); out.timerNotYet = f.machine('m').state === 'idle'; f.tick(0.2); out.timerFired = f.machine('m').state === 'busy'; f.tick(1); out.clipEndTransition = f.machine('show').state === 'done'; f.destroy();
    f = mount({schemaVersion: 1, nodes: [node], clips: [clip()], stateMachines: [{id: 'm', initial: 'a', states: [{id: 'a'}, {id: 'b'}], transitions: [{from: 'a', to: 'b', event: {name: 'go'}}]}]});
    out.idleStopped = f.timeline.playing === false; f.emit('go'); out.emitTransition = f.machine('m').state === 'b'; out.stillStopped = f.timeline.playing === false; f.destroy();
    return out;
  });
  ctx.check('test 1: idle before, click handler fired, machine on, interaction starts clip', r.idleBefore && r.clickSeen && r.machineOn && r.clipStartedByInteraction, r);
  ctx.check('test 1: clip clock advances to 1 s, no diagnostics', r.clipTime === 1 && r.diag1 === 0, r);
  ctx.check('test 2: initial onEnter playClip starts transport on mount', r.onEnterStartsTransport, r);
  ctx.check('test 2: timer transition fires at 2 s without transport', r.timerNotYet && r.timerFired, r);
  ctx.check('test 2: clip.end transition', r.clipEndTransition, r);
  ctx.check('test 3: idle machine reacts to emit, transport stays stopped', r.idleStopped && r.emitTransition && r.stillStopped, r);
});

scenario('sply-binary-web-player', 'examples/desert-stickfight.sply: JS decoder, CLI binary embed (sply-base64) -> runtime mount, posed bones', async ctx => {
  const {page} = ctx, canonical = ctx.prepared.canonicalStick;
  await page.goto(`${ctx.origin}/__blank`);
  // The runtime keeps its embedded copy of the decoder private; load the standalone source of truth as a plain script.
  await page.addScriptTag({url: '/runtime/src/sply_decode.js'});
  await page.waitForFunction(() => !!window.SplyDecode, null, {timeout: 8000});
  const decoded = await page.evaluate(async canonical => {
    const bytes = new Uint8Array(await (await fetch('/examples/desert-stickfight.sply')).arrayBuffer());
    const isSply = window.SplyDecode.isSply(bytes), project = window.SplyDecode.decodeSply(bytes);
    const eq = (a, b) => { if (a === b) return true; if (typeof a !== typeof b || a === null || b === null || typeof a !== 'object') return false;
      if (Array.isArray(a) !== Array.isArray(b)) return false; const ka = Object.keys(a), kb = Object.keys(b); return ka.length === kb.length && ka.every(k => eq(a[k], b[k])); };
    return {bytes: bytes.length, isSply, equal: eq(project, canonical), nodes: project.nodes.length, bones: project.bones.length, clips: project.clips.length};
  }, canonical);
  ctx.check('standalone JS decode of the .sply file is deep-equal to `splinely-cli cat` (exact f64s)', decoded.isSply && decoded.equal, decoded);
  ctx.check('decoded structure: 104 nodes, 20 bones, 5 clips', decoded.nodes === 104 && decoded.bones === 20 && decoded.clips === 5, decoded);
  await page.goto(`${ctx.origin}/gen/desert.svg`);
  await ctx.waitMounted(20000);
  const info = await page.evaluate(() => {
    const meta = document.querySelector('metadata#splinely-project'), f = document.documentElement.splinely;
    return {encoding: meta && meta.getAttribute('data-encoding'), source: f.sourceProject, state: f.machine('story').state, playing: f.timeline.playing, diagnostics: f.diagnostics.map(d => d.message || String(d))};
  });
  ctx.check('CLI export embeds binary project (data-encoding=sply-base64)', info.encoding === 'sply-base64', info.encoding);
  ctx.check('runtime decoded the embedded sply: clips/stateMachines/bones match canonical', JSON.stringify(info.source.clips) === JSON.stringify(canonical.clips) && JSON.stringify(info.source.stateMachines) === JSON.stringify(canonical.stateMachines) && info.source.bones.length === canonical.bones.length);
  ctx.check('story machine starts in "approach", plays by itself', info.state === 'approach' && info.playing, info);
  const samples = [];
  for (const t of [0.5, 1.5, 3, 4.5]) {
    samples.push(await page.evaluate(t => {
      const f = document.documentElement.splinely; f.timeline.pause(); for (const id of ['approach', 'punches', 'sweep']) f.clip(id).seek(Math.min(t, f.clip(id).duration));
      const num = s => (s || '').match(/[-+]?(?:\d*\.\d+|\d+\.?\d*)(?:[eE][-+]?\d+)?/g)?.map(Number) || [];
      return {bones: f.bones().map(b => [b.x, b.y, b.rotation]).flat(),
        nodes: ['reed-arm-front-ink', 'clay-arm-front-ink', 'reed-head', 'clay-head', 'cactus-2'].map(id => num(document.getElementById(id).getAttribute('transform'))).flat(), diagnostics: f.diagnostics.length};
    }, t));
  }
  ctx.check('seek-posed bones/nodes are finite, move between times, no diagnostics', samples.every(s => s.diagnostics === 0 && s.bones.every(Number.isFinite) && s.nodes.every(Number.isFinite)) && JSON.stringify(samples[0].bones) !== JSON.stringify(samples[3].bones), samples.map(s => s.diagnostics));
  const committed = await page.context().newPage();
  await committed.goto(`${ctx.origin}/examples/desert-stickfight.svg`);
  const enc = await committed.waitForFunction(() => document.documentElement.splinely && document.querySelector('metadata#splinely-project').getAttribute('data-encoding'), null, {timeout: 20000}).then(h => h.jsonValue());
  ctx.check('committed examples/desert-stickfight.svg mounts from sply-base64', enc === 'sply-base64', enc);
  await committed.close();
  ctx.metric('compare', {decodedNodes: decoded.nodes, samples});
});

// Press Tab until the element with this id has focus (the very first Tab may go to the document/browser chrome).
async function tabTo(page, id, max = 6) {
  for (let i = 0; i < max; i++) {
    await page.keyboard.press('Tab');
    if (await page.evaluate(want => !!document.activeElement && document.activeElement.id === want, id)) return true;
  }
  return false;
}

scenario('accessibility-web-player', 'web-player semantics: roles, accessible names, Tab order, Enter/Space, focus ring, aria-live, reduced motion, no change without a11y data', async ctx => {
  const {page} = ctx;
  await page.goto(`${ctx.origin}/gen/a11y.svg`);
  await ctx.waitMounted(10000);
  const tree = await page.evaluate(() => {
    const svg = document.documentElement, attr = (id, name) => { const el = document.getElementById(id); return el && el.getAttribute(name); };
    const labelled = svg.getAttribute('aria-labelledby'), described = svg.getAttribute('aria-describedby');
    return {rootRole: svg.getAttribute('role'), title: labelled && document.getElementById(labelled)?.textContent, desc: described && document.getElementById(described)?.textContent,
      switchRole: attr('switch', 'role'), switchLabel: attr('switch', 'aria-label'), switchTab: attr('switch', 'tabindex'), switchDesc: attr('switch', 'aria-description'),
      dimmerRole: attr('dimmer', 'role'), dimmerLabel: attr('dimmer', 'aria-label'), dimmerTab: attr('dimmer', 'tabindex'),
      decoHidden: attr('deco', 'aria-hidden'), decoTab: attr('deco', 'tabindex'), live: (() => { const l = document.querySelector('[data-splinely-live]'); return l && {role: l.getAttribute('role'), live: l.getAttribute('aria-live'), text: l.textContent}; })()};
  });
  ctx.check('root <svg> is a group named by <title>, described by <desc>', tree.rootRole === 'group' && tree.title === 'Lamp scene' && tree.desc === 'A lamp with a switch and a dimmer.', tree);
  ctx.check('interactive nodes: role=button, tabindex=0, label from a11y.label else node name', tree.switchRole === 'button' && tree.switchTab === '0' && tree.switchLabel === 'Lamp switch' && tree.dimmerRole === 'button' && tree.dimmerTab === '0' && tree.dimmerLabel === 'dimmer', tree);
  ctx.check('decorative node is aria-hidden and not focusable; description set on the switch', tree.decoHidden === 'true' && tree.decoTab === null && tree.switchDesc === 'Turns the lamp on or off', tree);
  ctx.check('polite live region exists and is empty before any transition', tree.live && tree.live.role === 'status' && tree.live.live === 'polite' && tree.live.text === '', tree.live);
  // Accessible names / roles through the browser's own role queries (best effort per engine).
  const named = await page.getByRole('button').evaluateAll(els => els.map(e => e.id));
  ctx.check('page.getByRole(button) finds exactly switch and dimmer, in DOM order (deco hidden)', JSON.stringify(named) === '["switch","dimmer"]', named);
  ctx.check('getByRole(button, name "Lamp switch") resolves the labelled node', await page.getByRole('button', {name: 'Lamp switch'}).count() === 1);
  ctx.check('getByRole(button, name "dimmer") resolves the unlabelled node by its name', await page.getByRole('button', {name: 'dimmer'}).count() === 1);
  const snapshot = await page.locator(':root').ariaSnapshot().catch(error => 'unsupported: ' + error.message);
  ctx.note('aria snapshot: ' + String(snapshot).replace(/\s+/g, ' ').slice(0, 300));
  if (!/^unsupported/.test(snapshot)) ctx.check('aria snapshot lists both buttons by name and no decoration', /button "Lamp switch"/.test(snapshot) && /button "dimmer"/.test(snapshot), snapshot);
  // Keyboard: Tab order, focus ring, Enter / Space.
  const active = () => page.evaluate(() => (document.activeElement && document.activeElement.id) || '');
  const ring = () => page.evaluate(() => { const g = document.querySelector('[data-splinely-focus-ring]'); if (!g) return null; const b = g.getBoundingClientRect(); return {w: b.width, h: b.height, parent: g.previousElementSibling && g.previousElementSibling.id}; });
  const state = () => page.evaluate(() => document.documentElement.splinely.machine('lamp').state);
  const live = () => page.evaluate(() => document.querySelector('[data-splinely-live]').textContent);
  await page.evaluate(() => { window.__clicks = []; for (const id of ['switch', 'dimmer']) document.getElementById(id).addEventListener('click', e => window.__clicks.push([id, e.detail, e.isTrusted])); });
  // The first Tab may land on the document/browser chrome (engine dependent): collect the focused ids of a few presses.
  const sequence = [], rings = [];
  for (let i = 0; i < 6 && sequence.length < 2; i++) {
    await page.keyboard.press('Tab');
    const id = await active();
    if (id) { sequence.push(id); rings.push(await ring()); }
  }
  ctx.check('Tab order: switch, then dimmer (DOM order); deco is skipped', JSON.stringify(sequence) === '["switch","dimmer"]', sequence);
  ctx.check('visible focus ring follows keyboard focus and is sized', rings.length === 2 && rings[0] && rings[0].w > 0 && rings[0].h > 0 && rings[0].parent === 'switch' && rings[1] && rings[1].w > 0 && rings[1].parent === 'dimmer', rings);
  const first = sequence[0], second = sequence[1];
  await page.keyboard.press('Shift+Tab');
  ctx.check('Shift+Tab goes back to switch', await active() === 'switch');
  ctx.check('machine starts in off', await state() === 'off');
  await page.keyboard.press('Enter');
  ctx.check('Enter on the focused node triggers the click transition and announces "Lamp is on"', await state() === 'on' && await live() === 'Lamp is on', {state: await state(), live: await live()});
  await page.keyboard.press('Space');
  ctx.check('Space on the focused node triggers the click transition and announces "Lamp is off"', await state() === 'off' && await live() === 'Lamp is off', {state: await state(), live: await live()});
  const clicks = await page.evaluate(() => window.__clicks);
  ctx.check('keyboard activation dispatches the same click event as the pointer (bubbles, detail 0)', clicks.length === 2 && clicks.every(c => c[0] === 'switch' && c[1] === 0), clicks);
  await page.keyboard.press('Enter');
  await tabTo(page, 'dimmer');
  await page.keyboard.press('Enter');
  ctx.check('second node activates too ("Lamp is dimmed")', await state() === 'dim' && await live() === 'Lamp is dimmed', {state: await state(), live: await live()});
  await page.keyboard.press('x');
  ctx.check('other keys do nothing', await state() === 'dim');
  // A real pointer click keeps working.
  await clickElement(page, '#dimmer');
  ctx.check('pointer click still works and announces too (dimmer: dim -> on)', await state() === 'on' && await live() === 'Lamp is on', {state: await state(), live: await live()});
  const sizes = await page.evaluate(() => { const b = document.getElementById('switch').getBoundingClientRect(); return [Math.round(b.width), Math.round(b.height)]; });
  ctx.check('nodes keep their geometry (no layout change)', sizes[0] > 0 && sizes[1] > 0, sizes);
  const diag = await page.evaluate(() => document.documentElement.splinely.diagnostics.map(d => d.message || String(d)));
  ctx.check('no runtime diagnostics', diag.length === 0, diag);
  // Forced colours: ring uses system colours.
  const forced = await page.context().newPage();
  await forced.emulateMedia({forcedColors: 'active'}).catch(() => {});
  await forced.goto(`${ctx.origin}/gen/a11y.svg`);
  await forced.waitForFunction(() => !!document.documentElement.splinely, null, {timeout: 10000});
  await tabTo(forced, 'switch');
  const forcedRing = await forced.evaluate(() => { const g = document.querySelector('[data-splinely-focus-ring]'); return g && [...g.children].map(c => c.getAttribute('stroke')); });
  ctx.metric('forcedColorsEmulated', await forced.evaluate(() => matchMedia('(forced-colors: active)').matches));
  ctx.check('focus ring present with forced-colors emulation', Array.isArray(forcedRing) && forcedRing.length >= 1, forcedRing);
  await forced.close();
  // Reduced motion: still accessible, no looping autoplay; control page animates.
  const calm = await page.context().newPage(), busy = await page.context().newPage();
  await calm.emulateMedia({reducedMotion: 'reduce'});
  await busy.emulateMedia({reducedMotion: 'no-preference'});
  const motion = {};
  for (const [name, p] of [['calm', calm], ['busy', busy]]) {
    await p.goto(`${ctx.origin}/gen/a11y.svg`);
    await p.waitForFunction(() => !!document.documentElement.splinely, null, {timeout: 10000});
    motion[name] = await p.evaluate(() => { const f = document.documentElement.splinely; return {reduced: f.snapshot().reducedMotion, playing: f.clip('pulse').playing}; });
  }
  ctx.check('prefers-reduced-motion: looping autoplay clip does not start (and does with no-preference)', motion.calm.reduced === true && motion.calm.playing === false && motion.busy.reduced === false && motion.busy.playing === true, motion);
  await tabTo(calm, 'switch');
  await calm.keyboard.press('Enter');
  const calmAfter = await calm.evaluate(() => ({state: document.documentElement.splinely.machine('lamp').state, live: document.querySelector('[data-splinely-live]').textContent}));
  ctx.check('reduced motion: keyboard activation and announcement still work', calmAfter.state === 'on' && calmAfter.live === 'Lamp is on', calmAfter);
  await calm.close(); await busy.close();
  // Projects without a11y data: no attributes, no listeners, no live region.
  const plain = await page.context().newPage();
  await plain.goto(`${ctx.origin}/gen/blending.svg`);
  await plain.waitForFunction(() => !!document.documentElement.splinely, null, {timeout: 10000});
  const none = await plain.evaluate(() => ({role: document.documentElement.getAttribute('role'), tabindex: document.querySelectorAll('[tabindex]').length, live: document.querySelectorAll('[data-splinely-live],[data-splinely-focus-ring],[data-splinely-focusable]').length, aria: document.querySelectorAll('[aria-hidden],[aria-label],[aria-live]').length}));
  ctx.check('project without a11y data: no role/tabindex/live region/aria-hidden added', none.role === null && none.tabindex === 0 && none.live === 0, none);
  await plain.close();
  ctx.metric('compare', {tree: {rootRole: tree.rootRole, switchLabel: tree.switchLabel, dimmerLabel: tree.dimmerLabel}, named, tabOrder: [first, second]});
});

scenario('runtime-plugin', 'hover-wiggle example plugin: plugin apply -> export --plugin --trusted-js -> hover/click run the declarative clips AND the runtime hook; not executed without the trust flag', async ctx => {
  const {page} = ctx, info = ctx.prepared.plugin;
  ctx.check('plugin apply merged clips/machines/variables (editor part)', info.applied[0].added.clips[0] === 'plugin-wiggle-clip-1' && info.applied[0].added.variables.length === 2 && info.applied[1].added.clips[0] === 'plugin-bounce-clip-1', info.applied);
  ctx.check('--plugin without --trusted-js is refused and writes nothing', info.refusal && /trust_required/.test(info.refusal) && !info.untrustedWritten, info);
  const read = () => page.evaluate(() => { const f = document.documentElement.splinely, v = id => f.variable(id).get(), c = id => f.clip(id);
    return {count: v('wiggleCount'), last: v('wiggleLast'), wiggle: {playing: c('plugin-wiggle-clip-1').playing, time: c('plugin-wiggle-clip-1').time}, bounce: {playing: c('plugin-bounce-clip-1').playing, time: c('plugin-bounce-clip-1').time},
      plugins: f.plugins ? f.plugins() : null, diagnostics: f.diagnostics.map(d => String(d.message || d)), meta: !!document.getElementById('splinely-plugins'), metaText: (document.getElementById('splinely-plugins') || {}).textContent || ''}; });
  const center = async () => page.evaluate(() => { const b = document.getElementById('btn').getBoundingClientRect(); return {x: b.left + b.width / 2, y: b.top + b.height / 2}; });
  // 1. Exported with the plugin and the trust flag.
  await page.goto(`${ctx.origin}/gen/plugin-trusted.svg`);
  await ctx.waitMounted(10000);
  let r = await read();
  ctx.check('plugin registered (hover-wiggle, apiVersion 1), no diagnostics, hash-pinned metadata present', Array.isArray(r.plugins) && r.plugins.length === 1 && r.plugins[0].id === 'hover-wiggle' && r.diagnostics.length === 0 && r.meta && /"sha256":"sha256:[0-9a-f]{64}"/.test(r.metaText), r);
  ctx.check('before any input: counter 0, no clip playing', r.count === 0 && !r.wiggle.playing && !r.bounce.playing, r);
  const c = await center();
  await page.mouse.move(2, 2);
  await page.mouse.move(c.x, c.y, {steps: 4});
  await sleep(250);
  r = await read();
  ctx.check('hover: declarative wiggle clip runs AND runtime hook fired (wiggleCount 1, wiggleLast pointerenter:btn)', r.wiggle.time > 0 && r.count === 1 && r.last === 'pointerenter:btn', r);
  await page.mouse.down(); await page.mouse.up();
  await sleep(250);
  r = await read();
  ctx.check('click: declarative bounce clip runs AND runtime hook fired (wiggleCount 2, wiggleLast click:btn)', r.bounce.time > 0 && r.count === 2 && r.last === 'click:btn', r);
  const props = await page.evaluate(() => { const el = document.getElementById('btn'); return {transform: el.getAttribute('transform') || el.parentNode.getAttribute('transform'), count: document.documentElement.splinely.snapshot().diagnostics.length}; });
  ctx.check('animation visibly moved the node (transform attribute set), still no diagnostics', !!props.transform && props.count === 0, props);
  ctx.metric('compare', {count: r.count, last: r.last});
  // 2. Same project exported WITHOUT the plugin: declarative behavior works, plugin code never runs.
  await page.goto(`${ctx.origin}/gen/plugin-none.svg`);
  await ctx.waitMounted(10000);
  const c2 = await center();
  await page.mouse.move(2, 2);
  await page.mouse.move(c2.x, c2.y, {steps: 4});
  await sleep(250);
  await page.mouse.down(); await page.mouse.up();
  await sleep(250);
  r = await read();
  ctx.check('without --plugin: no plugin metadata, plugin list empty, wiggleCount stays 0', !r.meta && r.plugins && r.plugins.length === 0 && r.count === 0 && r.last === '', r);
  ctx.check('without --plugin: declarative clips still play on hover/click', r.wiggle.time > 0 && r.bounce.time > 0, r);
});

scenario('isolated-preview', 'isolated browser preview security regression (scripts/isolated-preview-smoke.cjs, per engine)', async ctx => {
  const result = await new Promise(resolve => {
    const child = spawn(process.execPath, [path.join(ROOT, 'scripts/isolated-preview-smoke.cjs')], {
      env: {...process.env, SPLINELY_BROWSER: ctx.engine, SPLINELY_SKIP_BUILD: '1', SPLINELY_CLI: lib.cliPath()}, stdio: ['ignore', 'pipe', 'pipe']});
    let out = '', err = ''; child.stdout.on('data', d => { out += d; }); child.stderr.on('data', d => { err += d; });
    const timer = setTimeout(() => { child.kill('SIGKILL'); resolve({code: 'timeout', out, err}); }, 100000);
    child.on('close', code => { clearTimeout(timer); resolve({code, out, err}); });
  });
  ctx.check('isolated-preview-smoke.cjs exits 0 (consent, sandbox, CSP, popup/navigation denial, stop/pagehide/reload/history)', result.code === 0 && /PASS/.test(result.out), (result.err || result.out).split('\n').filter(Boolean).slice(0, 4).join(' | '));
  ctx.note(result.out.trim().slice(0, 300));
});

// ------------------------------------------------------------------------------------------------------------
// Runner
// ------------------------------------------------------------------------------------------------------------
async function runScenario(env, engine, browser, s) {
  const started = Date.now();
  const result = {status: 'fail', checks: [], durationMs: 0};
  let context, ctx;
  try {
    context = await browser.newContext({viewport: {width: 1000, height: 800}});
    const page = await context.newPage();
    ctx = makeContext(env, engine, page);
    await Promise.race([s.run(ctx), new Promise((_, reject) => setTimeout(() => reject(new Error(`timeout after ${env.args.scenarioTimeout}s (a timeout is a failure)`)), env.args.scenarioTimeout * 1000))]);
    ctx.check('no uncaught page errors', ctx.pageErrors.length === 0, ctx.pageErrors);
    result.status = ctx.checks.every(c => c.pass) ? 'pass' : 'fail';
  } catch (error) {
    if (error instanceof Skip) { result.status = 'skip'; result.reason = error.message; }
    else { result.status = 'fail'; result.error = String(error.stack || error).split('\n').slice(0, 5).join('\n'); }
  } finally {
    if (ctx) Object.assign(result, {checks: ctx.checks, metrics: ctx.metrics, notes: ctx.notes.length ? ctx.notes : undefined,
      pageErrors: ctx.pageErrors.length ? ctx.pageErrors : undefined, consoleErrors: ctx.consoleErrors.length ? ctx.consoleErrors : undefined});
    result.description = s.description;
    result.durationMs = Date.now() - started;
    await context?.close().catch(() => {});
  }
  return result;
}

// Flatten metrics.compare to {path: number|string|boolean}
function flatten(value, prefix, out) {
  if (typeof value === 'number' || typeof value === 'string' || typeof value === 'boolean') out[prefix] = value;
  else if (Array.isArray(value)) value.forEach((v, i) => flatten(v, `${prefix}[${i}]`, out));
  else if (value && typeof value === 'object') for (const [k, v] of Object.entries(value)) flatten(v, prefix ? `${prefix}.${k}` : k, out);
  return out;
}

function crossCompare(report, tolerance) {
  const out = [];
  const engines = Object.keys(report.browsers).filter(b => report.browsers[b].scenarios);
  for (const s of scenarios) {
    const per = {};
    for (const e of engines) { const r = report.browsers[e].scenarios[s.name]; if (r && r.status !== 'skip' && r.metrics && r.metrics.compare !== undefined) per[e] = flatten(r.metrics.compare, '', {}); }
    const have = Object.keys(per);
    if (have.length < 2) { if (have.length || engines.length) out.push({scenario: s.name, status: 'skip', reason: have.length ? `only ${have[0]} produced comparable metrics` : 'no comparable metrics'}); continue; }
    const ref = have[0];
    for (const other of have.slice(1)) {
      let maxAbs = 0, numeric = 0, exact = 0; const mismatches = [];
      for (const key of Object.keys(per[ref])) {
        const a = per[ref][key], b = per[other][key];
        if (typeof a === 'number' && typeof b === 'number') { numeric++; const d = Math.abs(a - b); if (d > maxAbs) maxAbs = d; if (!(d <= tolerance)) mismatches.push({key, [ref]: a, [other]: b, absDiff: d}); }
        else { exact++; if (a !== b) mismatches.push({key, [ref]: a, [other]: b}); }
      }
      const missing = Object.keys(per[other]).filter(k => !(k in per[ref]));
      out.push({scenario: s.name, reference: ref, other, numericValues: numeric, exactValues: exact, maxAbsDiff: maxAbs, tolerance,
        status: mismatches.length === 0 && missing.length === 0 ? 'pass' : 'fail', mismatches: mismatches.slice(0, 10), mismatchCount: mismatches.length, missingKeys: missing.slice(0, 5)});
    }
  }
  return out;
}

function runAudio(engine, outFile) {
  return new Promise(resolve => {
    const child = spawn(process.execPath, [path.join(__dirname, 'browser-audio-latency.cjs'), '--browser', engine, '--out', outFile, '--runs', '3'], {stdio: ['ignore', 'pipe', 'pipe']});
    let out = '', err = ''; child.stdout.on('data', d => { out += d; }); child.stderr.on('data', d => { err += d; });
    const timer = setTimeout(() => child.kill('SIGKILL'), 240000);
    child.on('close', code => {
      clearTimeout(timer);
      let json = null; try { json = JSON.parse(fs.readFileSync(outFile, 'utf8')); } catch (_) { /* no file */ }
      resolve(json || {status: 'fail', reason: 'audio script produced no JSON (exit ' + code + '): ' + (err || out).slice(0, 300)});
    });
  });
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const outDir = path.resolve(args.outDir || path.join(lib.scratchDir('matrix-').toString(), 'results'));
  fs.mkdirSync(outDir, {recursive: true});
  const report = {schema: 'splinely-browser-matrix/1', startedAt: new Date().toISOString(),
    host: {platform: os.platform(), release: os.release(), node: process.version, cpus: os.cpus().length, totalMemMB: Math.round(os.totalmem() / 1048576)},
    cli: lib.cliInfo(), options: args, browsers: {}, crossBrowser: [], audio: {}, summary: {}};
  const {pw, error: pwError} = lib.loadPlaywright();
  const selected = scenarios.filter(s => !args.scenarios || args.scenarios.includes(s.name));
  if (args.scenarios) for (const n of args.scenarios) if (!scenarios.some(s => s.name === n)) throw new Error('unknown scenario ' + n);
  const gen = lib.scratchDir('matrix-gen-');
  let server;
  try {
    if (report.cli.error) throw new Error(report.cli.error);
    server = await lib.startServer(gen);
    const env = {args, gen, server, prepared: prepare(gen)};
    for (const engine of args.browsers) {
      const entry = report.browsers[engine] = {status: 'skip', reason: '', scenarios: {}};
      let browser;
      try {
        if (!pw) throw Object.assign(new Error(pwError), {skip: true});
        browser = await lib.launchEngine(pw, engine, {});
      } catch (e) {
        entry.status = e.skip ? 'skip' : 'fail'; entry.reason = (e.skip ? 'SKIPPED, NOT PASSED: ' : '') + e.message.slice(0, 500);
        console.log(`[${engine}] ${entry.status.toUpperCase()}: ${entry.reason}`);
        continue;
      }
      entry.version = browser.version();
      console.log(`[${engine}] ${entry.version}`);
      for (const s of selected) {
        const r = entry.scenarios[s.name] = await runScenario(env, engine, browser, s);
        const failed = r.checks.filter(c => !c.pass);
        console.log(`  ${r.status.toUpperCase().padEnd(4)} ${s.name.padEnd(28)} ${(r.durationMs / 1000).toFixed(1)}s  checks ${r.checks.length - failed.length}/${r.checks.length}` +
          (r.status === 'fail' ? '  <- ' + (r.error ? r.error.split('\n')[0] : failed.map(c => c.name).join('; ')) : '') + (r.status === 'skip' ? '  ' + r.reason : ''));
      }
      await browser.close().catch(() => {});
      const all = Object.values(entry.scenarios);
      entry.counts = {pass: all.filter(r => r.status === 'pass').length, fail: all.filter(r => r.status === 'fail').length, skip: all.filter(r => r.status === 'skip').length,
        checksPassed: all.reduce((n, r) => n + r.checks.filter(c => c.pass).length, 0), checksTotal: all.reduce((n, r) => n + r.checks.length, 0)};
      entry.status = entry.counts.fail ? 'fail' : entry.counts.skip ? 'skip' : 'pass';
    }
    report.crossBrowser = crossCompare(report, args.tolerance);
    for (const c of report.crossBrowser) console.log(`  CROSS ${c.status.toUpperCase().padEnd(4)} ${c.scenario.padEnd(28)} ${c.other ? `${c.reference} vs ${c.other}: max|diff| ${c.maxAbsDiff} over ${c.numericValues} numbers (+${c.exactValues} exact), tol ${c.tolerance}` : c.reason}`);
  } finally {
    if (server) await server.close();
    fs.rmSync(gen, {recursive: true, force: true});
  }
  for (const engine of args.audio) {
    console.log(`[audio:${engine}] measuring (null sink + parec) ...`);
    report.audio[engine] = await runAudio(engine, path.join(outDir, `audio-${engine}.json`));
    const a = report.audio[engine];
    console.log(`  AUDIO ${String(a.status).toUpperCase()} ${a.reason || ''}` + (a.latencyMs?.clickToOnset ? `  click->sound median ${a.latencyMs.clickToOnset.median} ms (n=${a.latencyMs.clickToOnset.n})` : ''));
  }
  // ---- summary ----
  const legs = [];
  for (const [engine, e] of Object.entries(report.browsers)) {
    if (!e.scenarios || !Object.keys(e.scenarios).length) legs.push({leg: `browser:${engine}`, status: e.status, reason: e.reason});
    else for (const [name, r] of Object.entries(e.scenarios)) legs.push({leg: `${engine}:${name}`, status: r.status, reason: r.reason});
  }
  for (const c of report.crossBrowser) if (c.status !== 'skip') legs.push({leg: `cross:${c.scenario}:${c.other}`, status: c.status});
  for (const [engine, a] of Object.entries(report.audio)) legs.push({leg: `audio:${engine}`, status: a.status, nonblocking: args.audioNonblocking, reason: a.reason});
  if (report.audio.chromium?.ffplay) legs.push({leg: 'audio:ffplay', status: report.audio.chromium.ffplay.status, nonblocking: args.audioNonblocking});
  const effective = legs.filter(l => !(l.nonblocking && l.status !== 'pass'));
  const count = st => legs.filter(l => l.status === st).length;
  report.summary = {pass: count('pass'), fail: count('fail'), skip: count('skip'), legs: legs.length,
    overall: effective.some(l => l.status === 'fail') ? 'fail' : (effective.some(l => l.status === 'skip') && !args.allowSkip) ? 'incomplete' : 'pass',
    skipped: legs.filter(l => l.status === 'skip').map(l => `${l.leg}: ${l.reason || ''}`), failed: legs.filter(l => l.status === 'fail').map(l => l.leg)};
  // Honest scope statement, not a leg: things this matrix never measures (so nobody reads "pass" as "everything").
  report.summary.notMeasured = ['audio:webkit (no audible-output/latency measurement for WebKit; Playwright WebKit is a headless WPE build, audio leg covers chromium and firefox only)',
    'real audio hardware (the audio leg records a PulseAudio/PipeWire null sink, not speakers or Bluetooth)', 'Safari / WebKitGTK of the native viewer (not driven by Playwright)'];
  report.finishedAt = new Date().toISOString();
  const file = path.join(outDir, 'browser-matrix.json');
  fs.writeFileSync(file, JSON.stringify(report, null, 2) + '\n');
  console.log('');
  for (const [engine, e] of Object.entries(report.browsers)) console.log(`${engine.padEnd(9)} ${e.status.toUpperCase().padEnd(5)} ${e.counts ? `scenarios ${e.counts.pass} pass / ${e.counts.fail} fail / ${e.counts.skip} skip, checks ${e.counts.checksPassed}/${e.counts.checksTotal}` : e.reason}`);
  if (report.summary.skipped.length) console.log('SKIPPED (these did NOT run and are NOT passes):\n  ' + report.summary.skipped.join('\n  '));
  console.log('NOT MEASURED: ' + report.summary.notMeasured.join('; '));
  console.log(`OVERALL: ${report.summary.overall.toUpperCase()} (${report.summary.pass} pass, ${report.summary.fail} fail, ${report.summary.skip} skip)  ->  ${file}`);
  process.exitCode = report.summary.overall === 'pass' ? 0 : report.summary.overall === 'fail' ? 1 : 3;
}

main().then(() => process.exit(process.exitCode || 0), error => { console.error(error); process.exit(1); });
