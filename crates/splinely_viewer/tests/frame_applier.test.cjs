'use strict';
// Parity test for `splinely-viewer --runtime rust` (run by tests/rust_runtime.rs, or by hand:
//   node crates/splinely_viewer/tests/frame_applier.test.cjs bundle.json).
//
// bundle.json is written by the Rust test. Each scenario holds a project path, the static SVG
// (`Project::to_static_svg`) and a script of steps. The Rust side executed the script with
// `RustSession` and recorded the frame messages it sends to the page at each checkpoint.
// Here the same messages go through frame_applier.js onto a fake DOM, while the JS runtime
// (runtime/src/splinely.js) replays the script on a second fake DOM from the same static SVG.
// Every attribute of every element must then match, numbers within 1e-9.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { createFrameApplier } = require('../src/frame_applier.js');
const { createRustHost } = require('../src/rust_host.js');
const { createRuntime } = require('../../../runtime/src/splinely.js');

const TOLERANCE = 1e-9;
const STEP = 1 / 60;

class Element {
  constructor(tagName, attrs, parent) {
    this.tagName = tagName; this.attrs = new Map(attrs); this.children = []; this.parentNode = parent || null;
    this.listeners = new Map(); this.textContent = '';
  }
  get id() { return this.attrs.get('id') || ''; }
  getAttribute(k) { return this.attrs.has(k) ? this.attrs.get(k) : null; }
  setAttribute(k, v) { this.attrs.set(k, String(v)); }
  removeAttribute(k) { this.attrs.delete(k); }
  addEventListener(k, fn) { if (!this.listeners.has(k)) this.listeners.set(k, new Set()); this.listeners.get(k).add(fn); }
  removeEventListener(k, fn) { this.listeners.get(k)?.delete(fn); }
  fire(type, event = {}) { for (const fn of [...(this.listeners.get(type) || [])]) fn({ target: this, type, ...event }); }
  all() { return this.children.flatMap(c => [c, ...c.all()]); }
  querySelectorAll(selector) {
    assert.equal(selector, '[id]');
    return this.all().filter(e => e.attrs.has('id'));
  }
}
const unescape = s => s.replace(/&quot;/g, '"').replace(/&apos;/g, "'").replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&#13;/g, '\r').replace(/&amp;/g, '&');

// Minimal parser for the generated static SVG (no CDATA, no mixed content that matters).
function parseSvg(text, env) {
  const src = text.replace(/<!--[\s\S]*?-->/g, '');
  const tag = /<(\/?)([A-Za-z][\w:.-]*)((?:\s+[\w:.-]+="[^"]*")*)\s*(\/?)>/g;
  let root = null, current = null, m;
  while ((m = tag.exec(src))) {
    if (m[1]) { current = current.parentNode; continue; }
    const attrs = [...m[3].matchAll(/([\w:.-]+)="([^"]*)"/g)].map(a => [a[1], unescape(a[2])]);
    const el = new Element(m[2], attrs, current);
    if (current) current.children.push(el); else root = el;
    if (!m[4]) current = el;
  }
  assert.ok(root && root.tagName === 'svg', 'static SVG has an svg root');
  for (const el of root.all()) el.ownerSVGElement = root;
  root.ownerDocument = { defaultView: env };
  root.getScreenCTM = () => ({ inverse: () => ({}) });
  root.createSVGPoint = () => ({ x: 0, y: 0, matrixTransform() { return { x: this.x, y: this.y }; } });
  return root;
}

function snapshot(svg) {
  const out = new Map();
  for (const el of svg.querySelectorAll('[id]')) {
    if (['metadata', 'title'].includes(el.tagName)) continue;
    // Accessibility semantics are browser-runtime only (documented: not reproduced in rust mode).
    out.set(el.id, new Map([...el.attrs].filter(([k]) => !/^(aria-|role$|tabindex$|data-splinely-focusable$)/.test(k))));
  }
  return out;
}
const NUMBER = /[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?/g;
function sameValue(a, b) {
  if (a === b) return true;
  if (a == null || b == null) return false;
  const ta = a.split(NUMBER), tb = b.split(NUMBER);
  if (ta.length !== tb.length || ta.some((t, i) => t !== tb[i])) return false;
  const na = a.match(NUMBER) || [], nb = b.match(NUMBER) || [];
  return na.every((v, i) => Math.abs(Number(v) - Number(nb[i])) <= TOLERANCE * Math.max(1, Math.abs(Number(v))));
}
const stats = { compared: 0, gradient: 0, geometry: 0, strokes: 0 };
function compare(label, applied, js) {
  assert.deepEqual([...applied.keys()].sort(), [...js.keys()].sort(), `${label}: same elements`);
  for (const [id, attrs] of js) {
    const mine = applied.get(id);
    assert.deepEqual([...mine.keys()].sort(), [...attrs.keys()].sort(), `${label}: ${id} attribute names`);
    for (const [name, value] of attrs) {
      assert.ok(sameValue(mine.get(name), value), `${label}: ${id}@${name}: rust-applied ${JSON.stringify(mine.get(name))} != js ${JSON.stringify(value)}`);
      stats.compared++;
      if (name === 'stop-color' || name === 'x1') stats.gradient++;
      if (name === 'd' || name === 'width' || name === 'r') stats.geometry++;
      if (name === 'stroke' && value !== 'none') stats.strokes++;
    }
  }
}

function replay(scenario) {
  const project = JSON.parse(fs.readFileSync(scenario.projectPath, 'utf8'));
  const env = {};
  const jsSvg = parseSvg(scenario.staticSvg, env);
  const f = createRuntime(jsSvg, project, { autoTick: false, reducedMotion: false });
  const rustSvg = parseSvg(scenario.staticSvg, {});
  const applier = createFrameApplier(rustSvg);
  let checkpoints = 0;
  for (const step of scenario.steps) {
    if (step.tick !== undefined) for (let i = 0; i < step.tick; i++) f.tick(STEP);
    else if (step.event) f.emit(step.event.name, {}, step.event.target);
    else if (step.pointer) jsSvg.fire('pointermove', { clientX: step.pointer.x, clientY: step.pointer.y });
    else if (step.leave) jsSvg.fire('pointerleave', {});
    else if (step.checkpoint !== undefined) {
      if (step.message) {
        const result = applier.apply(step.message);
        assert.deepEqual(result.errors, [], `${scenario.name}@${step.checkpoint}: applier errors`);
      }
      compare(`${scenario.name}@${step.checkpoint}`, snapshot(rustSvg), snapshot(jsSvg));
      checkpoints++;
    } else assert.fail('unknown step ' + JSON.stringify(step));
  }
  assert.deepEqual(f.diagnostics.map(e => e.message), [], `${scenario.name}: JS runtime diagnostics`);
  f.destroy();
  return checkpoints;
}

// --- host glue: input batching, coalescing and drag emulation -----------------------------
function glueTest() {
  const svg = parseSvg('<svg xmlns="http://www.w3.org/2000/svg"><g id="grp" data-splinely-group="grp"><rect id="a" data-splinely-node="a" transform="x"/></g></svg>', {});
  svg.getScreenCTM = () => ({ inverse: () => ({}) });
  svg.createSVGPoint = () => ({ x: 0, y: 0, matrixTransform() { return { x: this.x / 2, y: this.y / 2 }; } });
  const posted = [], frames = [];
  const doc = new Element('document', []); doc.getElementById = () => null;
  const win = new Element('window', []); win.requestAnimationFrame = cb => frames.push(cb);
  const rect = svg.children[0].children[0];
  const applier = createFrameApplier(svg);
  const host = createRustHost({ window: win, document: doc, svg, applier, post: m => posted.push(JSON.parse(m)), config: { dragTargets: ['a'] } });
  host.start(null);
  assert.deepEqual(posted.shift(), { t: 'ready' });
  const run = () => { const cbs = frames.splice(0); cbs.forEach(cb => cb()); };
  rect.captured = null; rect.setPointerCapture = id => { rect.captured = id; }; rect.releasePointerCapture = () => { rect.captured = null; };
  const dispatch = (type, event) => svg.fire(type, { target: rect, pointerId: 1, ...event });
  dispatch('pointerdown', { clientX: 20, clientY: 40 });
  dispatch('pointermove', { clientX: 22, clientY: 40 });
  dispatch('pointermove', { clientX: 24, clientY: 44 });
  dispatch('pointerup', { clientX: 24, clientY: 44 });
  dispatch('click', { clientX: 24, clientY: 44 });
  assert.equal(rect.captured, null, 'pointer capture released');
  run();
  assert.equal(posted.length, 1);
  const input = posted[0];
  assert.equal(input.t, 'input');
  assert.deepEqual(input.pointer, { x: 12, y: 22 }, 'CTM-mapped, coalesced to the last position');
  assert.deepEqual(input.events.map(e => e.name), ['pointerdown', 'dragstart', 'pointermove', 'drag', 'pointerup', 'dragend', 'click']);
  assert.ok(input.events.every(e => e.target === 'a'));
  assert.equal(input.events.filter(e => e.name === 'pointermove').length, 1, 'pointermove coalesced');
  posted.length = 0;
  svg.fire('pointerleave', { target: svg });
  doc.fire('keydown', { target: doc, key: 'x' });
  run();
  assert.deepEqual(posted[0], { t: 'input', leave: true, events: [{ name: 'pointerleave' }, { name: 'keydown' }] });
  posted.length = 0;
  host.onFrame({ v: 1, nodes: [{ id: 'a', tf: [1, 2, 0, 1, 1], op: 1, sw: 1, fill: [1, 0, 0, 1], stroke: null }] });
  run();
  assert.deepEqual(posted, [{ t: 'ack' }]);
  assert.equal(rect.getAttribute('transform'), 'translate(1 2) rotate(0) scale(1 1)');
  assert.equal(rect.getAttribute('fill'), 'rgb(255,0,0)');
  host.destroy();
  // A bad frame never throws into the page: it is reported and acknowledged.
  assert.equal(createFrameApplier(svg).apply({ v: 1, nodes: [{ id: 'a', tf: [NaN, 0, 0, 1, 1] }] }).errors.length, 1);
  assert.throws(() => createFrameApplier(svg).apply({ v: 2, nodes: [] }), /Unsupported/);
  assert.equal(createFrameApplier(svg).apply({ v: 1, nodes: [{ id: 'a', tf: [0, 0, 0, 1, 1], op: 1, sw: 1, fill: [0, 0, 0, 1], stroke: null, geo: { d: 'url(http://x)' } }] }).errors.length, 1, 'url() rejected');
}

glueTest();
if (process.argv[2]) {
  const bundle = JSON.parse(fs.readFileSync(path.resolve(process.argv[2]), 'utf8'));
  let total = 0;
  for (const scenario of bundle.scenarios) total += replay(scenario);
  assert.ok(stats.gradient > 0 && stats.geometry > 0 && stats.strokes > 0, 'parity run covered gradients, geometry and strokes: ' + JSON.stringify(stats));
  console.log(`parity ok: ${bundle.scenarios.length} scenarios, ${total} checkpoints, ${stats.compared} attributes compared (${JSON.stringify(stats)})`);
} else console.log('glue ok (pass a bundle for the parity run)');
